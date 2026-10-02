use crate::timeline::{self, TimelineState};
use aaadaw_app::{
    AudioAssetManagementOperation, AudioAssetManagementWorker, AudioAssetSourceStatusEntry,
    AudioItemImportWorker, AudioWaveformResult, AudioWaveformWorker, add_quarter_note,
    adjust_midi_note_pitch, adjust_midi_note_velocity, create_four_beat_midi_item,
    delete_midi_note, duplicate_audio_item, move_midi_item_by_beat, move_midi_note_by_sixteenth,
    quantize_midi_item_to_sixteenth, set_audio_item_start_sample,
};
#[cfg(feature = "jack-backend")]
use aaadaw_app::{
    PlaybackBuildError, PreparedAudioPlayback, RunningJackPlayback, prepare_audio_playback_at,
};
#[cfg(feature = "jack-backend")]
use aaadaw_core::ProjectSnapshot;
use aaadaw_core::{AudioItem, DawAction, ItemId, MidiItem, Project, TrackId};
use aaadaw_media::AudioWaveform;
use aaadaw_storage::ProjectStore;
use iced::Task;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

mod commands;
mod keyboard_config;
mod media;
mod messages;
mod project_io;
#[cfg(test)]
mod tests;
mod view;

pub(crate) use messages::{MainMenu, Message, PathPickerTarget, WorkspacePage};

pub(crate) fn run() -> iced::Result {
    let application = iced::application(App::new, App::update, view::view)
        .title("AAADAW")
        .theme(|_: &App| iced::Theme::Dark)
        .window_size(iced::Size::new(1280.0, 800.0));
    let application = application.subscription(App::subscription);
    application.run()
}

