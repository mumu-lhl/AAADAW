#[cfg(feature = "jack-backend")]
use super::SharedPreparedPlayback;
use super::commands::CommandId;
use super::{SharedAudioAssetManagementWorker, SharedAudioImportWorker};
use crate::timeline::TimelineEvent;
use aaadaw_app::{AudioAssetManagementOperation, AudioAssetManagementResult};
use aaadaw_core::{DawAction, ItemId, NoteId, Project, TrackId};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub(crate) fn track_name_input_id(track_id: TrackId) -> iced::widget::Id {
    iced::widget::Id::from(format!("aaadaw-track-name-{}", track_id.value()))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WorkspacePage {
    #[default]
    Arrangement,
    Media,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MainMenu {
    File,
    Edit,
    View,
    Insert,
    Item,
    Track,
    Actions,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PathPickerTarget {
    OpenProject,
    SaveProject,
    ImportAudio,
    ImportAudioToProject,
    RelinkAudio,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    ToggleMainMenu(MainMenu),
    DismissMainMenu,
    Escape,
    SelectWorkspace(WorkspacePage),
    Timeline(TimelineEvent),
    TcpScrolled {
        offset: f32,
        height: f32,
    },
    TimelineScrolled {
        offset: f32,
        height: f32,
    },
    PickPath(PathPickerTarget),
    PathPicked(PathPickerTarget, Result<Option<PathBuf>, String>),
    AddTrack,
    AddMidiItem,
    AddMidiNote(ItemId),
    DeleteMidiItem(ItemId),
    NudgeMidiItem(ItemId, i8),
    NudgeMidiNote(ItemId, NoteId, i8),
    AdjustMidiNotePitch(ItemId, NoteId, i8),
    AdjustMidiNoteVelocity(ItemId, NoteId, i8),
    DeleteMidiNote(ItemId, NoteId),
    QuantizeMidiItem(ItemId),
    DeleteTrack(TrackId),
    MoveTrack(TrackId, i8),
    BeginTrackNameEdit(TrackId),
    TrackNameChanged(TrackId, String),
    CommitTrackName(TrackId),
    ToggleMute(TrackId),
    ToggleSolo(TrackId),
    AdjustVolume(TrackId, f32),
    AdjustPan(TrackId, f32),
    NudgeAudioItem(ItemId, i8, u32),
    BeginAudioItemStartSampleEdit(ItemId),
    AudioItemStartSampleChanged(ItemId, String),
    CommitAudioItemStartSample(ItemId),
    CancelAudioItemStartSampleEdit(ItemId),
    DeleteAudioItem(ItemId),
    DeleteSelectedItems,
    DuplicateAudioItem(ItemId),
    SplitSelectedItemsAtCursor,
    SplitSelectedItemsAtTimeSelection,
    Undo,
    Redo,
    ActionQueryChanged(String),
    RunActionQuery,
    ExecuteCommand(CommandId),
    OpenProject,
    SaveProject,
    AudioFilePathChanged(String),
    ImportAudio,
    CancelAudioImport,
    AudioImportStarted(SharedAudioImportWorker),
    AudioImportFinished(Result<DawAction, String>),
    RunAudioAssetManagement(AudioAssetManagementOperation),
    CancelAudioAssetManagement,
    AudioAssetManagementStarted(SharedAudioAssetManagementWorker),
    AudioAssetManagementFinished(Result<AudioAssetManagementResult, String>),
    RelinkSourcePathChanged(String),
    RelinkAudioItem(ItemId),
    AudioItemRelinked(ItemId, Result<(), String>),
    BackgroundTick,
    ProjectLoaded(PathBuf, Arc<Mutex<Option<Result<Project, String>>>>),
    ProjectSaved(PathBuf, u64, Result<(), String>),
    #[cfg(feature = "jack-backend")]
    TogglePlayback,
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
