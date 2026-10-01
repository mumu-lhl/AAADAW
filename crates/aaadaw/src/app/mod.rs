use aaadaw_app::{
    AudioAssetManagementOperation, AudioAssetManagementProgress, AudioAssetManagementResult,
    AudioAssetManagementWorker, AudioAssetSourceStatus, AudioAssetSourceStatusEntry,
    AudioItemImportProgress, AudioItemImportWorker, add_quarter_note, adjust_midi_note_pitch,
    adjust_midi_note_velocity, create_four_beat_midi_item, delete_midi_note, duplicate_audio_item,
    move_midi_item_by_beat, move_midi_note_by_sixteenth, quantize_midi_item_to_sixteenth,
    relink_external_audio_source, set_audio_item_start_sample, start_audio_asset_management,
    start_audio_item_import,
};
#[cfg(feature = "jack-backend")]
use aaadaw_app::{
    PlaybackBuildError, PreparedAudioPlayback, RunningJackPlayback, prepare_audio_playback_at,
};
use aaadaw_core::{DawAction, ItemId, Project, ProjectSnapshot, Track, TrackId};
use aaadaw_storage::ProjectStore;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

mod messages;
#[cfg(test)]
mod tests;

pub(crate) use messages::{MainMenu, Message, PathPickerTarget, WorkspacePage};

pub(crate) fn run() -> iced::Result {
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
    track_name_edits: HashMap<TrackId, String>,
    audio_item_start_edits: HashMap<ItemId, String>,
    active_workspace: WorkspacePage,
    active_menu: Option<MainMenu>,
    path_picker_busy: bool,
    audio_asset_source_statuses: HashMap<String, AudioAssetSourceStatusEntry>,
    audio_asset_management_worker: Option<AudioAssetManagementWorker>,
    audio_asset_management_busy: bool,
    audio_asset_management_finalizing: bool,
    audio_asset_management_cancel_requested: bool,
    audio_asset_management_operation: Option<AudioAssetManagementOperation>,
    audio_asset_management_status: String,
    relink_source_path_query: String,
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
pub(crate) struct SharedAudioImportWorker(
    Arc<Mutex<Option<Result<AudioItemImportWorker, String>>>>,
);

impl std::fmt::Debug for SharedAudioImportWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedAudioImportWorker(..)")
    }
}

#[derive(Clone)]
pub(crate) struct SharedAudioAssetManagementWorker(
    Arc<Mutex<Option<Result<AudioAssetManagementWorker, String>>>>,
);

impl std::fmt::Debug for SharedAudioAssetManagementWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedAudioAssetManagementWorker(..)")
    }
}

