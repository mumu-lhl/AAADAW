#[cfg(feature = "audio-device")]
use super::SharedPreparedPlayback;
use super::commands::CommandId;
use super::{SharedAudioAssetManagementWorker, SharedAudioImportWorker};
use crate::timeline::TimelineEvent;
use aaadaw_app::{AudioAssetManagementOperation, AudioAssetManagementResult, WavSampleFormat};
use aaadaw_core::{
    DawAction, ItemId, MidiControllerData, MidiNoteData, MidiPitchBendData, NoteId, Project,
    TrackId,
};
use aaadaw_engine::MasterOutputCeiling;
use aaadaw_storage::{ArrangementViewState, ProjectSessionLock};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) struct SharedProjectSessionLock(Arc<Mutex<Option<ProjectSessionLock>>>);

pub(crate) type SharedProjectLoadResult =
    Arc<Mutex<Option<Result<(Project, Option<ArrangementViewState>, ProjectSessionLock), String>>>>;

impl SharedProjectSessionLock {
    pub(crate) fn new(lock: Option<ProjectSessionLock>) -> Self {
        Self(Arc::new(Mutex::new(lock)))
    }

    pub(crate) fn take(&self) -> Option<ProjectSessionLock> {
        self.0.lock().ok().and_then(|mut lock| lock.take())
    }
}

