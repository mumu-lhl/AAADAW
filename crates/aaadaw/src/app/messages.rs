#[cfg(feature = "audio-device")]
use super::SharedPreparedPlayback;
use super::audio_export::{RenderBounds, RenderFilenameWildcard, RenderSampleRate, RenderSource};
use super::commands::{CommandId, ShortcutBindings};
use super::{SharedAudioAssetManagementWorker, SharedAudioImportWorker};
use crate::timeline::TimelineEvent;
use aaadaw_app::{
    AudioAssetManagementOperation, AudioAssetManagementResult, WavChannelCount, WavSampleFormat,
};
use aaadaw_core::{
    DawAction, ItemId, MarkerId, MidiChannelPressureData, MidiControllerData,
    MidiFileProjectImport, MidiNoteData, MidiPitchBendData, MidiPolyAftertouchData,
    MidiProgramChangeData, NoteId, Project, RegionId, TrackId,
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
    Options,
    Actions,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuNavigation {
    OpenMenu(MainMenu),
    NextMenu,
    PreviousMenu,
    FirstCommand,
    LastCommand,
    NextCommand,
    PreviousCommand,
    Activate,
}

pub(crate) fn action_search_input_id() -> iced::widget::Id {
    iced::widget::Id::from("aaadaw-action-search")
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MainWorkspace {
    #[default]
    Arrangement,
    Mixer,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobilePanel {
    #[default]
    Editor,
    MidiEditor,
    MediaBrowser,
    Settings,
    TimeMap,
    FxChain,
    PluginPicker,
    Routing,
}

impl MobilePanel {
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Editor => "Editor",
            Self::MidiEditor => "MIDI Editor",
            Self::MediaBrowser => "Media Browser",
            Self::Settings => "Settings",
            Self::TimeMap => "Time Map",
            Self::FxChain => "Track Effects",
            Self::PluginPicker => "Choose Plugin",
            Self::Routing => "Track routing",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PathPickerTarget {
    OpenProject,
    SaveProject,
    SaveProjectTemplate,
    ExportWav,
    ExportMidi,
    ImportAudio,
    ImportMidiToProject,
    ImportAudioToProject,
    RelinkAudio,
    AddClapPluginPath,
    ImportShortcutMap,
    ExportShortcutMap,
    ImportReaperKeyMap,
    ExportReaperKeyMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingProjectTransition {
    NewProject,
    CloseProject,
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
pub(crate) enum MidiEditorTool {
    #[default]
    Select,
    Draw,
    Erase,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MidiEditorMode {
    #[default]
    PianoRoll,
    EventList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MidiEditorEventListCursor {
    Note(NoteId),
    Controller(MidiControllerData),
    BankSelect(MidiControllerData, MidiControllerData),
    PitchBend(MidiPitchBendData),
    ProgramChange(MidiProgramChangeData),
    PolyAftertouch(MidiPolyAftertouchData),
    ChannelPressure(MidiChannelPressureData),
    Opaque(u32),
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MidiEditorEventListCursorDirection {
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MidiEditorCursorBoundary {
    FirstEvent,
    End,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum MidiEditorChannelSelection {
    #[default]
    AllChannels,
    Channel(u8),
    Multichannel,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MidiEditorEventTypeFilter {
    #[default]
    AllEvents,
    Notes,
    ControlChange,
    Pitch,
    ProgramChange,
    PolyAftertouch,
    ChannelPressure,
    SysExMeta,
}

impl MidiEditorEventTypeFilter {
    pub(crate) const OPTIONS: [Self; 8] = [
        Self::AllEvents,
        Self::Notes,
        Self::PolyAftertouch,
        Self::ControlChange,
        Self::ProgramChange,
        Self::ChannelPressure,
        Self::Pitch,
        Self::SysExMeta,
    ];

    pub(crate) fn matches(self, kind: MidiEditorEventKind) -> bool {
        matches!(
            (self, kind),
            (Self::AllEvents, _)
                | (Self::Notes, MidiEditorEventKind::Note)
                | (Self::ControlChange, MidiEditorEventKind::ControlChange)
                | (Self::Pitch, MidiEditorEventKind::Pitch)
                | (Self::ProgramChange, MidiEditorEventKind::ProgramChange)
                | (Self::PolyAftertouch, MidiEditorEventKind::PolyAftertouch)
                | (Self::ChannelPressure, MidiEditorEventKind::ChannelPressure)
                | (
                    Self::SysExMeta,
                    MidiEditorEventKind::SysEx | MidiEditorEventKind::Text
                )
        )
    }
}

impl std::fmt::Display for MidiEditorEventTypeFilter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::AllEvents => "<all events>",
            Self::Notes => "Note",
            Self::ControlChange => "Control Change (CC)",
            Self::Pitch => "Pitch",
            Self::ProgramChange => "Program Change (PC)",
            Self::PolyAftertouch => "Poly Aftertouch",
            Self::ChannelPressure => "Channel Pressure",
            Self::SysExMeta => "Sysex/Meta",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MidiEditorEventKind {
    Note,
    ControlChange,
    Pitch,
    ProgramChange,
    PolyAftertouch,
    ChannelPressure,
    SysEx,
    Text,
}

impl MidiEditorChannelSelection {
    pub(crate) const OPTIONS: [Self; 18] = [
        Self::AllChannels,
        Self::Channel(0),
        Self::Channel(1),
        Self::Channel(2),
        Self::Channel(3),
        Self::Channel(4),
        Self::Channel(5),
        Self::Channel(6),
        Self::Channel(7),
        Self::Channel(8),
        Self::Channel(9),
        Self::Channel(10),
        Self::Channel(11),
        Self::Channel(12),
        Self::Channel(13),
        Self::Channel(14),
        Self::Channel(15),
        Self::Multichannel,
    ];

    pub(crate) fn shows_event(
        self,
        channel: u8,
        filter_enabled: bool,
        filter_channels: u16,
        filter_inverted: bool,
        event_type: MidiEditorEventTypeFilter,
        event_kind: MidiEditorEventKind,
    ) -> bool {
        if !filter_enabled {
            return true;
        }

        let channel_matches = match self {
            Self::AllChannels => true,
            Self::Channel(selected) => channel == selected,
            Self::Multichannel => {
                filter_channels & (1_u16.checked_shl(u32::from(channel)).unwrap_or(0)) != 0
            }
        };
        let passes = channel_matches && event_type.matches(event_kind);
        if filter_inverted { !passes } else { passes }
    }
}

impl std::fmt::Display for MidiEditorChannelSelection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AllChannels => formatter.write_str("All channels"),
            Self::Channel(channel) => write!(formatter, "Channel {}", channel.saturating_add(1)),
            Self::Multichannel => formatter.write_str("Multichannel"),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MidiQuantizeScope {
    #[default]
    SelectedNotes,
    AllNotes,
    SelectedEvents,
    AllEvents,
}

impl MidiQuantizeScope {
    pub(crate) const OPTIONS: [Self; 4] = [
        Self::AllNotes,
        Self::SelectedNotes,
        Self::AllEvents,
        Self::SelectedEvents,
    ];
}

impl std::fmt::Display for MidiQuantizeScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::AllNotes => "All notes",
            Self::SelectedNotes => "Selected notes",
            Self::SelectedEvents => "Selected events",
            Self::AllEvents => "All events",
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MidiHumanizeScope {
    AllNotes,
    #[default]
    SelectedNotes,
}

impl MidiHumanizeScope {
    pub(crate) const OPTIONS: [Self; 2] = [Self::AllNotes, Self::SelectedNotes];
}

impl std::fmt::Display for MidiHumanizeScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::AllNotes => "All notes",
            Self::SelectedNotes => "Selected notes",
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MidiTransposeScope {
    AllNotes,
    #[default]
    SelectedNotes,
}

impl MidiTransposeScope {
    pub(crate) const OPTIONS: [Self; 2] = [Self::AllNotes, Self::SelectedNotes];
}

impl std::fmt::Display for MidiTransposeScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::AllNotes => "All notes",
            Self::SelectedNotes => "Selected notes",
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MidiQuantizeOperation {
    #[default]
    Position,
    PositionAndNoteEnd,
    PositionAndNoteLength,
    NoteEndOnly,
    NoteLengthOnly,
}

impl MidiQuantizeOperation {
    pub(crate) const OPTIONS: [Self; 5] = [
        Self::Position,
        Self::PositionAndNoteEnd,
        Self::PositionAndNoteLength,
        Self::NoteEndOnly,
        Self::NoteLengthOnly,
    ];
}

impl std::fmt::Display for MidiQuantizeOperation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Position => "Position",
            Self::PositionAndNoteEnd => "Position and note end",
            Self::PositionAndNoteLength => "Position and note length",
            Self::NoteEndOnly => "Note end only",
            Self::NoteLengthOnly => "Note length only",
        })
    }
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
    MenuKeyboard(MenuNavigation),
    DismissMainMenu,
    ToggleOfflineJobsPanel,
    ToggleTransportDetails,
    Escape,
    ConfirmFocusedTrackDeletion,
    CancelFocusedTrackDeletion,
    DismissMidiExpressionContextMenus(iced::window::Id),
    NewProject,
    CloseProject,
    ShowMainWorkspace(MainWorkspace),
    MobileNavigateBack,
    ToggleMediaBrowserPanel,
    ToggleMixerPanel,
    OpenSettings,
    OpenAbout,
    CloseAbout,
    OpenActionList,
    OpenActionMacroEditor,
    CloseActionList,
    ActionListQueryChanged(String),
    ActionListResultsScrolled {
        offset: f32,
        height: f32,
    },
    ActionListFilterSubmitted,
    ActionListSelect(String),
    ActionListSelectBinding(usize),
    ActionListAddBinding,
    ActionInputSpecialKey,
    ActionInputConfirm,
    ActionInputCancel,
    ActionListDeleteBinding,
    ActionListFindShortcut,
    ActionListOptionsToggled,
    ActionListRun(bool),
    OpenClapPluginSettings,
    OpenRenderWindow,
    OpenRenderQueue,
    OpenMidiExportDialog,
    CloseMidiExportDialog,
    BrowseMidiExportDestination,
    ConfirmMidiExport,
    ConfirmEmptyMidiExport,
    CancelEmptyMidiExport,
    MidiExportDestinationChanged(String),
    MidiExportFileTypeChanged(aaadaw_core::MidiFileType),
    MidiExportTimeScopeChanged(aaadaw_core::MidiExportTimeScope),
    MidiExportItemScopeChanged(aaadaw_core::MidiExportItemScope),
    MidiExportPpqChanged(String),
    MidiExportEmbedTempoChanged(bool),
    MidiExportEmbedSmpteChanged(bool),
    MidiExportMarkersChanged(bool),
    MidiExportMarkerTypeChanged(aaadaw_core::MidiMarkerType),
    MidiExportHashOnlyChanged(bool),
    MidiExportHiddenMarkersChanged(bool),
    OpenProjectSettings,
    CloseProjectSettings,
    OpenSnapGridSettings,
    CloseSnapGridSettings,
    SnapSwingGridChanged(bool),
    SnapSwingAmountChanged(i16),
    ToggleSnapSwingGrid,
    ApplyProjectSettings,
    SaveProjectSettingsAsDefault,
    ProjectFrameRateChanged(aaadaw_core::FrameRate),
    ProjectSettingsBpmBasisChanged(aaadaw_core::BpmBasis),
    ProjectSettingsBaseRulerLabelSpacingChanged(bool),
    ProjectSettingsStartTimeChanged(String),
    ProjectStartTimeMenuChanged(bool),
    ProjectStartTimeCommandChosen(super::project_settings::start_time_menu::StartTimeCommand),
    ProjectFrameRateMenuChanged(bool),
    ProjectSettingsTabChanged(super::project_settings::Tab),
    ProjectSettingsSampleRateChanged(u32),
    ProjectSettingsForceWholeSamplesChanged(bool),
    ProjectSettingsTempoChanged(String),
    ProjectSettingsMeterNumeratorChanged(String),
    ProjectSettingsMeterDenominatorChanged(String),
    ProjectSettingsStartMeasureChanged(String),
    ProjectNotesTitleChanged(String),
    ProjectNotesAuthorChanged(String),
    ProjectNotesEdited(iced::widget::text_editor::Action),
    ProjectNotesShowOnLoadChanged(bool),
    ProjectNotesWrapChanged(bool),
    OpenItemProperties,
    ToggleItemProperties,
    OpenItemNudge,
    CloseItemNudge,
    ItemNudgeModeChanged(super::item_nudge::Mode),
    ItemNudgeTargetChanged(super::item_nudge::Target),
    ItemNudgeUnitChanged(super::item_nudge::Unit),
    ItemNudgeNoteValueChanged(super::item_nudge::NoteValue),
    ItemNudgeValueChanged(String),
    ItemNudgeSnapToUnitChanged(bool),
    ItemNudgeRelativeSetChanged(bool),
    ItemNudgeGetCursor,
    ApplyItemNudgeSet,
    NudgeSelectedItems(i8),
    ItemPropertiesUnitChanged(super::item_properties::TimeUnit),
    ItemPropertyFieldChanged(super::item_properties::ItemPropertyField, String),
    ItemPropertyNameChanged(String),
    ItemPropertyTakeRateChanged(String),
    ItemPropertyLoopSourceChanged(bool),
    ItemPropertyMuteChanged(bool),
    BatchItemPropertyMuteChanged(bool),
    ItemPropertyPhaseChanged(bool),
    MidiItemPropertyNameChanged(String),
    MidiItemPropertySourceLoopLengthChanged(String),
    ToggleSelectedItemsMute,
    ApplyItemProperties(bool),
    CloseItemProperties,
    MarkerNameChanged(String),
    MarkerNumberChanged(String),
    MarkerPositionChanged(String),
    ApplyMarkerProperties,
    CloseMarkerProperties,
    RegionNameChanged(String),
    RegionNumberChanged(String),
    RegionPositionChanged(String),
    RegionLengthChanged(String),
    ApplyRegionProperties,
    CloseRegionProperties,
    OpenMarkerManager,
    OpenRegionRenderMatrix,
    CloseRegionRenderMatrix,
    SetRegionRenderMatrixMaster(RegionId, bool),
    SetRegionRenderMatrixTrack(RegionId, TrackId, bool),
    ToggleProjectMarkersOnRuler,
    ToggleProjectRegionsOnRuler,
    ToggleProjectRegionsAndMarkersOnRuler,
    ToggleSelectedProjectMarkers,
    ToggleSelectedProjectRegions,
    ToggleSelectedProjectRegionsAndMarkers,
    CloseMarkerManager,
    MarkerManagerFilterChanged(String),
    MarkerManagerShowRegionsChanged(bool),
    MarkerManagerShowMarkersChanged(bool),
    MarkerManagerClearFilter,
    MarkerManagerSelectMarker(MarkerId, bool),
    MarkerManagerSetMarkerVisibility(MarkerId, bool),
    MarkerManagerSetRegionVisibility(RegionId, bool),
    MarkerManagerEditMarker(MarkerId),
    MarkerManagerDeleteMarker(MarkerId),
    MarkerManagerSelectRegion(RegionId, bool),
    MarkerManagerEditRegion(aaadaw_core::RegionId),
    MarkerManagerDeleteRegion(aaadaw_core::RegionId),
    OpenTempoMap,
    OpenMeterMap,
    SelectTimeMapTab(TimeMapTab),
    ClearTrackMeter(TrackId),
    ClearInputMeter,
    ClearMasterMeter,
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
    PreviewMidiNote(TrackId, u8),
    ReleaseMidiPreview,
    SelectMidiEditorLane(MidiEditorLane),
    SelectMidiEditorTool(MidiEditorTool),
    SetMidiEditorMode(MidiEditorMode),
    DeleteSelectedMidiEvents(ItemId),
    OpenMidiEventProperties(ItemId),
    SetMidiEventPropertiesPosition(String),
    SetMidiEventPropertiesLength(String),
    SetMidiEventPropertiesChannel(String),
    SetMidiEventPropertiesParameter(String),
    SetMidiEventPropertiesValue(String),
    SetMidiEventPropertiesOffValue(String),
    SetMidiEventPropertiesBankMsb(String),
    SetMidiEventPropertiesBankLsb(String),
    ApplyMidiEventProperties,
    AcceptMidiEventProperties,
    CancelMidiEventProperties,
    SelectMidiEditorChannel(MidiEditorChannelSelection),
    SetMidiEditorDrawChannel(u8),
    ToggleMidiEditorChannelFilter,
    ToggleMidiEditorFilterChannel(u8),
    SetMidiEditorFilterChannelsAll(bool),
    ToggleMidiEditorFilterInvert,
    OpenMidiEditorFilter,
    SetMidiEditorEventTypeFilter(MidiEditorEventTypeFilter),
    CloseMidiEditorFilter,
    MidiEditorFeedback(String),
    SelectMidiNotes(HashSet<NoteId>),
    SelectMidiNoteEvent(NoteId, bool),
    SelectMidiControllerEvent(ItemId, MidiControllerData, bool),
    SelectMidiBankSelectEvent(ItemId, MidiControllerData, MidiControllerData, bool),
    SelectMidiPitchBendEvent(ItemId, MidiPitchBendData, bool),
    SelectMidiProgramChangeEvent(ItemId, MidiProgramChangeData, bool),
    SelectMidiPolyAftertouchEvent(ItemId, MidiPolyAftertouchData, bool),
    SelectMidiChannelPressureEvent(ItemId, MidiChannelPressureData, bool),
    SelectMidiOpaqueEvent(ItemId, u32, bool),
    SelectMidiEditorEventListEnd(bool),
    MoveMidiEditorEventListCursor(MidiEditorEventListCursorDirection, bool, bool),
    MoveMidiEditorCursorToBoundary(MidiEditorCursorBoundary),
    CopyMidiNotes(ItemId, Vec<NoteId>),
    PasteMidiNotes(ItemId),
    DuplicateMidiNotes(ItemId),
    SetPianoRollCursor(ItemId, u64),
    AddMidiNoteAt(ItemId, MidiNoteData),
    CopyDragMidiNotes(ItemId, Vec<MidiNoteData>),
    EditMidiNotes(ItemId, Vec<(NoteId, MidiNoteData)>),
    TransposeSelectedMidiNotes(i8),
    DeleteMidiNotes(ItemId, Vec<NoteId>),
    SetMidiControllers(ItemId, Vec<MidiControllerData>),
    SetMidiPitchBends(ItemId, Vec<MidiPitchBendData>),
    SetMidiProgramChanges(ItemId, Vec<MidiProgramChangeData>),
    SetMidiPolyAftertouches(ItemId, Vec<MidiPolyAftertouchData>),
    SetMidiChannelPressures(ItemId, Vec<MidiChannelPressureData>),
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
    AndroidAppFocusChanged(bool),
    WindowCloseRequested(iced::window::Id),
    WindowClosed(iced::window::Id),
    StartShortcutCapture(String),
    AddShortcutBinding(String),
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
    ActionListImportShortcutMap,
    ActionListExportShortcutMap,
    ShortcutMapImported(Result<ShortcutBindings, String>),
    ShortcutMapExported(Result<(), String>),
    ActionListImportReaperKeyMap,
    ActionListExportReaperKeyMap,
    ReaperKeyMapImported(Result<ShortcutBindings, String>),
    ReaperKeyMapExported(Result<(), String>),
    AddTrack,
    AddBusTrack,
    SetTrackOutput(TrackId, Option<TrackId>),
    OpenTrackRouting(TrackId),
    CloseTrackRouting,
    RoutingChange(DawAction),
    RoutingSendDraft(aaadaw_core::SendId, bool, String),
    CommitRoutingSend(aaadaw_core::SendId),
    AddMidiItem,
    AddProjectMarker,
    InsertRegionFromTimeSelection,
    DeleteProjectRegionNearCursor,
    EditProjectRegionNearCursor,
    PreviousProjectMarker,
    NextProjectMarker,
    DeleteProjectMarkerNearCursor,
    MoveProjectMarkerSlot(u8),
    AddMidiNote(ItemId),
    DeleteMidiItem(ItemId),
    MidiItemNameChanged(ItemId, String),
    CommitMidiItemName(ItemId),
    NudgeMidiItem(ItemId, i8),
    NudgeMidiNote(ItemId, NoteId, i8),
    AdjustMidiNotePitch(ItemId, NoteId, i8),
    AdjustMidiNoteVelocity(ItemId, NoteId, i8),
    DeleteMidiNote(ItemId, NoteId),
    QuantizeMidiItem(ItemId),
    QuantizeSelectedMidiNotes(ItemId),
    QuantizeSelectedMidiNotesAndEnds(ItemId),
    OpenMidiQuantizeDialog(ItemId),
    OpenMidiHumanizeDialog(ItemId),
    OpenMidiTransposeDialog(ItemId),
    SetMidiTransposeScope(MidiTransposeScope),
    SetMidiTransposeSemitones(String),
    AcceptMidiTranspose,
    CancelMidiTranspose,
    SetMidiHumanizeScope(MidiHumanizeScope),
    SetMidiHumanizeTiming(u8),
    SetMidiHumanizeVelocity(u8),
    SetMidiHumanizeTimingBias(i16),
    NewMidiHumanizeSeed,
    AcceptMidiHumanize,
    CancelMidiHumanize,
    SetMidiQuantizeScope(MidiQuantizeScope),
    SetMidiQuantizeOperation(MidiQuantizeOperation),
    SetMidiQuantizeStrength(u8),
    SetMidiQuantizeBypass(bool),
    SetMidiQuantizeFixOverlaps(bool),
    CommitMidiQuantize,
    AcceptMidiQuantize,
    CancelMidiQuantize,
    DeleteTrack(TrackId),
    DeleteByFocus,
    DeleteByFocusNoPrompting,
    MoveTrack(TrackId, i8),
    BeginTrackNameEdit(TrackId),
    TrackNameChanged(TrackId, String),
    CommitTrackName(TrackId),
    ToggleMute(TrackId),
    TogglePhase(TrackId),
    ToggleSolo(TrackId),
    ToggleRecordArm(TrackId),
    #[cfg(feature = "audio-device")]
    ToggleInputMonitor(TrackId),
    PreviewTrackVolume(TrackId, f32),
    PreviewMasterVolume(f32),
    PreviewMasterPan(f32),
    CommitMasterMix,
    CancelMasterMix,
    ResetMasterVolume,
    ResetMasterPan,
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
    TrimItemsToSelectedArea,
    CopyLoopOfSelectedArea,
    EncloseMediaItemsInRazorEdit,
    PasteItems,
    SelectAllItems,
    SelectAllItemsInTrack,
    SelectAllItemsInTimeSelection,
    SelectAllItemsOnSelectedTracksInTimeSelection,
    UnselectAllItems,
    InvertItemSelection,
    SelectNextAdjacentItem,
    SelectPreviousAdjacentItem,
    SelectItemOnAdjacentTrack(bool),
    MoveEditCursorToNearestItemEdge(bool),
    MoveEditCursorToSelectedItemsBoundary(bool),
    SnapSelectedItemsLeft,
    SnapSelectedItemsRight,
    SnapSelectedItemsToNearestPoint,
    DuplicateSelectedItems,
    DuplicateAudioItem(ItemId),
    DuplicateMidiItem(ItemId),
    SplitSelectedItemsAtCursor,
    SplitSelectedItemsAtTimeSelection,
    HealSplitsInItems,
    Undo,
    Redo,
    ActionQueryChanged(String),
    ActionMenuScrolled(f32),
    ActionMacroNameChanged(String),
    ActionMacroStepSelected(String),
    AddActionMacroStep,
    RemoveActionMacroStep(usize),
    MoveActionMacroStep(usize, isize),
    NewActionMacro,
    EditActionMacro(u64),
    SaveActionMacro,
    DeleteActionMacro(u64),
    ShortcutPressed(super::shortcut::ShortcutInput),
    SaveShortcutBindings,
    ResetShortcutBindings,
    RunActionQuery,
    ExecuteCommand(CommandId),
    ToggleRecentProjectsMenu,
    OpenRecentProject(PathBuf),
    ToggleProjectTemplatesMenu,
    OpenProjectTemplate(PathBuf),
    ProjectTemplateCopied(PathBuf, Result<(PathBuf, PathBuf), String>),
    ProjectTemplateSaved(PathBuf, Result<(), String>),
    ConfirmProjectTemplateOverwrite,
    CancelProjectTemplateOverwrite,
    OpenProject,
    SaveProject,
    SaveProjectNewVersion,
    SaveBeforeProjectTransition,
    DiscardProjectChanges,
    CancelProjectTransition,
    ImportAudio,
    AudioFilePathChanged(String),
    CancelAudioImport,
    AudioImportStarted(SharedAudioImportWorker),
    AudioImportFinished(Result<DawAction, String>),
    MidiFileImportFinished(Result<MidiFileProjectImport, String>),
    SetMidiFileImportExpandTracks(bool),
    SetMidiFileImportTempoMap(bool),
    SetMidiFileImportMarkers(bool),
    ConfirmMidiFileImport,
    CancelMidiFileImport,
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
    ProjectMidiExported(PathBuf, Result<(), String>),
    UnsavedSessionRecoveryScanned(Result<Vec<PathBuf>, String>),
    RecoverUnsavedSession(PathBuf),
    DiscardUnsavedSession(PathBuf),
    UnsavedSessionRecovered(PathBuf, SharedProjectLoadResult),
    UnsavedSessionDiscarded(PathBuf, Result<(), String>),
    UnsavedSessionSnapshotSaved(u64, u64, Result<(), String>),
    CancelOfflineRender,
    RemoveQueuedOfflineJob(u64),
    OfflineRenderFinished(Result<PathBuf, String>),
    SetWavSampleFormat(WavSampleFormat),
    SetWavDither(bool),
    SetRenderBounds(RenderBounds),
    SetRenderCustomStart(String),
    SetRenderCustomEnd(String),
    SetRenderSampleRate(RenderSampleRate),
    SetRenderSource(RenderSource),
    SetRenderStemsPreFader(bool),
    SetRenderChannels(WavChannelCount),
    SetRenderPeakNormalization(bool),
    SetRenderPeakTarget(String),
    SetRenderTailEnabled(bool),
    SetRenderTailDuration(String),
    SetRenderFilenamePattern(String),
    SetRenderWildcardAllowSlash(bool),
    ToggleRenderWildcardMenu,
    InsertRenderFilenameWildcard(RenderFilenameWildcard),
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
    ToggleRepeat,
    #[cfg(feature = "audio-device")]
    TogglePlayStop,
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

#[cfg(test)]
mod midi_channel_selection_tests {
    use super::{MidiEditorChannelSelection, MidiEditorEventKind, MidiEditorEventTypeFilter};

    #[test]
    fn channel_filter_matches_all_single_and_multiple_channel_modes() {
        let shows_note = |selection: MidiEditorChannelSelection,
                          channel: u8,
                          enabled: bool,
                          channels: u16,
                          inverted: bool| {
            selection.shows_event(
                channel,
                enabled,
                channels,
                inverted,
                MidiEditorEventTypeFilter::AllEvents,
                MidiEditorEventKind::Note,
            )
        };
        assert!(shows_note(
            MidiEditorChannelSelection::AllChannels,
            15,
            true,
            0,
            false
        ));
        assert!(!shows_note(
            MidiEditorChannelSelection::Channel(2),
            1,
            true,
            u16::MAX,
            false
        ));
        assert!(shows_note(
            MidiEditorChannelSelection::Channel(2),
            2,
            true,
            0,
            false
        ));
        assert!(!shows_note(
            MidiEditorChannelSelection::Multichannel,
            1,
            true,
            0b0101,
            false
        ));
        assert!(shows_note(
            MidiEditorChannelSelection::Multichannel,
            2,
            true,
            0b0100,
            false
        ));
        assert!(!shows_note(
            MidiEditorChannelSelection::Multichannel,
            0,
            true,
            0b0101,
            true
        ));
        assert!(shows_note(
            MidiEditorChannelSelection::Multichannel,
            1,
            true,
            0b0101,
            true
        ));
        assert!(shows_note(
            MidiEditorChannelSelection::Multichannel,
            15,
            false,
            0,
            true
        ));
        assert!(!shows_note(
            MidiEditorChannelSelection::AllChannels,
            15,
            true,
            u16::MAX,
            true
        ));
    }

    #[test]
    fn midi_channel_choices_cover_reaper_channel_menu() {
        assert_eq!(MidiEditorChannelSelection::OPTIONS.len(), 18);
        assert_eq!(
            MidiEditorChannelSelection::OPTIONS[0].to_string(),
            "All channels"
        );
        assert_eq!(
            MidiEditorChannelSelection::OPTIONS[1].to_string(),
            "Channel 1"
        );
        assert_eq!(
            MidiEditorChannelSelection::OPTIONS[16].to_string(),
            "Channel 16"
        );
        assert_eq!(
            MidiEditorChannelSelection::OPTIONS[17].to_string(),
            "Multichannel"
        );
    }

    #[test]
    fn event_type_filter_covers_modeled_events_and_inverts_the_combined_match() {
        assert!(MidiEditorEventTypeFilter::AllEvents.matches(MidiEditorEventKind::Note));
        assert!(MidiEditorEventTypeFilter::AllEvents.matches(MidiEditorEventKind::ControlChange));
        assert!(MidiEditorEventTypeFilter::AllEvents.matches(MidiEditorEventKind::Pitch));
        assert!(MidiEditorEventTypeFilter::AllEvents.matches(MidiEditorEventKind::ProgramChange));
        assert!(MidiEditorEventTypeFilter::AllEvents.matches(MidiEditorEventKind::PolyAftertouch));
        assert!(MidiEditorEventTypeFilter::AllEvents.matches(MidiEditorEventKind::ChannelPressure));
        assert!(MidiEditorEventTypeFilter::AllEvents.matches(MidiEditorEventKind::SysEx));
        assert!(MidiEditorEventTypeFilter::AllEvents.matches(MidiEditorEventKind::Text));
        assert!(MidiEditorEventTypeFilter::Notes.matches(MidiEditorEventKind::Note));
        assert!(!MidiEditorEventTypeFilter::Notes.matches(MidiEditorEventKind::ControlChange));
        assert!(
            MidiEditorEventTypeFilter::ControlChange.matches(MidiEditorEventKind::ControlChange)
        );
        assert!(MidiEditorEventTypeFilter::Pitch.matches(MidiEditorEventKind::Pitch));
        assert!(
            MidiEditorEventTypeFilter::ProgramChange.matches(MidiEditorEventKind::ProgramChange)
        );
        assert!(
            MidiEditorEventTypeFilter::PolyAftertouch.matches(MidiEditorEventKind::PolyAftertouch)
        );
        assert!(
            MidiEditorEventTypeFilter::ChannelPressure
                .matches(MidiEditorEventKind::ChannelPressure)
        );
        assert!(MidiEditorEventTypeFilter::SysExMeta.matches(MidiEditorEventKind::SysEx));
        assert!(MidiEditorEventTypeFilter::SysExMeta.matches(MidiEditorEventKind::Text));
        assert_eq!(MidiEditorEventTypeFilter::OPTIONS.len(), 8);
        assert_eq!(
            MidiEditorEventTypeFilter::OPTIONS[0].to_string(),
            "<all events>"
        );
        assert_eq!(
            MidiEditorEventTypeFilter::OPTIONS[2].to_string(),
            "Poly Aftertouch"
        );
        assert_eq!(
            MidiEditorEventTypeFilter::OPTIONS[3].to_string(),
            "Control Change (CC)"
        );
        assert_eq!(
            MidiEditorEventTypeFilter::OPTIONS[4].to_string(),
            "Program Change (PC)"
        );
        assert_eq!(
            MidiEditorEventTypeFilter::OPTIONS[5].to_string(),
            "Channel Pressure"
        );
        assert_eq!(MidiEditorEventTypeFilter::OPTIONS[6].to_string(), "Pitch");
        assert_eq!(
            MidiEditorEventTypeFilter::OPTIONS[7].to_string(),
            "Sysex/Meta"
        );

        let selection = MidiEditorChannelSelection::Multichannel;
        let selected_channel_note = (1, MidiEditorEventKind::Note);
        let selected_channel_cc = (1, MidiEditorEventKind::ControlChange);
        let other_channel_note = (2, MidiEditorEventKind::Note);
        let matches_filter = |(channel, kind), inverted| {
            selection.shows_event(
                channel,
                true,
                0b0010,
                inverted,
                MidiEditorEventTypeFilter::Notes,
                kind,
            )
        };
        assert!(matches_filter(selected_channel_note, false));
        assert!(!matches_filter(selected_channel_cc, false));
        assert!(!matches_filter(other_channel_note, false));
        assert!(!matches_filter(selected_channel_note, true));
        assert!(matches_filter(selected_channel_cc, true));
        assert!(matches_filter(other_channel_note, true));

        assert!(!MidiEditorChannelSelection::AllChannels.shows_event(
            0,
            true,
            u16::MAX,
            true,
            MidiEditorEventTypeFilter::Notes,
            MidiEditorEventKind::Note,
        ));
        assert!(MidiEditorChannelSelection::AllChannels.shows_event(
            0,
            true,
            u16::MAX,
            true,
            MidiEditorEventTypeFilter::Notes,
            MidiEditorEventKind::Pitch,
        ));
    }
}