#[derive(Default)]
struct App {
    project: Project,
    action_query: String,
    shortcut_bindings: Arc<std::sync::RwLock<commands::ShortcutBindings>>,
    shortcut_binding_edits: commands::ShortcutBindings,
    project_path_query: String,
    project_path: Option<PathBuf>,
    track_name_edits: HashMap<TrackId, String>,
    audio_item_start_edits: HashMap<ItemId, String>,
    active_workspace: WorkspacePage,
    active_menu: Option<MainMenu>,
    timeline: TimelineState,
    path_picker_busy: bool,
    audio_asset_source_statuses: HashMap<String, AudioAssetSourceStatusEntry>,
    audio_asset_management_worker: Option<AudioAssetManagementWorker>,
    audio_asset_management_busy: bool,
    audio_asset_management_finalizing: bool,
    audio_asset_management_cancel_requested: bool,
    audio_asset_management_operation: Option<AudioAssetManagementOperation>,
    audio_asset_management_status: String,
    audio_waveforms: HashMap<String, Arc<AudioWaveform>>,
    audio_waveform_worker: Option<AudioWaveformWorker>,
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
        match keyboard_config::load() {
            Ok(bindings) => {
                if let Ok(bindings) = commands::validate_bindings(&bindings) {
                    app.shortcut_binding_edits = bindings.clone();
                    *app.shortcut_bindings
                        .write()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = bindings;
                } else {
                    app.status =
                        "Keyboard shortcut config has conflicts; using defaults".to_owned();
                }
            }
            Err(error) => {
                app.status = format!("Keyboard shortcut config unavailable: {error}");
            }
        }
        let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
            return (app, Task::none());
        };
        app.project_path_query = path.to_string_lossy().into_owned();
        app.io_busy = true;
        app.status = format!("Opening {}…", path.display());
        let message_path = path.clone();
        let task = Task::perform(
            run_blocking("aaadaw-project-open", move || {
                project_io::load_project_file(path)
            }),
            move |result| Message::ProjectLoaded(message_path, Arc::new(Mutex::new(Some(result)))),
        );
        (app, task)
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        #[cfg(feature = "jack-backend")]
        let playback_active = self.playback.is_some();
        #[cfg(not(feature = "jack-backend"))]
        let playback_active = false;

        let background_ticks = if self.import_busy
            || playback_active
            || self.audio_asset_management_busy
            || self.audio_waveform_worker.is_some()
        {
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
        if !matches!(
            &message,
            Message::Timeline(
                timeline::TimelineEvent::OpenTrackContextMenu(_)
                    | timeline::TimelineEvent::ToggleTrackContextMenu(_)
            ) | Message::Escape
        ) {
            self.timeline.context_track = None;
        }
        if !matches!(
            &message,
            Message::Timeline(timeline::TimelineEvent::OpenItemContextMenu { .. })
                | Message::Escape
        ) {
            self.timeline.context_item = None;
            self.timeline.context_item_position = None;
        }
        if !matches!(
            &message,
            Message::ToggleMainMenu(_)
                | Message::DismissMainMenu
                | Message::Escape
                | Message::ActionQueryChanged(_)
        ) {
            self.active_menu = None;
        }
        let allowed_during_io = matches!(
            &message,
            Message::ProjectLoaded(..)
                | Message::ProjectSaved(..)
                | Message::ToggleMainMenu(_)
                | Message::DismissMainMenu
                | Message::Escape
                | Message::SelectWorkspace(_)
                | Message::ExecuteCommand(commands::CommandId::Workspace(_))
                | Message::Timeline(_)
                | Message::TcpScrolled { .. }
                | Message::TimelineScrolled { .. }
                | Message::PathPicked(..)
                | Message::AudioItemRelinked(..)
                | Message::BackgroundTick
        );
        if matches!(
            &message,
            Message::Timeline(
                timeline::TimelineEvent::BeginItemDrag { .. }
                    | timeline::TimelineEvent::UpdateItemDrag { .. }
                    | timeline::TimelineEvent::EndItemDrag
            )
        ) && let Some(status) = item_drag_edit_guard_status(
            self.path_picker_busy,
            self.import_busy,
            self.audio_asset_management_busy,
            self.playback_busy(),
            self.playback_active(),
            self.io_busy,
        ) {
            self.timeline
                .handle(timeline::TimelineEvent::CancelItemDrag);
            self.status = status.to_owned();
            return Task::none();
        }
        if self.path_picker_busy
            && !matches!(
                &message,
                Message::PathPicked(..)
                    | Message::ToggleMainMenu(_)
                    | Message::DismissMainMenu
                    | Message::Escape
                    | Message::SelectWorkspace(_)
                    | Message::ExecuteCommand(commands::CommandId::Workspace(_))
                    | Message::Timeline(_)
                    | Message::TcpScrolled { .. }
                    | Message::TimelineScrolled { .. }
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
                    | Message::Escape
                    | Message::SelectWorkspace(_)
                    | Message::ExecuteCommand(commands::CommandId::Workspace(_))
                    | Message::Timeline(_)
                    | Message::TcpScrolled { .. }
                    | Message::TimelineScrolled { .. }
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
                    | Message::DismissMainMenu
                    | Message::Escape
                    | Message::SelectWorkspace(_)
                    | Message::ExecuteCommand(commands::CommandId::Workspace(_))
                    | Message::Timeline(_)
                    | Message::TcpScrolled { .. }
                    | Message::TimelineScrolled { .. }
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
                        | Message::DismissMainMenu
                        | Message::Escape
                        | Message::SelectWorkspace(_)
                        | Message::ExecuteCommand(commands::CommandId::Workspace(_))
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
                        | Message::DeleteSelectedItems
                        | Message::DuplicateAudioItem(_)
                        | Message::DuplicateMidiItem(_)
                        | Message::SplitSelectedItemsAtCursor
                        | Message::SplitSelectedItemsAtTimeSelection
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
            Message::DismissMainMenu => self.active_menu = None,
            Message::Escape => {
                if self.active_menu.take().is_none() {
                    if self.timeline.context_item.take().is_some()
                        || self.timeline.context_track.take().is_some()
                    {
                        self.timeline.context_item_position = None;
                    } else {
                        self.timeline
                            .handle(timeline::TimelineEvent::ClearTimeSelection);
                    }
                }
            }
            Message::SelectWorkspace(page) => self.active_workspace = page,
            Message::Timeline(timeline::TimelineEvent::EndItemDrag) => self.finish_item_drag(),
            Message::Timeline(timeline::TimelineEvent::CancelItemDrag) => {
                self.timeline
                    .handle(timeline::TimelineEvent::CancelItemDrag);
                self.status = "Item drag cancelled".to_owned();
            }
            Message::Timeline(event) => self.timeline.handle(event),
            Message::BeginTrackNameEdit(track_id) => task = self.begin_track_name_edit(track_id),
            Message::TcpScrolled { offset, height } => {
                let offset_changed = (offset - self.timeline.vertical_scroll).abs() > 0.5;
                self.timeline.vertical_scroll = offset;
                self.timeline.viewport_height = height;
                if offset_changed {
                    task = scroll_arrangement_to(timeline::TIMELINE_SCROLL_ID, offset);
                }
            }
            Message::TimelineScrolled { offset, height } => {
                let offset_changed = (offset - self.timeline.vertical_scroll).abs() > 0.5;
                self.timeline.vertical_scroll = offset;
                self.timeline.viewport_height = height;
                if offset_changed {
                    task = scroll_arrangement_to(timeline::TCP_SCROLL_ID, offset);
                }
            }
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
            Message::DeleteSelectedItems => self.delete_selected_items(),
            Message::SplitSelectedItemsAtCursor => self.split_selected_items(false),
            Message::SplitSelectedItemsAtTimeSelection => self.split_selected_items(true),
            Message::DuplicateAudioItem(item_id) => {
                let action = duplicate_audio_item(&self.project, item_id);
                self.apply_edit(action, "Audio item duplicated");
            }
            Message::DuplicateMidiItem(item_id) => {
                self.apply_action(
                    DawAction::DuplicateMidiItem { item_id },
                    "MIDI item duplicated",
                );
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
            Message::ShortcutPressed(key, modifiers) => {
                let key = if key == " " {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Space)
                } else {
                    iced::keyboard::Key::Character(key.as_str())
                };
                let shortcut = {
                    let bindings = self
                        .shortcut_bindings
                        .read()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    commands::from_shortcut(&key, modifiers, &bindings).map(Message::ExecuteCommand)
                };
                if let Some(message) = shortcut {
                    task = self.update(message);
                }
            }
            Message::ShortcutBindingChanged(id, binding) => {
                self.shortcut_binding_edits.insert(id, binding);
            }
            Message::SaveShortcutBindings => self.save_shortcut_bindings(),
            Message::ResetShortcutBindings => self.reset_shortcut_bindings(),
            Message::RunActionQuery => task = self.run_action_query(),
            Message::ExecuteCommand(command) => task = commands::dispatch(self, command),
            Message::PickPath(target) => task = self.pick_path(target),
            Message::PathPicked(target, result) => task = self.path_picked(target, result),
            Message::OpenProject => task = self.open_project_command(),
            Message::SaveProject => {
                task = self.save_project_command();
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
                self.update_audio_waveforms();
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
                        self.timeline.rebuild(&self.project);
                        self.timeline.selected_item = None;
                        self.timeline.selected_track = None;
                        self.timeline.time_selection = None;
                        self.timeline.origin_tick = 0;
                        self.timeline.edit_cursor_tick = 0;
                        self.timeline.vertical_scroll = 0.0;
                        task = Task::batch([
                            scroll_arrangement_to(timeline::TCP_SCROLL_ID, 0.0),
                            scroll_arrangement_to(timeline::TIMELINE_SCROLL_ID, 0.0),
                        ]);
                        self.track_name_edits.clear();
                        self.audio_item_start_edits.clear();
                        self.audio_asset_source_statuses.clear();
                        self.project_path_query = path.to_string_lossy().into_owned();
                        self.project_path = Some(path.clone());
                        self.start_audio_waveform_scan(true);
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

    fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
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
                self.timeline.rebuild(&self.project);
                success.to_owned()
            }
            Err(error) => format!("Action failed: {error}"),
        };
    }

    fn finish_item_drag(&mut self) {
        let preview = self.timeline.drag_preview();
        self.timeline.handle(timeline::TimelineEvent::EndItemDrag);
        let Some(preview) = preview else {
            return;
        };
        if !preview.valid {
            self.status = "Drop rejected: item would leave the project bounds".to_owned();
            return;
        }
        match self.item_drag_action(preview) {
            Ok(Some(action)) => self.apply_action(action, "Items moved"),
            Ok(None) => {}
            Err(error) => self.status = format!("Drop rejected: {error}"),
        }
    }

    fn playback_busy(&self) -> bool {
        #[cfg(feature = "jack-backend")]
        {
            self.playback_busy
        }
        #[cfg(not(feature = "jack-backend"))]
        {
            false
        }
    }

    fn playback_active(&self) -> bool {
        #[cfg(feature = "jack-backend")]
        {
            self.playback.is_some()
        }
        #[cfg(not(feature = "jack-backend"))]
        {
            false
        }
    }

    fn item_drag_action(
        &self,
        preview: timeline::ItemDragPreview,
    ) -> Result<Option<DawAction>, String> {
        let mut item_ids = self
            .timeline
            .selected_items
            .iter()
            .copied()
            .collect::<Vec<_>>();
        item_ids.sort_unstable_by_key(|item_id| item_id.value());
        let mut actions = Vec::with_capacity(item_ids.len() * 2);
        for item_id in item_ids {
            let (source_track_id, start_tick) = if let Some(item) = self
                .project
                .audio_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                (
                    item.track_id(),
                    self.project
                        .tick_at_sample(item.start_sample())
                        .map_err(|error| error.to_string())?,
                )
            } else if let Some(item) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                (item.track_id(), item.start_tick())
            } else {
                return Err(format!("item {} no longer exists", item_id.value()));
            };
            let source_track_index = self
                .project
                .tracks()
                .iter()
                .position(|track| track.id() == source_track_id)
                .ok_or_else(|| "source track no longer exists".to_owned())?;
            let target_track_index =
                usize::try_from(source_track_index as i128 + i128::from(preview.track_delta))
                    .map_err(|_| "target track is outside the project".to_owned())?;
            let target_track_id = self
                .project
                .tracks()
                .get(target_track_index)
                .map(|track| track.id())
                .ok_or_else(|| "target track is outside the project".to_owned())?;
            let target_start_tick = u64::try_from(i128::from(start_tick) + preview.delta_ticks)
                .map_err(|_| "target position is outside the project".to_owned())?;

            if let Some(item) = self
                .project
                .audio_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                let target_sample = self
                    .project
                    .sample_at_tick(target_start_tick)
                    .map_err(|error| error.to_string())?;
                if target_sample != item.start_sample() {
                    actions.push(DawAction::EditAudioItem {
                        item_id,
                        media_ref: item.media_ref().to_owned(),
                        start_sample: target_sample,
                        source_offset_samples: item.source_offset_samples(),
                        length_samples: item.length_samples(),
                    });
                }
            } else if let Some(item) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
                && target_start_tick != item.start_tick()
            {
                actions.push(DawAction::EditMidiItem {
                    item_id,
                    start_tick: target_start_tick,
                    length_ticks: item.length_ticks(),
                });
            }

            if target_track_id != source_track_id {
                actions.push(DawAction::MoveItemToTrack {
                    item_id,
                    track_id: target_track_id,
                });
            }
        }
        if actions.is_empty() {
            return Ok(None);
        }
        Ok(Some(DawAction::BatchTransaction {
            tx_id: self.revision,
            actions,
        }))
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
                self.timeline.rebuild(&self.project);
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
                self.timeline.rebuild(&self.project);
                "Action redone".to_owned()
            }
            Ok(false) => "Nothing to redo".to_owned(),
            Err(error) => format!("Redo failed: {error}"),
        };
    }

    fn run_action_query(&mut self) -> Task<Message> {
        if let Some(command) = commands::find(self, &self.action_query) {
            self.update(Message::ExecuteCommand(command))
        } else {
            self.status =
                "Unknown action. Search for a command or choose one from the Actions menu."
                    .to_owned();
            Task::none()
        }
    }

    fn save_shortcut_bindings(&mut self) {
        let bindings = match commands::validate_bindings(&self.shortcut_binding_edits) {
            Ok(bindings) => bindings,
            Err(error) => {
                self.status = format!("Shortcut bindings not saved: {error}");
                return;
            }
        };
        match keyboard_config::save(&bindings) {
            Ok(()) => {
                *self
                    .shortcut_bindings
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = bindings.clone();
                self.shortcut_binding_edits = bindings;
                self.status = "Keyboard shortcuts saved".to_owned();
            }
            Err(error) => self.status = format!("Keyboard shortcuts could not be saved: {error}"),
        }
    }

    fn reset_shortcut_bindings(&mut self) {
        match keyboard_config::reset() {
            Ok(()) => {
                self.shortcut_binding_edits.clear();
                self.shortcut_bindings
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clear();
                self.status = "Keyboard shortcuts restored to defaults".to_owned();
            }
            Err(error) => {
                self.status = format!("Keyboard shortcuts could not be reset: {error}");
            }
        }
    }

    fn selected_track_id(&self) -> Option<TrackId> {
        self.timeline.selected_track.filter(|track_id| {
            self.project
                .tracks()
                .iter()
                .any(|track| track.id() == *track_id)
        })
    }

    fn begin_track_name_edit(&mut self, track_id: TrackId) -> Task<Message> {
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            return Task::none();
        };
        self.track_name_edits
            .insert(track_id, track.name().to_owned());
        self.timeline.selected_track = Some(track_id);
        let input_id = messages::track_name_input_id(track_id);
        Task::batch([
            iced::widget::operation::focus(input_id.clone()),
            iced::widget::operation::move_cursor_to_end(input_id),
        ])
    }

    pub(super) fn can_split_selected_items_at_cursor(&self) -> bool {
        self.selected_item_split_action(false, None)
            .is_ok_and(|action| action.is_some())
    }

    pub(super) fn can_split_selected_items_at_time_selection(&self) -> bool {
        self.selected_item_split_action(true, None)
            .is_ok_and(|action| action.is_some())
    }

    fn selected_item_split_action(
        &self,
        use_time_selection: bool,
        source_sample_rates: Option<&HashMap<String, u32>>,
    ) -> Result<Option<DawAction>, String> {
        let boundaries = if use_time_selection {
            let Some(selection) = self.timeline.time_selection else {
                return Ok(None);
            };
            vec![selection.start_tick, selection.end_tick]
        } else {
            vec![self.timeline.edit_cursor_tick]
        };
        let mut selected_item_ids = self
            .timeline
            .selected_items
            .iter()
            .copied()
            .collect::<Vec<_>>();
        selected_item_ids.sort_unstable_by_key(|item_id| item_id.value());

        let mut actions = Vec::new();
        for item_id in selected_item_ids {
            if let Some(item) = self
                .project
                .audio_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                let source_sample_rate = source_sample_rates
                    .and_then(|rates| rates.get(item.media_ref()).copied())
                    .unwrap_or_else(|| self.project.settings().sample_rate());
                split_audio_item_actions(
                    &self.project,
                    item,
                    &boundaries,
                    source_sample_rate,
                    &mut actions,
                )?;
            } else if let Some(item) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                split_midi_item_actions(item, &boundaries, &mut actions)?;
            }
        }
        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            }))
        }
    }

    fn selected_audio_source_sample_rates(
        &self,
        use_time_selection: bool,
    ) -> Result<HashMap<String, u32>, String> {
        let boundaries = if use_time_selection {
            let Some(selection) = self.timeline.time_selection else {
                return Ok(HashMap::new());
            };
            vec![selection.start_tick, selection.end_tick]
        } else {
            vec![self.timeline.edit_cursor_tick]
        };
        let mut media_refs = self
            .project
            .audio_items()
            .iter()
            .filter(|item| self.timeline.selected_items.contains(&item.id()))
            .filter(|item| {
                boundaries.iter().any(|tick| {
                    self.project.sample_at_tick(*tick).is_ok_and(|sample| {
                        item.start_sample() < sample && sample < item.end_sample()
                    })
                })
            })
            .map(|item| item.media_ref().to_owned())
            .collect::<Vec<_>>();
        media_refs.sort_unstable();
        media_refs.dedup();
        if media_refs.is_empty() {
            return Ok(HashMap::new());
        }

        let path = self
            .project_path
            .as_deref()
            .filter(|path| path.is_file())
            .ok_or_else(|| "save the project before splitting audio items".to_owned())?;
        let store = ProjectStore::open(path).map_err(|error| error.to_string())?;
        let mut rates = HashMap::with_capacity(media_refs.len());
        let mut metadata_result = Ok(());
        for media_ref in media_refs {
            match store.audio_asset_metadata(&media_ref) {
                Ok(Some(metadata)) => match metadata.sample_rate.filter(|rate| *rate > 0) {
                    Some(rate) => {
                        rates.insert(media_ref, rate);
                    }
                    None => {
                        metadata_result =
                            Err(format!("source sample rate is unavailable for {media_ref}"));
                        break;
                    }
                },
                Ok(None) => {
                    metadata_result =
                        Err(format!("source metadata is unavailable for {media_ref}"));
                    break;
                }
                Err(error) => {
                    metadata_result = Err(error.to_string());
                    break;
                }
            }
        }
        let close_result = store.close().map_err(|error| error.to_string());
        metadata_result?;
        close_result?;
        Ok(rates)
    }

    fn start_audio_waveform_scan(&mut self, reset: bool) {
        if let Some(worker) = self.audio_waveform_worker.take() {
            worker.cancel();
        }
        if reset {
            self.audio_waveforms.clear();
            self.timeline
                .set_audio_waveforms(&self.project, HashMap::new());
        }
        let Some(path) = self.project_path.as_deref().filter(|path| path.is_file()) else {
            return;
        };
        let media_refs = self
            .project
            .audio_items()
            .iter()
            .map(|item| item.media_ref().to_owned())
            .filter(|media_ref| !self.audio_waveforms.contains_key(media_ref))
            .collect::<Vec<_>>();
        if media_refs.is_empty() {
            return;
        }
        match AudioWaveformWorker::start(path.to_owned(), media_refs) {
            Ok(worker) => self.audio_waveform_worker = Some(worker),
            Err(error) => self.status = format!("Waveform scan could not start: {error}"),
        }
    }

    fn update_audio_waveforms(&mut self) {
        let Some(worker) = &self.audio_waveform_worker else {
            return;
        };
        let results = worker.results();
        let finished = worker.is_finished();
        let mut changed = false;
        for AudioWaveformResult {
            media_ref,
            waveform,
        } in results
        {
            match waveform {
                Ok(waveform) => {
                    self.audio_waveforms.insert(media_ref, waveform);
                    changed = true;
                }
                Err(error) => {
                    self.status = format!("Waveform unavailable: {error}");
                }
            }
        }
        if changed {
            self.timeline
                .set_audio_waveforms(&self.project, self.audio_waveforms.clone());
        }
        if finished {
            let worker = self
                .audio_waveform_worker
                .take()
                .expect("finished waveform worker is present");
            if let Err(error) = worker.join() {
                self.status = format!("Waveform scan failed: {error}");
            }
        }
    }

    fn split_selected_items(&mut self, use_time_selection: bool) {
        let source_sample_rates = match self.selected_audio_source_sample_rates(use_time_selection)
        {
            Ok(rates) => rates,
            Err(error) => {
                self.status = format!("Split failed: {error}");
                return;
            }
        };
        let action =
            match self.selected_item_split_action(use_time_selection, Some(&source_sample_rates)) {
                Ok(Some(action)) => action,
                Ok(None) => {
                    self.status = "No selected item crosses the split point".to_owned();
                    return;
                }
                Err(error) => {
                    self.status = format!("Split failed: {error}");
                    return;
                }
            };
        let previous_ids = self
            .project
            .audio_items()
            .iter()
            .map(|item| item.id())
            .chain(self.project.midi_items().iter().map(|item| item.id()))
            .collect::<HashSet<_>>();
        let previous_revision = self.revision;
        self.apply_action(action, "Selected items split");
        if self.revision == previous_revision {
            return;
        }
        let new_ids = self
            .project
            .audio_items()
            .iter()
            .map(|item| item.id())
            .chain(self.project.midi_items().iter().map(|item| item.id()))
            .filter(|item_id| !previous_ids.contains(item_id))
            .collect::<Vec<_>>();
        self.timeline.selected_items.extend(new_ids);
        self.audio_item_start_edits.clear();
    }

    fn delete_selected_items(&mut self) {
        let mut item_ids = self
            .timeline
            .selected_items
            .iter()
            .copied()
            .collect::<Vec<_>>();
        item_ids.sort_unstable_by_key(|item_id| item_id.value());
        if item_ids.is_empty() {
            self.status = "Select one or more items to delete".to_owned();
            return;
        }

        let mut actions = Vec::with_capacity(item_ids.len());
        for item_id in item_ids {
            if self
                .project
                .audio_items()
                .iter()
                .any(|item| item.id() == item_id)
            {
                self.audio_item_start_edits.remove(&item_id);
                actions.push(DawAction::DeleteAudioItem { item_id });
            } else if self
                .project
                .midi_items()
                .iter()
                .any(|item| item.id() == item_id)
            {
                actions.push(DawAction::DeleteMidiItem { item_id });
            }
        }
        if actions.is_empty() {
            self.status = "Selected items no longer exist".to_owned();
            return;
        }
        let action = if actions.len() == 1 {
            actions.pop().expect("single delete action is present")
        } else {
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            }
        };
        self.apply_action(action, "Selected items deleted");
    }

    fn save_project_command(&mut self) -> Task<Message> {
        if self.project_path.is_none() {
            self.pick_path(PathPickerTarget::SaveProject)
        } else {
            project_io::save_project(self, None)
        }
    }

    fn open_project_command(&mut self) -> Task<Message> {
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
        self.pick_path(PathPickerTarget::OpenProject)
    }
}

