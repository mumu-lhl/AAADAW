#[cfg(feature = "jack-backend")]
use super::SharedPreparedPlayback;
use super::{SharedAudioAssetManagementWorker, SharedAudioImportWorker};
use aaadaw_app::{AudioAssetManagementOperation, AudioAssetManagementResult};
use aaadaw_core::{DawAction, ItemId, NoteId, Project, TrackId};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
    Track,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PathPickerTarget {
    OpenProject,
    SaveProject,
    ImportAudio,
    RelinkAudio,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    ToggleMainMenu(MainMenu),
    SelectWorkspace(WorkspacePage),
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
    DuplicateAudioItem(ItemId),
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