impl std::fmt::Debug for SharedProjectSessionLock {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedProjectSessionLock(..)")
    }
}

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MainWorkspace {
    #[default]
    Arrangement,
    Mixer,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PathPickerTarget {
    OpenProject,
    SaveProject,
    ExportWav,
    ImportAudio,
    ImportAudioToProject,
    RelinkAudio,
    AddClapPluginPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingProjectTransition {
    NewProject,
    OpenProject,
    CloseMainWindow(iced::window::Id),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsCategory {
    #[default]
    KeyboardShortcuts,
    ActionMacros,
    ClapPlugins,
    Audio,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MidiEditorLane {
    #[default]
    Velocity,
    Sustain,
    Volume,
    Pan,
    Modulation,
    Expression,
    PitchBend,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeMapTab {
    #[default]
    Tempo,
    Meter,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    ToggleMainMenu(MainMenu),
    DismissMainMenu,
    ToggleOfflineJobsPanel,
    Escape,
    DismissMidiExpressionContextMenus(iced::window::Id),
    NewProject,
    ShowMainWorkspace(MainWorkspace),
    ToggleMediaBrowserPanel,
    OpenSettings,
    OpenClapPluginSettings,
    OpenRenderWindow,
    OpenTempoMap,
    OpenMeterMap,
    SelectTimeMapTab(TimeMapTab),
    AddTempoPoint,
    DeleteTempoPoint(usize),
    TempoPointTickChanged(usize, String),
    TempoPointBpmChanged(usize, String),
    CycleTempoCurve(usize),
    ApplyTempoMap,
    AddMeterPoint,
    DeleteMeterPoint(usize),
    MeterPointTickChanged(usize, String),
    MeterPointNumeratorChanged(usize, String),
    MeterPointDenominatorChanged(usize, String),
    ApplyMeterMap,
    OpenTrackFxChain(TrackId),
    OpenTrackInstrumentPicker(TrackId),
    SetTrackInstrumentGui(TrackId, bool),
    TrackInstrumentNativeParent(TrackId, bool, u64),
    OpenMidiEditor(ItemId),
    CloseMidiEditor,
    SelectMidiEditorLane(MidiEditorLane),
    MidiEditorFeedback(String),
    SelectMidiNotes(HashSet<NoteId>),
    CopyMidiNotes(ItemId, Vec<NoteId>),
    PasteMidiNotes(ItemId),
    DuplicateMidiNotes(ItemId),
    SetPianoRollCursor(ItemId, u64),
    AddMidiNoteAt(ItemId, MidiNoteData),
    EditMidiNotes(ItemId, Vec<(NoteId, MidiNoteData)>),
    DeleteMidiNotes(ItemId, Vec<NoteId>),
    SetMidiControllers(ItemId, Vec<MidiControllerData>),
    SetMidiPitchBends(ItemId, Vec<MidiPitchBendData>),
    PianoRollPan(i8),
    PianoRollPanPixels(f32),
    PianoRollZoom(f32),
    PianoRollZoomAt(f32, f32),
    PianoRollPitchScroll(i8),
    FitPianoRollToNotes(ItemId),
    TogglePianoRollFollowPlayhead,
    ClearTrackInstrument(TrackId),
    OpenPluginPicker,
    CloseTrackFxChain,
    ClosePluginPicker,
    PluginPickerSearchChanged(String),
    AddScannedPlugin(String),
    SelectScannedInstrument(String),
    SelectFxChainPlugin(usize),
    BeginFxChainPluginDrag(usize),
    HoverFxChainPluginDragTarget(usize),
    LeaveFxChainPluginDragTarget(usize),
    FinishFxChainPluginDrag,
    ReorderFxChainPlugin {
        from: usize,
        to: usize,
    },
    FxParameterChanged(u32, f64),
    CancelFxParameterGesture(u32),
    FxAutomationWriteToggled(u32),
    FxAutomationLaneToggled {
        parameter_id: u32,
        name: String,
        min_value: f64,
        max_value: f64,
        stepped: bool,
    },
    FxParameterEnded(u32),
    FxParameterValueTextChanged(u32, String),
    CommitFxParameterValue(u32),
    ResetFxParameterValue(u32),
    ToggleFxChainPlugin(usize),
    RemoveSelectedFxPlugin,
    FxChainWindowNativeHandle(iced::window::Id, Option<u64>),
    FxChainWindowScaleFactor(iced::window::Id, f32),
    WindowResized(iced::window::Id, iced::Size),
    WindowCloseRequested(iced::window::Id),
    WindowClosed(iced::window::Id),
    StartShortcutCapture(String),
    ClearShortcutBinding(String),
    RestoreShortcutDefault(String),
    SelectSettingsCategory(SettingsCategory),
    SetMasterOutputCeilingDbfs(MasterOutputCeiling),
    RecordingOffsetTextChanged(String),
    ApplyRecordingOffset,
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
    AddBusTrack,
    SetTrackOutput(TrackId, Option<TrackId>),
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
    #[cfg(feature = "audio-device")]
    ToggleInputMonitor(TrackId),
    PreviewTrackVolume(TrackId, f32),
    CancelTrackMixGesture,
    CommitTrackVolume(TrackId),
    PreviewTrackPan(TrackId, f32),
    CommitTrackPan(TrackId),
    TrackVolumeTextChanged(TrackId, String),
    TrackPanTextChanged(TrackId, String),
    CommitTrackVolumeText(TrackId),
    CommitTrackPanText(TrackId),
    ResetTrackVolume(TrackId),
    ResetTrackVolumeByDoubleClick(TrackId),
    ResetTrackPan(TrackId),
    ResetTrackPanByDoubleClick(TrackId),
    NudgeAudioItem(ItemId, i8, u32),
    BeginAudioItemStartSampleEdit(ItemId),
    AudioItemStartSampleChanged(ItemId, String),
    CommitAudioItemStartSample(ItemId),
    CancelAudioItemStartSampleEdit(ItemId),
    DeleteAudioItem(ItemId),
    DeleteSelectedItems,
    DuplicateSelectedItems,
    DuplicateAudioItem(ItemId),
    DuplicateMidiItem(ItemId),
    SplitSelectedItemsAtCursor,
    SplitSelectedItemsAtTimeSelection,
    Undo,
    Redo,
    ActionQueryChanged(String),
    ActionMacroNameChanged(String),
    ActionMacroStepSelected(String),
    AddActionMacroStep,
    RemoveActionMacroStep(usize),
    MoveActionMacroStep(usize, isize),
    NewActionMacro,
    EditActionMacro(u64),
    SaveActionMacro,
    DeleteActionMacro(u64),
    ShortcutPressed(String, iced::keyboard::Modifiers),
    SaveShortcutBindings,
    ResetShortcutBindings,
    RunActionQuery,
    ExecuteCommand(CommandId),
    OpenProject,
    SaveProject,
    SaveBeforeProjectTransition,
    DiscardProjectChanges,
    CancelProjectTransition,
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
    MeterTick,
    ProjectLoaded(PathBuf, SharedProjectLoadResult),
    ProjectSaved(
        PathBuf,
        u64,
        Result<(), String>,
        Option<String>,
        SharedProjectSessionLock,
    ),
    CancelOfflineRender,
    RemoveQueuedOfflineJob(u64),
    OfflineRenderFinished(Result<PathBuf, String>),
    SetWavSampleFormat(WavSampleFormat),
    SetWavDither(bool),
    FreezeTrack(TrackId),
    FreezeTrackFinished(Result<FreezeTrackResult, String>),
    UnfreezeTrack(TrackId),
    RecordingRecoveryScanned(
        PathBuf,
        Result<Vec<aaadaw_app::RecordingRecoveryCandidate>, String>,
    ),
    RecoverRecording(PathBuf),
    DiscardRecording(PathBuf),
    RecordingRecoveryPrepared(Result<aaadaw_app::RecordingRecoveryCandidate, String>),
    RecordingRecoveryDiscarded(PathBuf, Result<(), String>),
    RecordingRecoveryCleaned(Result<Vec<PathBuf>, String>),
    #[cfg(feature = "audio-device")]
    TogglePlayback,
    #[cfg(feature = "audio-device")]
    StartPlayback,
    #[cfg(feature = "audio-device")]
    StopPlayback,
    #[cfg(feature = "audio-device")]
    PanicMidi,
    #[cfg(feature = "audio-device")]
    StartRecording,
    #[cfg(all(feature = "audio-device", target_os = "android"))]
    MicrophonePermissionResult(Result<bool, String>),
    #[cfg(feature = "audio-device")]
    StopRecording,
    #[cfg(feature = "audio-device")]
    RecordingStarted(super::SharedRecordingStart),
    #[cfg(all(feature = "audio-device", target_os = "android"))]
    RecordingInputReconnected(super::SharedRecordingInputRecovery),
    #[cfg(feature = "audio-device")]
    StandbyInputStarted(TrackId, u64, super::SharedStandbyInput),
    #[cfg(feature = "audio-device")]
    StandbyInputClosed,
    #[cfg(feature = "audio-device")]
    RecordingInputDiscarded,
    #[cfg(feature = "audio-device")]
    RecordingPositionSaved(super::SharedRecordingPositionSaved),
    #[cfg(feature = "audio-device")]
    RecordingClockAnchorReady(super::SharedRecordingClockAnchorReady),
    #[cfg(feature = "audio-device")]
    RecordingStopped(super::SharedRecordingStop),
    #[cfg(feature = "audio-device")]
    RestartPlayback,
    #[cfg(feature = "audio-device")]
    SeekSampleChanged(String),
    #[cfg(feature = "audio-device")]
    SeekToItem(u64),
    #[cfg(feature = "audio-device")]
    SeekToSample,
    #[cfg(feature = "audio-device")]
    ClosePlayback,
    #[cfg(feature = "audio-device")]
    PlaybackPrepared {
        target_sample: u64,
        start_when_ready: bool,
        result: SharedPreparedPlayback,
    },
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    CpalOutputDevicesLoaded(Result<Vec<aaadaw_engine::CpalOutputDeviceInfo>, String>),
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    RefreshCpalOutputDevices,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    SelectCpalOutputDevice(Option<String>),
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    CpalInputDevicesLoaded(Result<Vec<aaadaw_engine::CpalInputDeviceInfo>, String>),
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    RefreshCpalInputDevices,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    SelectCpalInputDevice(Option<String>),
    #[cfg(all(feature = "audio-device", target_os = "android"))]
    RefreshAndroidMidiDevices,
    #[cfg(any(
        all(feature = "jack-backend", feature = "pipewire-backend"),
        all(
            feature = "jack-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos", target_os = "android")
        ),
        all(
            feature = "pipewire-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos", target_os = "android")
        )
    ))]
    SelectPlaybackBackend(aaadaw_app::PlaybackBackend),
}

#[derive(Clone, Debug)]
pub(crate) struct FreezeTrackResult {
    pub track_id: TrackId,
    pub media_ref: String,
    pub start_sample: u64,
    pub length_samples: u64,
}