fn split_audio_item_actions(
    project: &Project,
    item: &AudioItem,
    boundaries: &[u64],
    source_sample_rate: u32,
    actions: &mut Vec<DawAction>,
) -> Result<(), String> {
    let mut cut_samples = boundaries
        .iter()
        .filter_map(|tick| project.sample_at_tick(*tick).ok())
        .filter(|sample| item.start_sample() < *sample && *sample < item.end_sample())
        .collect::<Vec<_>>();
    cut_samples.sort_unstable();
    cut_samples.dedup();
    if cut_samples.is_empty() {
        return Ok(());
    }

    let mut points = Vec::with_capacity(cut_samples.len() + 2);
    points.push(item.start_sample());
    points.extend(cut_samples);
    points.push(item.end_sample());
    for (index, segment) in points.windows(2).enumerate() {
        let start_sample = segment[0];
        let length_samples = segment[1] - segment[0];
        let project_sample_rate = project.settings().sample_rate();
        let source_delta = u64::try_from(
            (u128::from(start_sample - item.start_sample()) * u128::from(source_sample_rate)
                + u128::from(project_sample_rate) / 2)
                / u128::from(project_sample_rate),
        )
        .map_err(|_| "audio source offset exceeds the supported range".to_owned())?;
        let source_offset_samples = item
            .source_offset_samples()
            .checked_add(source_delta)
            .ok_or_else(|| "audio source offset exceeds the supported range".to_owned())?;
        if index == 0 {
            actions.push(DawAction::EditAudioItem {
                item_id: item.id(),
                media_ref: item.media_ref().to_owned(),
                start_sample: item.start_sample(),
                source_offset_samples,
                length_samples,
            });
        } else {
            actions.push(DawAction::InsertAudioItem {
                track_id: item.track_id(),
                media_ref: item.media_ref().to_owned(),
                start_sample,
                source_offset_samples,
                length_samples,
            });
        }
    }
    Ok(())
}

