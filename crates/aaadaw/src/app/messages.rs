#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
use super::SharedPreparedPlayback;
use super::commands::CommandId;
use super::{SharedAudioAssetManagementWorker, SharedAudioImportWorker};
use crate::timeline::TimelineEvent;
use aaadaw_app::{AudioAssetManagementOperation, AudioAssetManagementResult};
use aaadaw_core::{DawAction, ItemId, MidiNoteData, NoteId, Project, TrackId};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub(crate) fn track_name_input_id(track_id: TrackId) -> iced::widget::Id {
    iced::widget::Id::from(format!("aaadaw-track-name-{}", track_id.value()))
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
    AddClapPluginPath,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsCategory {
    #[default]
    KeyboardShortcuts,
    ClapPlugins,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    ToggleMainMenu(MainMenu),
    DismissMainMenu,
    Escape,
    NewProject,
    ToggleMediaBrowserPanel,
    OpenSettings,
    OpenTrackFxChain(TrackId),
    OpenTrackInstrumentPicker(TrackId),
    OpenMidiEditor(ItemId),
    CloseMidiEditor,
    SelectMidiNotes(HashSet<NoteId>),
    CopyMidiNotes(ItemId, Vec<NoteId>),
    PasteMidiNotes(ItemId),
    AddMidiNoteAt(ItemId, MidiNoteData),
    EditMidiNotes(ItemId, Vec<(NoteId, MidiNoteData)>),
    DeleteMidiNotes(ItemId, Vec<NoteId>),
    PianoRollPan(i8),
    PianoRollZoom(f32),
    PianoRollPitchScroll(i8),
    ClearTrackInstrument(TrackId),
    OpenPluginPicker,
    CloseTrackFxChain,
    ClosePluginPicker,
    PluginPickerSearchChanged(String),
    AddScannedPlugin(String),
    SelectScannedInstrument(String),
    SelectFxChainPlugin(usize),
    ToggleFxChainPlugin(usize),
    RemoveSelectedFxPlugin,
    FxChainWindowNativeHandle(iced::window::Id, Option<u64>),
    FxChainWindowScaleFactor(iced::window::Id, f32),
    FxChainWindowResized(iced::window::Id, iced::Size),
    WindowCloseRequested(iced::window::Id),
    WindowClosed(iced::window::Id),
    StartShortcutCapture(String),
    ClearShortcutBinding(String),
    RestoreShortcutDefault(String),
    SelectSettingsCategory(SettingsCategory),
    RemoveClapPluginPath(PathBuf),
    RescanClapPlugins,
    ClapPluginsScanned(Result<aaadaw_app::ClapPluginScanReport, String>),
    CancelShortcutCapture,
    ShortcutCaptureKey {
        action_id: String,
        key: String,
        modifiers: iced::keyboard::Modifiers,
    },
    RuntimeKeyboardEvent(iced::Event, iced::event::Status, iced::window::Id),
    MediaPanelResized(iced::widget::pane_grid::Split, f32),
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
    ToggleRecordArm(TrackId),
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
    DuplicateMidiItem(ItemId),
    SplitSelectedItemsAtCursor,
    SplitSelectedItemsAtTimeSelection,
    Undo,
    Redo,
    ActionQueryChanged(String),
    ShortcutPressed(String, iced::keyboard::Modifiers),
    SaveShortcutBindings,
    ResetShortcutBindings,
    RunActionQuery,
    ExecuteCommand(CommandId),
    OpenProject,
    SaveProject,
    ImportAudio,
    AudioFilePathChanged(String),
    CancelAudioImport,
    AudioImportStarted(SharedAudioImportWorker),
    AudioImportFinished(Result<DawAction, String>),
    ReimportAudioItem(ItemId),
    RunAudioAssetManagement(AudioAssetManagementOperation),
    CancelAudioAssetManagement,
    AudioAssetManagementStarted(SharedAudioAssetManagementWorker),
    AudioAssetManagementFinished(Result<AudioAssetManagementResult, String>),
    RelinkSourcePathChanged(String),
    RelinkAudioItem(ItemId),
    AudioItemRelinked(ItemId, Result<(), String>),
    BackgroundTick,
    ProjectLoaded(PathBuf, Arc<Mutex<Option<Result<Project, String>>>>),
    ProjectSaved(PathBuf, u64, Result<(), String>, Option<String>),
    RecordingRecoveryScanned(
        PathBuf,
        Result<Vec<aaadaw_app::RecordingRecoveryCandidate>, String>,
    ),
    RecoverRecording(PathBuf),
    DiscardRecording(PathBuf),
    RecordingRecoveryPrepared(Result<aaadaw_app::RecordingRecoveryCandidate, String>),
    RecordingRecoveryDiscarded(PathBuf, Result<(), String>),
    RecordingRecoveryCleaned(Result<Vec<PathBuf>, String>),
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    TogglePlayback,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    StartPlayback,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    StopPlayback,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    StartRecording,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    StopRecording,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    RecordingStarted(super::SharedRecordingStart),
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    RecordingPositionSaved(super::SharedRecordingPositionSaved),
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    RecordingStopped(super::SharedRecordingStop),
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    RestartPlayback,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    SeekSampleChanged(String),
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    SeekToItem(u64),
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    SeekToSample,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    ClosePlayback,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    PlaybackPrepared {
        target_sample: u64,
        start_when_ready: bool,
        result: SharedPreparedPlayback,
    },
    #[cfg(all(feature = "jack-backend", feature = "pipewire-backend"))]
    SelectPlaybackBackend(aaadaw_app::PlaybackBackend),
}