#[cfg(feature = "jack-backend")]
#[derive(Clone)]
pub(crate) struct SharedPreparedPlayback(Arc<Mutex<Option<Result<PreparedAudioPlayback, String>>>>);

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

        let background_ticks =
            if self.import_busy || playback_active || self.audio_asset_management_busy {
                iced::time::every(Duration::from_millis(100)).map(|_| Message::BackgroundTick)
            } else {
                iced::Subscription::none()
            };
        iced::Subscription::batch([
            iced::event::listen_with(keyboard_shortcut_event),
            background_ticks,
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let allowed_during_io = matches!(
            &message,
            Message::ProjectLoaded(..)
                | Message::ProjectSaved(..)
                | Message::ToggleMainMenu(_)
                | Message::SelectWorkspace(_)
                | Message::PathPicked(..)
                | Message::AudioItemRelinked(..)
                | Message::BackgroundTick
        );
        if self.path_picker_busy
            && !matches!(
                &message,
                Message::PathPicked(..)
                    | Message::ToggleMainMenu(_)
                    | Message::SelectWorkspace(_)
                    | Message::BackgroundTick
            )
        {
            self.status = "Wait for the file dialog to finish".to_owned();
            return Task::none();
        }
        if self.import_busy
            && !matches!(
                &message,
                Message::AudioFilePathChanged(_)
                    | Message::ToggleMainMenu(_)
                    | Message::SelectWorkspace(_)
                    | Message::CancelAudioImport
                    | Message::AudioImportStarted(_)
                    | Message::AudioImportFinished(_)
                    | Message::BackgroundTick
            )
        {
            self.status = "Wait for audio import to finish or cancel it".to_owned();
            return Task::none();
        }
        if self.audio_asset_management_busy
            && !matches!(
                &message,
                Message::ToggleMainMenu(_)
                    | Message::SelectWorkspace(_)
                    | Message::AudioAssetManagementStarted(_)
                    | Message::AudioAssetManagementFinished(_)
                    | Message::CancelAudioAssetManagement
                    | Message::BackgroundTick
            )
        {
            self.status = "Wait for audio asset maintenance to finish or cancel it".to_owned();
            return Task::none();
        }
        #[cfg(feature = "jack-backend")]
        {
            if self.playback_busy
                && !matches!(
                    &message,
                    Message::PlaybackPrepared { .. }
                        | Message::ToggleMainMenu(_)
                        | Message::SelectWorkspace(_)
                        | Message::BackgroundTick
                )
            {
                self.status = "Wait for playback preparation to finish".to_owned();
                return Task::none();
            }
            if self.playback.is_some()
                && matches!(
                    &message,
                    Message::AddTrack
                        | Message::AddMidiItem
                        | Message::AddMidiNote(_)
                        | Message::DeleteMidiItem(_)
                        | Message::NudgeMidiItem(..)
                        | Message::NudgeMidiNote(..)
                        | Message::AdjustMidiNotePitch(..)
                        | Message::AdjustMidiNoteVelocity(..)
                        | Message::DeleteMidiNote(..)
                        | Message::QuantizeMidiItem(_)
                        | Message::DeleteTrack(_)
                        | Message::MoveTrack(..)
                        | Message::TrackNameChanged(..)
                        | Message::CommitTrackName(_)
                        | Message::ToggleMute(_)
                        | Message::ToggleSolo(_)
                        | Message::AdjustVolume(..)
                        | Message::AdjustPan(..)
                        | Message::NudgeAudioItem(..)
                        | Message::BeginAudioItemStartSampleEdit(_)
                        | Message::AudioItemStartSampleChanged(..)
                        | Message::CommitAudioItemStartSample(_)
                        | Message::CancelAudioItemStartSampleEdit(_)
                        | Message::DeleteAudioItem(_)
                        | Message::DuplicateAudioItem(_)
                        | Message::Undo
                        | Message::Redo
                        | Message::RunActionQuery
                        | Message::ImportAudio
                        | Message::RunAudioAssetManagement(_)
                        | Message::CancelAudioAssetManagement
                        | Message::RelinkAudioItem(_)
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
            Message::ToggleMainMenu(menu) => {
                self.active_menu = (self.active_menu != Some(menu)).then_some(menu);
            }
            Message::SelectWorkspace(page) => self.active_workspace = page,
            #[cfg(feature = "jack-backend")]
            Message::TogglePlayback => {
                if self.playback_playing {
                    self.stop_playback();
                } else {
                    task = self.start_playback();
                }
            }
            Message::AddTrack => {
                self.active_menu = None;
                self.add_track();
            }
            Message::AddMidiItem => {
                let action = create_four_beat_midi_item(&self.project);
                self.apply_edit(action, "Four-beat MIDI item created");
            }
            Message::AddMidiNote(item_id) => {
                let action = add_quarter_note(&self.project, item_id);
                self.apply_edit(action, "C4 MIDI note added");
            }
            Message::DeleteMidiItem(item_id) => {
                self.apply_action(DawAction::DeleteMidiItem { item_id }, "MIDI item deleted");
            }
            Message::NudgeMidiItem(item_id, direction) => {
                let action = move_midi_item_by_beat(&self.project, item_id, direction);
                self.apply_edit(action, "MIDI item moved by one beat");
            }
            Message::NudgeMidiNote(item_id, note_id, direction) => {
                let action =
                    move_midi_note_by_sixteenth(&self.project, item_id, note_id, direction);
                self.apply_edit(action, "MIDI note changed");
            }
            Message::AdjustMidiNotePitch(item_id, note_id, delta) => {
                let action = adjust_midi_note_pitch(&self.project, item_id, note_id, delta);
                self.apply_edit(action, "MIDI note changed");
            }
            Message::AdjustMidiNoteVelocity(item_id, note_id, delta) => {
                let action = adjust_midi_note_velocity(&self.project, item_id, note_id, delta);
                self.apply_edit(action, "MIDI note changed");
            }
            Message::DeleteMidiNote(item_id, note_id) => {
                self.apply_action(delete_midi_note(item_id, note_id), "MIDI note deleted");
            }
            Message::QuantizeMidiItem(item_id) => {
                let action = quantize_midi_item_to_sixteenth(item_id);
                self.apply_edit(action, "MIDI item quantized to 1/16");
            }
            Message::DeleteTrack(track_id) => self.delete_track(track_id),
            Message::MoveTrack(track_id, direction) => self.move_track(track_id, direction),
            Message::TrackNameChanged(track_id, name) => {
                self.track_name_edits.insert(track_id, name);
            }
            Message::CommitTrackName(track_id) => self.commit_track_name(track_id),
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
            Message::AdjustPan(track_id, delta) => {
                if let Some(track) = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                {
                    let pan = (track.pan() + delta).clamp(-1.0, 1.0);
                    self.apply_action(
                        DawAction::SetTrackPan { track_id, pan },
                        "Track pan changed",
                    );
                }
            }
            Message::NudgeAudioItem(item_id, direction, milliseconds) => {
                self.nudge_audio_item(item_id, direction, milliseconds)
            }
            Message::BeginAudioItemStartSampleEdit(item_id) => {
                if let Some(item) = self
                    .project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                {
                    self.audio_item_start_edits
                        .entry(item_id)
                        .or_insert_with(|| item.start_sample().to_string());
                }
            }
            Message::AudioItemStartSampleChanged(item_id, query) => {
                self.audio_item_start_edits.insert(item_id, query);
            }
            Message::CommitAudioItemStartSample(item_id) => {
                self.commit_audio_item_start_sample(item_id);
            }
            Message::CancelAudioItemStartSampleEdit(item_id) => {
                self.audio_item_start_edits.remove(&item_id);
            }
            Message::DeleteAudioItem(item_id) => {
                self.audio_item_start_edits.remove(&item_id);
                self.apply_action(DawAction::DeleteAudioItem { item_id }, "Audio item deleted");
            }
            Message::DuplicateAudioItem(item_id) => {
                let action = duplicate_audio_item(&self.project, item_id);
                self.apply_edit(action, "Audio item duplicated");
            }
            Message::Undo => {
                self.active_menu = None;
                self.undo();
            }
            Message::Redo => {
                self.active_menu = None;
                self.redo();
            }
            Message::ActionQueryChanged(query) => self.action_query = query,
            Message::RunActionQuery => self.run_action_query(),
            Message::ProjectPathChanged(path) => {
                if self.io_busy {
                    self.status = "Wait for current project operation to finish".to_owned();
                } else {
                    self.project_path_query = path;
                }
            }
            Message::PickPath(target) => task = self.pick_path(target),
            Message::PathPicked(target, result) => task = self.path_picked(target, result),
            Message::OpenProject => {
                self.active_menu = None;
                task = self.open_project();
            }
            Message::SaveProject => {
                self.active_menu = None;
                task = self.save_project();
            }
            Message::AudioFilePathChanged(path) => self.audio_file_path_query = path,
            Message::ImportAudio => task = self.start_audio_import(),
            Message::CancelAudioImport => self.cancel_audio_import(),
            Message::AudioImportStarted(worker) => self.audio_import_started(worker),
            Message::AudioImportFinished(result) => self.finish_audio_import(result),
            Message::RunAudioAssetManagement(operation) => {
                task = self.start_audio_asset_management(operation);
            }
            Message::CancelAudioAssetManagement => self.cancel_audio_asset_management(),
            Message::AudioAssetManagementStarted(worker) => {
                self.audio_asset_management_started(worker);
            }
            Message::AudioAssetManagementFinished(result) => {
                self.finish_audio_asset_management(result);
            }
            Message::RelinkSourcePathChanged(path) => self.relink_source_path_query = path,
            Message::RelinkAudioItem(item_id) => task = self.relink_audio_item(item_id),
            Message::AudioItemRelinked(item_id, result) => {
                self.finish_audio_item_relink(item_id, result);
            }
            Message::BackgroundTick => {
                #[cfg(feature = "jack-backend")]
                self.update_playback_stats();
                task = Task::batch([
                    self.update_audio_import(),
                    self.update_audio_asset_management(),
                ]);
            }
            Message::ProjectLoaded(path, result) => {
                self.io_busy = false;
                let result = result.lock().ok().and_then(|mut result| result.take());
                match result {
                    Some(Ok(project)) => {
                        self.project = project;
                        self.track_name_edits.clear();
                        self.audio_item_start_edits.clear();
                        self.audio_asset_source_statuses.clear();
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
        let project_name = self
            .project_path
            .as_ref()
            .and_then(|path| path.file_name())
            .map_or_else(
                || "New project".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            );
        let file_label = if self.active_menu == Some(MainMenu::File) {
            "File ▴"
        } else {
            "File ▾"
        };
        let edit_label = if self.active_menu == Some(MainMenu::Edit) {
            "Edit ▴"
        } else {
            "Edit ▾"
        };
        let track_label = if self.active_menu == Some(MainMenu::Track) {
            "Track ▴"
        } else {
            "Track ▾"
        };
        let toolbar = row![
            text("AAADAW").size(24),
            button(file_label).on_press(Message::ToggleMainMenu(MainMenu::File)),
            button(edit_label).on_press(Message::ToggleMainMenu(MainMenu::Edit)),
            button(track_label).on_press(Message::ToggleMainMenu(MainMenu::Track)),
            text(project_name.clone()).width(Length::Fill),
            self.playback_controls(),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let menu_panel: Option<Element<'_, Message>> = match self.active_menu {
            Some(MainMenu::File) => Some(
                column![
                    text("Project files").size(16),
                    row![
                        text_input("Project file path", &self.project_path_query)
                            .on_input(Message::ProjectPathChanged)
                            .width(Length::Fill),
                        button("Open path").on_press(Message::OpenProject),
                        button("Save path").on_press(Message::SaveProject),
                    ]
                    .spacing(8),
                    row![
                        button("Open…").on_press(Message::PickPath(PathPickerTarget::OpenProject)),
                        button("Save as…")
                            .on_press(Message::PickPath(PathPickerTarget::SaveProject)),
                        text(if self.is_dirty() {
                            "Unsaved changes"
                        } else if self.project_path.is_some() {
                            "Saved"
                        } else {
                            "New project"
                        }),
                    ]
                    .spacing(8),
                ]
                .spacing(8)
                .into(),
            ),
            Some(MainMenu::Edit) => Some(
                row![
                    button("Undo").on_press(Message::Undo),
                    button("Redo").on_press(Message::Redo),
                ]
                .spacing(8)
                .into(),
            ),
            Some(MainMenu::Track) => Some(
                row![button("Add track").on_press(Message::AddTrack)]
                    .spacing(8)
                    .into(),
            ),
            None => None,
        };

        let workspace_tabs = row![
            button(if self.active_workspace == WorkspacePage::Arrangement {
                "● Arrangement"
            } else {
                "Arrangement"
            })
            .on_press(Message::SelectWorkspace(WorkspacePage::Arrangement)),
            button(if self.active_workspace == WorkspacePage::Media {
                "● Media"
            } else {
                "Media"
            })
            .on_press(Message::SelectWorkspace(WorkspacePage::Media)),
            button(if self.active_workspace == WorkspacePage::Project {
                "● Project"
            } else {
                "Project"
            })
            .on_press(Message::SelectWorkspace(WorkspacePage::Project)),
        ]
        .spacing(8);

        let import_progress = if !self.import_busy {
            "The source is embedded in the project after import.".to_owned()
        } else if self.import_finalizing {
            "Audio embedded; placing the timeline item…".to_owned()
        } else if let Some(total_bytes) = self.import_total_bytes {
            format!("Importing: {} / {total_bytes} bytes", self.import_bytes)
        } else {
            "Starting audio import…".to_owned()
        };
        let mut import_row = row![
            text_input("Audio file path", &self.audio_file_path_query)
                .on_input(Message::AudioFilePathChanged)
                .width(Length::Fill),
            button("Choose…").on_press(Message::PickPath(PathPickerTarget::ImportAudio)),
            button(if self.import_busy {
                "Importing…"
            } else {
                "Import audio"
            })
            .on_press(Message::ImportAudio),
        ]
        .spacing(8);
        if self.import_busy {
            import_row = import_row.push(button("Cancel").on_press(Message::CancelAudioImport));
        }
        let import_controls = column![import_row, text(import_progress)].spacing(8);

        let mut asset_row = row![
            button(if self.audio_asset_management_busy {
                "Working…"
            } else {
                "Scan sources"
            })
            .on_press(Message::RunAudioAssetManagement(
                AudioAssetManagementOperation::ScanSources,
            )),
            button("Pack external audio").on_press(Message::RunAudioAssetManagement(
                AudioAssetManagementOperation::PackExternalAssets,
            )),
        ]
        .spacing(8);
        if self.audio_asset_management_busy {
            asset_row =
                asset_row.push(button("Cancel").on_press(Message::CancelAudioAssetManagement));
        }
        let asset_status = if self.audio_asset_management_status.is_empty() {
            "Scan source state or pack external links into the project".to_owned()
        } else {
            self.audio_asset_management_status.clone()
        };
        let asset_controls = column![asset_row, text(asset_status)].spacing(8);

        let relink_controls = row![
            text_input(
                "Replacement path for missing external audio",
                &self.relink_source_path_query
            )
            .on_input(Message::RelinkSourcePathChanged)
            .width(Length::Fill),
            button("Choose replacement…")
                .on_press(Message::PickPath(PathPickerTarget::RelinkAudio)),
            text("Choose a file, then relink a missing item below"),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut source_status_list = column![text("Scanned sources").size(16)].spacing(6);
        if self.audio_asset_source_statuses.is_empty() {
            source_status_list = source_status_list.push(text("No scan results yet."));
        } else {
            let mut entries: Vec<_> = self.audio_asset_source_statuses.values().collect();
            entries.sort_by(|left, right| left.media_ref.cmp(&right.media_ref));
            for entry in entries {
                let mut status_row = row![
                    text(format!("{} · {:?}", entry.media_ref, entry.status)).width(Length::Fill),
                ];
                if entry.is_external_link && entry.status == AudioAssetSourceStatus::Missing {
                    if let Some(item) = self
                        .project
                        .audio_items()
                        .iter()
                        .find(|item| item.media_ref() == entry.media_ref)
                    {
                        status_row = status_row
                            .push(button("Relink").on_press(Message::RelinkAudioItem(item.id())));
                    }
                }
                source_status_list = source_status_list.push(status_row.spacing(8));
            }
        }

        let media_workspace = column![
            text("Media library").size(24),
            text("Import and repair audio sources, or package external files into this project."),
            text("Import audio").size(18),
            import_controls,
            text("Source management").size(18),
            asset_controls,
            text("Repair a missing external link").size(18),
            relink_controls,
            source_status_list,
        ]
        .spacing(14);

        let action_search = row![
            text_input("Search actions: add track, undo, redo", &self.action_query)
                .on_input(Message::ActionQueryChanged)
                .on_submit(Message::RunActionQuery)
                .width(Length::Fill),
            button("Run").on_press(Message::RunActionQuery),
        ]
        .spacing(8);
        #[cfg(feature = "jack-backend")]
        let shortcut_help = text(
            "Ctrl/Cmd+Z undo · Ctrl/Cmd+Shift+Z redo · Ctrl/Cmd+S save · Ctrl/Cmd+O open · Space play/stop",
        );
        #[cfg(not(feature = "jack-backend"))]
        let shortcut_help =
            text("Ctrl/Cmd+Z undo · Ctrl/Cmd+Shift+Z redo · Ctrl/Cmd+S save · Ctrl/Cmd+O open");
        let project_workspace = column![
            text("Project tools").size(24),
            text("Run supported commands or use the keyboard shortcuts."),
            action_search,
            text("Keyboard shortcuts").size(18),
            shortcut_help,
        ]
        .spacing(14);

        let workspace: Element<'_, Message> = match self.active_workspace {
            WorkspacePage::Arrangement => {
                let mut track_list = column![
                    row![
                        text("Tracks").size(18).width(Length::Fill),
                        button("Add track").on_press(Message::AddTrack),
                    ]
                    .spacing(8),
                ]
                .spacing(10);
                for track in self.project.tracks() {
                    let track_id = track.id();
                    let edited_name = self
                        .track_name_edits
                        .get(&track_id)
                        .map_or(track.name(), String::as_str);
                    track_list = track_list.push(track_row(
                        track,
                        edited_name,
                        self.track_name_edits.contains_key(&track_id),
                    ));
                }
                if self.project.tracks().is_empty() {
                    track_list = track_list.push(text("No tracks yet. Add one to start."));
                }
                let tracks = container(scrollable(track_list))
                    .width(300)
                    .height(Length::Fill)
                    .padding(14);
                let editor = crate::timeline::view(
                    &self.project,
                    &self.audio_asset_source_statuses,
                    &self.audio_item_start_edits,
                );
                row![tracks, editor].spacing(12).height(Length::Fill).into()
            }
            WorkspacePage::Media => container(scrollable(media_workspace))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(14)
                .into(),
            WorkspacePage::Project => container(scrollable(project_workspace))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(14)
                .into(),
        };

        let project_state = if self.is_dirty() {
            format!("{project_name} · Unsaved changes")
        } else {
            project_name
        };
        let mut content = column![toolbar]
            .spacing(12)
            .padding(14)
            .height(Length::Fill);
        if let Some(menu_panel) = menu_panel {
            content = content.push(
                container(menu_panel)
                    .padding(10)
                    .style(iced::widget::container::rounded_box),
            );
        }
        let status_text = if self.status.is_empty() {
            project_state
        } else {
            self.status.clone()
        };
        content = content
            .push(workspace_tabs)
            .push(workspace)
            .push(text(status_text));
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn pick_path(&mut self, target: PathPickerTarget) -> Task<Message> {
        if self.path_picker_busy {
            self.status = "A file dialog is already open".to_owned();
            return Task::none();
        }
        self.path_picker_busy = true;
        self.active_menu = None;
        Task::perform(
            run_blocking("aaadaw-native-file-dialog", move || {
                let path = match target {
                    PathPickerTarget::OpenProject => rfd::FileDialog::new()
                        .set_title("Open AAADAW project")
                        .add_filter("AAADAW project", &["aaadaw"])
                        .pick_file(),
                    PathPickerTarget::SaveProject => rfd::FileDialog::new()
                        .set_title("Save AAADAW project")
                        .set_file_name("project.aaadaw")
                        .add_filter("AAADAW project", &["aaadaw"])
                        .save_file(),
                    PathPickerTarget::ImportAudio => rfd::FileDialog::new()
                        .set_title("Choose audio to import")
                        .add_filter(
                            "Audio files",
                            &["wav", "flac", "mp3", "ogg", "aif", "aiff", "m4a"],
                        )
                        .pick_file(),
                    PathPickerTarget::RelinkAudio => rfd::FileDialog::new()
                        .set_title("Choose replacement audio")
                        .add_filter(
                            "Audio files",
                            &["wav", "flac", "mp3", "ogg", "aif", "aiff", "m4a"],
                        )
                        .pick_file(),
                };
                Ok(path)
            }),
            move |result| Message::PathPicked(target, result),
        )
    }

    fn path_picked(
        &mut self,
        target: PathPickerTarget,
        result: Result<Option<PathBuf>, String>,
    ) -> Task<Message> {
        self.path_picker_busy = false;
        match result {
            Ok(Some(path)) => {
                let path = path.to_string_lossy().into_owned();
                match target {
                    PathPickerTarget::OpenProject => {
                        self.project_path_query = path;
                        self.active_menu = None;
                        self.open_project()
                    }
                    PathPickerTarget::SaveProject => {
                        self.project_path_query = path;
                        self.active_menu = None;
                        self.save_project()
                    }
                    PathPickerTarget::ImportAudio => {
                        self.audio_file_path_query = path;
                        Task::none()
                    }
                    PathPickerTarget::RelinkAudio => {
                        self.relink_source_path_query = path;
                        Task::none()
                    }
                }
            }
            Ok(None) => Task::none(),
            Err(error) => {
                self.status = format!("File dialog failed: {error}");
                Task::none()
            }
        }
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

    fn start_audio_asset_management(
        &mut self,
        operation: AudioAssetManagementOperation,
    ) -> Task<Message> {
        if self.io_busy || self.import_busy || self.audio_asset_management_busy {
            self.status = "Wait for the current project operation to finish".to_owned();
            return Task::none();
        }
        #[cfg(feature = "jack-backend")]
        if self.playback.is_some() {
            self.status = "Close JACK output before maintaining audio assets".to_owned();
            return Task::none();
        }
        if self.is_dirty() {
            self.status = "Save the project before scanning or packing audio assets".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.project_path.clone() else {
            self.status = "Save the project before scanning or packing audio assets".to_owned();
            return Task::none();
        };
        self.audio_asset_management_busy = true;
        self.audio_asset_management_finalizing = false;
        self.audio_asset_management_cancel_requested = false;
        self.audio_asset_management_operation = Some(operation);
        if operation == AudioAssetManagementOperation::ScanSources {
            self.audio_asset_source_statuses.clear();
        }
        self.audio_asset_management_status = match operation {
            AudioAssetManagementOperation::ScanSources => "Starting source scan…".to_owned(),
            AudioAssetManagementOperation::PackExternalAssets => {
                "Starting asset packing…".to_owned()
            }
        };
        Task::perform(
            run_blocking("aaadaw-audio-asset-operation-start", move || {
                start_audio_asset_management(project_path, operation)
            }),
            move |result| {
                Message::AudioAssetManagementStarted(SharedAudioAssetManagementWorker(Arc::new(
                    Mutex::new(Some(result)),
                )))
            },
        )
    }

    fn audio_asset_management_started(&mut self, result: SharedAudioAssetManagementWorker) {
        let worker = result.0.lock().ok().and_then(|mut result| result.take());
        match worker {
            Some(Ok(worker)) => {
                if self.audio_asset_management_cancel_requested {
                    worker.cancel();
                    self.audio_asset_management_status = "Cancelling asset operation…".to_owned();
                }
                self.audio_asset_management_worker = Some(worker);
            }
            Some(Err(error)) => {
                self.audio_asset_management_busy = false;
                self.audio_asset_management_finalizing = false;
                self.audio_asset_management_cancel_requested = false;
                self.audio_asset_management_operation = None;
                self.audio_asset_management_status =
                    format!("Could not start asset operation: {error}");
                self.status = self.audio_asset_management_status.clone();
            }
            None => {
                self.audio_asset_management_busy = false;
                self.audio_asset_management_finalizing = false;
                self.audio_asset_management_cancel_requested = false;
                self.audio_asset_management_operation = None;
                self.audio_asset_management_status =
                    "Asset worker result was unavailable".to_owned();
                self.status = self.audio_asset_management_status.clone();
            }
        }
    }

    fn cancel_audio_asset_management(&mut self) {
        if !self.audio_asset_management_busy {
            self.status = "No audio asset operation is active".to_owned();
            return;
        }
        if self.audio_asset_management_finalizing {
            self.status = "Asset operation is finishing and can no longer be cancelled".to_owned();
            return;
        }
        self.audio_asset_management_cancel_requested = true;
        if let Some(worker) = self.audio_asset_management_worker.as_ref() {
            worker.cancel();
        }
        self.audio_asset_management_status = "Cancelling asset operation…".to_owned();
        self.status = self.audio_asset_management_status.clone();
    }

    fn update_audio_asset_management(&mut self) -> Task<Message> {
        let Some(worker) = self.audio_asset_management_worker.as_ref() else {
            return Task::none();
        };
        for progress in worker.progress() {
            match progress {
                AudioAssetManagementProgress::SourceScan(update) => {
                    self.audio_asset_source_statuses.insert(
                        update.media_ref.clone(),
                        AudioAssetSourceStatusEntry {
                            media_ref: update.media_ref.clone(),
                            status: update.status,
                            is_external_link: false,
                        },
                    );
                    self.audio_asset_management_status = format!(
                        "Scanned {}/{} · {}: {:?}",
                        update.scanned_assets, update.total_assets, update.media_ref, update.status
                    );
                }
                AudioAssetManagementProgress::Pack(update) => {
                    self.audio_asset_management_status = format!(
                        "Packing {}/{} · {} · {} / {} bytes",
                        update.completed_assets,
                        update.total_assets,
                        update.media_ref,
                        update.bytes_imported,
                        update.total_bytes
                    );
                }
            }
        }
        if !worker.is_finished() {
            return Task::none();
        }
        let Some(worker) = self.audio_asset_management_worker.take() else {
            return Task::none();
        };
        self.audio_asset_management_finalizing = true;
        self.audio_asset_management_status = "Finishing asset operation…".to_owned();
        Task::perform(
            run_blocking("aaadaw-audio-asset-operation-finish", move || worker.join()),
            Message::AudioAssetManagementFinished,
        )
    }

    fn finish_audio_asset_management(
        &mut self,
        result: Result<AudioAssetManagementResult, String>,
    ) {
        let was_cancelled = self.audio_asset_management_cancel_requested;
        let operation = self.audio_asset_management_operation.take();
        self.audio_asset_management_busy = false;
        self.audio_asset_management_finalizing = false;
        self.audio_asset_management_cancel_requested = false;
        match result {
            Ok(AudioAssetManagementResult::SourceScan(results)) => {
                self.audio_asset_source_statuses = results
                    .into_iter()
                    .map(|entry| (entry.media_ref.clone(), entry))
                    .collect();
                self.audio_asset_management_status = format!(
                    "Scanned {} audio assets",
                    self.audio_asset_source_statuses.len()
                );
            }
            Ok(AudioAssetManagementResult::Packed(media_refs)) => {
                self.audio_asset_source_statuses.clear();
                self.audio_asset_management_status =
                    format!("Packed {} external audio assets", media_refs.len());
            }
            Err(error) if was_cancelled => {
                self.audio_asset_management_status = match operation {
                    Some(AudioAssetManagementOperation::PackExternalAssets) => {
                        format!("Packing cancelled; completed assets remain embedded: {error}")
                    }
                    _ => format!("Source scan cancelled: {error}"),
                };
            }
            Err(error) => {
                self.audio_asset_management_status = format!("Asset operation failed: {error}");
            }
        }
        self.status = self.audio_asset_management_status.clone();
    }

    fn relink_audio_item(&mut self, item_id: ItemId) -> Task<Message> {
        if self.io_busy || self.import_busy || self.audio_asset_management_busy {
            self.status = "Wait for the current project operation to finish".to_owned();
            return Task::none();
        }
        #[cfg(feature = "jack-backend")]
        if self.playback.is_some() {
            self.status = "Close JACK output before relinking audio".to_owned();
            return Task::none();
        }
        if self.is_dirty() {
            self.status = "Save the project before relinking audio".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.project_path.clone() else {
            self.status = "Save the project before relinking audio".to_owned();
            return Task::none();
        };
        let Some(source_path) = project_path_from_query(&self.relink_source_path_query) else {
            self.status = "Enter the replacement source path first".to_owned();
            return Task::none();
        };
        let Some(media_ref) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            .map(|item| item.media_ref().to_owned())
        else {
            self.status = "Audio item no longer exists".to_owned();
            return Task::none();
        };
        self.io_busy = true;
        self.status = format!("Relinking {media_ref}…");
        Task::perform(
            run_blocking("aaadaw-audio-relink", move || {
                relink_external_audio_source(project_path, media_ref, source_path)
            }),
            move |result| Message::AudioItemRelinked(item_id, result),
        )
    }

    fn finish_audio_item_relink(&mut self, item_id: ItemId, result: Result<(), String>) {
        self.io_busy = false;
        match result {
            Ok(()) => {
                if let Some(media_ref) = self
                    .project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .map(|item| item.media_ref().to_owned())
                {
                    self.audio_asset_source_statuses.insert(
                        media_ref.clone(),
                        AudioAssetSourceStatusEntry {
                            media_ref,
                            status: AudioAssetSourceStatus::Linked,
                            is_external_link: true,
                        },
                    );
                }
                self.status = "External audio source relinked".to_owned();
            }
            Err(error) => self.status = format!("Audio relink failed: {error}"),
        }
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

    fn apply_edit<E: std::fmt::Display>(&mut self, action: Result<DawAction, E>, success: &str) {
        match action {
            Ok(action) => self.apply_action(action, success),
            Err(error) => self.status = format!("Edit failed: {error}"),
        }
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

    fn delete_track(&mut self, track_id: TrackId) {
        self.track_name_edits.remove(&track_id);
        self.apply_action(DawAction::DeleteTrack { track_id }, "Track deleted");
    }

    fn commit_track_name(&mut self, track_id: TrackId) {
        let Some(name) = self.track_name_edits.get(&track_id).cloned() else {
            return;
        };
        let name = name.trim().to_owned();
        if name.is_empty() {
            self.status = "Track name must not be empty".to_owned();
            return;
        }
        let Some(current_name) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            .map(|track| track.name().to_owned())
        else {
            self.track_name_edits.remove(&track_id);
            self.status = "Track no longer exists".to_owned();
            return;
        };
        self.track_name_edits.remove(&track_id);
        if name == current_name {
            self.status = "Track name unchanged".to_owned();
            return;
        }
        self.apply_action(DawAction::SetTrackName { track_id, name }, "Track renamed");
    }

    fn move_track(&mut self, track_id: TrackId, direction: i8) {
        let Some(index) = self
            .project
            .tracks()
            .iter()
            .position(|track| track.id() == track_id)
        else {
            self.status = "Track no longer exists".to_owned();
            return;
        };
        let target_index = match direction {
            -1 => index.checked_sub(1),
            1 if index + 1 < self.project.tracks().len() => Some(index + 1),
            _ => None,
        };
        let Some(target_index) = target_index else {
            self.status = "Track is already at that end of the list".to_owned();
            return;
        };
        self.apply_action(
            DawAction::MoveTrack {
                track_id,
                index: target_index,
            },
            "Track order changed",
        );
    }

    fn nudge_audio_item(&mut self, item_id: ItemId, direction: i8, milliseconds: u32) {
        self.audio_item_start_edits.remove(&item_id);
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
        let delta = (u64::from(self.project.settings().sample_rate()) * u64::from(milliseconds)
            / 1_000)
            .max(1);
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
            &format!("Audio item moved by {milliseconds} ms"),
        );
    }

    fn commit_audio_item_start_sample(&mut self, item_id: ItemId) {
        let Some(query) = self.audio_item_start_edits.get(&item_id).cloned() else {
            return;
        };
        let start_sample = match query.trim().parse::<u64>() {
            Ok(sample) => sample,
            Err(_) => {
                self.status = "Enter a non-negative sample position".to_owned();
                return;
            }
        };
        if self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            .is_some_and(|item| item.start_sample() == start_sample)
        {
            self.audio_item_start_edits.remove(&item_id);
            self.status = "Audio item position is unchanged".to_owned();
            return;
        }
        self.audio_item_start_edits.remove(&item_id);
        let action = set_audio_item_start_sample(&self.project, item_id, start_sample);
        self.apply_edit(action, "Audio item moved to exact sample position");
    }

    fn undo(&mut self) {
        self.audio_item_start_edits.clear();
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
        self.audio_item_start_edits.clear();
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

fn keyboard_shortcut_event(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    if status != iced::event::Status::Ignored {
        return None;
    }
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat: false,
        ..
    }) = event
    else {
        return None;
    };
    shortcut_message(key.as_ref(), modifiers)
}

fn shortcut_message(
    key: iced::keyboard::Key<&str>,
    modifiers: iced::keyboard::Modifiers,
) -> Option<Message> {
    use iced::keyboard::{Key, Modifiers, key::Named};

    if modifiers == Modifiers::COMMAND {
        return match key {
            Key::Character("z") | Key::Character("Z") => Some(Message::Undo),
            Key::Character("y") | Key::Character("Y") => Some(Message::Redo),
            Key::Character("s") | Key::Character("S") => Some(Message::SaveProject),
            Key::Character("o") | Key::Character("O") => Some(Message::OpenProject),
            _ => None,
        };
    }
    if modifiers == (Modifiers::COMMAND | Modifiers::SHIFT) {
        return match key {
            Key::Character("z") | Key::Character("Z") => Some(Message::Redo),
            _ => None,
        };
    }
    if modifiers == Modifiers::NONE {
        return match key {
            Key::Named(Named::Space) => {
                #[cfg(feature = "jack-backend")]
                {
                    Some(Message::TogglePlayback)
                }
                #[cfg(not(feature = "jack-backend"))]
                {
                    None
                }
            }
            _ => None,
        };
    }
    None
}

fn track_row<'a>(track: &'a Track, edited_name: &'a str, has_edit: bool) -> Element<'a, Message> {
    let track_id = track.id();
    let heading = row![
        text_input("Track name", edited_name)
            .on_input(move |name| Message::TrackNameChanged(track_id, name))
            .on_submit(Message::CommitTrackName(track_id))
            .width(Length::Fill),
        button(if has_edit { "Save" } else { "Rename" })
            .on_press(Message::CommitTrackName(track_id)),
        button("↑").on_press(Message::MoveTrack(track_id, -1)),
        button("↓").on_press(Message::MoveTrack(track_id, 1)),
        button("Delete").on_press(Message::DeleteTrack(track_id)),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let controls = row![
        button(if track.is_muted() { "Unmute" } else { "Mute" })
            .on_press(Message::ToggleMute(track_id)),
        button(if track.is_solo() { "Unsolo" } else { "Solo" })
            .on_press(Message::ToggleSolo(track_id)),
        button("−dB").on_press(Message::AdjustVolume(track_id, -1.0)),
        button("+dB").on_press(Message::AdjustVolume(track_id, 1.0)),
        button("◀").on_press(Message::AdjustPan(track_id, -0.1)),
        button("▶").on_press(Message::AdjustPan(track_id, 0.1)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    column![heading, controls].spacing(6).into()
}