fn split_midi_item_actions(
    item: &MidiItem,
    boundaries: &[u64],
    actions: &mut Vec<DawAction>,
) -> Result<(), String> {
    let item_end = item
        .start_tick()
        .checked_add(item.length_ticks())
        .ok_or_else(|| "MIDI item end exceeds the supported range".to_owned())?;
    let mut split_ticks = boundaries
        .iter()
        .copied()
        .filter(|tick| item.start_tick() < *tick && *tick < item_end)
        .collect::<Vec<_>>();
    split_ticks.sort_unstable();
    split_ticks.dedup();
    if !split_ticks.is_empty() {
        actions.push(DawAction::SplitMidiItem {
            item_id: item.id(),
            split_ticks,
        });
    }
    Ok(())
}

fn item_drag_edit_guard_status(
    path_picker_busy: bool,
    import_busy: bool,
    asset_management_busy: bool,
    playback_busy: bool,
    playback_active: bool,
    io_busy: bool,
) -> Option<&'static str> {
    if path_picker_busy {
        Some("Wait for the file dialog to finish")
    } else if import_busy {
        Some("Wait for audio import to finish or cancel it")
    } else if asset_management_busy {
        Some("Wait for audio asset maintenance to finish or cancel it")
    } else if playback_busy {
        Some("Wait for playback preparation to finish")
    } else if playback_active {
        Some("Close JACK output before editing the project")
    } else if io_busy {
        Some("Wait for current project operation to finish")
    } else {
        None
    }
}

