use aaadaw_app::{AudioItemImportProgress, AudioItemImportWorker, start_audio_item_import};
#[cfg(feature = "jack-backend")]
use aaadaw_app::{
    PlaybackBuildError, PreparedAudioPlayback, RunningJackPlayback, prepare_audio_playback_at,
};
use aaadaw_core::{DawAction, ItemId, Project, ProjectSnapshot, Track, TrackId};
use aaadaw_storage::ProjectStore;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

mod timeline;

fn main() -> iced::Result {
    let application = iced::application(App::new, App::update, App::view)
        .title("AAADAW")
        .window_size(iced::Size::new(1280.0, 800.0));
    let application = application.subscription(App::subscription);
    application.run()
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
    audio_file_path_query: String,
    import_busy: bool,
    import_finalizing: bool,
    import_cancel_requested: bool,
    import_worker: Option<PendingAudioImport>,
    import_bytes: u64,
    import_total_bytes: Option<u64>,
    status: String,
    #[cfg(feature = "jack-backend")]
    playback: Option<RunningJackPlayback>,
    #[cfg(feature = "jack-backend")]
    playback_busy: bool,
    #[cfg(feature = "jack-backend")]
    playback_playing: bool,
    #[cfg(feature = "jack-backend")]
    playhead_sample: u64,
    #[cfg(feature = "jack-backend")]
    seek_sample_query: String,
}

struct PendingAudioImport {
    worker: AudioItemImportWorker,
}

#[derive(Clone)]
struct SharedAudioImportWorker(Arc<Mutex<Option<Result<AudioItemImportWorker, String>>>>);

impl std::fmt::Debug for SharedAudioImportWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedAudioImportWorker(..)")
    }
}

#[derive(Debug, Clone)]
enum Message {
    AddTrack,
    ToggleMute(TrackId),
    ToggleSolo(TrackId),
    AdjustVolume(TrackId, f32),
    NudgeAudioItem(ItemId, i8),
    Undo,
    Redo,
    ActionQueryChanged(String),
    RunActionQuery,
    ProjectPathChanged(String),
    OpenProject,
    SaveProject,
    AudioFilePathChanged(String),
    ImportAudio,
    CancelAudioImport,
    AudioImportStarted(SharedAudioImportWorker),
    AudioImportFinished(Result<DawAction, String>),
    BackgroundTick,
    ProjectLoaded(PathBuf, Arc<Mutex<Option<Result<Project, String>>>>),
    ProjectSaved(PathBuf, u64, Result<(), String>),
    #[cfg(feature = "jack-backend")]
    StartPlayback,
    #[cfg(feature = "jack-backend")]
    StopPlayback,
    #[cfg(feature = "jack-backend")]
    RestartPlayback,
    #[cfg(feature = "jack-backend")]
    SeekSampleChanged(String),
    #[cfg(feature = "jack-backend")]
    SeekToItem(u64),
    #[cfg(feature = "jack-backend")]
    SeekToSample,
    #[cfg(feature = "jack-backend")]
    ClosePlayback,
    #[cfg(feature = "jack-backend")]
    PlaybackPrepared {
        target_sample: u64,
        start_when_ready: bool,
        result: SharedPreparedPlayback,
    },
}

#[cfg(feature = "jack-backend")]
#[derive(Clone)]
struct SharedPreparedPlayback(Arc<Mutex<Option<Result<PreparedAudioPlayback, String>>>>);

