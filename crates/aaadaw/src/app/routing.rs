use super::{App, Message, MobilePanel};
use aaadaw_core::{DawAction, SendId, TrackId};
use iced::Task;

#[derive(Debug)]
pub(super) struct SendDraft {
    pub(super) volume_db: String,
    pub(super) pan: String,
}

impl App {
    pub(super) fn open_track_routing(&mut self, track_id: TrackId) -> Task<Message> {
        if !self
            .project
            .tracks()
            .iter()
            .any(|track| track.id() == track_id)
        {
            self.status = "Routing track no longer exists".into();
            return Task::none();
        }
        self.routing_track_id = Some(track_id);
        self.routing_send_drafts.clear();
        if self.is_mobile_main_window() {
            self.show_mobile_panel(MobilePanel::Routing);
            return Task::none();
        }
        if let Some(id) = self.routing_window_id {
            return iced::window::gain_focus(id);
        }
        let (id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(760.0, 560.0),
            min_size: Some(iced::Size::new(520.0, 360.0)),
            ..Default::default()
        });
        self.routing_window_id = Some(id);
        task.discard()
    }

    pub(super) fn close_track_routing(&mut self) -> Task<Message> {
        self.routing_track_id = None;
        self.routing_send_drafts.clear();
        if self.mobile_panel == MobilePanel::Routing {
            self.navigate_back_mobile_panel();
        }
        self.routing_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn apply_routing_change(&mut self, action: DawAction) -> bool {
        let revision = self.revision;
        if !matches!(
            action,
            DawAction::SetTrackOutput { .. }
                | DawAction::SetTrackFolder { .. }
                | DawAction::SetTrackParent { .. }
                | DawAction::SetTrackMainSend { .. }
                | DawAction::CreateAudioSend { .. }
                | DawAction::UpdateAudioSend { .. }
                | DawAction::DeleteAudioSend { .. }
        ) {
            self.status = "Invalid routing command".into();
        } else if self.project_graph_edit_busy() {
            self.status = "Stop playback or recording before changing track routing".into();
        } else {
            self.apply_action(action, "Track routing changed");
        }
        self.revision != revision
    }

    pub(super) fn set_routing_send_draft(&mut self, id: SendId, is_pan: bool, value: String) {
        let Some(send) = self
            .routing_track_id
            .and_then(|source| {
                self.project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == source)
            })
            .and_then(|track| track.sends().iter().find(|send| send.id() == id))
        else {
            return;
        };
        let parameters = send.parameters();
        let draft = self
            .routing_send_drafts
            .entry(id)
            .or_insert_with(|| SendDraft {
                volume_db: parameters.volume_db.to_string(),
                pan: parameters.pan.to_string(),
            });
        if is_pan {
            draft.pan = value;
        } else {
            draft.volume_db = value;
        }
    }

    pub(super) fn commit_routing_send(&mut self, id: SendId) {
        let Some(source) = self.routing_track_id else {
            return;
        };
        let Some(send) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == source)
            .and_then(|track| track.sends().iter().find(|send| send.id() == id))
        else {
            return;
        };
        let Some(draft) = self.routing_send_drafts.get(&id) else {
            return;
        };
        let (Ok(volume_db), Ok(pan)) = (draft.volume_db.parse(), draft.pan.parse()) else {
            self.status = "Send volume and pan must be numbers".into();
            return;
        };
        let mut parameters = send.parameters();
        parameters.volume_db = volume_db;
        parameters.pan = pan;
        let accepted = self.apply_routing_change(DawAction::UpdateAudioSend {
            track_id: source,
            send_id: id,
            destination: send.destination(),
            parameters,
        });
        if accepted {
            self.routing_send_drafts.remove(&id);
        }
    }
}
