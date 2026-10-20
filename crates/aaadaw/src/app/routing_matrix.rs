use super::{App, Message, commands};
use aaadaw_core::{AudioSendParameters, DawAction, TrackId};
use iced::Task;

impl App {
    pub(super) fn open_routing_matrix(&mut self) -> Task<Message> {
        if let Some(window_id) = self.routing_matrix_window_id {
            return iced::window::gain_focus(window_id);
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(1280.0, 800.0),
            min_size: Some(iced::Size::new(720.0, 480.0)),
            ..iced::window::Settings::default()
        });
        self.routing_matrix_window_id = Some(window_id);
        task.discard()
    }

    pub(super) fn close_routing_matrix(&mut self) -> Task<Message> {
        self.routing_matrix_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn set_routing_matrix_main_send(&mut self, track_id: TrackId, enabled: bool) {
        if commands::project_edit_busy(self) {
            self.status =
                "Stop playback or finish the current project operation before changing routing"
                    .to_owned();
            return;
        }
        if !self
            .project
            .tracks()
            .iter()
            .any(|track| track.id() == track_id)
        {
            self.status = "Project track no longer exists".to_owned();
            return;
        }
        self.apply_routing_change(DawAction::SetTrackMainSend { track_id, enabled });
    }

    pub(super) fn set_routing_matrix_send(
        &mut self,
        source: TrackId,
        destination: TrackId,
    ) -> Task<Message> {
        if commands::project_edit_busy(self) {
            self.status =
                "Stop playback or finish the current project operation before changing routing"
                    .to_owned();
            return Task::none();
        }
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == source)
        else {
            self.status = "Source track no longer exists".to_owned();
            return Task::none();
        };
        if !self
            .project
            .tracks()
            .iter()
            .any(|track| track.id() == destination)
        {
            self.status = "Destination track no longer exists".to_owned();
            return Task::none();
        }
        let send_ids: Vec<_> = track
            .sends()
            .iter()
            .filter(|send| send.destination() == destination)
            .map(|send| send.id())
            .collect();
        if send_ids.is_empty() {
            if !self.project.can_route_to(source, destination) {
                self.status = "This route would create a track routing cycle".to_owned();
                return Task::none();
            }
            self.apply_routing_change(DawAction::CreateAudioSend {
                track_id: source,
                destination,
                parameters: AudioSendParameters::default(),
            });
            return Task::none();
        }
        self.open_track_routing(source)
    }
}
