use super::{App, Message};
#[cfg(feature = "audio-device")]
use aaadaw_app::PreparedAudioPlayback;
use aaadaw_core::{DawAction, TrackFxPlugin, TrackId};
#[cfg(feature = "audio-device")]
use aaadaw_engine::TrackFxProcessor;
use aaadaw_engine::{ClapParameterCommand, ClapParameterInfo};
use iced::Task;
use iced::window::raw_window_handle::RawWindowHandle;
#[cfg(feature = "audio-device")]
use std::path::Path;

#[allow(clippy::useless_conversion)]
fn x11_window_id(handle: RawWindowHandle) -> Option<u64> {
    match handle {
        RawWindowHandle::Xlib(handle) => Some(u64::from(handle.window)),
        _ => None,
    }
}

impl App {
    #[cfg(feature = "audio-device")]
    pub(super) fn install_track_fx_processors(
        &mut self,
        prepared: &mut PreparedAudioPlayback,
    ) -> Result<Vec<u64>, String> {
        let mut owners = Vec::new();
        let mut owner_targets = Vec::new();
        let mut parameter_targets = Vec::new();
        let mut parameter_senders = Vec::new();
        let mut processors = Vec::new();
        let sample_rate = self.project.settings().sample_rate();
        let max_block_frames = prepared.graph().max_block_frames();

        for track in self.project.tracks() {
            for (chain_index, plugin) in track.fx_chain().iter().enumerate() {
                if !plugin.is_enabled() {
                    continue;
                }
                let parameter_values = plugin.parameter_values().collect::<Vec<_>>();
                // SAFETY: this project plugin was explicitly added from the user's scanned CLAP
                // catalog; in-process CLAP plugins are documented as trusted native code.
                let loaded = unsafe {
                    aaadaw_engine::ClapEffectOwner::load_with_state(
                        Path::new(plugin.bundle_path()),
                        plugin.plugin_id(),
                        plugin.state(),
                        &parameter_values,
                        sample_rate,
                        max_block_frames,
                    )
                };
                let (loaded, state_restored) = match loaded {
                    Err(state_error)
                        if plugin.state().is_some() && state_error.is_state_restore_error() =>
                    {
                        self.clap_plugin_warnings.push(format!(
                            "{} state could not be restored; using its default state ({state_error})",
                            plugin.plugin_id()
                        ));
                        // SAFETY: same trusted plugin entry selected by the project; retry only
                        // omits its optional saved state after the plugin rejected that state.
                        (
                            unsafe {
                                aaadaw_engine::ClapEffectOwner::load_with_state(
                                    Path::new(plugin.bundle_path()),
                                    plugin.plugin_id(),
                                    None,
                                    &parameter_values,
                                    sample_rate,
                                    max_block_frames,
                                )
                            },
                            false,
                        )
                    }
                    result => (result, true),
                };
                match loaded {
                    Ok((owner, mut processor)) => {
                        let instance_id = owner.instance_id();
                        parameter_targets.push((instance_id, (track.id(), chain_index)));
                        if state_restored {
                            owner_targets.push((
                                owner.instance_id(),
                                (track.id(), chain_index, plugin.plugin_id().to_owned()),
                            ));
                        }
                        owners.push((instance_id, owner));
                        parameter_senders.push((instance_id, processor.take_parameter_sender()));
                        processors.push(TrackFxProcessor::new(
                            track.id(),
                            chain_index,
                            plugin.plugin_id(),
                            processor,
                        ));
                    }
                    Err(error) => {
                        self.clap_plugin_warnings.push(format!(
                            "Could not activate effect {}; it was skipped ({error})",
                            plugin.plugin_id()
                        ));
                    }
                }
            }
        }

        if let Err(error) = prepared
            .graph_mut()
            .install_fx_processors(&self.project, &mut processors)
        {
            deactivate_uninstalled_fx(owners, processors);
            return Err(format!("Could not prepare track CLAP effects: {error}"));
        }

        let ids = owners.iter().map(|(instance_id, _)| *instance_id).collect();
        self.clap_effect_targets.extend(owner_targets);
        self.clap_effect_parameter_targets.extend(parameter_targets);
        self.clap_effect_parameter_senders.extend(parameter_senders);
        for (instance_id, owner) in owners {
            self.clap_effect_owners.insert(instance_id, owner);
        }
        Ok(ids)
    }

