use super::{App, Message, commands};
use aaadaw_core::{DawAction, MarkerId, RegionId};
use iced::Task;

#[derive(Debug, Clone)]
pub(super) struct MarkerManager {
    pub(super) filter: String,
    pub(super) show_regions: bool,
    pub(super) show_markers: bool,
    pub(super) selected_marker: Option<MarkerId>,
    pub(super) selected_region: Option<RegionId>,
}

impl Default for MarkerManager {
    fn default() -> Self {
        Self {
            filter: String::new(),
            // REAPER 7.82 opens the manager with Regions enabled and Markers disabled.
            show_regions: true,
            show_markers: false,
            selected_marker: None,
            selected_region: None,
        }
    }
}

impl App {
    pub(super) fn open_marker_manager(&mut self) -> Task<Message> {
        if let Some(window_id) = self.marker_manager_window_id {
            return iced::window::gain_focus(window_id);
        }
        self.marker_manager = Some(MarkerManager::default());
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(674.0, 387.0),
            min_size: Some(iced::Size::new(520.0, 260.0)),
            ..iced::window::Settings::default()
        });
        self.marker_manager_window_id = Some(window_id);
        task.discard()
    }

    pub(super) fn close_marker_manager(&mut self) -> Task<Message> {
        self.marker_manager = None;
        self.marker_manager_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn select_marker_from_manager(&mut self, marker_id: MarkerId) {
        let Some(marker) = self
            .project
            .markers()
            .iter()
            .find(|marker| marker.id() == marker_id)
        else {
            self.status = "Project marker no longer exists".to_owned();
            return;
        };
        let position_ticks = marker.position_ticks();
        if let Some(manager) = &mut self.marker_manager {
            manager.selected_marker = Some(marker_id);
            manager.selected_region = None;
        }
        self.handle_timeline_view_event(crate::timeline::TimelineEvent::SetEditCursor(
            position_ticks,
        ));
        self.status = "Moved to project marker".to_owned();
    }

    pub(super) fn delete_marker_from_manager(&mut self, marker_id: MarkerId) {
        if commands::project_edit_busy(self) {
            self.status =
                "Stop playback or finish the current project operation before deleting a marker"
                    .to_owned();
            return;
        }
        if !self
            .project
            .markers()
            .iter()
            .any(|marker| marker.id() == marker_id)
        {
            self.status = "Project marker no longer exists".to_owned();
            return;
        }
        self.apply_action(
            DawAction::DeleteProjectMarker { marker_id },
            "Project marker deleted",
        );
        if let Some(manager) = &mut self.marker_manager
            && manager.selected_marker == Some(marker_id)
        {
            manager.selected_marker = None;
        }
    }

    pub(super) fn select_region_from_manager(&mut self, region_id: RegionId) {
        let Some(region) = self
            .project
            .regions()
            .iter()
            .find(|region| region.id() == region_id)
        else {
            self.status = "Project region no longer exists".to_owned();
            return;
        };
        let start_ticks = region.start_ticks();
        let end_ticks = region.end_ticks();
        if let Some(manager) = &mut self.marker_manager {
            manager.selected_region = Some(region_id);
            manager.selected_marker = None;
        }
        self.handle_timeline_view_event(crate::timeline::TimelineEvent::SetTimeSelection {
            start_tick: start_ticks,
            end_tick: end_ticks,
        });
        self.handle_timeline_view_event(crate::timeline::TimelineEvent::SetEditCursor(start_ticks));
        self.status = "Selected project region".to_owned();
    }

    pub(super) fn delete_region_from_manager(&mut self, region_id: RegionId) {
        if commands::project_edit_busy(self) {
            self.status =
                "Stop playback or finish the current project operation before deleting a region"
                    .to_owned();
            return;
        }
        if !self
            .project
            .regions()
            .iter()
            .any(|region| region.id() == region_id)
        {
            self.status = "Project region no longer exists".to_owned();
            return;
        }
        self.apply_action(
            DawAction::DeleteProjectRegion { region_id },
            "Project region deleted",
        );
        if let Some(manager) = &mut self.marker_manager
            && manager.selected_region == Some(region_id)
        {
            manager.selected_region = None;
        }
    }
}
