use super::{App, Message};
#[cfg(feature = "audio-device")]
use aaadaw_app::PreparedAudioPlayback;
use aaadaw_core::{DawAction, TrackId, TrackInstrument};
#[cfg(all(feature = "audio-device", not(target_os = "android")))]
use aaadaw_engine::{CLAP_IPC_MAX_BLOCK_FRAMES, CLAP_IPC_MAX_EVENTS, ClapIpcConfig};
#[cfg(all(feature = "audio-device", not(target_os = "android")))]
use aaadaw_engine::{ClapInstrumentHelperProcess, TrackIsolatedInstrument};
#[cfg(all(feature = "audio-device", target_os = "android"))]
use aaadaw_engine::{ClapInstrumentOwner, TrackInstrumentProcessor};
use iced::Task;
#[cfg(feature = "audio-device")]
use std::path::PathBuf;

#[allow(clippy::useless_conversion)]
fn native_parent_handle(handle: iced::window::raw_window_handle::RawWindowHandle) -> u64 {
    match handle {
        iced::window::raw_window_handle::RawWindowHandle::Xlib(handle) => u64::from(handle.window),
        iced::window::raw_window_handle::RawWindowHandle::Win32(handle) => {
            handle.hwnd.get() as usize as u64
        }
        _ => 0,
    }
}

impl App {
    pub(super) fn track_instrument_gui_open(&self, _track_id: TrackId) -> bool {
        #[cfg(feature = "audio-device")]
        {
            self.clap_instrument_helper_targets
                .iter()
                .any(|(instance_id, target)| {
                    target.0 == _track_id
                        && self
                            .clap_instrument_helper_owners
                            .get(instance_id)
                            .is_some_and(|owner| owner.gui_status() == 1)
                })
        }
        #[cfg(not(feature = "audio-device"))]
        false
    }

    pub(super) fn request_track_instrument_gui(
        &self,
        track_id: TrackId,
        open: bool,
    ) -> Task<Message> {
        iced::window::oldest().then(move |window_id| {
            window_id.map_or_else(Task::none, |window_id| {
                iced::window::run(window_id, |window| {
                    window
                        .window_handle()
                        .ok()
                        .map(|handle| native_parent_handle(handle.as_raw()))
                        .unwrap_or_default()
                })
                .map(move |parent| Message::TrackInstrumentNativeParent(track_id, open, parent))
            })
        })
    }

    pub(super) fn set_track_instrument_gui(&mut self, track_id: TrackId, open: bool, parent: u64) {
        #[cfg(target_os = "android")]
        {
            let _ = (track_id, open, parent);
            self.status = "Android CLAP plugin editors are not supported".to_owned();
        }
        #[cfg(all(feature = "audio-device", not(target_os = "android")))]
        {
            let Some(instrument) = self
                .project
                .tracks()
                .iter()
                .find(|track| track.id() == track_id)
                .and_then(|track| track.instrument())
                .cloned()
            else {
                self.status = "Track has no assigned CLAP instrument".to_owned();
                return;
            };
            let existing = self
                .clap_instrument_helper_targets
                .iter()
                .find(|(_, target)| target.0 == track_id && target.1 == instrument.plugin_id())
                .map(|(instance_id, _)| *instance_id);
            let instance_id = if let Some(instance_id) = existing {
                instance_id
            } else if open {
                let executable = match std::env::current_exe() {
                    Ok(executable) => executable,
                    Err(error) => {
                        self.status =
                            format!("Could not locate the CLAP helper executable: {error}");
                        return;
                    }
                };
                let config = ClapIpcConfig::new(
                    self.project.settings().sample_rate(),
                    CLAP_IPC_MAX_BLOCK_FRAMES,
                    CLAP_IPC_MAX_EVENTS,
                );
                let Some(config) = config else {
                    self.status = "Could not configure the isolated CLAP editor helper".to_owned();
                    return;
                };
                let entry_path = PathBuf::from(instrument.bundle_path());
                let first_attempt = ClapInstrumentHelperProcess::spawn(
                    &executable,
                    &entry_path,
                    instrument.plugin_id(),
                    config,
                    instrument.state(),
                );
                let loaded = match first_attempt {
                    Err(error)
                        if instrument.state().is_some()
                            && error
                                .to_string()
                                .to_ascii_lowercase()
                                .contains("state restore") =>
                    {
                        self.clap_plugin_warnings.push(format!(
                            "{} state could not be restored; using its default state ({error})",
                            instrument.plugin_id()
                        ));
                        ClapInstrumentHelperProcess::spawn(
                            &executable,
                            &entry_path,
                            instrument.plugin_id(),
                            config,
                            None,
                        )
                    }
                    result => result,
                };
                match loaded {
                    Ok(owner) => {
                        let instance_id = owner.instance_id();
                        self.clap_instrument_helper_targets
                            .insert(instance_id, (track_id, instrument.plugin_id().to_owned()));
                        self.clap_instrument_helper_owners
                            .insert(instance_id, owner);
                        instance_id
                    }
                    Err(error) => {
                        self.status =
                            format!("Could not start isolated instrument editor: {error}");
                        return;
                    }
                }
            } else {
                self.status = "Instrument editor is already closed".to_owned();
                return;
            };
            let result = self
                .clap_instrument_helper_owners
                .get(&instance_id)
                .ok_or_else(|| "Instrument helper is unavailable".to_owned())
                .and_then(|owner| {
                    owner
                        .request_gui(open, parent)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                });
            self.status = match result {
                Ok(()) if open => "Opening isolated instrument editor…".to_owned(),
                Ok(()) => "Closing isolated instrument editor…".to_owned(),
                Err(error) => format!("Could not change instrument editor state: {error}"),
            };
        }
        #[cfg(all(not(feature = "audio-device"), not(target_os = "android")))]
        {
            let _ = (track_id, open, parent);
            self.status = "CLAP editor windows require an audio-device build".to_owned();
        }
    }