    #[cfg(feature = "audio-device")]
    pub(super) fn discard_unused_effect_owners(&mut self, ids: &[u64]) -> Option<String> {
        let mut error_message = None;
        for id in ids {
            let result = self
                .clap_effect_owners
                .get_mut(id)
                .map(|owner| owner.try_deactivate_unused());
            match result {
                Some(Ok(())) => {
                    self.clap_effect_owners.remove(id);
                    self.clap_effect_targets.remove(id);
                    self.clap_effect_parameter_targets.remove(id);
                    self.clap_effect_parameter_senders.remove(id);
                }
                Some(Err(error)) => {
                    error_message = Some(format!(
                        "Could not deactivate unused CLAP instance {id}: {error}"
                    ));
                }
                None => {}
            }
        }
        error_message
    }

    #[cfg(feature = "audio-device")]
    pub(super) fn deactivate_stopped_effects(
        &mut self,
        processors: Vec<aaadaw_engine::StoppedTrackFxProcessor>,
    ) {
        for stopped in processors {
            let instance_id = stopped.instance_id();
            if let Some(owner) = self.clap_effect_owners.remove(&instance_id) {
                self.clap_effect_targets.remove(&instance_id);
                self.clap_effect_parameter_targets.remove(&instance_id);
                self.clap_effect_parameter_senders.remove(&instance_id);
                let (_, _, _, processor) = stopped.into_parts();
                owner.deactivate(processor);
            } else {
                self.status = format!(
                    "Stopped CLAP instance {instance_id} has no matching owner; plugin cleanup was skipped"
                );
            }
        }
    }
}

#[cfg(feature = "audio-device")]
fn deactivate_uninstalled_fx(
    owners: Vec<(u64, aaadaw_engine::ClapEffectOwner)>,
    processors: Vec<TrackFxProcessor>,
) {
    debug_assert_eq!(owners.len(), processors.len());
    for ((owner_id, owner), effect) in owners.into_iter().zip(processors) {
        let (processor_id, _, _, _, processor) = effect.into_parts();
        debug_assert_eq!(owner_id, processor_id);
        owner.deactivate(processor.stop());
    }
}