#[cfg(feature = "jack-backend")]
impl std::fmt::Debug for SharedPreparedPlayback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedPreparedPlayback(..)")
    }
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

    fn subscription(&self) -> iced::Subscription<Message> {
        #[cfg(feature = "jack-backend")]
        let playback_active = self.playback.is_some();
        #[cfg(not(feature = "jack-backend"))]
        let playback_active = false;

        if self.import_busy || playback_active {
            iced::time::every(Duration::from_millis(100)).map(|_| Message::BackgroundTick)
        } else {
            iced::Subscription::none()
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let allowed_during_io = matches!(
            &message,
            Message::ProjectLoaded(..) | Message::ProjectSaved(..) | Message::BackgroundTick
        );
        if self.import_busy
            && !matches!(
                &message,
                Message::AudioFilePathChanged(_)
                    | Message::CancelAudioImport
                    | Message::AudioImportStarted(_)
                    | Message::AudioImportFinished(_)
                    | Message::BackgroundTick
            )
        {
            self.status = "Wait for audio import to finish or cancel it".to_owned();
            return Task::none();
        }
        #[cfg(feature = "jack-backend")]
        {
            if self.playback_busy
                && !matches!(
                    &message,
                    Message::PlaybackPrepared { .. } | Message::BackgroundTick
                )
            {
                self.status = "Wait for playback preparation to finish".to_owned();
                return Task::none();
            }
            if self.playback.is_some()
                && matches!(
                    &message,
                    Message::AddTrack
                        | Message::ToggleMute(_)
                        | Message::ToggleSolo(_)
                        | Message::AdjustVolume(..)
                        | Message::NudgeAudioItem(..)
                        | Message::Undo
                        | Message::Redo
                        | Message::RunActionQuery
                        | Message::ImportAudio
                )
            {
                self.status = "Close JACK output before editing the project".to_owned();
                return Task::none();
            }
        }
        if self.io_busy && !allowed_during_io {
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
            Message::NudgeAudioItem(item_id, direction) => {
                self.nudge_audio_item(item_id, direction)
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
            Message::AudioFilePathChanged(path) => self.audio_file_path_query = path,
            Message::ImportAudio => task = self.start_audio_import(),
            Message::CancelAudioImport => self.cancel_audio_import(),
            Message::AudioImportStarted(worker) => self.audio_import_started(worker),
            Message::AudioImportFinished(result) => self.finish_audio_import(result),
            Message::BackgroundTick => {
                #[cfg(feature = "jack-backend")]
                self.update_playback_stats();
                task = self.update_audio_import();
            }
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
            #[cfg(feature = "jack-backend")]
            Message::StartPlayback => task = self.start_playback(),
            #[cfg(feature = "jack-backend")]
            Message::StopPlayback => self.stop_playback(),
            #[cfg(feature = "jack-backend")]
            Message::RestartPlayback => task = self.restart_playback(),
            #[cfg(feature = "jack-backend")]
            Message::SeekSampleChanged(sample) => self.seek_sample_query = sample,
            #[cfg(feature = "jack-backend")]
            Message::SeekToItem(sample) => {
                self.seek_sample_query = sample.to_string();
                task = self.prepare_playback(sample, self.playback_playing);
            }
            #[cfg(feature = "jack-backend")]
            Message::SeekToSample => task = self.seek_to_sample(),
            #[cfg(feature = "jack-backend")]
            Message::ClosePlayback => self.close_playback(),
            #[cfg(feature = "jack-backend")]
            Message::PlaybackPrepared {
                target_sample,
                start_when_ready,
                result,
            } => self.finish_playback_preparation(target_sample, start_when_ready, result),
        }
        task
    }

    #[cfg(feature = "jack-backend")]
    fn playback_controls(&self) -> Element<'_, Message> {
        let playback_state = if self.playback_busy {
            "Preparing JACK…".to_owned()
        } else if self.playback.is_none() {
            "JACK closed".to_owned()
        } else {
            let seconds =
                self.playhead_sample as f64 / self.project.settings().sample_rate() as f64;
            format!(
                "{} · {seconds:.2}s",
                if self.playback_playing {
                    "Playing"
                } else {
                    "Stopped"
                }
            )
        };
        row![
            button("Play").on_press(Message::StartPlayback),
            button("Stop").on_press(Message::StopPlayback),
            button("Restart").on_press(Message::RestartPlayback),
            text_input("Sample", &self.seek_sample_query)
                .on_input(Message::SeekSampleChanged)
                .width(100),
            button("Seek").on_press(Message::SeekToSample),
            button("Close JACK").on_press(Message::ClosePlayback),
            text(playback_state),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    #[cfg(not(feature = "jack-backend"))]
    fn playback_controls(&self) -> Element<'_, Message> {
        text("Playback: build with jack-backend").into()
    }

    fn view(&self) -> Element<'_, Message> {
        let toolbar = row![
            text("AAADAW").size(24),
            button("Add Track").on_press(Message::AddTrack),
            button("Undo").on_press(Message::Undo),
            button("Redo").on_press(Message::Redo),
            self.playback_controls(),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let current_project_path = self.project_path.as_ref().map_or_else(
            || "New project".to_owned(),
            |path| path.display().to_string(),
        );
        let project_controls = row![
            text(format!("Current: {current_project_path}")),
            text_input("Path to open / first save", &self.project_path_query)
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

        let import_progress = if !self.import_busy {
            "Import audio to the first track after existing items; the source is embedded."
                .to_owned()
        } else if self.import_finalizing {
            "Audio embedded; placing the timeline item…".to_owned()
        } else if let Some(total_bytes) = self.import_total_bytes {
            format!("Importing: {} / {total_bytes} bytes", self.import_bytes)
        } else {
            "Starting audio import…".to_owned()
        };
        let import_controls = row![
            text_input("Audio file to import", &self.audio_file_path_query)
                .on_input(Message::AudioFilePathChanged)
                .width(Length::Fill),
            button(if self.import_busy {
                "Importing…"
            } else {
                "Import audio"
            })
            .on_press(Message::ImportAudio),
            button("Cancel import").on_press(Message::CancelAudioImport),
            text(import_progress),
        ]
        .spacing(8)
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
        let editor = timeline::view(&self.project);

        let workspace = row![tracks, editor].spacing(12).height(Length::Fill);

        container(
            column![
                toolbar,
                project_controls,
                import_controls,
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

    fn start_audio_import(&mut self) -> Task<Message> {
        if self.import_busy || self.io_busy {
            self.status = "Wait for the current project operation to finish".to_owned();
            return Task::none();
        }
        #[cfg(feature = "jack-backend")]
        if self.playback.is_some() {
            self.status = "Close JACK output before importing audio".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.project_path.clone() else {
            self.status = "Save the project before importing audio".to_owned();
            return Task::none();
        };
        let Some(track_id) = self.project.tracks().first().map(Track::id) else {
            self.status = "Add a track before importing audio".to_owned();
            return Task::none();
        };
        let Some(source_path) = project_path_from_query(&self.audio_file_path_query) else {
            self.status = "Enter an audio file path first".to_owned();
            return Task::none();
        };
        let start_sample = self
            .project
            .audio_items()
            .iter()
            .filter(|item| item.track_id() == track_id)
            .filter_map(|item| item.start_sample().checked_add(item.length_samples()))
            .max()
            .unwrap_or(0);
        let project_sample_rate = self.project.settings().sample_rate();

        self.import_busy = true;
        self.import_finalizing = false;
        self.import_cancel_requested = false;
        self.import_bytes = 0;
        self.import_total_bytes = None;
        self.status = format!("Starting import of {}…", source_path.display());
        Task::perform(
            run_blocking("aaadaw-audio-import-start", move || {
                start_audio_item_import(
                    project_path,
                    source_path,
                    track_id,
                    start_sample,
                    project_sample_rate,
                )
                .map_err(|error| error.to_string())
            }),
            move |result| {
                Message::AudioImportStarted(SharedAudioImportWorker(Arc::new(Mutex::new(Some(
                    result,
                )))))
            },
        )
    }

    fn audio_import_started(&mut self, worker: SharedAudioImportWorker) {
        let worker = worker.0.lock().ok().and_then(|mut result| result.take());
        match worker {
            Some(Ok(worker)) => {
                if self.import_cancel_requested {
                    worker.cancel();
                    self.status = "Cancelling audio import…".to_owned();
                } else {
                    self.status = "Importing audio into the project…".to_owned();
                }
                self.import_worker = Some(PendingAudioImport { worker });
            }
            Some(Err(error)) => {
                self.import_busy = false;
                self.import_finalizing = false;
                self.import_cancel_requested = false;
                self.status = format!("Audio import could not start: {error}");
            }
            None => {
                self.import_busy = false;
                self.import_finalizing = false;
                self.import_cancel_requested = false;
                self.status = "Audio import worker result was unavailable".to_owned();
            }
        }
    }

    fn cancel_audio_import(&mut self) {
        if !self.import_busy {
            self.status = "No audio import is active".to_owned();
            return;
        }
        if self.import_finalizing {
            self.status = "Audio bytes are committed; finishing item placement".to_owned();
            return;
        }
        self.import_cancel_requested = true;
        if let Some(import) = self.import_worker.as_ref() {
            import.worker.cancel();
        }
        self.status = "Cancelling audio import…".to_owned();
    }

    fn update_audio_import(&mut self) -> Task<Message> {
        let Some(import) = self.import_worker.as_mut() else {
            return Task::none();
        };
        let progress = import.worker.progress().try_iter().collect::<Vec<_>>();
        let finished = import.worker.is_finished();
        for AudioItemImportProgress {
            bytes_imported,
            total_bytes,
        } in progress
        {
            self.import_bytes = bytes_imported;
            self.import_total_bytes = Some(total_bytes);
        }
        if !finished {
            return Task::none();
        }

        let import = self
            .import_worker
            .take()
            .expect("finished import is present");
        self.import_finalizing = true;
        self.status = "Finalizing audio metadata and placement…".to_owned();
        Task::perform(
            run_blocking("aaadaw-audio-import-finish", move || {
                import.worker.finish().map_err(|error| error.to_string())
            }),
            Message::AudioImportFinished,
        )
    }

    fn finish_audio_import(&mut self, result: Result<DawAction, String>) {
        self.import_busy = false;
        self.import_finalizing = false;
        self.import_cancel_requested = false;
        match result {
            Ok(action) => {
                self.apply_action(action, "Audio imported and appended to the first track")
            }
            Err(error) => self.status = format!("Audio import could not be finalized: {error}"),
        }
    }

    fn open_project(&mut self) -> Task<Message> {
        #[cfg(feature = "jack-backend")]
        if self.playback.is_some() {
            self.status = "Close JACK output before opening another project".to_owned();
            return Task::none();
        }
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

    #[cfg(feature = "jack-backend")]
    fn start_playback(&mut self) -> Task<Message> {
        if let Some(playback) = self.playback.as_mut() {
            return match playback.play() {
                Ok(()) => {
                    self.playback_playing = true;
                    self.status = "Playback started".to_owned();
                    Task::none()
                }
                Err(error) => {
                    self.status = format!("JACK play failed: {error}");
                    Task::none()
                }
            };
        }
        self.prepare_playback(self.playhead_sample, true)
    }

    #[cfg(feature = "jack-backend")]
    fn stop_playback(&mut self) {
        let Some(playback) = self.playback.as_mut() else {
            self.status = "JACK output is not open".to_owned();
            return;
        };
        match playback.stop() {
            Ok(()) => {
                self.playback_playing = false;
                self.status = "Playback stopped".to_owned();
            }
            Err(error) => self.status = format!("JACK stop failed: {error}"),
        }
    }

    #[cfg(feature = "jack-backend")]
    fn restart_playback(&mut self) -> Task<Message> {
        let start_when_ready = self.playback.is_none() || self.playback_playing;
        self.prepare_playback(0, start_when_ready)
    }

    #[cfg(feature = "jack-backend")]
    fn seek_to_sample(&mut self) -> Task<Message> {
        match self.seek_sample_query.trim().parse::<u64>() {
            Ok(target_sample) => self.prepare_playback(target_sample, self.playback_playing),
            Err(error) => {
                self.status = format!("Invalid seek sample: {error}");
                Task::none()
            }
        }
    }

    #[cfg(feature = "jack-backend")]
    fn close_playback(&mut self) {
        self.playback.take();
        self.playback_playing = false;
        self.playhead_sample = 0;
        self.seek_sample_query = "0".to_owned();
        self.status = "JACK output closed".to_owned();
    }

    #[cfg(feature = "jack-backend")]
    fn prepare_playback(&mut self, target_sample: u64, start_when_ready: bool) -> Task<Message> {
        if self.playback_busy || self.io_busy {
            self.status = "Wait for current operation to finish".to_owned();
            return Task::none();
        }
        let Some(path) = self.project_path.clone() else {
            self.status = "Save or open project before playback".to_owned();
            return Task::none();
        };
        let snapshot = self.project.snapshot();
        self.playback_busy = true;
        self.status = format!("Preparing playback at sample {target_sample}…");
        let result = Arc::new(Mutex::new(None));
        let message_result = Arc::clone(&result);
        Task::perform(
            run_blocking("aaadaw-playback-prepare", move || {
                prepare_project_playback_file(path, snapshot, target_sample)
            }),
            move |prepared| {
                *message_result
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(prepared);
                Message::PlaybackPrepared {
                    target_sample,
                    start_when_ready,
                    result: SharedPreparedPlayback(result),
                }
            },
        )
    }

    #[cfg(feature = "jack-backend")]
    fn finish_playback_preparation(
        &mut self,
        target_sample: u64,
        start_when_ready: bool,
        result: SharedPreparedPlayback,
    ) {
        self.playback_busy = false;
        let result = result.0.lock().ok().and_then(|mut result| result.take());
        let prepared = match result {
            Some(Ok(prepared)) => prepared,
            Some(Err(error)) => {
                self.status = format!("Playback preparation failed: {error}");
                return;
            }
            None => {
                self.status = "Playback preparation result was unavailable".to_owned();
                return;
            }
        };

        if let Some(playback) = self.playback.as_mut() {
            match playback.replace_graph(prepared) {
                Ok(()) => {
                    self.playhead_sample = target_sample;
                    self.seek_sample_query = target_sample.to_string();
                    self.status = format!("Queued seek to sample {target_sample}");
                }
                Err(error) => self.status = format!("JACK graph replacement failed: {error}"),
            }
            return;
        }

        let mut playback = match prepared.into_jack_output() {
            Ok(playback) => playback,
            Err(error) => {
                self.status = format!("JACK output setup failed: {error}");
                return;
            }
        };
        if start_when_ready {
            if let Err(error) = playback.play() {
                self.status = format!("JACK play failed: {error}");
                return;
            }
        }
        self.playback = Some(playback);
        self.playback_playing = start_when_ready;
        self.playhead_sample = target_sample;
        self.seek_sample_query = target_sample.to_string();
        self.status = if start_when_ready {
            "Playback started".to_owned()
        } else {
            "JACK output ready".to_owned()
        };
    }

    #[cfg(feature = "jack-backend")]
    fn update_playback_stats(&mut self) {
        if let Some(playback) = self.playback.as_mut() {
            self.playhead_sample = playback.stats().playhead_sample;
            playback.collect_retired_graphs();
        }
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

    fn nudge_audio_item(&mut self, item_id: ItemId, direction: i8) {
        let Some((media_ref, source_offset_samples, length_samples, start_sample)) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            .map(|item| {
                (
                    item.media_ref().to_owned(),
                    item.source_offset_samples(),
                    item.length_samples(),
                    item.start_sample(),
                )
            })
        else {
            self.status = "Audio item no longer exists".to_owned();
            return;
        };
        let delta = u64::from(self.project.settings().sample_rate());
        let moved_sample = match direction {
            -1 => start_sample.checked_sub(delta),
            1 => start_sample.checked_add(delta),
            _ => None,
        };
        let Some(start_sample) = moved_sample else {
            self.status = "Audio item cannot move beyond the sample timeline".to_owned();
            return;
        };
        self.apply_action(
            DawAction::EditAudioItem {
                item_id,
                media_ref,
                start_sample,
                source_offset_samples,
                length_samples,
            },
            "Audio item moved by one second",
        );
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

#[cfg(feature = "jack-backend")]
fn prepare_project_playback_file(
    path: PathBuf,
    snapshot: ProjectSnapshot,
    target_sample: u64,
) -> Result<PreparedAudioPlayback, String> {
    if !path.is_file() {
        return Err(format!("project file {} does not exist", path.display()));
    }
    let project = Project::from_snapshot(snapshot).map_err(|error| error.to_string())?;
    let store = ProjectStore::open(path).map_err(|error| error.to_string())?;
    let prepared = prepare_audio_playback_at(&project, &store, target_sample, 16_384, 8_192)
        .map_err(|error: PlaybackBuildError| error.to_string());
    let close = store.close().map_err(|error| error.to_string());
    let prepared = prepared?;
    close?;
    Ok(prepared)
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
    #[cfg(feature = "jack-backend")]
    use super::prepare_project_playback_file;
    use super::{App, Message, load_project_file, save_project_file};
    use aaadaw_core::{DawAction, Project};
    #[cfg(feature = "jack-backend")]
    use aaadaw_storage::ProjectStore;
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
        #[cfg(feature = "jack-backend")]
        assert!(prepare_project_playback_file(path.clone(), Project::new().snapshot(), 0).is_err());
        assert!(!path.exists());
    }

    #[cfg(feature = "jack-backend")]
    #[test]
    fn jack_feature_prepares_offline_graph_without_opening_device() {
        let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aaadaw-ui-playback-{}-{file_id}.aaadaw",
            std::process::id()
        ));
        let store = ProjectStore::open(&path).expect("empty project store should open");
        store.close().expect("empty project store should close");

        let prepared = prepare_project_playback_file(path.clone(), Project::new().snapshot(), 0)
            .expect("empty project should prepare without a JACK device");
        assert_eq!(prepared.feeder_count(), 0);
        drop(prepared);

        let _ = std::fs::remove_file(&path);
        for suffix in ["-wal", "-shm"] {
            let sidecar = format!("{}{suffix}", path.display());
            let _ = std::fs::remove_file(sidecar);
        }
    }

    #[test]
    fn audio_timeline_nudge_is_undoable_and_cannot_cross_sample_zero() {
        let mut app = App::default();
        let _ = app.update(Message::AddTrack);
        let track_id = app.project.tracks()[0].id();
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: "asset://nudge-test".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 256,
            })
            .expect("test item should be inserted");
        let item_id = app.project.audio_items()[0].id();

        let _ = app.update(Message::NudgeAudioItem(item_id, 1));
        assert_eq!(app.project.audio_items()[0].start_sample(), 48_000);
        let _ = app.update(Message::Undo);
        assert_eq!(app.project.audio_items()[0].start_sample(), 0);
        let _ = app.update(Message::NudgeAudioItem(item_id, -1));
        assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    }

    #[test]
    fn completed_audio_import_places_item_through_project_action() {
        let mut app = App::default();
        let _ = app.update(Message::AddTrack);
        let track_id = app.project.tracks()[0].id();
        app.import_busy = true;

        let _ = app.update(Message::AudioImportFinished(Ok(
            DawAction::InsertAudioItem {
                track_id,
                media_ref: "asset://test-audio".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 128,
            },
        )));

        assert_eq!(app.project.audio_items().len(), 1);
        assert_eq!(app.project.audio_items()[0].start_sample(), 0);
        assert_eq!(app.project.audio_items()[0].length_samples(), 128);
        assert_eq!(app.revision, 2);
        assert!(!app.import_busy);
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
