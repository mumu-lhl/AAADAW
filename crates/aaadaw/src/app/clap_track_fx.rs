use super::{App, Message};
use aaadaw_core::{DawAction, TrackFxPlugin, TrackId};
use iced::Task;
use iced::window::raw_window_handle::RawWindowHandle;

#[allow(clippy::useless_conversion)]
fn x11_window_id(handle: RawWindowHandle) -> Option<u64> {
    match handle {
        RawWindowHandle::Xlib(handle) => Some(u64::from(handle.window)),
        _ => None,
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
                editor_width,
                editor_height,
                self.fx_chain_window_scale_factor,
            )
        };
        match loaded {
            Ok(mut plugin_gui) => {
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
        if let Some(mut plugin_gui) = self.fx_chain_plugin_gui.take() {
            plugin_gui.close();
        }
        self.fx_chain_plugin_gui_identity = None;
    }

    pub(super) fn close_fx_editor_resources(&mut self) {
        self.close_selected_fx_plugin_gui();
        self.fx_chain_editor_host = None;
        self.fx_chain_native_parent = None;
        self.fx_chain_editor_status.clear();
    }
}