fn project_path_from_query(query: &str) -> Option<PathBuf> {
    let query = query.trim();
    (!query.is_empty()).then(|| PathBuf::from(query))
}

fn scroll_arrangement_to(target: &'static str, offset_y: f32) -> Task<Message> {
    use iced::advanced::widget::operation::scrollable::{self, AbsoluteOffset};

    let target = iced::widget::Id::new(target);
    let offset = AbsoluteOffset {
        x: None,
        y: Some(offset_y.max(0.0)),
    };
    iced::advanced::widget::operate(scrollable::scroll_to(target, offset))
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
    let is_escape = matches!(
        &event,
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            modifiers: iced::keyboard::Modifiers::NONE,
            repeat: false,
            ..
        })
    );
    if status != iced::event::Status::Ignored && !is_escape {
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
    match key.as_ref() {
        iced::keyboard::Key::Character(character) => {
            Some(Message::ShortcutPressed(character.to_owned(), modifiers))
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Space) => {
            Some(Message::ShortcutPressed(" ".to_owned(), modifiers))
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => Some(Message::Escape),
        _ => None,
    }
}

#[cfg(test)]
fn shortcut_message(
    key: iced::keyboard::Key<&str>,
    modifiers: iced::keyboard::Modifiers,
    bindings: &commands::ShortcutBindings,
) -> Option<Message> {
    if modifiers == iced::keyboard::Modifiers::NONE
        && key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
    {
        return Some(Message::Escape);
    }
    commands::from_shortcut(&key, modifiers, bindings).map(Message::ExecuteCommand)
}
