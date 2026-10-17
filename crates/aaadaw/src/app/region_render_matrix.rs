use super::{App, Message, commands};
use aaadaw_core::{DawAction, RegionId, TrackId};
use iced::Task;

impl App {
    pub(super) fn open_region_render_matrix(&mut self) -> Task<Message> {
        if let Some(window_id) = self.region_render_matrix_window_id {
            return iced::window::gain_focus(window_id);
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(820.0, 520.0),
            min_size: Some(iced::Size::new(540.0, 320.0)),
            ..iced::window::Settings::default()
        });
        self.region_render_matrix_window_id = Some(window_id);
        task.discard()
    }

    pub(super) fn close_region_render_matrix(&mut self) -> Task<Message> {
        self.region_render_matrix_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn set_region_render_matrix_master(&mut self, region_id: RegionId, enabled: bool) {
        self.set_region_render_matrix_cell(region_id, None, enabled);
    }

    pub(super) fn set_region_render_matrix_track(
        &mut self,
        region_id: RegionId,
        track_id: TrackId,
        enabled: bool,
    ) {
        self.set_region_render_matrix_cell(region_id, Some(track_id), enabled);
    }

    fn set_region_render_matrix_cell(
        &mut self,
        region_id: RegionId,
        track_id: Option<TrackId>,
        enabled: bool,
    ) {
        if commands::project_edit_busy(self) {
            self.status = "Stop playback or finish the current project operation before editing the render matrix".to_owned();
            return;
        }
        let Some(region) = self
            .project
            .regions()
            .iter()
            .find(|region| region.id() == region_id)
        else {
            self.status = "Project region no longer exists".to_owned();
            return;
        };
        let mut render_master_mix = region.renders_master_mix();
        let mut render_track_ids = region.render_track_ids().to_vec();
        match track_id {
            Some(track_id) => {
                if !self
                    .project
                    .tracks()
                    .iter()
                    .any(|track| track.id() == track_id)
                {
                    self.status = "Project track no longer exists".to_owned();
                    return;
                }
                if enabled && !render_track_ids.contains(&track_id) {
                    render_track_ids.push(track_id);
                } else if !enabled {
                    render_track_ids.retain(|assigned| *assigned != track_id);
                }
            }
            None => render_master_mix = enabled,
        }
        self.apply_action(
            DawAction::SetProjectRegionRenderMatrix {
                region_id,
                render_master_mix,
                render_track_ids,
            },
            "Region render matrix changed",
        );
    }
}
