use super::{App, Message};
use aaadaw_core::{DawAction, TrackFxPlugin, TrackId};
use iced::Task;

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
            return Task::none();
        }

        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(880.0, 560.0),
            min_size: Some(iced::Size::new(680.0, 420.0)),
            ..iced::window::Settings::default()
        });
        self.fx_chain_window_id = Some(window_id);
        task.discard()
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
        self.plugin_picker_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn select_fx_chain_plugin(&mut self, index: usize) {
        let Some(track_id) = self.fx_chain_track_id else {
            return;
        };
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            return;
        };
        if index < track.fx_chain().len() {
            self.fx_chain_selected_index = Some(index);
        }
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

    pub(super) fn remove_selected_fx_plugin(&mut self) {
        let (Some(track_id), Some(index)) = (self.fx_chain_track_id, self.fx_chain_selected_index)
        else {
            self.status = "Select a plugin to remove".to_owned();
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
        if index >= chain.len() {
            self.fx_chain_selected_index = None;
            return;
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
    }
}
