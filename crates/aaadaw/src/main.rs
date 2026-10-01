use aaadaw_core::{DawAction, Project, Track, TrackId};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task};

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("AAADAW")
        .window_size(iced::Size::new(1280.0, 800.0))
        .run()
}

#[derive(Default)]
struct App {
    project: Project,
    action_query: String,
    status: String,
}

#[derive(Debug, Clone)]
enum Message {
    AddTrack,
    ToggleMute(TrackId),
    ToggleSolo(TrackId),
    AdjustVolume(TrackId, f32),
    Undo,
    Redo,
    ActionQueryChanged(String),
    RunActionQuery,
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AddTrack => self.add_track(),
            Message::ToggleMute(track_id) => {
                if let Some(track) = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                {
                    self.apply_action(
                        DawAction::SetTrackMute {
                            track_id,
                            muted: !track.is_muted(),
                        },
                        "Track mute changed",
                    );
                }
            }
            Message::ToggleSolo(track_id) => {
                if let Some(track) = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                {
                    self.apply_action(
                        DawAction::SetTrackSolo {
                            track_id,
                            solo: !track.is_solo(),
                        },
                        "Track solo changed",
                    );
                }
            }
            Message::AdjustVolume(track_id, delta_db) => {
                if let Some(track) = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                {
                    let volume_db = (track.volume_db() + delta_db).clamp(-60.0, 6.0);
                    self.apply_action(
                        DawAction::SetTrackVolume {
                            track_id,
                            volume_db,
                        },
                        "Track volume changed",
                    );
                }
            }
            Message::Undo => self.undo(),
            Message::Redo => self.redo(),
            Message::ActionQueryChanged(query) => self.action_query = query,
            Message::RunActionQuery => self.run_action_query(),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let toolbar = row![
            text("AAADAW").size(24),
            button("Add Track").on_press(Message::AddTrack),
            button("Undo").on_press(Message::Undo),
            button("Redo").on_press(Message::Redo),
            text("Playback: not connected"),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let action_search = row![
            text_input("Search actions: add track, undo, redo", &self.action_query)
                .on_input(Message::ActionQueryChanged)
                .on_submit(Message::RunActionQuery)
                .width(Length::Fill),
            button("Run").on_press(Message::RunActionQuery),
        ]
        .spacing(8);

        let mut track_list = column![text("Tracks").size(18)].spacing(10);
        for track in self.project.tracks() {
            track_list = track_list.push(track_row(track));
        }
        if self.project.tracks().is_empty() {
            track_list = track_list.push(text("No tracks. Add one to start editing."));
        }

        let tracks = container(scrollable(track_list))
            .width(300)
            .height(Length::Fill)
            .padding(14);
        let editor = container(
            column![
                text("Timeline").size(20),
                text("Audio and MIDI item editing will appear here."),
                text(format!(
                    "Project rate: {} Hz · {} tracks · {} audio items · {} MIDI items",
                    self.project.settings().sample_rate(),
                    self.project.tracks().len(),
                    self.project.audio_items().len(),
                    self.project.midi_items().len(),
                )),
            ]
            .spacing(12),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(18);

        let workspace = row![tracks, editor].spacing(12).height(Length::Fill);

        container(
            column![
                toolbar,
                action_search,
                workspace,
                text(if self.status.is_empty() {
                    "Project is in memory. Save and open actions are not implemented yet."
                } else {
                    &self.status
                }),
            ]
            .spacing(12)
            .padding(14)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn add_track(&mut self) {
        let index = self.project.tracks().len();
        self.apply_action(
            DawAction::CreateTrack {
                index,
                name: format!("Audio {}", index + 1),
            },
            "Track created",
        );
    }

    fn apply_action(&mut self, action: DawAction, success: &str) {
        self.status = match self.project.apply(action) {
            Ok(()) => success.to_owned(),
            Err(error) => format!("Action failed: {error}"),
        };
    }

    fn undo(&mut self) {
        self.status = match self.project.undo() {
            Ok(true) => "Action undone".to_owned(),
            Ok(false) => "Nothing to undo".to_owned(),
            Err(error) => format!("Undo failed: {error}"),
        };
    }

    fn redo(&mut self) {
        self.status = match self.project.redo() {
            Ok(true) => "Action redone".to_owned(),
            Ok(false) => "Nothing to redo".to_owned(),
            Err(error) => format!("Redo failed: {error}"),
        };
    }

    fn run_action_query(&mut self) {
        match self.action_query.trim().to_ascii_lowercase().as_str() {
            "add track" | "create track" => self.add_track(),
            "undo" => self.undo(),
            "redo" => self.redo(),
            _ => self.status = "Unknown action. Try add track, undo, or redo.".to_owned(),
        }
    }
}

fn track_row(track: &Track) -> Element<'_, Message> {
    let track_id = track.id();
    row![
        text(format!("{} · {:.1} dB", track.name(), track.volume_db())).width(Length::Fill),
        button(if track.is_muted() { "Unmute" } else { "Mute" })
            .on_press(Message::ToggleMute(track_id)),
        button(if track.is_solo() { "Unsolo" } else { "Solo" })
            .on_press(Message::ToggleSolo(track_id)),
        button("−").on_press(Message::AdjustVolume(track_id, -1.0)),
        button("+").on_press(Message::AdjustVolume(track_id, 1.0)),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(test)]
mod tests {
    use super::{App, Message};

    #[test]
    fn track_controls_and_undo_change_project_only_through_actions() {
        let mut app = App::default();
        let _ = app.update(Message::AddTrack);
        let track_id = app.project.tracks()[0].id();

        let _ = app.update(Message::ToggleMute(track_id));
        let _ = app.update(Message::AdjustVolume(track_id, -3.0));
        assert!(app.project.tracks()[0].is_muted());
        assert_eq!(app.project.tracks()[0].volume_db(), -3.0);

        let _ = app.update(Message::Undo);
        assert_eq!(app.project.tracks()[0].volume_db(), 0.0);
        let _ = app.update(Message::Undo);
        assert!(!app.project.tracks()[0].is_muted());
    }

    #[test]
    fn action_search_dispatches_supported_commands() {
        let mut app = App::default();
        let _ = app.update(Message::ActionQueryChanged("add track".to_owned()));
        let _ = app.update(Message::RunActionQuery);
        assert_eq!(app.project.tracks().len(), 1);

        let _ = app.update(Message::ActionQueryChanged("undo".to_owned()));
        let _ = app.update(Message::RunActionQuery);
        assert!(app.project.tracks().is_empty());
    }
}