impl App {
    pub(super) fn open_track_fx_chain(&mut self, track_id: TrackId) -> Task<Message> {
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            self.status = "Track no longer exists".to_owned();
            return Task::none();
        };
        self.fx_chain_track_id = Some(track_id);
        self.fx_chain_selected_index = (!track.fx_chain().is_empty()).then_some(0);
        if self.fx_chain_window_id.is_some() {
            self.close_selected_fx_plugin_gui();
            return self.open_selected_fx_plugin_gui();
        }

        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(880.0, 560.0),
            min_size: Some(iced::Size::new(680.0, 420.0)),
            ..iced::window::Settings::default()
        });
        self.fx_chain_window_id = Some(window_id);
        self.fx_chain_window_size = iced::Size::new(880.0, 560.0);
        task.then(move |opened_id| {
            let handle = iced::window::run(opened_id, |window| {
                window
                    .window_handle()
                    .ok()
                    .and_then(|handle| x11_window_id(handle.as_raw()))
            })
            .map(move |handle| Message::FxChainWindowNativeHandle(opened_id, handle));
            let scale_factor = iced::window::scale_factor(opened_id)
                .map(move |scale| Message::FxChainWindowScaleFactor(opened_id, scale));
            Task::batch([handle, scale_factor])
        })
    }

    pub(super) fn open_plugin_picker(&mut self) -> Task<Message> {
        let Some(track_id) = self.fx_chain_track_id else {
            self.status = "Open a track FX chain before adding a plugin".to_owned();
            return Task::none();
        };
        if !self
            .project
            .tracks()
            .iter()
            .any(|track| track.id() == track_id)
        {
            self.status = "Track no longer exists".to_owned();
            return Task::none();
        }
        self.plugin_picker_track_id = Some(track_id);
        self.plugin_picker_instrument_track_id = None;
        self.plugin_picker_search.clear();
        if self.plugin_picker_window_id.is_some() {
            return Task::none();
        }

        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(600.0, 440.0),
            min_size: Some(iced::Size::new(460.0, 320.0)),
            ..iced::window::Settings::default()
        });
        self.plugin_picker_window_id = Some(window_id);
        task.discard()
    }

    pub(super) fn add_scanned_plugin(&mut self, plugin_id: &str) -> Task<Message> {
        let Some(track_id) = self.plugin_picker_track_id else {
            self.status = "Plugin selection has no target track".to_owned();
            return Task::none();
        };
        let Some(plugin) = self
            .clap_plugin_scan
            .plugins
            .iter()
            .find(|plugin| plugin.plugin_id == plugin_id)
            .cloned()
        else {
            self.status =
                "Plugin is no longer in the scan results; rescan the CLAP paths".to_owned();
            return Task::none();
        };
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            self.status = "Track no longer exists".to_owned();
            return Task::none();
        };
        let Some(reference) = TrackFxPlugin::new(
            plugin.plugin_id,
            plugin.entry_path.to_string_lossy().into_owned(),
        ) else {
            self.status = "Scanned plugin has an invalid CLAP reference".to_owned();
            return Task::none();
        };
        let mut chain = track.fx_chain().to_vec();
        let selected_index = chain.len();
        chain.push(reference);
        let previous_revision = self.revision;
        self.apply_action(
            DawAction::SetTrackFxChain {
                track_id,
                plugins: chain,
            },
            &format!("Added {} to the track FX chain", plugin.name),
        );
        if self.revision == previous_revision {
            return Task::none();
        }

        self.fx_chain_track_id = Some(track_id);
        self.fx_chain_selected_index = Some(selected_index);
        self.plugin_picker_track_id = None;
        self.plugin_picker_instrument_track_id = None;
        self.plugin_picker_search.clear();
        let close_picker: Task<Message> = self
            .plugin_picker_window_id
            .take()
            .map_or_else(Task::none, iced::window::close::<Message>);
        Task::batch([close_picker, self.open_selected_fx_plugin_gui()])
    }

    pub(super) fn select_fx_chain_plugin(&mut self, index: usize) -> Task<Message> {
        let Some(track_id) = self.fx_chain_track_id else {
            return Task::none();
        };
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            return Task::none();
        };
        if index < track.fx_chain().len() {
            self.fx_chain_selected_index = Some(index);
            self.close_selected_fx_plugin_gui();
            return self.open_selected_fx_plugin_gui();
        }
        Task::none()
    }

    pub(super) fn toggle_fx_chain_plugin(&mut self, index: usize) {
        let Some(track_id) = self.fx_chain_track_id else {
            return;
        };
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            self.status = "Track no longer exists".to_owned();
            return;
        };
        let mut chain = track.fx_chain().to_vec();
        let Some(plugin) = chain.get_mut(index) else {
            return;
        };
        let enabled = !plugin.is_enabled();
        *plugin = plugin.clone().with_enabled(enabled);
        self.apply_action(
            DawAction::SetTrackFxChain {
                track_id,
                plugins: chain,
            },
            if enabled {
                "Track FX enabled"
            } else {
                "Track FX bypassed"
            },
        );
    }

    pub(super) fn remove_selected_fx_plugin(&mut self) -> Task<Message> {
        let (Some(track_id), Some(index)) = (self.fx_chain_track_id, self.fx_chain_selected_index)
        else {
            self.status = "Select a plugin to remove".to_owned();
            return Task::none();
        };
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            self.status = "Track no longer exists".to_owned();
            return Task::none();
        };
        let mut chain = track.fx_chain().to_vec();
        if index >= chain.len() {
            self.fx_chain_selected_index = None;
            self.close_selected_fx_plugin_gui();
            return Task::none();
        }
        chain.remove(index);
        self.apply_action(
            DawAction::SetTrackFxChain {
                track_id,
                plugins: chain,
            },
            "Plugin removed from the track FX chain",
        );
        self.fx_chain_selected_index = if self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            .is_some_and(|track| !track.fx_chain().is_empty())
        {
            Some(
                index.min(
                    self.project
                        .tracks()
                        .iter()
                        .find(|track| track.id() == track_id)
                        .map_or(0, |track| track.fx_chain().len() - 1),
                ),
            )
        } else {
            None
        };
        self.close_selected_fx_plugin_gui();
        self.open_selected_fx_plugin_gui()
    }

    pub(super) fn attach_fx_editor_host(&mut self) {
        self.close_selected_fx_plugin_gui();
        self.fx_chain_editor_host = None;
        let has_selected_plugin = self.fx_chain_track_id.is_some_and(|track_id| {
            self.project
                .tracks()
                .iter()
                .find(|track| track.id() == track_id)
                .and_then(|track| {
                    self.fx_chain_selected_index
                        .and_then(|index| track.fx_chain().get(index))
                })
                .is_some()
        });
        if !has_selected_plugin {
            self.fx_chain_editor_status.clear();
            return;
        }
        let Some(parent) = self.fx_chain_native_parent else {
            self.fx_chain_editor_status =
                "Native plugin editors require an X11 or XWayland window".to_owned();
            return;
        };
        match super::x11_plugin_editor::X11PluginEditorHost::new(
            parent,
            self.fx_chain_window_size,
            self.fx_chain_window_scale_factor,
        ) {
            Ok(host) => {
                self.fx_chain_editor_status.clear();
                self.fx_chain_editor_host = Some(host);
            }
            Err(error) => self.fx_chain_editor_status = error,
        }
    }

    pub(super) fn resize_fx_editor_host(&mut self) {
        let Some(host) = &self.fx_chain_editor_host else {
            return;
        };
        if let Err(error) =
            host.resize(self.fx_chain_window_size, self.fx_chain_window_scale_factor)
        {
            self.fx_chain_editor_status = error;
            return;
        }
        let (width, height) =
            host.available_size(self.fx_chain_window_size, self.fx_chain_window_scale_factor);
        if let Some(plugin_gui) = &mut self.fx_chain_plugin_gui
            && let Err(error) = plugin_gui.resize(width, height)
        {
            self.fx_chain_editor_status = error.to_string();
        }
    }

    pub(super) fn open_selected_fx_plugin_gui(&mut self) -> Task<Message> {
        let (Some(track_id), Some(index)) = (self.fx_chain_track_id, self.fx_chain_selected_index)
        else {
            self.close_selected_fx_plugin_gui();
            self.fx_chain_editor_host = None;
            self.fx_chain_editor_status.clear();
            return Task::none();
        };
        if self.fx_chain_editor_host.is_none() {
            self.attach_fx_editor_host();
        }
        let Some(parent) = self
            .fx_chain_editor_host
            .as_ref()
            .map(|host| host.window_id())
        else {
            self.fx_chain_editor_status = if self.fx_chain_native_parent.is_some() {
                "Could not create the native editor area".to_owned()
            } else {
                "Waiting for the FX chain window to initialize".to_owned()
            };
            return Task::none();
        };
        let Some(plugin) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            .and_then(|track| track.fx_chain().get(index))
        else {
            return Task::none();
        };
        let plugin_id = plugin.plugin_id().to_owned();
        let bundle_path = plugin.bundle_path().to_owned();
        let saved_state = plugin.state().map(<[u8]>::to_vec);
        let parameter_values = plugin.parameter_values().collect::<Vec<_>>();
        let identity = (track_id, index, plugin_id.clone());
        if self.fx_chain_plugin_gui_identity.as_ref() == Some(&identity) {
            return Task::none();
        }
        let (editor_width, editor_height) = self
            .fx_chain_editor_host
            .as_ref()
            .expect("the selected plugin has an X11 editor host")
            .available_size(self.fx_chain_window_size, self.fx_chain_window_scale_factor);
        self.close_selected_fx_plugin_gui();
        // Loading a native plugin runs its own code on the UI thread. The entry was either chosen
        // by the user from the scan list or restored from their project.
        let loaded = unsafe {
            aaadaw_engine::ClapPluginGuiOwner::open_embedded_x11(
                std::path::Path::new(&bundle_path),
                &plugin_id,
                parent,
                (editor_width, editor_height),
                self.fx_chain_window_scale_factor,
                saved_state.as_deref(),
                &parameter_values,
            )
        };
        match loaded {
            Ok(mut plugin_gui) => {
                self.fx_chain_parameters = plugin_gui.parameters();
                self.fx_parameter_value_edits.clear();
                self.fx_parameter_value_edit_pending.clear();
                for parameter in &self.fx_chain_parameters {
                    self.fx_parameter_value_edits
                        .insert(parameter.id, parameter.value.to_string());
                }
                let mut resize_task = Task::none();
                if let Some(editor_size) = plugin_gui.preferred_size() {
                    let required_size = super::x11_plugin_editor::window_size_for_editor(
                        editor_size,
                        self.fx_chain_window_scale_factor,
                    );
                    let expanded_size = iced::Size::new(
                        self.fx_chain_window_size.width.max(required_size.width),
                        self.fx_chain_window_size.height.max(required_size.height),
                    );
                    if expanded_size != self.fx_chain_window_size {
                        self.fx_chain_window_size = expanded_size;
                        if let Some(window_id) = self.fx_chain_window_id {
                            resize_task = iced::window::resize(window_id, expanded_size);
                        }
                    }
                }
                self.fx_chain_editor_status = plugin_gui
                    .show_warning()
                    .map(str::to_owned)
                    .unwrap_or_default();
                self.fx_chain_plugin_gui = Some(plugin_gui);
                self.fx_chain_plugin_gui_identity = Some(identity);
                return resize_task;
            }
            Err(error) => {
                self.fx_chain_editor_status = error.to_string();
                self.fx_chain_editor_host = None;
            }
        }
        Task::none()
    }

    pub(super) fn close_selected_fx_plugin_gui(&mut self) {
        if let Some(gesture) = self.fx_parameter_gesture.as_ref() {
            self.end_fx_parameter_gesture(gesture.parameter_id);
        }
        if let Some(mut plugin_gui) = self.fx_chain_plugin_gui.take() {
            let state = plugin_gui.save_state();
            if let Err(error) = &state {
                self.fx_chain_editor_status = format!("Could not save CLAP editor state: {error}");
            }
            if let (Some((track_id, chain_index, plugin_id)), Ok(Some(state))) =
                (self.fx_chain_plugin_gui_identity.clone(), state)
            {
                #[cfg(feature = "audio-device")]
                self.clap_effect_state_overrides
                    .insert((track_id, chain_index, plugin_id.clone()));
                if let Some(track) = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                {
                    let mut plugins = track.fx_chain().to_vec();
                    if let Some(plugin) = plugins.get_mut(chain_index)
                        && plugin.plugin_id() == plugin_id
                        && plugin.state() != Some(state.as_slice())
                    {
                        *plugin = plugin.clone().with_state(Some(state));
                        #[cfg(feature = "audio-device")]
                        let previous_revision = self.revision;
                        self.apply_action(
                            DawAction::SetTrackFxChain { track_id, plugins },
                            "Saved CLAP editor state",
                        );
                        #[cfg(feature = "audio-device")]
                        if self.revision != previous_revision {
                            self.playback_graph_dirty = true;
                        }
                    }
                }
            }
            plugin_gui.close();
        }
        self.fx_chain_plugin_gui_identity = None;
        self.fx_chain_parameters.clear();
        self.fx_parameter_value_edits.clear();
        self.fx_parameter_value_edit_pending.clear();
    }

    pub(super) fn change_fx_parameter(&mut self, parameter_id: u32, value: f64) {
        if self.fx_parameter_end_requested || self.pending_fx_parameter_sync.is_some() {
            self.status = "Waiting for the active CLAP parameter update to finish".to_owned();
            return;
        }
        let (Some(track_id), Some(chain_index)) =
            (self.fx_chain_track_id, self.fx_chain_selected_index)
        else {
            return;
        };
        let Some(parameter) = self
            .fx_chain_parameters
            .iter()
            .find(|parameter| parameter.id == parameter_id)
        else {
            return;
        };
        if parameter.read_only
            || !value.is_finite()
            || value < parameter.min_value
            || value > parameter.max_value
            || (parameter.stepped && value.fract() != 0.0)
        {
            return;
        }
        let stepped = parameter.stepped;
        let Some(plugin) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            .and_then(|track| track.fx_chain().get(chain_index))
        else {
            return;
        };
        if self.fx_parameter_gesture.is_none() {
            let before = parameter.value;
            if (before - value).abs() < f64::EPSILON {
                return;
            }
            let before_state = plugin.state().map(<[u8]>::to_vec);
            let command = ClapParameterCommand::Begin { id: parameter_id };
            if !self.send_fx_parameter_command(track_id, chain_index, command) {
                self.status = "CLAP parameter queue is full; edit was not started".to_owned();
                return;
            }
            if let Some(gui) = self.fx_chain_plugin_gui.as_mut()
                && let Err(error) = gui.apply_parameter_command(command)
            {
                self.status = format!("Could not begin CLAP parameter edit: {error}");
                return;
            }
            self.fx_parameter_gesture = Some(super::FxParameterGesture {
                track_id,
                chain_index,
                parameter_id,
                before,
                after: before,
                before_state,
            });
        }
        let command = ClapParameterCommand::Set {
            id: parameter_id,
            value,
        };
        if !self.send_fx_parameter_command(track_id, chain_index, command) {
            self.status = "CLAP parameter queue is full; the latest change was skipped".to_owned();
            return;
        }
        let formatted_value = if let Some(gui) = self.fx_chain_plugin_gui.as_mut() {
            if let Err(error) = gui.apply_parameter_command(command) {
                self.status = format!("Could not update CLAP parameter: {error}");
                return;
            }
            stepped
                .then(|| {
                    gui.parameters()
                        .into_iter()
                        .find(|parameter| parameter.id == parameter_id)
                        .map(|parameter| parameter.display_value)
                })
                .flatten()
        } else {
            None
        };
        if let Some(parameter) = self
            .fx_chain_parameters
            .iter_mut()
            .find(|parameter| parameter.id == parameter_id)
        {
            update_parameter_display(parameter, value, formatted_value);
        }
        if !self.fx_parameter_value_edit_pending.contains(&parameter_id) {
            self.fx_parameter_value_edits
                .insert(parameter_id, value.to_string());
        }
        if let Some(gesture) = &mut self.fx_parameter_gesture {
            gesture.after = value;
        }
    }

    pub(super) fn commit_fx_parameter_value(&mut self, parameter_id: u32) {
        let Some(text) = self.fx_parameter_value_edits.get(&parameter_id).cloned() else {
            return;
        };
        let parsed = text.trim().parse::<f64>();
        let Some(parameter) = self
            .fx_chain_parameters
            .iter()
            .find(|parameter| parameter.id == parameter_id)
        else {
            return;
        };
        let Ok(value) = parsed else {
            self.status = "Enter a valid numeric parameter value".to_owned();
            return;
        };
        if !value.is_finite() || value < parameter.min_value || value > parameter.max_value {
            self.status = format!(
                "Value must be between {} and {}",
                parameter.min_value, parameter.max_value
            );
            return;
        }
        if parameter.stepped && value.fract() != 0.0 {
            self.status = "Stepped parameters require a whole-number value".to_owned();
            return;
        }
        self.change_fx_parameter(parameter_id, value);
        self.end_fx_parameter_gesture(parameter_id);
        self.fx_parameter_value_edit_pending.remove(&parameter_id);
        self.fx_parameter_value_edits
            .insert(parameter_id, value.to_string());
    }

    pub(super) fn reset_fx_parameter_value(&mut self, parameter_id: u32) {
        let Some(value) = self
            .fx_chain_parameters
            .iter()
            .find(|parameter| parameter.id == parameter_id)
            .map(|parameter| parameter.default_value)
        else {
            return;
        };
        self.change_fx_parameter(parameter_id, value);
        self.end_fx_parameter_gesture(parameter_id);
        self.fx_parameter_value_edit_pending.remove(&parameter_id);
        self.fx_parameter_value_edits
            .insert(parameter_id, value.to_string());
    }

    pub(super) fn end_fx_parameter_gesture(&mut self, parameter_id: u32) {
        let Some(gesture) = self.fx_parameter_gesture.take() else {
            return;
        };
        if gesture.parameter_id != parameter_id {
            self.fx_parameter_gesture = Some(gesture);
            return;
        }
        let command = ClapParameterCommand::End { id: parameter_id };
        if !self.send_fx_parameter_command(gesture.track_id, gesture.chain_index, command) {
            self.fx_parameter_gesture = Some(gesture);
            self.fx_parameter_end_requested = true;
            self.status =
                "CLAP parameter queue is full; the edit will finish when the audio thread catches up"
                    .to_owned();
            return;
        }
        self.fx_parameter_end_requested = false;
        if let Some(gui) = self.fx_chain_plugin_gui.as_mut()
            && let Err(error) = gui.apply_parameter_command(command)
        {
            self.status = format!("Could not finish CLAP parameter edit: {error}");
        }
        if (gesture.after - gesture.before).abs() < f64::EPSILON {
            return;
        }
        let after_state = match self
            .fx_chain_plugin_gui
            .as_mut()
            .map(|gui| gui.save_state())
        {
            Some(Ok(state)) => state,
            Some(Err(error)) => {
                self.status = format!("Could not capture edited CLAP state: {error}");
                gesture.before_state.clone()
            }
            None => gesture.before_state.clone(),
        };
        self.apply_action(
            DawAction::SetTrackFxParameter {
                track_id: gesture.track_id,
                chain_index: gesture.chain_index,
                parameter_id,
                before: gesture.before,
                after: gesture.after,
                before_state: gesture.before_state,
                after_state,
            },
            "Effect parameter changed",
        );
    }

    fn send_fx_parameter_command(
        &mut self,
        track_id: TrackId,
        chain_index: usize,
        command: ClapParameterCommand,
    ) -> bool {
        #[cfg(feature = "audio-device")]
        {
            let instance_id =
                self.clap_effect_parameter_targets
                    .iter()
                    .find_map(|(instance_id, target)| {
                        (*target == (track_id, chain_index)).then_some(*instance_id)
                    });
            if let Some(sender) = instance_id
                .and_then(|instance_id| self.clap_effect_parameter_senders.get_mut(&instance_id))
                && sender.try_send(command).is_err()
            {
                return false;
            }
            true
        }
        #[cfg(not(feature = "audio-device"))]
        {
            let _ = (track_id, chain_index, command);
            true
        }
    }

    pub(super) fn sync_fx_parameter_cache_from_project(&mut self) {
        let Some(change) = self.project.last_fx_parameter_change() else {
            return;
        };
        self.sync_fx_parameter_change(change);
    }

    pub(super) fn sync_fx_parameter_change(&mut self, change: aaadaw_core::FxParameterChange) {
        let stepped = self
            .fx_chain_parameters
            .iter()
            .find(|parameter| parameter.id == change.parameter_id)
            .is_some_and(|parameter| parameter.stepped);
        let begin = ClapParameterCommand::Begin {
            id: change.parameter_id,
        };
        let begin_queued =
            self.send_fx_parameter_command(change.track_id, change.chain_index, begin);
        let gui_matches_target =
            self.fx_chain_plugin_gui_identity
                .as_ref()
                .is_some_and(|identity| {
                    identity.0 == change.track_id && identity.1 == change.chain_index
                });
        let set = ClapParameterCommand::Set {
            id: change.parameter_id,
            value: change.value,
        };
        let end = ClapParameterCommand::End {
            id: change.parameter_id,
        };
        let (set_queued, end_queued) = if begin_queued {
            let set_queued =
                self.send_fx_parameter_command(change.track_id, change.chain_index, set);
            // Always close a gesture after Begin, even if a full queue rejected Set.
            let end_queued =
                self.send_fx_parameter_command(change.track_id, change.chain_index, end);
            (set_queued, end_queued)
        } else {
            (false, false)
        };
        let formatted_value = if gui_matches_target {
            self.fx_chain_plugin_gui.as_mut().and_then(|gui| {
                let _ = gui.apply_parameter_command(begin);
                let _ = gui.apply_parameter_command(set);
                let _ = gui.apply_parameter_command(end);
                stepped
                    .then(|| {
                        gui.parameters()
                            .into_iter()
                            .find(|parameter| parameter.id == change.parameter_id)
                            .map(|parameter| parameter.display_value)
                    })
                    .flatten()
            })
        } else {
            None
        };
        if !begin_queued || !set_queued || !end_queued {
            self.pending_fx_parameter_sync = Some(change);
            self.status = "CLAP parameter queue is full; undo/redo sync will retry".to_owned();
        } else {
            self.pending_fx_parameter_sync = None;
        }
        if self.fx_chain_track_id == Some(change.track_id)
            && self.fx_chain_selected_index == Some(change.chain_index)
        {
            if let Some(parameter) = self
                .fx_chain_parameters
                .iter_mut()
                .find(|parameter| parameter.id == change.parameter_id)
            {
                update_parameter_display(parameter, change.value, formatted_value);
            }
            self.fx_parameter_value_edits
                .insert(change.parameter_id, change.value.to_string());
            self.fx_parameter_value_edit_pending
                .remove(&change.parameter_id);
        }
    }

    pub(super) fn close_fx_editor_resources(&mut self) {
        self.close_selected_fx_plugin_gui();
        self.fx_chain_editor_host = None;
        self.fx_chain_native_parent = None;
        self.fx_chain_editor_status.clear();
    }
}

fn update_parameter_display(
    parameter: &mut ClapParameterInfo,
    value: f64,
    formatted_value: Option<String>,
) {
    parameter.value = value;
    if let Some(formatted_value) = formatted_value {
        parameter.display_value = formatted_value;
    } else if !parameter.stepped {
        parameter.display_value = format!("{value:.3}");
    }
}
