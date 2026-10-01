use aaadaw_core::{DawAction, Project, ProjectSnapshot, Track, TrackId};
use aaadaw_storage::ProjectStore;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("AAADAW")
        .window_size(iced::Size::new(1280.0, 800.0))
        .run()
}

#[derive(Default)]
struct App {
    project: Project,
    action_query: String,
    project_path_query: String,
    project_path: Option<PathBuf>,
    revision: u64,
    saved_revision: u64,
    io_busy: bool,
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
    ProjectPathChanged(String),
    OpenProject,
    SaveProject,
    ProjectLoaded(PathBuf, Arc<Mutex<Option<Result<Project, String>>>>),
    ProjectSaved(PathBuf, u64, Result<(), String>),
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let mut app = Self::default();
        let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
            return (app, Task::none());
        };
        app.project_path_query = path.to_string_lossy().into_owned();
        app.io_busy = true;
        app.status = format!("Opening {}…", path.display());
        let message_path = path.clone();
        let task = Task::perform(
            run_blocking("aaadaw-project-open", move || load_project_file(path)),
            move |result| Message::ProjectLoaded(message_path, Arc::new(Mutex::new(Some(result)))),
        );
        (app, task)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        if self.io_busy
            && !matches!(
                &message,
                Message::ProjectLoaded(..) | Message::ProjectSaved(..)
            )
        {
            self.status = "Wait for current project operation to finish".to_owned();
            return Task::none();
        }
        let mut task = Task::none();
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
            Message::ProjectPathChanged(path) => {
                if self.io_busy {
                    self.status = "Wait for current project operation to finish".to_owned();
                } else {
                    self.project_path_query = path;
                }
            }
            Message::OpenProject => task = self.open_project(),
            Message::SaveProject => task = self.save_project(),
            Message::ProjectLoaded(path, result) => {
                self.io_busy = false;
                let result = result.lock().ok().and_then(|mut result| result.take());
                match result {
                    Some(Ok(project)) => {
                        self.project = project;
                        self.project_path_query = path.to_string_lossy().into_owned();
                        self.project_path = Some(path.clone());
                        self.revision = 0;
                        self.saved_revision = 0;
                        self.status = format!("Opened {}", path.display());
                    }
                    Some(Err(error)) => self.status = format!("Open failed: {error}"),
                    None => self.status = "Project open result was unavailable".to_owned(),
                }
            }
            Message::ProjectSaved(path, revision, result) => {
                self.io_busy = false;
                match result {
                    Ok(()) => {
                        self.project_path_query = path.to_string_lossy().into_owned();
                        self.project_path = Some(path.clone());
                        self.saved_revision = revision;
                        self.status = if self.revision == revision {
                            format!("Saved {}", path.display())
                        } else {
                            format!("Saved {}; newer edits remain unsaved", path.display())
                        };
                    }
                    Err(error) => self.status = format!("Save failed: {error}"),
                }
            }
        }
        task
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

        let project_controls = row![
            text_input("Path to .aaadaw project", &self.project_path_query)
                .on_input(Message::ProjectPathChanged)
                .width(Length::Fill),
            button("Open").on_press(Message::OpenProject),
            button(if self.io_busy { "Working…" } else { "Save" }).on_press(Message::SaveProject),
            text(if self.is_dirty() {
                "Unsaved"
            } else if self.project_path.is_some() {
                "Saved"
            } else {
                "New"
            }),
        ]
        .spacing(8);

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
                project_controls,
                action_search,
                workspace,
                text(if self.status.is_empty() {
                    "New project. Enter a path to save or open a project."
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

    fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    fn open_project(&mut self) -> Task<Message> {
        if self.io_busy {
            self.status = "Wait for current project operation to finish".to_owned();
            return Task::none();
        }
        if self.is_dirty() {
            self.status = "Save current project before opening another".to_owned();
            return Task::none();
        }
        let Some(path) = project_path_from_query(&self.project_path_query) else {
            self.status = "Enter a project file path first".to_owned();
            return Task::none();
        };
        self.io_busy = true;
        self.status = format!("Opening {}…", path.display());
        let message_path = path.clone();
        Task::perform(
            run_blocking("aaadaw-project-open", move || load_project_file(path)),
            move |result| Message::ProjectLoaded(message_path, Arc::new(Mutex::new(Some(result)))),
        )
    }

    fn save_project(&mut self) -> Task<Message> {
        if self.io_busy {
            self.status = "Wait for current project operation to finish".to_owned();
            return Task::none();
        }
        let Some(path) = self
            .project_path
            .clone()
            .or_else(|| project_path_from_query(&self.project_path_query))
        else {
            self.status = "Enter a project file path first".to_owned();
            return Task::none();
        };
        let can_overwrite = self.project_path.is_some();
        let revision = self.revision;
        let snapshot = self.project.snapshot();
        self.io_busy = true;
        self.status = format!("Saving {}…", path.display());
        let message_path = path.clone();
        Task::perform(
            run_blocking("aaadaw-project-save", move || {
                save_project_file(path, snapshot, can_overwrite)
            }),
            move |result| Message::ProjectSaved(message_path, revision, result),
        )
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
            Ok(()) => {
                self.revision = self.revision.wrapping_add(1);
                success.to_owned()
            }
            Err(error) => format!("Action failed: {error}"),
        };
    }

    fn undo(&mut self) {
        self.status = match self.project.undo() {
            Ok(true) => {
                self.revision = self.revision.wrapping_add(1);
                "Action undone".to_owned()
            }
            Ok(false) => "Nothing to undo".to_owned(),
            Err(error) => format!("Undo failed: {error}"),
        };
    }

    fn redo(&mut self) {
        self.status = match self.project.redo() {
            Ok(true) => {
                self.revision = self.revision.wrapping_add(1);
                "Action redone".to_owned()
            }
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

fn project_path_from_query(query: &str) -> Option<PathBuf> {
    let query = query.trim();
    (!query.is_empty()).then(|| PathBuf::from(query))
}

/// Runs blocking project storage work away from the Iced update thread.
#[allow(clippy::unused_async)]
async fn run_blocking<T: Send + 'static>(
    name: &'static str,
    operation: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(operation)
        .map_err(|error| format!("could not start worker: {error}"))?
        .join()
        .map_err(|_| format!("{name} worker panicked"))?
}

fn load_project_file(path: PathBuf) -> Result<Project, String> {
    if !path.is_file() {
        return Err(format!("project file {} does not exist", path.display()));
    }
    let store = ProjectStore::open(&path).map_err(|error| error.to_string())?;
    let project = store.load().map_err(|error| error.to_string());
    let close = store.close().map_err(|error| error.to_string());
    let project = project?;
    close?;
    Ok(project)
}

fn save_project_file(
    path: PathBuf,
    snapshot: ProjectSnapshot,
    can_overwrite: bool,
) -> Result<(), String> {
    if path == Path::new(":memory:") {
        return Err("project path must name a file".to_owned());
    }
    if !can_overwrite && path.exists() {
        return Err("file exists; open it before saving to that path".to_owned());
    }
    let project = Project::from_snapshot(snapshot).map_err(|error| error.to_string())?;
    let mut store = ProjectStore::open(&path).map_err(|error| error.to_string())?;
    let save = store.save(&project).map_err(|error| error.to_string());
    let close = store.close().map_err(|error| error.to_string());
    save?;
    close?;
    Ok(())
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
    use super::{App, Message, load_project_file, save_project_file};
    use aaadaw_core::{DawAction, Project};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_FILE: AtomicU64 = AtomicU64::new(0);

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

    #[test]
    fn opening_missing_path_does_not_create_a_project_file() {
        let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aaadaw-ui-missing-{}-{file_id}.aaadaw",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        assert!(load_project_file(path.clone()).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn project_file_save_and_open_round_trip_core_snapshot() {
        let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("aaadaw-ui-{}-{file_id}.aaadaw", std::process::id()));
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Persisted".to_owned(),
            })
            .expect("track should be created");
        let expected = project.snapshot();

        save_project_file(path.clone(), expected.clone(), false)
            .expect("new project file should save");
        assert!(save_project_file(path.clone(), Project::new().snapshot(), false).is_err());
        let loaded = load_project_file(path.clone()).expect("project should open");
        assert_eq!(loaded.snapshot(), expected);

        let _ = std::fs::remove_file(&path);
        for suffix in ["-wal", "-shm"] {
            let sidecar = format!("{}{suffix}", path.display());
            let _ = std::fs::remove_file(sidecar);
        }
    }
}