    pub(super) fn open_track_instrument_picker(&mut self, track_id: TrackId) -> Task<Message> {
        if !self
            .project
            .tracks()
            .iter()
            .any(|track| track.id() == track_id)
        {
            self.status = "Track no longer exists".to_owned();
            return Task::none();
        }
        self.plugin_picker_track_id = None;
        self.plugin_picker_instrument_track_id = Some(track_id);
        self.plugin_picker_search.clear();
        if self.is_mobile_main_window() {
            self.show_mobile_panel(super::MobilePanel::PluginPicker);
            return Task::none();
        }
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

    pub(super) fn select_scanned_instrument(&mut self, plugin_id: &str) -> Task<Message> {
        let Some(track_id) = self.plugin_picker_instrument_track_id else {
            self.status = "Instrument selection has no target track".to_owned();
            return Task::none();
        };
        let Some(plugin) = self
            .clap_plugin_scan
            .plugins
            .iter()
            .find(|plugin| plugin.plugin_id == plugin_id && plugin.is_instrument())
            .cloned()
        else {
            self.status =
                "Instrument is no longer in the scan results; rescan the CLAP paths".to_owned();
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
        let Some(instrument) = TrackInstrument::new(
            plugin.plugin_id.clone(),
            plugin.entry_path.to_string_lossy().into_owned(),
        ) else {
            self.status = "Scanned instrument has an invalid CLAP reference".to_owned();
            return Task::none();
        };
        if track.instrument() == Some(&instrument) {
            self.status = format!("{} is already assigned to the track", plugin.name);
            return self.close_instrument_picker();
        }

        self.apply_action(
            DawAction::SetTrackInstrument {
                track_id,
                instrument: Some(instrument),
            },
            &format!("Assigned {} to the track", plugin.name),
        );
        self.close_instrument_picker()
    }

    pub(super) fn clear_track_instrument(&mut self, track_id: TrackId) {
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            self.status = "Track no longer exists".to_owned();
            return;
        };
        if track.instrument().is_none() {
            self.status = "Track has no assigned instrument".to_owned();
            return;
        }
        self.apply_action(
            DawAction::SetTrackInstrument {
                track_id,
                instrument: None,
            },
            "Track instrument cleared",
        );
    }

    fn close_instrument_picker(&mut self) -> Task<Message> {
        self.plugin_picker_instrument_track_id = None;
        self.plugin_picker_search.clear();
        if self.is_mobile_main_window() {
            self.navigate_back_mobile_panel();
            return Task::none();
        }
        self.plugin_picker_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    #[cfg(feature = "audio-device")]
    pub(super) fn install_track_instrument_processors(
        &mut self,
        prepared: &mut PreparedAudioPlayback,
    ) -> Result<Vec<u64>, String> {
        #[cfg(target_os = "android")]
        {
            self.install_android_track_instrument_processors(prepared)
        }
        #[cfg(not(target_os = "android"))]
        {
            self.install_desktop_track_instrument_processors(prepared)
        }
    }

    #[cfg(all(feature = "audio-device", not(target_os = "android")))]
    fn install_desktop_track_instrument_processors(
        &mut self,
        prepared: &mut PreparedAudioPlayback,
    ) -> Result<Vec<u64>, String> {
        if !self
            .project
            .tracks()
            .iter()
            .any(|track| track.instrument().is_some())
        {
            return Ok(Vec::new());
        }
        let mut owners = Vec::new();
        let mut owner_targets = Vec::new();
        let mut routes = Vec::new();
        let mut active_ids = Vec::new();
        let sample_rate = self.project.settings().sample_rate();
        let max_block_frames = prepared
            .graph()
            .max_block_frames()
            .min(CLAP_IPC_MAX_BLOCK_FRAMES);
        let executable = std::env::current_exe()
            .map_err(|error| format!("Could not locate the AAADAW helper executable: {error}"))?;

        for track in self.project.tracks() {
            if track.is_frozen() {
                continue;
            }
            let Some(instrument) = track.instrument() else {
                continue;
            };
            let config = ClapIpcConfig::new(sample_rate, max_block_frames, CLAP_IPC_MAX_EVENTS)
                .ok_or_else(|| "Could not create CLAP helper protocol configuration".to_owned())?;
            let reusable_id = self
                .clap_instrument_helper_targets
                .iter()
                .find(|(_, target)| target.0 == track.id() && target.1 == instrument.plugin_id())
                .and_then(|(id, _)| {
                    self.clap_instrument_helper_owners
                        .get(id)
                        .filter(|owner| {
                            let current = owner.config();
                            current.sample_rate == config.sample_rate
                                && current.max_block_frames >= config.max_block_frames
                        })
                        .map(|owner| (*id, owner.audio_port(), owner.config()))
                });
            if let Some((instance_id, audio_port, helper_config)) = reusable_id {
                routes.push(TrackIsolatedInstrument::new(
                    track.id(),
                    instance_id,
                    audio_port,
                    helper_config,
                ));
                active_ids.push(instance_id);
                continue;
            }
            let entry_path = PathBuf::from(instrument.bundle_path());
            let loaded = ClapInstrumentHelperProcess::spawn(
                &executable,
                &entry_path,
                instrument.plugin_id(),
                config,
                instrument.state(),
            );
            let loaded = match loaded {
                Err(state_error)
                    if instrument.state().is_some()
                        && state_error
                            .to_string()
                            .to_ascii_lowercase()
                            .contains("state restore") =>
                {
                    self.clap_plugin_warnings.push(format!(
                        "{} state could not be restored; using its default state ({state_error})",
                        instrument.plugin_id()
                    ));
                    ClapInstrumentHelperProcess::spawn(
                        &executable,
                        &entry_path,
                        instrument.plugin_id(),
                        config,
                        None,
                    )
                }
                result => result,
            };
            match loaded {
                Ok(owner) => {
                    let instance_id = owner.instance_id();
                    owner_targets.push((
                        instance_id,
                        track.id(),
                        instrument.plugin_id().to_owned(),
                    ));
                    routes.push(TrackIsolatedInstrument::new(
                        track.id(),
                        instance_id,
                        owner.audio_port(),
                        owner.config(),
                    ));
                    active_ids.push(instance_id);
                    owners.push((instance_id, owner));
                }
                Err(error) => {
                    self.clap_plugin_warnings.push(format!(
                        "Could not activate instrument {}; its track will be silent ({error})",
                        instrument.plugin_id()
                    ));
                }
            }
        }

        if let Err(error) = prepared
            .graph_mut()
            .install_isolated_instrument_ports(&self.project, &mut routes)
        {
            for (_, owner) in &mut owners {
                let _ = owner.shutdown();
                let _ = owner.take_saved_state();
            }
            return Err(format!("Could not prepare track CLAP instruments: {error}"));
        }

        let ids = active_ids;
        self.clap_instrument_helper_targets.extend(
            owner_targets
                .into_iter()
                .map(|(id, track_id, plugin_id)| (id, (track_id, plugin_id))),
        );
        for (instance_id, owner) in owners {
            self.clap_instrument_helper_owners
                .insert(instance_id, owner);
        }
        Ok(ids)
    }

    #[cfg(all(feature = "audio-device", target_os = "android"))]
    fn install_android_track_instrument_processors(
        &mut self,
        prepared: &mut PreparedAudioPlayback,
    ) -> Result<Vec<u64>, String> {
        if !self
            .project
            .tracks()
            .iter()
            .any(|track| track.instrument().is_some())
        {
            return Ok(Vec::new());
        }

        let sample_rate = self.project.settings().sample_rate();
        let max_block_frames = prepared.graph().max_block_frames();
        let mut processors = Vec::new();
        let mut owners = Vec::new();
        let mut targets = Vec::new();
        let mut active_ids = Vec::new();

        for track in self.project.tracks() {
            if track.is_frozen() {
                continue;
            }
            let Some(instrument) = track.instrument() else {
                continue;
            };
            let entry_path = PathBuf::from(instrument.bundle_path());
            let event_capacity = prepared
                .graph()
                .midi_event_capacity_for_track(track.id())
                .saturating_add(64)
                .max(64);
            // SAFETY: Android plugins enter app-private storage only through an explicit user
            // import. CLAP code runs in-process and can still crash AAADAW; import trusted plugins.
            let loaded = unsafe {
                ClapInstrumentOwner::load_with_state(
                    &entry_path,
                    instrument.plugin_id(),
                    instrument.state(),
                    sample_rate,
                    max_block_frames,
                    event_capacity,
                )
            };
            let loaded = match loaded {
                Err(error) if error.is_state_restore_error() => {
                    self.clap_plugin_warnings.push(format!(
                        "{} state could not be restored; using its default state ({error})",
                        instrument.plugin_id()
                    ));
                    // SAFETY: same user-imported Android plugin entry as the first attempt.
                    unsafe {
                        ClapInstrumentOwner::load_with_state(
                            &entry_path,
                            instrument.plugin_id(),
                            None,
                            sample_rate,
                            max_block_frames,
                            event_capacity,
                        )
                    }
                }
                result => result,
            };

            match loaded {
                Ok((owner, processor)) => {
                    let instance_id = owner.instance_id();
                    processors.push(TrackInstrumentProcessor::new(track.id(), processor));
                    targets.push((instance_id, track.id()));
                    active_ids.push(instance_id);
                    owners.push((instance_id, owner));
                }
                Err(error) => self.clap_plugin_warnings.push(format!(
                    "Could not activate Android CLAP instrument {}; its track will be silent ({error})",
                    instrument.plugin_id()
                )),
            }
        }

        if let Err(error) = prepared
            .graph_mut()
            .install_instrument_processors(&self.project, &mut processors)
        {
            drop(processors);
            for (_, owner) in &mut owners {
                let _ = owner.try_deactivate_unused();
            }
            return Err(format!(
                "Could not prepare Android CLAP instruments: {error}"
            ));
        }

        for (instance_id, target) in targets {
            self.clap_instrument_targets.insert(instance_id, target);
        }
        for (instance_id, owner) in owners {
            self.clap_instrument_owners.insert(instance_id, owner);
        }
        Ok(active_ids)
    }

    #[cfg(feature = "audio-device")]
    pub(super) fn discard_unused_instrument_owners(&mut self, ids: &[u64]) -> Option<String> {
        let mut error_message = None;
        for id in ids {
            #[cfg(target_os = "android")]
            let direct_result = self
                .clap_instrument_owners
                .get_mut(id)
                .map(ClapInstrumentOwner::try_deactivate_unused);
            #[cfg(not(target_os = "android"))]
            let direct_result: Option<Result<(), aaadaw_engine::ClapInstrumentError>> = None;
            match direct_result {
                Some(Ok(())) => {
                    self.clap_instrument_owners.remove(id);
                    self.clap_instrument_targets.remove(id);
                }
                Some(Err(error)) => {
                    error_message = Some(format!(
                        "Could not stop Android CLAP instrument {id}: {error}"
                    ));
                }
                None => {}
            }
            let result = self.clap_instrument_helper_owners.get_mut(id).map(|owner| {
                owner
                    .shutdown()
                    .and_then(|_| owner.take_saved_state().map(|_| ()))
            });
            match result {
                Some(Ok(())) => {
                    self.clap_instrument_helper_owners.remove(id);
                    self.clap_instrument_helper_targets.remove(id);
                }
                Some(Err(error)) => {
                    error_message =
                        Some(format!("Could not stop unused CLAP helper {id}: {error}"));
                }
                None => {}
            }
        }
        error_message
    }

    #[cfg(feature = "audio-device")]
    pub(super) fn deactivate_stopped_instruments(
        &mut self,
        processors: Vec<aaadaw_engine::StoppedTrackInstrument>,
    ) {
        for stopped in processors {
            let instance_id = stopped.instance_id();
            if let Some(owner) = self.clap_instrument_owners.remove(&instance_id) {
                self.clap_instrument_targets.remove(&instance_id);
                let (_, processor) = stopped.into_parts();
                owner.deactivate(processor);
            } else {
                self.status = format!(
                    "Stopped CLAP instrument {instance_id} has no matching owner; plugin cleanup was skipped"
                );
            }
        }
    }
}
