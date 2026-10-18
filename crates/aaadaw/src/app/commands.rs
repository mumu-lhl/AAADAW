use super::action_macros::ActionMacro;
use super::shortcut::{Shortcut, parse_bindings, serialize_bindings};
use super::{App, MainMenu, MainWorkspace, Message, PathPickerTarget};
use crate::timeline::{DeleteFocusTarget, HoveredItemSelectionMode, TimelineEvent};
use aaadaw_core::TrackId;
use iced::Task;
use iced::keyboard::key::{Code, Named};
use iced::keyboard::{Key, Modifiers};
use std::collections::HashMap;

pub(crate) type ShortcutBindings = HashMap<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandId {
    NewProject,
    Quit,
    OpenProject,
    SaveProject,
    SaveProjectAs,
    SaveProjectNewVersion,
    SaveAllProjects,
    OpenProjectTemplates,
    OpenProjectTemplate(usize),
    SaveProjectAsTemplate,
    ShowProjectTemplatesPath,
    ProjectTemplateHeader,
    NoProjectTemplates,
    OpenRecentProjects,
    OpenRecentProject(usize),
    CloseProject,
    CloseAllProjects,
    ExportWav,
    ExportProjectMidi,
    OpenRenderQueue,
    CancelOfflineRender,
    OpenSettings,
    OpenAbout,
    OpenProjectSettings,
    OpenSnapGridSettings,
    ToggleSwingGrid,
    OpenActionList,
    Undo,
    Redo,
    PasteItems,
    ToggleMediaBrowserPanel,
    ToggleOfflineJobsPanel,
    ShowArrangement,
    ShowMixer,
    OpenMarkerManager,
    OpenRegionRenderMatrix,
    ToggleProjectMarkersOnRuler,
    ToggleProjectRegionsOnRuler,
    ToggleProjectRegionsAndMarkersOnRuler,
    ToggleMixerPanel,
    AddMidiItem,
    ToggleMidiInlineEditors,
    CloseMidiInlineEditors,
    MidiNotesUpSemitone,
    MidiNotesDownSemitone,
    MidiNotesUpOctave,
    MidiNotesDownOctave,
    MidiNotesUpSemitoneIgnoringScale,
    MidiNotesDownSemitoneIgnoringScale,
    MidiTransposeNotes,
    MidiEditorPianoRoll,
    MidiEditorEventList,
    MidiEventProperties,
    AddProjectMarker,
    InsertRegionFromTimeSelection,
    DeleteProjectRegionNearCursor,
    EditProjectRegionNearCursor,
    LoopTime(LoopTimeCommand),
    PreviousProjectMarker,
    NextProjectMarker,
    DeleteProjectMarkerNearCursor,
    ToggleSelectedProjectMarkers,
    ToggleSelectedProjectRegions,
    ToggleSelectedProjectRegionsAndMarkers,
    MoveProjectMarkerSlot(u8),
    ImportMidiFile,
    ImportAudio,
    OpenItemProperties,
    ToggleItemProperties,
    ToggleMuteSelectedItems,
    OpenItemNudgeDialog,
    CloseItemNudgeDialog,
    NudgeItemsLeft,
    NudgeItemsRight,
    SelectAllItems,
    SelectAllItemsInTrack,
    SelectAllItemsInTimeSelection,
    SelectAllItemsOnSelectedTracksInTimeSelection,
    UnselectAllItems,
    InvertItemSelection,
    SelectNextAdjacentNonOverlappingItem,
    SelectPreviousAdjacentNonOverlappingItem,
    SelectItemOnPreviousTrack,
    SelectItemOnNextTrack,
    MoveEditCursorToPreviousItemEdge,
    MoveEditCursorToNextItemEdge,
    MoveEditCursorToStartOfItems,
    MoveEditCursorToEndOfItems,
    SnapSelectedItemsLeft,
    SnapSelectedItemsRight,
    SnapSelectedItemsToNearestPoint,
    SelectItemUnderMouseCursor,
    AddItemUnderMouseToSelection,
    ToggleSelectionOfItemUnderMouse,
    DuplicateSelectedItem,
    DuplicateSelectedAudioItem,
    DuplicateSelectedMidiItem,
    DeleteSelectedItems,
    DeleteByFocus,
    DeleteByFocusNoPrompting,
    TrimItemsToSelectedArea,
    CopyLoopOfSelectedArea,
    EncloseMediaItemsInRazorEdit,
    SplitSelectedItemsAtCursor,
    SplitSelectedItemsAtTimeSelection,
    HealSplitsInItems,
    AddTrack,
    SelectedTrack(TrackCommand),
    Track {
        track_id: TrackId,
        command: TrackCommand,
    },
    #[cfg(feature = "audio-device")]
    TogglePlayback,
    #[cfg(feature = "audio-device")]
    ToggleRepeat,
    #[cfg(feature = "audio-device")]
    TogglePlayStop,
    #[cfg(feature = "audio-device")]
    StopPlayback,
    #[cfg(feature = "audio-device")]
    PanicMidi,
    Macro(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrackCommand {
    Routing,
    ToggleFolder,
    CycleFolderCompact,
    Rename,
    ToggleMute,
    TogglePhase,
    ToggleSolo,
    ToggleRecordArm,
    MoveUp,
    MoveDown,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopTimeCommand {
    SetStartAtCursor,
    SetEndAtCursor,
    GoToStart,
    GoToEnd,
    ToggleLinked,
    MoveLoopToTimeSelection,
    MoveTimeSelectionToLoop,
    RemoveLoopIfUnlinked,
    RemoveLoop,
    RemoveTimeSelection,
    RemoveTimeSelectionAndLoop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandKind {
    NewProject,
    Quit,
    OpenProject,
    SaveProject,
    SaveProjectAs,
    SaveProjectNewVersion,
    SaveAllProjects,
    OpenProjectTemplates,
    SaveProjectAsTemplate,
    OpenRecentProjects,
    CloseProject,
    CloseAllProjects,
    ExportWav,
    ExportProjectMidi,
    OpenRenderQueue,
    CancelOfflineRender,
    OpenSettings,
    OpenAbout,
    OpenProjectSettings,
    OpenSnapGridSettings,
    ToggleSwingGrid,
    OpenActionList,
    Undo,
    Redo,
    PasteItems,
    ToggleMediaBrowserPanel,
    ToggleOfflineJobsPanel,
    ShowArrangement,
    ShowMixer,
    OpenMarkerManager,
    OpenRegionRenderMatrix,
    ToggleProjectMarkersOnRuler,
    ToggleProjectRegionsOnRuler,
    ToggleProjectRegionsAndMarkersOnRuler,
    ToggleMixerPanel,
    AddMidiItem,
    ToggleMidiInlineEditors,
    CloseMidiInlineEditors,
    MidiNotesUpSemitone,
    MidiNotesDownSemitone,
    MidiNotesUpOctave,
    MidiNotesDownOctave,
    MidiNotesUpSemitoneIgnoringScale,
    MidiNotesDownSemitoneIgnoringScale,
    MidiTransposeNotes,
    MidiEditorPianoRoll,
    MidiEditorEventList,
    MidiEventProperties,
    AddProjectMarker,
    InsertRegionFromTimeSelection,
    DeleteProjectRegionNearCursor,
    EditProjectRegionNearCursor,
    LoopTime(LoopTimeCommand),
    PreviousProjectMarker,
    NextProjectMarker,
    DeleteProjectMarkerNearCursor,
    ToggleSelectedProjectMarkers,
    ToggleSelectedProjectRegions,
    ToggleSelectedProjectRegionsAndMarkers,
    MoveProjectMarkerSlot(u8),
    ImportMidiFile,
    ImportAudio,
    OpenItemProperties,
    ToggleItemProperties,
    ToggleMuteSelectedItems,
    OpenItemNudgeDialog,
    CloseItemNudgeDialog,
    NudgeItemsLeft,
    NudgeItemsRight,
    SelectAllItems,
    SelectAllItemsInTrack,
    SelectAllItemsInTimeSelection,
    SelectAllItemsOnSelectedTracksInTimeSelection,
    UnselectAllItems,
    InvertItemSelection,
    SelectNextAdjacentNonOverlappingItem,
    SelectPreviousAdjacentNonOverlappingItem,
    SelectItemOnPreviousTrack,
    SelectItemOnNextTrack,
    MoveEditCursorToPreviousItemEdge,
    MoveEditCursorToNextItemEdge,
    MoveEditCursorToStartOfItems,
    MoveEditCursorToEndOfItems,
    SnapSelectedItemsLeft,
    SnapSelectedItemsRight,
    SnapSelectedItemsToNearestPoint,
    SelectItemUnderMouseCursor,
    AddItemUnderMouseToSelection,
    ToggleSelectionOfItemUnderMouse,
    DuplicateSelectedItem,
    DuplicateSelectedAudioItem,
    DuplicateSelectedMidiItem,
    DeleteSelectedItems,
    DeleteByFocus,
    DeleteByFocusNoPrompting,
    TrimItemsToSelectedArea,
    CopyLoopOfSelectedArea,
    EncloseMediaItemsInRazorEdit,
    SplitSelectedItemsAtCursor,
    SplitSelectedItemsAtTimeSelection,
    HealSplitsInItems,
    AddTrack,
    Track(TrackCommand),
    #[cfg(feature = "audio-device")]
    TogglePlayback,
    #[cfg(feature = "audio-device")]
    ToggleRepeat,
    #[cfg(feature = "audio-device")]
    TogglePlayStop,
    #[cfg(feature = "audio-device")]
    StopPlayback,
    #[cfg(feature = "audio-device")]
    PanicMidi,
}

#[derive(Clone, Copy)]
struct CommandDefinition {
    kind: CommandKind,
    menu: Option<MainMenu>,
    category: &'static str,
    label: &'static str,
    aliases: &'static [&'static str],
    shortcuts: &'static [Shortcut],
    destructive: bool,
    separator_before: bool,
}

const ACTION_LIST_SHORTCUT: &[Shortcut] = &[Shortcut::character('/', Modifiers::SHIFT)];
const ABOUT_SHORTCUT: &[Shortcut] = &[Shortcut::named(Named::F1, Modifiers::CTRL)];
const ADD_TRACK_SHORTCUT: &[Shortcut] = &[Shortcut::character('t', Modifiers::COMMAND)];
const OPEN_SHORTCUT: &[Shortcut] = &[Shortcut::character('o', Modifiers::COMMAND)];
const NEW_PROJECT_SHORTCUT: &[Shortcut] = &[Shortcut::character('n', Modifiers::COMMAND)];
const QUIT_SHORTCUT: &[Shortcut] = &[Shortcut::character('q', Modifiers::CTRL)];
const CLOSE_PROJECT_SHORTCUT: &[Shortcut] = &[Shortcut::named(Named::F4, Modifiers::CTRL)];
const SAVE_SHORTCUT: &[Shortcut] = &[Shortcut::character('s', Modifiers::COMMAND)];
const SAVE_PROJECT_AS_SHORTCUT: &[Shortcut] = &[Shortcut::character(
    's',
    Modifiers::CTRL.union(Modifiers::ALT),
)];
const SAVE_PROJECT_NEW_VERSION_SHORTCUT: &[Shortcut] = &[Shortcut::character(
    's',
    Modifiers::CTRL
        .union(Modifiers::ALT)
        .union(Modifiers::SHIFT),
)];
const RENDER_PROJECT_SHORTCUT: &[Shortcut] = &[Shortcut::character(
    'r',
    Modifiers::CTRL.union(Modifiers::ALT),
)];
const PREFERENCES_SHORTCUT: &[Shortcut] = &[Shortcut::character('p', Modifiers::COMMAND)];
const UNDO_SHORTCUT: &[Shortcut] = &[Shortcut::character('z', Modifiers::COMMAND)];
const PASTE_ITEMS_SHORTCUT: &[Shortcut] = &[Shortcut::character('v', Modifiers::CTRL)];
const REDO_SHORTCUT: &[Shortcut] = &[
    Shortcut::character('z', Modifiers::COMMAND.union(Modifiers::SHIFT)),
    Shortcut::character('y', Modifiers::COMMAND),
];
const DUPLICATE_ITEM_SHORTCUT: &[Shortcut] = &[Shortcut::character('d', Modifiers::COMMAND)];
const DELETE_BY_FOCUS_SHORTCUT: &[Shortcut] = &[Shortcut::named(Named::Delete, Modifiers::NONE)];
const COPY_LOOP_OF_SELECTED_AREA_SHORTCUT: &[Shortcut] = &[Shortcut::character(
    'l',
    Modifiers::CTRL.union(Modifiers::ALT),
)];
const SPLIT_ITEMS_SHORTCUT: &[Shortcut] = &[Shortcut::character('s', Modifiers::NONE)];
const SPLIT_ITEMS_AT_TIME_SELECTION_SHORTCUT: &[Shortcut] =
    &[Shortcut::character('s', Modifiers::SHIFT)];
const ADD_PROJECT_MARKER_SHORTCUT: &[Shortcut] = &[Shortcut::character('m', Modifiers::NONE)];
const INSERT_REGION_FROM_TIME_SELECTION_SHORTCUT: &[Shortcut] =
    &[Shortcut::character('r', Modifiers::SHIFT)];
const REMOVE_TIME_SELECTION_AND_LOOP_SHORTCUT: &[Shortcut] =
    &[Shortcut::named(Named::Escape, Modifiers::NONE)];
const SET_LOOP_START_SHORTCUT: &[Shortcut] = &[Shortcut::character('{', Modifiers::SHIFT)];
const SET_LOOP_END_SHORTCUT: &[Shortcut] = &[Shortcut::character('}', Modifiers::SHIFT)];
const PREVIOUS_PROJECT_MARKER_SHORTCUT: &[Shortcut] = &[Shortcut::character('[', Modifiers::NONE)];
const NEXT_PROJECT_MARKER_SHORTCUT: &[Shortcut] = &[Shortcut::character(']', Modifiers::NONE)];
const PROJECT_MARKER_DELETE_RADIUS_SECONDS: f64 = 1.0;
const PROJECT_MARKER_SLOT_1_SHORTCUT: &[Shortcut] =
    &[Shortcut::standard_character('1', Modifiers::CTRL)];
const PROJECT_MARKER_SLOT_2_SHORTCUT: &[Shortcut] =
    &[Shortcut::standard_character('2', Modifiers::CTRL)];
const PROJECT_MARKER_SLOT_3_SHORTCUT: &[Shortcut] =
    &[Shortcut::standard_character('3', Modifiers::CTRL)];
const PROJECT_MARKER_SLOT_4_SHORTCUT: &[Shortcut] =
    &[Shortcut::standard_character('4', Modifiers::CTRL)];
const PROJECT_MARKER_SLOT_5_SHORTCUT: &[Shortcut] =
    &[Shortcut::standard_character('5', Modifiers::CTRL)];
const PROJECT_MARKER_SLOT_6_SHORTCUT: &[Shortcut] =
    &[Shortcut::standard_character('6', Modifiers::CTRL)];
const PROJECT_MARKER_SLOT_7_SHORTCUT: &[Shortcut] =
    &[Shortcut::standard_character('7', Modifiers::CTRL)];
const PROJECT_MARKER_SLOT_8_SHORTCUT: &[Shortcut] =
    &[Shortcut::standard_character('8', Modifiers::CTRL)];
const PROJECT_MARKER_SLOT_9_SHORTCUT: &[Shortcut] =
    &[Shortcut::standard_character('9', Modifiers::CTRL)];
const PROJECT_MARKER_SLOT_10_SHORTCUT: &[Shortcut] =
    &[Shortcut::standard_character('0', Modifiers::CTRL)];
const MARKER_MANAGER_SHORTCUT: &[Shortcut] = &[Shortcut::character(
    'r',
    Modifiers::CTRL
        .union(Modifiers::ALT)
        .union(Modifiers::SHIFT),
)];
#[cfg(feature = "audio-device")]
const PLAYBACK_SHORTCUT: &[Shortcut] = &[
    Shortcut::named(Named::Enter, Modifiers::NONE),
    Shortcut::named(Named::Space, Modifiers::CTRL),
];
#[cfg(feature = "audio-device")]
const PLAY_STOP_SHORTCUT: &[Shortcut] = &[Shortcut::named(Named::Space, Modifiers::NONE)];
#[cfg(feature = "audio-device")]
const STOP_PLAYBACK_SHORTCUT: &[Shortcut] = &[];
const MIDI_NOTES_UP_SEMITONE_SHORTCUT: &[Shortcut] = &[
    Shortcut::numpad(Code::Numpad8, Modifiers::NONE),
    Shortcut::named(Named::ArrowUp, Modifiers::CTRL),
];
const MIDI_NOTES_DOWN_SEMITONE_SHORTCUT: &[Shortcut] = &[
    Shortcut::numpad(Code::Numpad2, Modifiers::NONE),
    Shortcut::named(Named::ArrowDown, Modifiers::CTRL),
];
const MIDI_NOTES_UP_OCTAVE_SHORTCUT: &[Shortcut] =
    &[Shortcut::numpad(Code::Numpad8, Modifiers::ALT)];
const MIDI_NOTES_DOWN_OCTAVE_SHORTCUT: &[Shortcut] =
    &[Shortcut::numpad(Code::Numpad2, Modifiers::ALT)];
const MIDI_NOTES_UP_SEMITONE_IGNORE_SCALE_SHORTCUT: &[Shortcut] = &[
    Shortcut::numpad(Code::Numpad8, Modifiers::CTRL.union(Modifiers::ALT)),
    Shortcut::named(
        Named::ArrowUp,
        Modifiers::CTRL
            .union(Modifiers::ALT)
            .union(Modifiers::SHIFT),
    ),
];
const MIDI_NOTES_DOWN_SEMITONE_IGNORE_SCALE_SHORTCUT: &[Shortcut] = &[
    Shortcut::numpad(Code::Numpad2, Modifiers::CTRL.union(Modifiers::ALT)),
    Shortcut::named(
        Named::ArrowDown,
        Modifiers::CTRL
            .union(Modifiers::ALT)
            .union(Modifiers::SHIFT),
    ),
];
const MIDI_TRANSPOSE_NOTES_SHORTCUT: &[Shortcut] = &[Shortcut::character('t', Modifiers::NONE)];
const MIDI_EDITOR_PIANO_ROLL_SHORTCUT: &[Shortcut] = &[Shortcut::character('1', Modifiers::ALT)];
const MIDI_EDITOR_EVENT_LIST_SHORTCUT: &[Shortcut] = &[Shortcut::character('3', Modifiers::ALT)];
const MIDI_INLINE_EDITORS_SHORTCUT: &[Shortcut] = &[Shortcut::character('e', Modifiers::NONE)];
const MIDI_EVENT_PROPERTIES_SHORTCUT: &[Shortcut] = &[Shortcut::named(Named::F2, Modifiers::CTRL)];

const COMMANDS: &[CommandDefinition] = &[
    CommandDefinition {
        kind: CommandKind::NewProject,
        menu: Some(MainMenu::File),
        category: "File",
        label: "New project",
        aliases: &["new", "new project"],
        shortcuts: NEW_PROJECT_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::OpenProject,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Open project…",
        aliases: &["open project", "open project…"],
        shortcuts: OPEN_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SaveProject,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Save project",
        aliases: &["save"],
        shortcuts: SAVE_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SaveProjectAs,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Save project as...",
        aliases: &[
            "save project as",
            "save project as...",
            "save project as…",
            "save as",
            "save as...",
            "save as…",
        ],
        shortcuts: SAVE_PROJECT_AS_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SaveProjectNewVersion,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Save new version of project (automatically increment project name)",
        aliases: &[
            "save new version of project",
            "save new version",
            "increment project version",
        ],
        shortcuts: SAVE_PROJECT_NEW_VERSION_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SaveAllProjects,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Save all projects",
        aliases: &["save all projects", "save all"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::OpenProjectTemplates,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Project templates",
        aliases: &["project templates", "open project template"],
        shortcuts: &[],
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::OpenRecentProjects,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Recent projects",
        aliases: &["recent projects", "open recent project"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SaveProjectAsTemplate,
        menu: None,
        category: "File",
        label: "Save project as template...",
        aliases: &["save project as template", "save as template"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::CloseProject,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Close current project tab",
        aliases: &["close project", "close current project"],
        shortcuts: CLOSE_PROJECT_SHORTCUT,
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::CloseAllProjects,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Close all projects",
        aliases: &["close all projects", "close all"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ExportWav,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Render project to disk...",
        aliases: &["render", "export audio", "bounce project", "render wav"],
        shortcuts: RENDER_PROJECT_SHORTCUT,
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::OpenRenderQueue,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Open render queue",
        aliases: &["open render queue", "queued renders", "render queue"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ExportProjectMidi,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Export project MIDI...",
        aliases: &["export project midi", "export midi", "midi export"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::CancelOfflineRender,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Cancel active offline job",
        aliases: &["cancel render", "cancel export"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::OpenProjectSettings,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Project settings...",
        aliases: &["project settings", "video frame rate"],
        shortcuts: &[Shortcut::named(Named::Enter, Modifiers::ALT)],
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::Quit,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Quit REAPER",
        aliases: &["quit", "quit reaper"],
        shortcuts: QUIT_SHORTCUT,
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::OpenSettings,
        menu: Some(MainMenu::Options),
        category: "Options",
        label: "Preferences...",
        aliases: &["settings", "preferences", "open settings"],
        shortcuts: PREFERENCES_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::OpenSnapGridSettings,
        menu: Some(MainMenu::Options),
        category: "Grid",
        label: "Snap/grid settings...",
        aliases: &["snap grid settings", "grid settings", "swing grid"],
        shortcuts: &[Shortcut::character('l', Modifiers::ALT)],
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::ToggleSwingGrid,
        menu: None,
        category: "Grid",
        label: "Grid: Toggle swing grid",
        aliases: &["toggle swing", "swing grid toggle"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::OpenAbout,
        menu: Some(MainMenu::Help),
        category: "Help",
        label: "About AAADAW...",
        aliases: &["about", "about aaadaw", "version"],
        shortcuts: ABOUT_SHORTCUT,
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::OpenActionList,
        menu: Some(MainMenu::Actions),
        category: "Actions",
        label: "Show action list...",
        aliases: &["actions", "action list", "find shortcut"],
        shortcuts: ACTION_LIST_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Undo,
        menu: Some(MainMenu::Edit),
        category: "Edit",
        label: "Undo",
        aliases: &[],
        shortcuts: UNDO_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Redo,
        menu: Some(MainMenu::Edit),
        category: "Edit",
        label: "Redo",
        aliases: &[],
        shortcuts: REDO_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::PasteItems,
        menu: Some(MainMenu::Edit),
        category: "Edit",
        label: "Paste items/tracks",
        aliases: &["paste items", "paste tracks"],
        shortcuts: PASTE_ITEMS_SHORTCUT,
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::ToggleMediaBrowserPanel,
        menu: Some(MainMenu::View),
        category: "View",
        label: "Toggle Media Browser",
        aliases: &[
            "show media browser",
            "hide media browser",
            "toggle media panel",
        ],
        shortcuts: &[],
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::ToggleOfflineJobsPanel,
        menu: Some(MainMenu::View),
        category: "View",
        label: "Toggle Offline Jobs",
        aliases: &["show offline jobs", "hide offline jobs"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ShowArrangement,
        menu: Some(MainMenu::View),
        category: "View",
        label: "Arrange workspace",
        aliases: &["arrangement", "arrange", "show arrange"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ToggleMixerPanel,
        menu: Some(MainMenu::View),
        category: "View",
        label: "Toggle mixer visible",
        aliases: &["show mixer", "hide mixer", "mixer dock"],
        shortcuts: &[Shortcut::character('m', Modifiers::COMMAND)],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ShowMixer,
        menu: Some(MainMenu::View),
        category: "View",
        label: "Mixer workspace",
        aliases: &["mixer", "mix", "show mixer"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::OpenMarkerManager,
        menu: Some(MainMenu::View),
        category: "View",
        label: "Region/Marker Manager",
        aliases: &[
            "marker manager",
            "region marker manager",
            "show marker manager",
        ],
        shortcuts: MARKER_MANAGER_SHORTCUT,
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::OpenRegionRenderMatrix,
        menu: Some(MainMenu::View),
        category: "View",
        label: "Region Render Matrix",
        aliases: &["region render matrix", "render matrix"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ToggleProjectMarkersOnRuler,
        menu: Some(MainMenu::View),
        category: "Ruler",
        label: "Ruler: Show/hide all project markers",
        aliases: &["show/hide all project markers", "toggle marker ruler"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ToggleProjectRegionsOnRuler,
        menu: Some(MainMenu::View),
        category: "Ruler",
        label: "Ruler: Show/hide all project regions",
        aliases: &["show/hide all project regions", "toggle region ruler"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ToggleProjectRegionsAndMarkersOnRuler,
        menu: None,
        category: "Ruler",
        label: "Ruler: Show/hide all project regions and markers",
        aliases: &["show/hide all project regions and markers"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::AddMidiItem,
        menu: Some(MainMenu::Insert),
        category: "Insert",
        label: "MIDI item",
        aliases: &["insert midi item", "add midi item"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::AddProjectMarker,
        menu: Some(MainMenu::Insert),
        category: "Insert",
        label: "Project marker at edit cursor",
        aliases: &["insert marker", "add marker", "marker at cursor"],
        shortcuts: ADD_PROJECT_MARKER_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::InsertRegionFromTimeSelection,
        menu: Some(MainMenu::Insert),
        category: "Regions",
        label: "Region from time selection",
        aliases: &["insert region", "create region from selection"],
        shortcuts: INSERT_REGION_FROM_TIME_SELECTION_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::DeleteProjectRegionNearCursor,
        menu: None,
        category: "Regions",
        label: "Delete region near cursor",
        aliases: &["delete region near cursor", "delete nearby region"],
        shortcuts: &[],
        destructive: true,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::EditProjectRegionNearCursor,
        menu: None,
        category: "Regions",
        label: "Edit region near cursor",
        aliases: &["edit region near cursor", "edit nearby region"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::LoopTime(LoopTimeCommand::SetStartAtCursor),
        menu: None,
        category: "Loop points",
        label: "Set loop start point at edit cursor",
        aliases: &["set loop start point", "set loop start at cursor"],
        shortcuts: SET_LOOP_START_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::LoopTime(LoopTimeCommand::SetEndAtCursor),
        menu: None,
        category: "Loop points",
        label: "Set loop end point at edit cursor",
        aliases: &["set loop end point", "set loop end at cursor"],
        shortcuts: SET_LOOP_END_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::LoopTime(LoopTimeCommand::GoToStart),
        menu: None,
        category: "Loop points",
        label: "Go to start of loop",
        aliases: &["go to loop start", "loop start"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::LoopTime(LoopTimeCommand::GoToEnd),
        menu: None,
        category: "Loop points",
        label: "Go to end of loop",
        aliases: &["go to loop end", "loop end"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::LoopTime(LoopTimeCommand::ToggleLinked),
        menu: Some(MainMenu::Options),
        category: "Loop points",
        label: "Loop points linked to time selection",
        aliases: &[
            "toggle loop points linked",
            "toggle loop points linked to time selection",
            "link loop points to time selection",
        ],
        shortcuts: &[],
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::LoopTime(LoopTimeCommand::MoveLoopToTimeSelection),
        menu: None,
        category: "Loop points",
        label: "Move loop points to time selection",
        aliases: &[
            "copy time selection to loop points",
            "set loop points from time selection",
        ],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::LoopTime(LoopTimeCommand::MoveTimeSelectionToLoop),
        menu: None,
        category: "Time selection",
        label: "Move time selection to loop points",
        aliases: &[
            "copy loop points to time selection",
            "set time selection from loop points",
        ],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::LoopTime(LoopTimeCommand::RemoveLoopIfUnlinked),
        menu: None,
        category: "Loop points",
        label: "Remove loop points if unlinked",
        aliases: &["clear loop points if unlinked"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::LoopTime(LoopTimeCommand::RemoveLoop),
        menu: None,
        category: "Loop points",
        label: "Remove loop points",
        aliases: &["clear loop points", "unselect loop points"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::LoopTime(LoopTimeCommand::RemoveTimeSelection),
        menu: None,
        category: "Time selection",
        label: "Remove time selection",
        aliases: &["clear time selection", "unselect time selection"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::LoopTime(LoopTimeCommand::RemoveTimeSelectionAndLoop),
        menu: None,
        category: "Time selection",
        label: "Remove time selection and loop points",
        aliases: &["clear time selection and loop points"],
        shortcuts: REMOVE_TIME_SELECTION_AND_LOOP_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::PreviousProjectMarker,
        menu: None,
        category: "Navigation",
        label: "Previous project marker",
        aliases: &["previous marker", "go to previous marker"],
        shortcuts: PREVIOUS_PROJECT_MARKER_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::NextProjectMarker,
        menu: None,
        category: "Navigation",
        label: "Next project marker",
        aliases: &["next marker", "go to next marker"],
        shortcuts: NEXT_PROJECT_MARKER_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::DeleteProjectMarkerNearCursor,
        menu: None,
        category: "Markers",
        label: "Delete project marker near cursor",
        aliases: &["delete marker near cursor", "delete nearby marker"],
        shortcuts: &[],
        destructive: true,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ToggleSelectedProjectMarkers,
        menu: None,
        category: "Ruler",
        label: "Ruler: Show/hide selected project markers",
        aliases: &[
            "show/hide selected project markers",
            "toggle selected marker visibility",
        ],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ToggleSelectedProjectRegions,
        menu: None,
        category: "Ruler",
        label: "Ruler: Show/hide selected project regions",
        aliases: &["show/hide selected project regions"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ToggleSelectedProjectRegionsAndMarkers,
        menu: None,
        category: "Ruler",
        label: "Ruler: Show/hide selected project regions and markers",
        aliases: &["show/hide selected project regions and markers"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveProjectMarkerSlot(1),
        menu: None,
        category: "Markers",
        label: "Add/move marker 1 to play/edit cursor",
        aliases: &["add marker 1", "move marker 1 to cursor", "marker slot 1"],
        shortcuts: PROJECT_MARKER_SLOT_1_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveProjectMarkerSlot(2),
        menu: None,
        category: "Markers",
        label: "Add/move marker 2 to play/edit cursor",
        aliases: &["add marker 2", "move marker 2 to cursor", "marker slot 2"],
        shortcuts: PROJECT_MARKER_SLOT_2_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveProjectMarkerSlot(3),
        menu: None,
        category: "Markers",
        label: "Add/move marker 3 to play/edit cursor",
        aliases: &["add marker 3", "move marker 3 to cursor", "marker slot 3"],
        shortcuts: PROJECT_MARKER_SLOT_3_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveProjectMarkerSlot(4),
        menu: None,
        category: "Markers",
        label: "Add/move marker 4 to play/edit cursor",
        aliases: &["add marker 4", "move marker 4 to cursor", "marker slot 4"],
        shortcuts: PROJECT_MARKER_SLOT_4_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveProjectMarkerSlot(5),
        menu: None,
        category: "Markers",
        label: "Add/move marker 5 to play/edit cursor",
        aliases: &["add marker 5", "move marker 5 to cursor", "marker slot 5"],
        shortcuts: PROJECT_MARKER_SLOT_5_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveProjectMarkerSlot(6),
        menu: None,
        category: "Markers",
        label: "Add/move marker 6 to play/edit cursor",
        aliases: &["add marker 6", "move marker 6 to cursor", "marker slot 6"],
        shortcuts: PROJECT_MARKER_SLOT_6_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveProjectMarkerSlot(7),
        menu: None,
        category: "Markers",
        label: "Add/move marker 7 to play/edit cursor",
        aliases: &["add marker 7", "move marker 7 to cursor", "marker slot 7"],
        shortcuts: PROJECT_MARKER_SLOT_7_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveProjectMarkerSlot(8),
        menu: None,
        category: "Markers",
        label: "Add/move marker 8 to play/edit cursor",
        aliases: &["add marker 8", "move marker 8 to cursor", "marker slot 8"],
        shortcuts: PROJECT_MARKER_SLOT_8_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveProjectMarkerSlot(9),
        menu: None,
        category: "Markers",
        label: "Add/move marker 9 to play/edit cursor",
        aliases: &["add marker 9", "move marker 9 to cursor", "marker slot 9"],
        shortcuts: PROJECT_MARKER_SLOT_9_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveProjectMarkerSlot(10),
        menu: None,
        category: "Markers",
        label: "Add/move marker 10 to play/edit cursor",
        aliases: &[
            "add marker 10",
            "move marker 10 to cursor",
            "marker slot 10",
        ],
        shortcuts: PROJECT_MARKER_SLOT_10_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ImportMidiFile,
        menu: Some(MainMenu::Insert),
        category: "Insert",
        label: "MIDI file…",
        aliases: &["import midi", "import midi file", "midi file"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ImportAudio,
        menu: Some(MainMenu::Insert),
        category: "Insert",
        label: "Import audio…",
        aliases: &["import audio", "import audio…", "audio file"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SelectAllItems,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Select all items",
        aliases: &["select all media items"],
        shortcuts: &[],
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::OpenItemNudgeDialog,
        menu: Some(MainMenu::Item),
        category: "Item editing",
        label: "Item edit: Nudge/Set...",
        aliases: &["nudge set items", "item nudge dialog"],
        shortcuts: &[Shortcut::character('n', Modifiers::NONE)],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::CloseItemNudgeDialog,
        menu: None,
        category: "Item editing",
        label: "Item edit: Close nudge/Set dialog",
        aliases: &["close nudge set dialog"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::NudgeItemsLeft,
        menu: None,
        category: "Item editing",
        label: "Item edit: Nudge left by last nudge dialog settings",
        aliases: &["nudge selected items left"],
        shortcuts: &[],
        destructive: true,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::NudgeItemsRight,
        menu: None,
        category: "Item editing",
        label: "Item edit: Nudge right by last nudge dialog settings",
        aliases: &["nudge selected items right"],
        shortcuts: &[],
        destructive: true,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ToggleItemProperties,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Item properties…",
        aliases: &["toggle media item properties"],
        shortcuts: &[Shortcut::named(Named::F2, Modifiers::NONE)],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ToggleMidiInlineEditors,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Item: Open item inline editors",
        aliases: &["open midi inline editor", "toggle midi inline editor"],
        shortcuts: MIDI_INLINE_EDITORS_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::CloseMidiInlineEditors,
        menu: None,
        category: "Item",
        label: "Item: Close item inline editors",
        aliases: &["close midi inline editors"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::OpenItemProperties,
        menu: None,
        category: "Item",
        label: "Item properties…",
        aliases: &["media item properties"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SelectAllItemsInTrack,
        menu: None,
        category: "Item",
        label: "Select all items in track",
        aliases: &["select all track items"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SelectAllItemsInTimeSelection,
        menu: None,
        category: "Item",
        label: "Select all items in current time selection",
        aliases: &["select items in time selection"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SelectAllItemsOnSelectedTracksInTimeSelection,
        menu: None,
        category: "Item",
        label: "Select all items on selected tracks in current time selection",
        aliases: &["select items on selected tracks in time selection"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::UnselectAllItems,
        menu: None,
        category: "Item",
        label: "Unselect (clear selection of) all items",
        aliases: &["unselect all items", "clear selection of all items"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::InvertItemSelection,
        menu: None,
        category: "Item",
        label: "Invert selection",
        aliases: &["invert selection", "invert item selection"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SelectNextAdjacentNonOverlappingItem,
        menu: None,
        category: "Item",
        label: "Select next adjacent non-overlapping item",
        aliases: &["select next adjacent item"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SelectPreviousAdjacentNonOverlappingItem,
        menu: None,
        category: "Item",
        label: "Select previous adjacent non-overlapping item",
        aliases: &["select previous adjacent item"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SelectItemOnPreviousTrack,
        menu: None,
        category: "Item navigation",
        label: "Select and move to item in previous track without changing track selection",
        aliases: &["select item in previous track", "previous track item"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SelectItemOnNextTrack,
        menu: None,
        category: "Item navigation",
        label: "Select and move to item in next track without changing track selection",
        aliases: &["select item in next track", "next track item"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveEditCursorToPreviousItemEdge,
        menu: None,
        category: "Item navigation",
        label: "Move cursor left to nearest item edge",
        aliases: &["move cursor left to item edge", "previous item edge"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveEditCursorToNextItemEdge,
        menu: None,
        category: "Item navigation",
        label: "Move cursor right to nearest item edge",
        aliases: &["move cursor right to item edge", "next item edge"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveEditCursorToStartOfItems,
        menu: None,
        category: "Item navigation",
        label: "Move cursor to start of items",
        aliases: &["move cursor to start of selected items", "item start"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MoveEditCursorToEndOfItems,
        menu: None,
        category: "Item navigation",
        label: "Move cursor to end of items",
        aliases: &["move cursor to end of selected items", "item end"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SnapSelectedItemsLeft,
        menu: None,
        category: "Item",
        label: "Snap items left",
        aliases: &["snap selected items left"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SnapSelectedItemsRight,
        menu: None,
        category: "Item",
        label: "Snap items right",
        aliases: &["snap selected items right"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SnapSelectedItemsToNearestPoint,
        menu: None,
        category: "Item",
        label: "Snap items to nearest snap point",
        aliases: &[
            "snap selected items to nearest snap point",
            "snap items to grid",
        ],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SelectItemUnderMouseCursor,
        menu: None,
        category: "Item",
        label: "Select item under mouse cursor",
        aliases: &["select item under mouse"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::AddItemUnderMouseToSelection,
        menu: None,
        category: "Item",
        label: "Select item under mouse cursor (leaving other items selected)",
        aliases: &["add item under mouse to selection"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ToggleSelectionOfItemUnderMouse,
        menu: None,
        category: "Item",
        label: "Toggle selection of item under mouse cursor",
        aliases: &["toggle item under mouse selection"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::DuplicateSelectedItem,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Duplicate selected items",
        aliases: &["duplicate item", "duplicate selected item"],
        shortcuts: DUPLICATE_ITEM_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ToggleMuteSelectedItems,
        menu: Some(MainMenu::Item),
        category: "Item properties",
        label: "Toggle mute",
        aliases: &[
            "mute selected items",
            "unmute selected items",
            "item properties: toggle mute",
        ],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::DuplicateSelectedAudioItem,
        menu: None,
        category: "Item",
        label: "Duplicate selected audio item",
        aliases: &[],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::DuplicateSelectedMidiItem,
        menu: None,
        category: "Item",
        label: "Duplicate selected MIDI item",
        aliases: &["duplicate midi item", "duplicate selected midi item"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::DeleteSelectedItems,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Remove items",
        aliases: &[
            "delete selected items",
            "delete selected item",
            "delete items",
        ],
        shortcuts: &[],
        destructive: true,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::DeleteByFocus,
        menu: None,
        category: "Edit",
        label: "Remove items/tracks/envelope points (depending on focus)",
        aliases: &["delete by focus", "remove focused objects"],
        shortcuts: DELETE_BY_FOCUS_SHORTCUT,
        destructive: true,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::DeleteByFocusNoPrompting,
        menu: None,
        category: "Edit",
        label: "Remove items/tracks/envelope points (depending on focus) - no prompting",
        aliases: &["delete by focus without prompting"],
        shortcuts: &[],
        destructive: true,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::TrimItemsToSelectedArea,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Trim items to selected area",
        aliases: &["trim items to time selection", "trim items to selection"],
        shortcuts: &[],
        destructive: true,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::CopyLoopOfSelectedArea,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Copy loop of selected area of audio items",
        aliases: &[
            "copy loop of selected area of items",
            "copy selected audio area as loop",
        ],
        shortcuts: COPY_LOOP_OF_SELECTED_AREA_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::EncloseMediaItemsInRazorEdit,
        menu: Some(MainMenu::Item),
        category: "Razor edit",
        label: "Enclose media items, including space between items",
        aliases: &[
            "enclose media items in razor edit",
            "enclose selected items in razor edit",
        ],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SplitSelectedItemsAtCursor,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Split items at edit or play cursor (select right)",
        aliases: &[
            "split at cursor",
            "split items at cursor",
            "split selected items at edit cursor",
        ],
        shortcuts: SPLIT_ITEMS_SHORTCUT,
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::SplitSelectedItemsAtTimeSelection,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Split items at time selection or razor edit",
        aliases: &[
            "split at time selection",
            "split items at selection",
            "split selected items at time selection",
        ],
        shortcuts: SPLIT_ITEMS_AT_TIME_SELECTION_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::HealSplitsInItems,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Heal splits in items",
        aliases: &["heal splits", "heal item splits"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::AddTrack,
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Add track",
        aliases: &["create track"],
        shortcuts: ADD_TRACK_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Track(TrackCommand::CycleFolderCompact),
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Cycle folder compact state",
        aliases: &["folder compact", "collapse folder", "expand folder"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Track(TrackCommand::ToggleFolder),
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Set track as folder",
        aliases: &["folder", "folder track"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Track(TrackCommand::Routing),
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Track routing…",
        aliases: &["routing", "track io", "send", "receive"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Track(TrackCommand::Rename),
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Rename track…",
        aliases: &["rename selected track", "rename selected track…"],
        shortcuts: &[],
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::Track(TrackCommand::ToggleMute),
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Mute track",
        aliases: &[
            "mute",
            "unmute",
            "mute selected track",
            "unmute selected track",
        ],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Track(TrackCommand::TogglePhase),
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Invert track polarity",
        aliases: &["phase", "polarity", "invert phase"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Track(TrackCommand::ToggleSolo),
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Solo track",
        aliases: &[
            "solo",
            "unsolo",
            "solo selected track",
            "unsolo selected track",
        ],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Track(TrackCommand::ToggleRecordArm),
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Arm track for recording",
        aliases: &["arm track", "record arm", "record-enable track"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Track(TrackCommand::MoveUp),
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Move track up",
        aliases: &["move selected track up"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Track(TrackCommand::MoveDown),
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Move track down",
        aliases: &["move selected track down"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Track(TrackCommand::Delete),
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Delete track",
        aliases: &["delete selected track"],
        shortcuts: &[],
        destructive: true,
        separator_before: false,
    },
    #[cfg(feature = "audio-device")]
    CommandDefinition {
        kind: CommandKind::TogglePlayback,
        menu: None,
        category: "Transport",
        label: "Play/Pause",
        aliases: &["play", "pause", "toggle playback"],
        shortcuts: PLAYBACK_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    #[cfg(feature = "audio-device")]
    CommandDefinition {
        kind: CommandKind::ToggleRepeat,
        menu: None,
        category: "Transport",
        label: "Toggle repeat",
        aliases: &["repeat", "toggle repeat on/off"],
        shortcuts: &[Shortcut::character('r', Modifiers::empty())],
        destructive: false,
        separator_before: false,
    },
    #[cfg(feature = "audio-device")]
    CommandDefinition {
        kind: CommandKind::TogglePlayStop,
        menu: None,
        category: "Transport",
        label: "Play/Stop",
        aliases: &["play stop", "toggle play stop"],
        shortcuts: PLAY_STOP_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    #[cfg(feature = "audio-device")]
    CommandDefinition {
        kind: CommandKind::StopPlayback,
        menu: None,
        category: "Transport",
        label: "Stop playback",
        aliases: &["stop", "stop transport"],
        shortcuts: STOP_PLAYBACK_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    #[cfg(feature = "audio-device")]
    CommandDefinition {
        kind: CommandKind::PanicMidi,
        menu: Some(MainMenu::Actions),
        category: "Transport",
        label: "MIDI Panic · release all notes",
        aliases: &["panic", "midi panic", "all notes off", "stuck notes"],
        shortcuts: &[],
        destructive: true,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MidiNotesUpSemitone,
        menu: None,
        category: "MIDI Editor",
        label: "Edit: Move notes up one semitone",
        aliases: &["transpose notes up one semitone", "move midi notes up"],
        shortcuts: MIDI_NOTES_UP_SEMITONE_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MidiNotesDownSemitone,
        menu: None,
        category: "MIDI Editor",
        label: "Edit: Move notes down one semitone",
        aliases: &["transpose notes down one semitone", "move midi notes down"],
        shortcuts: MIDI_NOTES_DOWN_SEMITONE_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MidiNotesUpOctave,
        menu: None,
        category: "MIDI Editor",
        label: "Edit: Move notes up one octave",
        aliases: &["transpose notes up one octave"],
        shortcuts: MIDI_NOTES_UP_OCTAVE_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MidiNotesDownOctave,
        menu: None,
        category: "MIDI Editor",
        label: "Edit: Move notes down one octave",
        aliases: &["transpose notes down one octave"],
        shortcuts: MIDI_NOTES_DOWN_OCTAVE_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MidiNotesUpSemitoneIgnoringScale,
        menu: None,
        category: "MIDI Editor",
        label: "Edit: Move notes up one semitone ignoring scale/key",
        aliases: &[
            "move notes up semitone ignoring scale",
            "chromatic transpose up",
        ],
        shortcuts: MIDI_NOTES_UP_SEMITONE_IGNORE_SCALE_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MidiNotesDownSemitoneIgnoringScale,
        menu: None,
        category: "MIDI Editor",
        label: "Edit: Move notes down one semitone ignoring scale/key",
        aliases: &[
            "move notes down semitone ignoring scale",
            "chromatic transpose down",
        ],
        shortcuts: MIDI_NOTES_DOWN_SEMITONE_IGNORE_SCALE_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MidiTransposeNotes,
        menu: None,
        category: "MIDI Editor",
        label: "Transpose notes...",
        aliases: &["transpose midi notes", "transpose notes"],
        shortcuts: MIDI_TRANSPOSE_NOTES_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MidiEditorPianoRoll,
        menu: None,
        category: "MIDI Editor",
        label: "Mode: Piano roll",
        aliases: &["piano roll mode"],
        shortcuts: MIDI_EDITOR_PIANO_ROLL_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MidiEditorEventList,
        menu: None,
        category: "MIDI Editor",
        label: "Mode: Event List",
        aliases: &["event list mode"],
        shortcuts: MIDI_EDITOR_EVENT_LIST_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::MidiEventProperties,
        menu: None,
        category: "MIDI Editor",
        label: "Edit: Event properties",
        aliases: &["midi event properties", "edit selected midi event"],
        shortcuts: MIDI_EVENT_PROPERTIES_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
];

#[derive(Clone)]
pub(super) struct CommandEntry {
    pub(super) id: CommandId,
    pub(super) category: &'static str,
    pub(super) label: String,
    pub(super) shortcut: Option<String>,
    pub(super) enabled: bool,
    pub(super) checked: bool,
    pub(super) destructive: bool,
    pub(super) separator_before: bool,
    aliases: Vec<String>,
}

pub(super) struct ShortcutEntry {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) category: String,
    pub(super) binding: String,
    pub(super) default_binding: String,
}

impl CommandEntry {
    pub(super) fn matches_query(&self, query: &str) -> bool {
        self.label.to_ascii_lowercase().contains(query)
            || self.category.to_ascii_lowercase().contains(query)
            || self
                .aliases
                .iter()
                .any(|alias| alias.to_ascii_lowercase().contains(query))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MacroStepChoice {
    pub(super) id: String,
    pub(super) label: String,
}

impl std::fmt::Display for MacroStepChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

pub(super) fn macro_step_label(id: &str) -> &str {
    COMMANDS
        .iter()
        .find(|definition| command_kind_id(definition.kind) == id)
        .map(|definition| definition.label)
        .unwrap_or(id)
}

pub(super) fn macro_id(id: u64) -> String {
    format!("macro.{id}")
}

pub(super) fn macro_step_ids() -> std::collections::HashSet<String> {
    COMMANDS
        .iter()
        .filter(|definition| macro_step_supported(definition.kind))
        .map(|definition| command_kind_id(definition.kind).to_owned())
        .collect()
}

pub(super) fn validate_action_macros(macros: Vec<ActionMacro>) -> Result<Vec<ActionMacro>, String> {
    let macros = super::action_macros::validate(macros, &macro_step_ids())?;
    for action_macro in &macros {
        if COMMANDS.iter().any(|definition| {
            definition.label.eq_ignore_ascii_case(&action_macro.name)
                || definition
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(&action_macro.name))
        }) {
            return Err(format!(
                "macro name conflicts with an existing action: {}",
                action_macro.name
            ));
        }
    }
    Ok(macros)
}

pub(super) fn macro_step_choices() -> Vec<MacroStepChoice> {
    COMMANDS
        .iter()
        .filter(|definition| macro_step_supported(definition.kind))
        .map(|definition| MacroStepChoice {
            id: command_kind_id(definition.kind).to_owned(),
            label: definition.label.to_owned(),
        })
        .collect()
}

fn macro_step_supported(kind: CommandKind) -> bool {
    matches!(
        kind,
        CommandKind::Undo
            | CommandKind::Redo
            | CommandKind::ToggleMediaBrowserPanel
            | CommandKind::ToggleOfflineJobsPanel
            | CommandKind::ShowArrangement
            | CommandKind::ShowMixer
            | CommandKind::ToggleMixerPanel
            | CommandKind::AddMidiItem
            | CommandKind::DuplicateSelectedItem
            | CommandKind::SelectAllItems
            | CommandKind::SelectAllItemsInTrack
            | CommandKind::SelectAllItemsInTimeSelection
            | CommandKind::SelectAllItemsOnSelectedTracksInTimeSelection
            | CommandKind::UnselectAllItems
            | CommandKind::InvertItemSelection
            | CommandKind::SelectNextAdjacentNonOverlappingItem
            | CommandKind::SelectPreviousAdjacentNonOverlappingItem
            | CommandKind::SelectItemOnPreviousTrack
            | CommandKind::SelectItemOnNextTrack
            | CommandKind::MoveEditCursorToPreviousItemEdge
            | CommandKind::MoveEditCursorToNextItemEdge
            | CommandKind::MoveEditCursorToStartOfItems
            | CommandKind::MoveEditCursorToEndOfItems
            | CommandKind::SnapSelectedItemsLeft
            | CommandKind::SnapSelectedItemsRight
            | CommandKind::SnapSelectedItemsToNearestPoint
            | CommandKind::SelectItemUnderMouseCursor
            | CommandKind::AddItemUnderMouseToSelection
            | CommandKind::ToggleSelectionOfItemUnderMouse
            | CommandKind::DuplicateSelectedAudioItem
            | CommandKind::DuplicateSelectedMidiItem
            | CommandKind::PasteItems
            | CommandKind::CopyLoopOfSelectedArea
            | CommandKind::EncloseMediaItemsInRazorEdit
            | CommandKind::SplitSelectedItemsAtCursor
            | CommandKind::SplitSelectedItemsAtTimeSelection
            | CommandKind::HealSplitsInItems
            | CommandKind::NudgeItemsLeft
            | CommandKind::NudgeItemsRight
            | CommandKind::AddTrack
            | CommandKind::Track(TrackCommand::TogglePhase)
            | CommandKind::Track(TrackCommand::ToggleMute)
            | CommandKind::Track(TrackCommand::ToggleSolo)
            | CommandKind::Track(TrackCommand::ToggleRecordArm)
            | CommandKind::Track(TrackCommand::MoveUp)
            | CommandKind::Track(TrackCommand::MoveDown)
    ) || {
        #[cfg(feature = "audio-device")]
        if matches!(
            kind,
            CommandKind::TogglePlayback | CommandKind::TogglePlayStop
        ) {
            return true;
        }
        false
    }
}

pub(super) fn for_menu(app: &App, menu: MainMenu) -> Vec<CommandEntry> {
    for_menu_with_recent_entries(app, menu, false)
}

pub(super) fn for_mobile_menu(app: &App, menu: MainMenu) -> Vec<CommandEntry> {
    for_menu_with_recent_entries(app, menu, true)
}

pub(super) fn recent_project_entries(app: &App) -> Vec<CommandEntry> {
    app.recent_project_paths
        .iter()
        .enumerate()
        .map(|(index, path)| CommandEntry {
            id: CommandId::OpenRecentProject(index),
            category: "File",
            label: path.display().to_string(),
            shortcut: None,
            enabled: !project_file_busy(app),
            checked: false,
            destructive: false,
            separator_before: false,
            aliases: vec![path.to_string_lossy().into_owned()],
        })
        .collect()
}

pub(super) fn project_template_entries(app: &App) -> Vec<CommandEntry> {
    let mut template_entries = Vec::with_capacity(app.project_template_paths.len() + 3);
    if let Some(save_template) = entries(app)
        .into_iter()
        .find(|entry| entry.command.id == CommandId::SaveProjectAsTemplate)
        .map(|entry| entry.command)
    {
        template_entries.push(save_template);
    }
    template_entries.push(CommandEntry {
        id: CommandId::ShowProjectTemplatesPath,
        category: "File",
        label: "Show project template path in explorer/finder".to_owned(),
        shortcut: None,
        enabled: cfg!(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "windows"
        )),
        checked: false,
        destructive: false,
        separator_before: false,
        aliases: vec!["show project template folder".to_owned()],
    });
    template_entries.push(CommandEntry {
        id: CommandId::ProjectTemplateHeader,
        category: "File",
        label: "Load project template:".to_owned(),
        shortcut: None,
        enabled: false,
        checked: false,
        destructive: false,
        separator_before: true,
        aliases: Vec::new(),
    });
    if app.project_template_paths.is_empty() {
        template_entries.push(CommandEntry {
            id: CommandId::NoProjectTemplates,
            category: "File",
            label: "(no project templates)".to_owned(),
            shortcut: None,
            enabled: false,
            checked: false,
            destructive: false,
            separator_before: false,
            aliases: Vec::new(),
        });
        return template_entries;
    }

    template_entries.extend(
        app.project_template_paths
            .iter()
            .enumerate()
            .map(|(index, path)| CommandEntry {
                id: CommandId::OpenProjectTemplate(index),
                category: "File",
                label: path.file_stem().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ),
                shortcut: None,
                enabled: !project_file_busy(app),
                checked: false,
                destructive: false,
                separator_before: false,
                aliases: vec![path.to_string_lossy().into_owned()],
            }),
    );
    template_entries
}

fn for_menu_with_recent_entries(
    app: &App,
    menu: MainMenu,
    inline_recent_entries: bool,
) -> Vec<CommandEntry> {
    let all_entries = entries(app);
    let mut menu_entries = all_entries
        .iter()
        .filter(|entry| entry.section == Some(menu))
        .map(|entry| entry.command.clone())
        .collect::<Vec<_>>();
    if inline_recent_entries && menu == MainMenu::File {
        if app.project_templates_menu_open
            && let Some(parent) = menu_entries
                .iter()
                .position(|entry| entry.id == CommandId::OpenProjectTemplates)
        {
            let templates = project_template_entries(app);
            menu_entries.splice(parent + 1..parent + 1, templates);
        }
        if app.recent_projects_menu_open
            && let Some(parent) = menu_entries
                .iter()
                .position(|entry| entry.id == CommandId::OpenRecentProjects)
        {
            let recent_entries = recent_project_entries(app)
                .into_iter()
                .map(|mut entry| {
                    entry.label = format!("  {}", entry.label);
                    entry
                })
                .collect::<Vec<_>>();
            menu_entries.splice(parent + 1..parent + 1, recent_entries);
        }
    }
    if menu == MainMenu::Item {
        if let Some(entry) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::OpenItemNudgeDialog)
        {
            entry.label = "Nudge/set items...".to_owned();
        }
        if let Some(index) = menu_entries
            .iter()
            .position(|entry| entry.id == CommandId::DeleteSelectedItems)
        {
            let remove_items = menu_entries.remove(index);
            menu_entries.insert(0, remove_items);
        }
        if let Some(index) = menu_entries
            .iter()
            .position(|entry| entry.id == CommandId::TrimItemsToSelectedArea)
        {
            let trim_items = menu_entries.remove(index);
            menu_entries.insert(1.min(menu_entries.len()), trim_items);
        }
        if let Some(index) = menu_entries
            .iter()
            .position(|entry| entry.id == CommandId::CopyLoopOfSelectedArea)
        {
            let copy_loop = menu_entries.remove(index);
            menu_entries.insert(2.min(menu_entries.len()), copy_loop);
        }
        if let Some(index) = menu_entries
            .iter()
            .position(|entry| entry.id == CommandId::EncloseMediaItemsInRazorEdit)
        {
            let enclose = menu_entries.remove(index);
            menu_entries.insert(3.min(menu_entries.len()), enclose);
        }
        if let Some(copy_loop) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::CopyLoopOfSelectedArea)
        {
            copy_loop.label = "Copy loop of selected area of items".to_owned();
        }
        if let Some(enclose) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::EncloseMediaItemsInRazorEdit)
        {
            enclose.label = "Enclose media items in razor edit".to_owned();
        }
        if let Some(select_all) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::SelectAllItems)
        {
            select_all.label = "Select all".to_owned();
        }
        if let Some(split_at_cursor) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::SplitSelectedItemsAtCursor)
        {
            split_at_cursor.label = "Split items at cursor".to_owned();
        }
        if let Some(split_at_time_selection) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::SplitSelectedItemsAtTimeSelection)
        {
            split_at_time_selection.label = "Split items at time selection".to_owned();
        }
    }
    if menu == MainMenu::Edit
        && let Some(paste) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::PasteItems)
    {
        paste.label = "Paste".to_owned();
    }
    if menu == MainMenu::File
        && let Some(quit) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::Quit)
    {
        quit.label = "Quit".to_owned();
    }
    if menu == MainMenu::File
        && let Some(close_project) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::CloseProject)
    {
        close_project.label = "Close project".to_owned();
    }
    if menu == MainMenu::File
        && let Some(save_version) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::SaveProjectNewVersion)
    {
        save_version.label = "Save new version of project".to_owned();
    }
    if menu == MainMenu::File
        && let Some(render) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::ExportWav)
    {
        render.label = "Render...".to_owned();
    }
    if menu == MainMenu::File
        && let Some(render_queue) = menu_entries
            .iter_mut()
            .find(|entry| entry.id == CommandId::OpenRenderQueue)
    {
        render_queue.label = "Queued Renders".to_owned();
    }
    if menu == MainMenu::Help
        && let Some(mut action_list) = all_entries
            .into_iter()
            .find(|entry| entry.command.id == CommandId::OpenActionList)
            .map(|entry| entry.command)
    {
        action_list.category = "Help";
        action_list.label = "Action list and shortcuts...".to_owned();
        action_list.separator_before = false;
        menu_entries.insert(0, action_list);
    }
    menu_entries
}

pub(super) fn for_actions_menu(app: &App) -> Vec<CommandEntry> {
    entries(app)
        .into_iter()
        .map(|entry| entry.command)
        .collect()
}

pub(super) fn matching_actions_menu(app: &App, query: &str) -> Vec<CommandEntry> {
    let query = query.trim().to_ascii_lowercase();
    for_actions_menu(app)
        .into_iter()
        .filter(|entry| query.is_empty() || entry.matches_query(&query))
        .collect()
}

pub(super) fn shortcut_entries(app: &App) -> Vec<ShortcutEntry> {
    let mut entries = COMMANDS
        .iter()
        .map(|definition| {
            let id = command_kind_id(definition.kind);
            let binding = if app.shortcut_defaults_restored.contains(id) {
                definition
                    .shortcuts
                    .iter()
                    .map(|shortcut| shortcut.config_label())
                    .collect::<Vec<_>>()
                    .join("; ")
            } else {
                app.shortcut_binding_edits
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| config_binding_for(app, id, definition.shortcuts))
            };
            ShortcutEntry {
                id: id.to_owned(),
                label: definition.label.to_owned(),
                category: definition.category.to_owned(),
                binding: format_shortcut_label(&binding),
                default_binding: format_shortcut_label(
                    &definition
                        .shortcuts
                        .iter()
                        .map(|shortcut| shortcut.config_label())
                        .collect::<Vec<_>>()
                        .join("; "),
                ),
            }
        })
        .collect::<Vec<_>>();
    entries.extend(app.action_macros.iter().map(|action_macro| {
        let id = macro_id(action_macro.id);
        let binding = app
            .shortcut_defaults_restored
            .contains(&id)
            .then(String::new)
            .unwrap_or_else(|| {
                app.shortcut_binding_edits
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| config_binding_for(app, &id, &[]))
            });
        ShortcutEntry {
            binding: format_shortcut_label(&binding),
            id,
            label: action_macro.name.clone(),
            category: "Macros".to_owned(),
            default_binding: String::new(),
        }
    }));
    entries
}

pub(super) fn staged_binding_for_id(app: &App, id: &str) -> String {
    let defaults = COMMANDS
        .iter()
        .find(|definition| command_kind_id(definition.kind) == id)
        .map_or(&[][..], |definition| definition.shortcuts);
    if app.shortcut_defaults_restored.contains(id) {
        return serialize_bindings(defaults);
    }
    app.shortcut_binding_edits
        .get(id)
        .cloned()
        .unwrap_or_else(|| config_binding_for(app, id, defaults))
}

fn default_may_yield(kind: CommandKind) -> bool {
    #[cfg(feature = "audio-device")]
    if matches!(
        kind,
        CommandKind::TogglePlayback | CommandKind::ToggleRepeat | CommandKind::TogglePlayStop
    ) {
        return true;
    }
    matches!(
        kind,
        CommandKind::Quit
            | CommandKind::CloseProject
            | CommandKind::OpenActionList
            | CommandKind::OpenAbout
            | CommandKind::OpenProjectSettings
            | CommandKind::OpenSnapGridSettings
            | CommandKind::OpenItemProperties
            | CommandKind::ToggleItemProperties
            | CommandKind::OpenItemNudgeDialog
            | CommandKind::AddTrack
            | CommandKind::ToggleMixerPanel
    )
}

fn effective_defaults(
    definition: &CommandDefinition,
    bindings: &ShortcutBindings,
) -> Vec<Shortcut> {
    definition
        .shortcuts
        .iter()
        .copied()
        .filter(|shortcut| {
            !default_may_yield(definition.kind)
                || !bindings
                    .values()
                    .filter_map(|value| parse_bindings(value).ok())
                    .flatten()
                    .any(|bound| shortcut.conflicts(bound))
        })
        .collect()
}

pub(super) fn binding_for_id(id: &str, bindings: &ShortcutBindings) -> String {
    bindings.get(id).cloned().unwrap_or_else(|| {
        COMMANDS
            .iter()
            .find(|definition| command_kind_id(definition.kind) == id)
            .map_or_else(String::new, |definition| {
                serialize_bindings(&effective_defaults(definition, bindings))
            })
    })
}

pub(super) fn label_for_id(app: &App, id: &str) -> Option<String> {
    COMMANDS
        .iter()
        .find(|definition| command_kind_id(definition.kind) == id)
        .map(|definition| definition.label.to_owned())
        .or_else(|| {
            app.action_macros
                .iter()
                .find(|action_macro| macro_id(action_macro.id) == id)
                .map(|action_macro| action_macro.name.clone())
        })
}

#[cfg(test)]
pub(super) fn validate_bindings(bindings: &ShortcutBindings) -> Result<ShortcutBindings, String> {
    validate_bindings_with_macros(bindings, &[])
}

pub(super) fn validate_bindings_with_macros(
    bindings: &ShortcutBindings,
    macros: &[ActionMacro],
) -> Result<ShortcutBindings, String> {
    const RETIRED_WORKSPACE_IDS: &[&str] = &["view.arrangement", "view.media", "view.project"];
    let mut bindings = bindings.clone();
    for id in RETIRED_WORKSPACE_IDS {
        bindings.remove(*id);
    }
    for id in bindings.keys() {
        if !COMMANDS
            .iter()
            .any(|definition| command_kind_id(definition.kind) == id)
            && !macros
                .iter()
                .any(|action_macro| macro_id(action_macro.id) == *id)
        {
            return Err(format!("unknown action ID: {id}"));
        }
    }
    let mut resolved = Vec::<(Shortcut, String)>::new();
    let mut normalized_bindings = ShortcutBindings::new();
    for definition in COMMANDS {
        let id = command_kind_id(definition.kind);
        let Some(value) = bindings.get(id) else {
            continue;
        };
        insert_normalized_binding(id, value, true, &mut resolved, &mut normalized_bindings)?;
    }
    for action_macro in macros {
        let id = macro_id(action_macro.id);
        let Some(value) = bindings.get(&id) else {
            continue;
        };
        insert_normalized_binding(&id, value, false, &mut resolved, &mut normalized_bindings)?;
    }
    for definition in COMMANDS {
        let id = command_kind_id(definition.kind);
        if bindings.contains_key(id) {
            continue;
        }
        for shortcut in definition.shortcuts {
            // Adding a new factory binding must not invalidate older custom maps.
            if resolved.iter().any(|(bound, bound_id)| {
                shortcut.conflicts(*bound)
                    && (default_may_yield(definition.kind)
                        || (bindings.contains_key(bound_id)
                            && COMMANDS
                                .iter()
                                .any(|candidate| command_kind_id(candidate.kind) == bound_id)))
            }) {
                continue;
            }
            let allow_reserved_default = matches!(
                definition.kind,
                CommandKind::LoopTime(LoopTimeCommand::RemoveTimeSelectionAndLoop)
            );
            register_shortcut(*shortcut, id, &mut resolved, allow_reserved_default)?;
        }
    }
    Ok(normalized_bindings)
}

fn register_shortcut(
    shortcut: Shortcut,
    id: &str,
    resolved: &mut Vec<(Shortcut, String)>,
    allow_reserved_default: bool,
) -> Result<(), String> {
    if shortcut.is_reserved() && !allow_reserved_default {
        return Err("Escape is reserved for cancelling the current operation".to_owned());
    }
    if let Some((_, other_id)) = resolved
        .iter()
        .find(|(other, other_id)| other_id != id && shortcut.conflicts(*other))
    {
        return Err(format!(
            "{} is already assigned to both {other_id} and {id}",
            shortcut.config_label()
        ));
    }
    resolved.push((shortcut, id.to_owned()));
    Ok(())
}

fn insert_normalized_binding(
    id: &str,
    value: &str,
    allow_action_conflicts: bool,
    resolved: &mut Vec<(Shortcut, String)>,
    normalized_bindings: &mut ShortcutBindings,
) -> Result<(), String> {
    let shortcuts = parse_bindings(value)?;
    for shortcut in &shortcuts {
        if shortcut.is_reserved() {
            return Err("Escape is reserved for cancelling the current operation".to_owned());
        }
        if let Some((_, other_id)) = resolved.iter().find(|(other, other_id)| {
            shortcut.conflicts(*other) && (!allow_action_conflicts || other_id == id)
        }) {
            return Err(format!(
                "{} is already assigned to both {other_id} and {id}",
                shortcut.config_label()
            ));
        }
        resolved.push((*shortcut, id.to_owned()));
    }
    normalized_bindings.insert(id.to_owned(), serialize_bindings(&shortcuts));
    Ok(())
}

pub(super) fn for_track_context(app: &App, track_id: TrackId) -> Vec<CommandEntry> {
    let Some(track) = track_state(app, track_id) else {
        return Vec::new();
    };
    COMMANDS
        .iter()
        .filter_map(|definition| {
            let CommandKind::Track(command) = definition.kind else {
                return None;
            };
            Some(entry_for(
                app,
                definition,
                CommandId::Track { track_id, command },
                Some(track),
            ))
        })
        .collect()
}

pub(super) fn is_enabled(app: &App, command: CommandId) -> bool {
    match command {
        CommandId::OpenProjectTemplate(index) => {
            index < app.project_template_paths.len() && !project_file_busy(app)
        }
        CommandId::ShowProjectTemplatesPath => {
            cfg!(any(
                target_os = "linux",
                target_os = "macos",
                target_os = "windows"
            ))
        }
        CommandId::ProjectTemplateHeader | CommandId::NoProjectTemplates => false,
        CommandId::OpenRecentProject(index) => {
            index < app.recent_project_paths.len() && !project_file_busy(app)
        }
        CommandId::Macro(id) => app
            .action_macros
            .iter()
            .any(|action_macro| action_macro.id == id),
        CommandId::SelectedTrack(track_command) => {
            let track = app
                .selected_track_id()
                .and_then(|track_id| track_state(app, track_id));
            command_enabled(app, CommandKind::Track(track_command), track)
        }
        CommandId::Track {
            track_id,
            command: track_command,
        } => command_enabled(
            app,
            CommandKind::Track(track_command),
            track_state(app, track_id),
        ),
        _ => COMMANDS
            .iter()
            .find(|definition| command_id(definition.kind) == command)
            .is_some_and(|definition| command_enabled(app, definition.kind, None)),
    }
}

pub(super) fn find(app: &App, query: &str) -> Option<CommandId> {
    let query = query.trim().to_ascii_lowercase();
    entries(app)
        .into_iter()
        .find(|entry| {
            entry.command.enabled
                && (entry.command.label.eq_ignore_ascii_case(&query)
                    || entry
                        .command
                        .aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(&query)))
        })
        .map(|entry| entry.command.id)
}

pub(super) fn from_shortcut(
    key: &Key<&str>,
    modifiers: Modifiers,
    bindings: &ShortcutBindings,
    macros: &[ActionMacro],
) -> Option<CommandId> {
    find_shortcut(
        |shortcut| shortcut.matches(key, modifiers),
        bindings,
        macros,
    )
}

pub(super) fn from_shortcut_input(
    input: &super::shortcut::ShortcutInput,
    bindings: &ShortcutBindings,
    macros: &[ActionMacro],
) -> Option<CommandId> {
    find_shortcut(|shortcut| shortcut.matches_input(input), bindings, macros)
}

fn find_shortcut(
    matches: impl Fn(&Shortcut) -> bool,
    bindings: &ShortcutBindings,
    macros: &[ActionMacro],
) -> Option<CommandId> {
    if let Some(action_macro) = macros.iter().find(|action_macro| {
        let id = macro_id(action_macro.id);
        bindings
            .get(&id)
            .and_then(|binding| parse_bindings(binding).ok())
            .is_some_and(|shortcuts| shortcuts.iter().any(&matches))
    }) {
        return Some(CommandId::Macro(action_macro.id));
    }
    COMMANDS
        .iter()
        .find_map(|definition| {
            bindings
                .get(command_kind_id(definition.kind))
                .and_then(|value| parse_bindings(value).ok())
                .is_some_and(|shortcuts| shortcuts.iter().any(&matches))
                .then(|| command_id(definition.kind))
        })
        .or_else(|| {
            COMMANDS.iter().find_map(|definition| {
                (!bindings.contains_key(command_kind_id(definition.kind))
                    && effective_defaults(definition, bindings)
                        .iter()
                        .any(&matches))
                .then(|| command_id(definition.kind))
            })
        })
}

pub(super) fn capture_binding(key: &str, modifiers: Modifiers) -> Result<String, String> {
    Shortcut::capture(key, modifiers).map(Shortcut::config_label)
}

pub(super) fn friendly_shortcut_error(error: &str, macros: &[ActionMacro]) -> String {
    let mut definitions = COMMANDS.iter().collect::<Vec<_>>();
    definitions.sort_unstable_by_key(|definition| {
        std::cmp::Reverse(command_kind_id(definition.kind).len())
    });
    let message = definitions
        .into_iter()
        .fold(error.to_owned(), |message, definition| {
            message.replace(command_kind_id(definition.kind), definition.label)
        });
    let mut macros = macros.iter().collect::<Vec<_>>();
    macros.sort_unstable_by_key(|action_macro| std::cmp::Reverse(macro_id(action_macro.id).len()));
    macros.into_iter().fold(message, |message, action_macro| {
        message.replace(&macro_id(action_macro.id), &action_macro.name)
    })
}

fn binding_for(app: &App, id: &str, defaults: &[Shortcut]) -> String {
    format_shortcut_label(&config_binding_for(app, id, defaults))
}

pub(super) fn format_shortcut_label(binding: &str) -> String {
    format_shortcut_label_for_platform(binding, cfg!(target_os = "macos"))
}

fn format_shortcut_label_for_platform(binding: &str, is_macos: bool) -> String {
    let modifier = format!("{}+", modifier_name_for_platform(is_macos));
    let platform_binding = binding
        .replace("Ctrl/Cmd+", &modifier)
        .replace("Mod+", &modifier);
    platform_binding
        .split(';')
        .map(|chord| {
            chord
                .trim()
                .split('+')
                .map(|part| part.strip_prefix("Standard").unwrap_or(part))
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn modifier_name_for_platform(is_macos: bool) -> &'static str {
    if is_macos { "Cmd" } else { "Ctrl" }
}

pub(super) fn shortcut_capture_help() -> String {
    "Select a binding, then press a character, function key, or navigation key with Ctrl, Alt, Shift, or Super. Escape cancels; unmodified Backspace clears.".to_owned()
}

fn config_binding_for(app: &App, id: &str, _defaults: &[Shortcut]) -> String {
    let custom = app
        .shortcut_bindings
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    binding_for_id(id, &custom)
}

fn command_kind_id(kind: CommandKind) -> &'static str {
    match kind {
        CommandKind::NewProject => "file.new-project",
        CommandKind::Quit => "file.quit",
        CommandKind::OpenProject => "file.open-project",
        CommandKind::SaveProject => "file.save-project",
        CommandKind::SaveProjectAs => "file.save-project-as",
        CommandKind::SaveProjectNewVersion => "file.save-project-new-version",
        CommandKind::OpenProjectTemplates => "file.project-templates",
        CommandKind::SaveProjectAsTemplate => "file.save-project-as-template",
        CommandKind::SaveAllProjects => "file.save-all-projects",
        CommandKind::OpenRecentProjects => "file.recent-projects",
        CommandKind::CloseProject => "file.close-project",
        CommandKind::CloseAllProjects => "file.close-all-projects",
        CommandKind::ExportWav => "audio.export-wav",
        CommandKind::ExportProjectMidi => "file.export-project-midi",
        CommandKind::OpenRenderQueue => "file.open-render-queue",
        CommandKind::CancelOfflineRender => "audio.cancel-render",
        CommandKind::OpenSettings => "file.settings",
        CommandKind::OpenAbout => "help.about",
        CommandKind::OpenProjectSettings => "file.project-settings",
        CommandKind::OpenSnapGridSettings => "grid.snap-settings",
        CommandKind::ToggleSwingGrid => "grid.toggle-swing",
        CommandKind::OpenActionList => "actions.show-list",
        CommandKind::Undo => "edit.undo",
        CommandKind::Redo => "edit.redo",
        CommandKind::PasteItems => "edit.paste-items",
        CommandKind::ToggleMediaBrowserPanel => "view.media-browser-panel",
        CommandKind::ToggleOfflineJobsPanel => "view.offline-jobs-panel",
        CommandKind::ShowArrangement => "view.arrangement-workspace",
        CommandKind::ShowMixer => "view.mixer-workspace",
        CommandKind::OpenMarkerManager => "view.marker-manager",
        CommandKind::OpenRegionRenderMatrix => "view.region-render-matrix",
        CommandKind::ToggleProjectMarkersOnRuler => "ruler.toggle-project-markers",
        CommandKind::ToggleProjectRegionsOnRuler => "ruler.toggle-project-regions",
        CommandKind::ToggleProjectRegionsAndMarkersOnRuler => {
            "ruler.toggle-project-regions-and-markers"
        }
        CommandKind::ToggleMixerPanel => "view.toggle-mixer",
        CommandKind::AddMidiItem => "insert.midi-item",
        CommandKind::ToggleMidiInlineEditors => "item.toggle-midi-inline-editors",
        CommandKind::CloseMidiInlineEditors => "item.close-midi-inline-editors",
        CommandKind::MidiNotesUpSemitone => "midi-editor.notes-up-semitone",
        CommandKind::MidiNotesDownSemitone => "midi-editor.notes-down-semitone",
        CommandKind::MidiNotesUpOctave => "midi-editor.notes-up-octave",
        CommandKind::MidiNotesDownOctave => "midi-editor.notes-down-octave",
        CommandKind::MidiNotesUpSemitoneIgnoringScale => {
            "midi-editor.notes-up-semitone-ignoring-scale"
        }
        CommandKind::MidiNotesDownSemitoneIgnoringScale => {
            "midi-editor.notes-down-semitone-ignoring-scale"
        }
        CommandKind::MidiTransposeNotes => "midi-editor.transpose-notes",
        CommandKind::MidiEditorPianoRoll => "midi-editor.mode-piano-roll",
        CommandKind::MidiEditorEventList => "midi-editor.mode-event-list",
        CommandKind::MidiEventProperties => "midi-editor.event-properties",
        CommandKind::AddProjectMarker => "insert.project-marker",
        CommandKind::InsertRegionFromTimeSelection => "regions.insert-from-time-selection",
        CommandKind::DeleteProjectRegionNearCursor => "regions.delete-near-cursor",
        CommandKind::EditProjectRegionNearCursor => "regions.edit-near-cursor",
        CommandKind::LoopTime(LoopTimeCommand::ToggleLinked) => "loop-time.toggle-link",
        CommandKind::LoopTime(LoopTimeCommand::SetStartAtCursor) => "loop-time.set-start",
        CommandKind::LoopTime(LoopTimeCommand::SetEndAtCursor) => "loop-time.set-end",
        CommandKind::LoopTime(LoopTimeCommand::GoToStart) => "loop-time.go-to-start",
        CommandKind::LoopTime(LoopTimeCommand::GoToEnd) => "loop-time.go-to-end",
        CommandKind::LoopTime(LoopTimeCommand::MoveLoopToTimeSelection) => {
            "loop-time.loop-to-time-selection"
        }
        CommandKind::LoopTime(LoopTimeCommand::MoveTimeSelectionToLoop) => {
            "loop-time.time-selection-to-loop"
        }
        CommandKind::LoopTime(LoopTimeCommand::RemoveLoopIfUnlinked) => {
            "loop-time.remove-loop-if-unlinked"
        }
        CommandKind::LoopTime(LoopTimeCommand::RemoveLoop) => "loop-time.remove-loop",
        CommandKind::LoopTime(LoopTimeCommand::RemoveTimeSelection) => {
            "loop-time.remove-time-selection"
        }
        CommandKind::LoopTime(LoopTimeCommand::RemoveTimeSelectionAndLoop) => {
            "loop-time.remove-time-selection-and-loop"
        }
        CommandKind::PreviousProjectMarker => "navigation.previous-project-marker",
        CommandKind::NextProjectMarker => "navigation.next-project-marker",
        CommandKind::DeleteProjectMarkerNearCursor => "markers.delete-near-cursor",
        CommandKind::ToggleSelectedProjectMarkers => "ruler.toggle-selected-project-markers",
        CommandKind::ToggleSelectedProjectRegions => "ruler.toggle-selected-project-regions",
        CommandKind::ToggleSelectedProjectRegionsAndMarkers => {
            "ruler.toggle-selected-project-regions-and-markers"
        }
        CommandKind::MoveProjectMarkerSlot(slot) => numbered_project_marker_action_id(slot),
        CommandKind::ImportMidiFile => "insert.import-midi-file",
        CommandKind::ImportAudio => "insert.import-audio",
        CommandKind::OpenItemProperties => "item.properties",
        CommandKind::ToggleItemProperties => "item.properties.toggle",
        CommandKind::ToggleMuteSelectedItems => "item.toggle-mute",
        CommandKind::OpenItemNudgeDialog => "item.nudge-set",
        CommandKind::CloseItemNudgeDialog => "item.nudge-set.close",
        CommandKind::NudgeItemsLeft => "item.nudge-left",
        CommandKind::NudgeItemsRight => "item.nudge-right",
        CommandKind::SelectAllItems => "item.select-all",
        CommandKind::SelectAllItemsInTrack => "item.select-all-in-track",
        CommandKind::SelectAllItemsInTimeSelection => "item.select-all-in-time-selection",
        CommandKind::SelectAllItemsOnSelectedTracksInTimeSelection => {
            "item.select-all-on-selected-tracks-in-time-selection"
        }
        CommandKind::UnselectAllItems => "item.unselect-all",
        CommandKind::InvertItemSelection => "item.invert-selection",
        CommandKind::SelectNextAdjacentNonOverlappingItem => "item.select-next-adjacent",
        CommandKind::SelectPreviousAdjacentNonOverlappingItem => "item.select-previous-adjacent",
        CommandKind::SelectItemOnPreviousTrack => "item.select-previous-track",
        CommandKind::SelectItemOnNextTrack => "item.select-next-track",
        CommandKind::MoveEditCursorToPreviousItemEdge => "item.navigate-cursor-left-edge",
        CommandKind::MoveEditCursorToNextItemEdge => "item.navigate-cursor-right-edge",
        CommandKind::MoveEditCursorToStartOfItems => "item.navigate-cursor-selected-start",
        CommandKind::MoveEditCursorToEndOfItems => "item.navigate-cursor-selected-end",
        CommandKind::SnapSelectedItemsLeft => "item.snap-left",
        CommandKind::SnapSelectedItemsRight => "item.snap-right",
        CommandKind::SnapSelectedItemsToNearestPoint => "item.snap-nearest",
        CommandKind::SelectItemUnderMouseCursor => "item.select-under-mouse",
        CommandKind::AddItemUnderMouseToSelection => "item.add-under-mouse-to-selection",
        CommandKind::ToggleSelectionOfItemUnderMouse => "item.toggle-under-mouse-selection",
        CommandKind::DuplicateSelectedItem => "item.duplicate",
        CommandKind::DuplicateSelectedAudioItem => "item.duplicate-audio",
        CommandKind::DuplicateSelectedMidiItem => "item.duplicate-midi",
        CommandKind::DeleteSelectedItems => "item.delete-selected",
        CommandKind::DeleteByFocus => "edit.delete-focused",
        CommandKind::DeleteByFocusNoPrompting => "edit.delete-focused-no-prompting",
        CommandKind::TrimItemsToSelectedArea => "item.trim-to-selection",
        CommandKind::CopyLoopOfSelectedArea => "item.copy-loop-selected-area",
        CommandKind::EncloseMediaItemsInRazorEdit => "item.enclose-media-in-razor-edit",
        CommandKind::SplitSelectedItemsAtCursor => "item.split-at-cursor",
        CommandKind::SplitSelectedItemsAtTimeSelection => "item.split-at-selection",
        CommandKind::HealSplitsInItems => "item.heal-splits",
        CommandKind::AddTrack => "track.add",
        CommandKind::Track(TrackCommand::Routing) => "track.routing",
        CommandKind::Track(TrackCommand::ToggleFolder) => "track.toggle-folder",
        CommandKind::Track(TrackCommand::CycleFolderCompact) => "track.cycle-folder-compact",
        CommandKind::Track(TrackCommand::Rename) => "track.rename",
        CommandKind::Track(TrackCommand::ToggleMute) => "track.toggle-mute",
        CommandKind::Track(TrackCommand::TogglePhase) => "track.toggle-phase",
        CommandKind::Track(TrackCommand::ToggleSolo) => "track.toggle-solo",
        CommandKind::Track(TrackCommand::ToggleRecordArm) => "track.toggle-record-arm",
        CommandKind::Track(TrackCommand::MoveUp) => "track.move-up",
        CommandKind::Track(TrackCommand::MoveDown) => "track.move-down",
        CommandKind::Track(TrackCommand::Delete) => "track.delete",
        #[cfg(feature = "audio-device")]
        CommandKind::TogglePlayback => "transport.toggle-playback",
        #[cfg(feature = "audio-device")]
        CommandKind::ToggleRepeat => "transport.toggle-repeat",
        #[cfg(feature = "audio-device")]
        CommandKind::TogglePlayStop => "transport.play-stop",
        #[cfg(feature = "audio-device")]
        CommandKind::StopPlayback => "transport.stop-playback",
        #[cfg(feature = "audio-device")]
        CommandKind::PanicMidi => "transport.panic-midi",
    }
}

pub(super) fn dispatch(app: &mut App, command: CommandId) -> Task<Message> {
    match command {
        CommandId::MidiTransposeNotes => {
            return app.midi_editor_item_id.map_or_else(Task::none, |item_id| {
                app.update(Message::OpenMidiTransposeDialog(item_id))
            });
        }
        CommandId::MidiEditorPianoRoll => {
            return app.midi_editor_item_id.map_or_else(Task::none, |_| {
                app.update(Message::SetMidiEditorMode(super::MidiEditorMode::PianoRoll))
            });
        }
        CommandId::MidiEditorEventList => {
            return app.midi_editor_item_id.map_or_else(Task::none, |_| {
                app.update(Message::SetMidiEditorMode(super::MidiEditorMode::EventList))
            });
        }
        CommandId::MidiEventProperties => {
            return app.midi_editor_item_id.map_or_else(Task::none, |item_id| {
                app.update(Message::OpenMidiEventProperties(item_id))
            });
        }
        _ => {}
    }
    if let CommandId::OpenProjectTemplate(index) = command {
        let Some(path) = app.project_template_paths.get(index).cloned() else {
            app.status = "Project template is no longer available".to_owned();
            return Task::none();
        };
        return app.update(Message::OpenProjectTemplate(path));
    }
    if let CommandId::OpenRecentProject(index) = command {
        let Some(path) = app.recent_project_paths.get(index).cloned() else {
            app.status = "Recent project is no longer available".to_owned();
            return Task::none();
        };
        return app.update(Message::OpenRecentProject(path));
    }
    if let CommandId::Macro(id) = command {
        let Some(action_macro) = app
            .action_macros
            .iter()
            .find(|action_macro| action_macro.id == id)
        else {
            app.status = "Action macro no longer exists".to_owned();
            return Task::none();
        };
        let steps = action_macro.steps.clone();
        let mut tasks = Vec::with_capacity(steps.len());
        for (index, step) in steps.iter().enumerate() {
            let Some(step_command) = command_for_action_id(step) else {
                app.status = format!("Macro stopped: unknown action at step {}", index + 1);
                break;
            };
            if !is_enabled(app, step_command) {
                app.status = format!(
                    "Macro stopped at step {}: {} is unavailable",
                    index + 1,
                    label_for_action_id(step).unwrap_or(step)
                );
                break;
            }
            tasks.push(dispatch(app, step_command));
        }
        return Task::batch(tasks);
    }
    let message = match command {
        CommandId::OpenProjectTemplate(_) => {
            unreachable!("project template dispatch returned above")
        }
        CommandId::ShowProjectTemplatesPath => {
            show_project_templates_path(app);
            return Task::none();
        }
        CommandId::ProjectTemplateHeader | CommandId::NoProjectTemplates => return Task::none(),
        CommandId::OpenRecentProject(_) => unreachable!("recent project dispatch returned above"),
        CommandId::NewProject => Message::NewProject,
        CommandId::CloseProject => Message::CloseProject,
        CommandId::CloseAllProjects => Message::CloseProject,
        CommandId::Quit => {
            return app.main_window_id.map_or_else(Task::none, |window_id| {
                app.update(Message::WindowCloseRequested(window_id))
            });
        }
        CommandId::OpenProject => Message::OpenProject,
        CommandId::SaveProject => Message::SaveProject,
        CommandId::SaveAllProjects => Message::SaveProject,
        CommandId::OpenProjectTemplates => Message::ToggleProjectTemplatesMenu,
        CommandId::SaveProjectAsTemplate => {
            Message::PickPath(PathPickerTarget::SaveProjectTemplate)
        }
        CommandId::OpenRecentProjects => Message::ToggleRecentProjectsMenu,
        CommandId::SaveProjectAs => Message::PickPath(PathPickerTarget::SaveProject),
        CommandId::SaveProjectNewVersion => Message::SaveProjectNewVersion,
        CommandId::ExportWav => Message::OpenRenderWindow,
        CommandId::ExportProjectMidi => Message::OpenMidiExportDialog,
        CommandId::OpenRenderQueue => Message::OpenRenderQueue,
        CommandId::CancelOfflineRender => Message::CancelOfflineRender,
        CommandId::OpenSettings => Message::OpenSettings,
        CommandId::OpenAbout => Message::OpenAbout,
        CommandId::OpenProjectSettings => Message::OpenProjectSettings,
        CommandId::OpenSnapGridSettings => Message::OpenSnapGridSettings,
        CommandId::ToggleSwingGrid => Message::ToggleSnapSwingGrid,
        CommandId::OpenActionList => Message::OpenActionList,
        CommandId::Undo => Message::Undo,
        CommandId::Redo => Message::Redo,
        CommandId::PasteItems => Message::PasteItems,
        CommandId::ToggleMediaBrowserPanel => Message::ToggleMediaBrowserPanel,
        CommandId::ToggleOfflineJobsPanel => Message::ToggleOfflineJobsPanel,
        CommandId::ShowArrangement => Message::ShowMainWorkspace(MainWorkspace::Arrangement),
        CommandId::ShowMixer => Message::ShowMainWorkspace(MainWorkspace::Mixer),
        CommandId::OpenMarkerManager => Message::OpenMarkerManager,
        CommandId::OpenRegionRenderMatrix => Message::OpenRegionRenderMatrix,
        CommandId::ToggleProjectMarkersOnRuler => Message::ToggleProjectMarkersOnRuler,
        CommandId::ToggleProjectRegionsOnRuler => Message::ToggleProjectRegionsOnRuler,
        CommandId::ToggleProjectRegionsAndMarkersOnRuler => {
            Message::ToggleProjectRegionsAndMarkersOnRuler
        }
        CommandId::ToggleMixerPanel => Message::ToggleMixerPanel,
        CommandId::AddMidiItem => Message::AddMidiItem,
        CommandId::ToggleMidiInlineEditors => Message::ToggleMidiInlineEditors,
        CommandId::CloseMidiInlineEditors => Message::CloseMidiInlineEditors,
        CommandId::MidiNotesUpSemitone | CommandId::MidiNotesUpSemitoneIgnoringScale => {
            Message::TransposeSelectedMidiNotes(1)
        }
        CommandId::MidiNotesDownSemitone | CommandId::MidiNotesDownSemitoneIgnoringScale => {
            Message::TransposeSelectedMidiNotes(-1)
        }
        CommandId::MidiNotesUpOctave => Message::TransposeSelectedMidiNotes(12),
        CommandId::MidiNotesDownOctave => Message::TransposeSelectedMidiNotes(-12),
        CommandId::MidiTransposeNotes => unreachable!("transpose dialog dispatch returned above"),
        CommandId::MidiEditorPianoRoll | CommandId::MidiEditorEventList => {
            unreachable!("MIDI Editor mode dispatch returned above")
        }
        CommandId::MidiEventProperties => {
            unreachable!("MIDI event properties dispatch returned above")
        }
        CommandId::AddProjectMarker => Message::AddProjectMarker,
        CommandId::InsertRegionFromTimeSelection => Message::InsertRegionFromTimeSelection,
        CommandId::DeleteProjectRegionNearCursor => Message::DeleteProjectRegionNearCursor,
        CommandId::EditProjectRegionNearCursor => Message::EditProjectRegionNearCursor,
        CommandId::LoopTime(command) => Message::Timeline(match command {
            LoopTimeCommand::SetStartAtCursor => TimelineEvent::SetLoopPointStartAtCursor,
            LoopTimeCommand::SetEndAtCursor => TimelineEvent::SetLoopPointEndAtCursor,
            LoopTimeCommand::GoToStart => app.timeline.loop_points.map_or(
                TimelineEvent::SetEditCursor(app.timeline.edit_cursor_tick),
                |range| TimelineEvent::SetEditCursor(range.start_tick),
            ),
            LoopTimeCommand::GoToEnd => app.timeline.loop_points.map_or(
                TimelineEvent::SetEditCursor(app.timeline.edit_cursor_tick),
                |range| TimelineEvent::SetEditCursor(range.end_tick),
            ),
            LoopTimeCommand::ToggleLinked => TimelineEvent::ToggleLoopPointsLinked,
            LoopTimeCommand::MoveLoopToTimeSelection => match app.timeline.time_selection {
                Some(selection) => TimelineEvent::SetLoopPoints {
                    start_tick: selection.start_tick,
                    end_tick: selection.end_tick,
                },
                None => TimelineEvent::ClearLoopPoints,
            },
            LoopTimeCommand::MoveTimeSelectionToLoop => {
                TimelineEvent::MoveLoopPointsToTimeSelection
            }
            LoopTimeCommand::RemoveLoopIfUnlinked => TimelineEvent::ClearLoopPointsIfUnlinked,
            LoopTimeCommand::RemoveLoop => TimelineEvent::ClearLoopPoints,
            LoopTimeCommand::RemoveTimeSelection => TimelineEvent::ClearTimeSelection,
            LoopTimeCommand::RemoveTimeSelectionAndLoop => {
                TimelineEvent::ClearTimeSelectionAndLoopPoints
            }
        }),
        CommandId::PreviousProjectMarker => Message::PreviousProjectMarker,
        CommandId::NextProjectMarker => Message::NextProjectMarker,
        CommandId::DeleteProjectMarkerNearCursor => Message::DeleteProjectMarkerNearCursor,
        CommandId::ToggleSelectedProjectMarkers => Message::ToggleSelectedProjectMarkers,
        CommandId::ToggleSelectedProjectRegions => Message::ToggleSelectedProjectRegions,
        CommandId::ToggleSelectedProjectRegionsAndMarkers => {
            Message::ToggleSelectedProjectRegionsAndMarkers
        }
        CommandId::MoveProjectMarkerSlot(slot) => Message::MoveProjectMarkerSlot(slot),
        CommandId::ImportAudio => Message::PickPath(PathPickerTarget::ImportAudioToProject),
        CommandId::ImportMidiFile => Message::PickPath(PathPickerTarget::ImportMidiToProject),
        CommandId::OpenItemProperties => Message::OpenItemProperties,
        CommandId::ToggleItemProperties => Message::ToggleItemProperties,
        CommandId::ToggleMuteSelectedItems => Message::ToggleSelectedItemsMute,
        CommandId::OpenItemNudgeDialog => Message::OpenItemNudge,
        CommandId::CloseItemNudgeDialog => Message::CloseItemNudge,
        CommandId::NudgeItemsLeft => Message::NudgeSelectedItems(-1),
        CommandId::NudgeItemsRight => Message::NudgeSelectedItems(1),
        CommandId::SelectAllItems => Message::SelectAllItems,
        CommandId::SelectAllItemsInTrack => Message::SelectAllItemsInTrack,
        CommandId::SelectAllItemsInTimeSelection => Message::SelectAllItemsInTimeSelection,
        CommandId::SelectAllItemsOnSelectedTracksInTimeSelection => {
            Message::SelectAllItemsOnSelectedTracksInTimeSelection
        }
        CommandId::UnselectAllItems => Message::UnselectAllItems,
        CommandId::InvertItemSelection => Message::InvertItemSelection,
        CommandId::SelectNextAdjacentNonOverlappingItem => Message::SelectNextAdjacentItem,
        CommandId::SelectPreviousAdjacentNonOverlappingItem => Message::SelectPreviousAdjacentItem,
        CommandId::SelectItemOnPreviousTrack => Message::SelectItemOnAdjacentTrack(false),
        CommandId::SelectItemOnNextTrack => Message::SelectItemOnAdjacentTrack(true),
        CommandId::MoveEditCursorToPreviousItemEdge => {
            Message::MoveEditCursorToNearestItemEdge(false)
        }
        CommandId::MoveEditCursorToNextItemEdge => Message::MoveEditCursorToNearestItemEdge(true),
        CommandId::MoveEditCursorToStartOfItems => {
            Message::MoveEditCursorToSelectedItemsBoundary(false)
        }
        CommandId::MoveEditCursorToEndOfItems => {
            Message::MoveEditCursorToSelectedItemsBoundary(true)
        }
        CommandId::SnapSelectedItemsLeft => Message::SnapSelectedItemsLeft,
        CommandId::SnapSelectedItemsRight => Message::SnapSelectedItemsRight,
        CommandId::SnapSelectedItemsToNearestPoint => Message::SnapSelectedItemsToNearestPoint,
        CommandId::SelectItemUnderMouseCursor => Message::Timeline(
            TimelineEvent::SelectHoveredItem(HoveredItemSelectionMode::Replace),
        ),
        CommandId::AddItemUnderMouseToSelection => Message::Timeline(
            TimelineEvent::SelectHoveredItem(HoveredItemSelectionMode::Add),
        ),
        CommandId::ToggleSelectionOfItemUnderMouse => Message::Timeline(
            TimelineEvent::SelectHoveredItem(HoveredItemSelectionMode::Toggle),
        ),
        CommandId::DuplicateSelectedItem => Message::DuplicateSelectedItems,
        CommandId::DuplicateSelectedAudioItem => {
            let selected_audio = app.timeline.selected_item.filter(|item_id| {
                app.timeline.selected_items.len() == 1
                    && app
                        .project
                        .audio_items()
                        .iter()
                        .any(|item| item.id() == *item_id)
            });
            let Some(item_id) = selected_audio else {
                app.status = "Select one audio item to duplicate".to_owned();
                return Task::none();
            };
            Message::DuplicateAudioItem(item_id)
        }
        CommandId::DuplicateSelectedMidiItem => {
            let selected_midi = app.timeline.selected_item.filter(|item_id| {
                app.timeline.selected_items.len() == 1
                    && app
                        .project
                        .midi_items()
                        .iter()
                        .any(|item| item.id() == *item_id)
            });
            let Some(item_id) = selected_midi else {
                app.status = "Select one MIDI item to duplicate".to_owned();
                return Task::none();
            };
            Message::DuplicateMidiItem(item_id)
        }
        CommandId::DeleteSelectedItems => Message::DeleteSelectedItems,
        CommandId::DeleteByFocus => Message::DeleteByFocus,
        CommandId::DeleteByFocusNoPrompting => Message::DeleteByFocusNoPrompting,
        CommandId::TrimItemsToSelectedArea => Message::TrimItemsToSelectedArea,
        CommandId::CopyLoopOfSelectedArea => Message::CopyLoopOfSelectedArea,
        CommandId::EncloseMediaItemsInRazorEdit => Message::EncloseMediaItemsInRazorEdit,
        CommandId::SplitSelectedItemsAtCursor => Message::SplitSelectedItemsAtCursor,
        CommandId::SplitSelectedItemsAtTimeSelection => Message::SplitSelectedItemsAtTimeSelection,
        CommandId::HealSplitsInItems => Message::HealSplitsInItems,
        CommandId::AddTrack => Message::AddTrack,
        CommandId::SelectedTrack(command) => {
            let Some(track_id) = app.selected_track_id() else {
                app.status = "Select a track first".to_owned();
                return Task::none();
            };
            return dispatch(app, CommandId::Track { track_id, command });
        }
        CommandId::Track { track_id, command } => {
            if !app
                .project
                .tracks()
                .iter()
                .any(|track| track.id() == track_id)
            {
                app.status = "Track no longer exists".to_owned();
                return Task::none();
            }
            match command {
                TrackCommand::Routing => Message::OpenTrackRouting(track_id),
                TrackCommand::CycleFolderCompact => {
                    Message::Timeline(crate::timeline::TimelineEvent::CycleFolderCompact(track_id))
                }
                TrackCommand::ToggleFolder => {
                    Message::RoutingChange(aaadaw_core::DawAction::SetTrackFolder {
                        track_id,
                        enabled: !app
                            .project
                            .tracks()
                            .iter()
                            .find(|track| track.id() == track_id)
                            .expect("validated track")
                            .is_folder(),
                    })
                }
                TrackCommand::Rename => Message::BeginTrackNameEdit(track_id),
                TrackCommand::ToggleMute => Message::ToggleMute(track_id),
                TrackCommand::TogglePhase => Message::TogglePhase(track_id),
                TrackCommand::ToggleSolo => Message::ToggleSolo(track_id),
                TrackCommand::ToggleRecordArm => Message::ToggleRecordArm(track_id),
                TrackCommand::MoveUp => Message::MoveTrack(track_id, -1),
                TrackCommand::MoveDown => Message::MoveTrack(track_id, 1),
                TrackCommand::Delete => Message::DeleteTrack(track_id),
            }
        }
        #[cfg(feature = "audio-device")]
        CommandId::TogglePlayback => Message::TogglePlayback,
        #[cfg(feature = "audio-device")]
        CommandId::ToggleRepeat => Message::ToggleRepeat,
        #[cfg(feature = "audio-device")]
        CommandId::TogglePlayStop => Message::TogglePlayStop,
        #[cfg(feature = "audio-device")]
        CommandId::StopPlayback => Message::StopPlayback,
        #[cfg(feature = "audio-device")]
        CommandId::PanicMidi => Message::PanicMidi,
        CommandId::Macro(_) => unreachable!("macros are dispatched before built-in commands"),
    };
    app.update(message)
}

fn show_project_templates_path(app: &mut App) {
    let Some(directory) = super::project_templates::directory() else {
        app.status = "Could not locate the project template folder".to_owned();
        return;
    };
    if let Err(error) = std::fs::create_dir_all(&directory) {
        app.status = format!("Could not create project template folder: {error}");
        return;
    }
    match open_directory(&directory) {
        Ok(()) => app.status = format!("Opened {}", directory.display()),
        Err(error) => app.status = format!("Could not open {}: {error}", directory.display()),
    }
}

#[cfg(target_os = "linux")]
fn open_directory(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn open_directory(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn open_directory(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn open_directory(_path: &std::path::Path) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "opening the project template folder is not supported on this platform",
    ))
}

fn command_for_action_id(id: &str) -> Option<CommandId> {
    COMMANDS
        .iter()
        .find(|definition| {
            macro_step_supported(definition.kind) && command_kind_id(definition.kind) == id
        })
        .map(|definition| match definition.kind {
            CommandKind::Track(command) => CommandId::SelectedTrack(command),
            kind => command_id(kind),
        })
}

fn label_for_action_id(id: &str) -> Option<&'static str> {
    COMMANDS
        .iter()
        .find(|definition| command_kind_id(definition.kind) == id)
        .map(|definition| definition.label)
}

struct ResolvedEntry {
    command: CommandEntry,
    section: Option<MainMenu>,
}

fn entries(app: &App) -> Vec<ResolvedEntry> {
    let selected_track = app
        .selected_track_id()
        .and_then(|track_id| track_state(app, track_id));
    let mut entries = COMMANDS
        .iter()
        .map(|definition| {
            let command_id = match definition.kind {
                CommandKind::Track(command) => CommandId::SelectedTrack(command),
                kind => command_id(kind),
            };
            let mut entry = entry_for(app, definition, command_id, selected_track);
            if definition.kind == CommandKind::CloseProject {
                entry.category = "";
            }
            ResolvedEntry {
                command: entry,
                section: definition.menu,
            }
        })
        .collect::<Vec<_>>();
    entries.extend(app.action_macros.iter().map(|action_macro| {
        let binding = binding_for(app, &macro_id(action_macro.id), &[]);
        ResolvedEntry {
            command: CommandEntry {
                id: CommandId::Macro(action_macro.id),
                category: "Macros",
                label: action_macro.name.clone(),
                shortcut: (!binding.is_empty()).then_some(binding),
                enabled: true,
                checked: false,
                destructive: false,
                separator_before: false,
                aliases: vec![action_macro.name.clone()],
            },
            section: None,
        }
    }));
    entries
}

#[derive(Clone, Copy)]
struct TrackState {
    index: usize,
    muted: bool,
    phase_inverted: bool,
    solo: bool,
    record_armed: bool,
    folder: bool,
}

fn entry_for(
    app: &App,
    definition: &CommandDefinition,
    id: CommandId,
    track: Option<TrackState>,
) -> CommandEntry {
    let label = match (definition.kind, track) {
        (CommandKind::Track(TrackCommand::ToggleMute), Some(track)) if track.muted => {
            "Unmute track"
        }
        (CommandKind::Track(TrackCommand::ToggleSolo), Some(track)) if track.solo => "Unsolo track",
        (CommandKind::Track(TrackCommand::ToggleFolder), Some(track)) if track.folder => {
            "Set track as normal track"
        }
        (CommandKind::Track(TrackCommand::ToggleRecordArm), Some(track)) if track.record_armed => {
            "Disarm track"
        }
        _ => definition.label,
    };
    let binding = binding_for(app, command_kind_id(definition.kind), definition.shortcuts);
    CommandEntry {
        id,
        category: definition.category,
        label: label.to_owned(),
        shortcut: (!binding.is_empty()).then_some(binding),
        enabled: command_enabled(app, definition.kind, track),
        checked: toggle_state(app, id).unwrap_or(false),
        destructive: definition.destructive,
        separator_before: definition.separator_before,
        aliases: definition
            .aliases
            .iter()
            .map(|alias| (*alias).to_owned())
            .collect(),
    }
}

fn command_enabled(app: &App, kind: CommandKind, track: Option<TrackState>) -> bool {
    match kind {
        CommandKind::NewProject
        | CommandKind::OpenProject
        | CommandKind::CloseProject
        | CommandKind::CloseAllProjects => !project_edit_busy(app),
        CommandKind::Quit => true,
        CommandKind::SaveProject | CommandKind::SaveAllProjects => !project_file_busy(app),
        CommandKind::OpenProjectTemplates | CommandKind::OpenRecentProjects => true,
        CommandKind::SaveProjectAsTemplate => {
            !cfg!(target_os = "android") && !project_file_busy(app) && !app.path_picker_busy
        }
        CommandKind::OpenSettings
        | CommandKind::OpenAbout
        | CommandKind::OpenProjectSettings
        | CommandKind::OpenSnapGridSettings
        | CommandKind::ToggleSwingGrid
        | CommandKind::OpenActionList
        | CommandKind::OpenMarkerManager => true,
        CommandKind::OpenRegionRenderMatrix => true,
        CommandKind::ToggleProjectMarkersOnRuler
        | CommandKind::ToggleProjectRegionsOnRuler
        | CommandKind::ToggleProjectRegionsAndMarkersOnRuler => !project_edit_busy(app),
        CommandKind::SaveProjectAs => !project_edit_busy(app),
        CommandKind::SaveProjectNewVersion => !project_file_busy(app),
        CommandKind::ExportWav => {
            app.media_store_path().is_some()
                && !recording_busy(app)
                && app.offline_job_submission_allowed()
        }
        CommandKind::ExportProjectMidi => !project_file_busy(app),
        CommandKind::OpenRenderQueue => true,
        CommandKind::CancelOfflineRender => app.offline_render_busy,
        CommandKind::Undo => {
            let item_selection_next = app.item_selection_is_next_undo();
            (app.can_undo_action()
                || app.track_mix_commit_at.is_some()
                || app.master_mix_commit_at.is_some())
                && history_command_enabled(
                    app,
                    item_selection_next
                        || app.project.can_undo_track_mix()
                        || app.track_mix_commit_at.is_some()
                        || app.master_mix_commit_at.is_some(),
                )
        }
        CommandKind::Redo => {
            app.can_redo_action()
                && app.track_mix_commit_at.is_none()
                && app.master_mix_commit_at.is_none()
                && history_command_enabled(
                    app,
                    app.item_selection_is_next_redo() || app.project.can_redo_track_mix(),
                )
        }
        CommandKind::PasteItems => !project_edit_busy(app) && app.can_paste_audio_items(),
        CommandKind::CopyLoopOfSelectedArea => true,
        CommandKind::EncloseMediaItemsInRazorEdit => true,
        CommandKind::ToggleMediaBrowserPanel => true,
        CommandKind::ToggleOfflineJobsPanel => true,
        CommandKind::ShowArrangement | CommandKind::ShowMixer | CommandKind::ToggleMixerPanel => {
            true
        }
        CommandKind::AddMidiItem => {
            !project_edit_busy(app)
                && app.timeline.selected_track.is_some_and(|selected_track| {
                    app.project
                        .tracks()
                        .iter()
                        .any(|track| track.id() == selected_track && !track.is_bus())
                })
        }
        CommandKind::ToggleMidiInlineEditors => {
            !project_edit_busy(app)
                && app.timeline.selected_items.iter().any(|item_id| {
                    app.project
                        .midi_items()
                        .iter()
                        .any(|item| item.id() == *item_id)
                })
        }
        CommandKind::CloseMidiInlineEditors => app
            .timeline
            .selected_items
            .iter()
            .any(|item_id| app.timeline.inline_midi_items.contains(item_id)),
        CommandKind::MidiNotesUpSemitone
        | CommandKind::MidiNotesDownSemitone
        | CommandKind::MidiNotesUpOctave
        | CommandKind::MidiNotesDownOctave
        | CommandKind::MidiNotesUpSemitoneIgnoringScale
        | CommandKind::MidiNotesDownSemitoneIgnoringScale => {
            !project_edit_busy(app)
                && app.midi_editor_item_id.is_some()
                && !app.midi_editor_selected_notes.is_empty()
        }
        CommandKind::MidiTransposeNotes => {
            !project_edit_busy(app)
                && app.midi_editor_item_id.is_some_and(|item_id| {
                    app.project
                        .midi_items()
                        .iter()
                        .any(|item| item.id() == item_id)
                })
        }
        CommandKind::MidiEditorPianoRoll | CommandKind::MidiEditorEventList => {
            !project_edit_busy(app) && app.midi_editor_item_id.is_some()
        }
        CommandKind::MidiEventProperties => {
            if project_edit_busy(app) {
                return false;
            }
            let Some(item_id) = app.midi_editor_item_id else {
                return false;
            };
            let selected_types = [
                !app.midi_editor_selected_notes.is_empty(),
                !app.midi_editor_selected_controllers.is_empty(),
                !app.midi_editor_selected_pitch_bends.is_empty(),
                !app.midi_editor_selected_program_changes.is_empty(),
                !app.midi_editor_selected_poly_aftertouches.is_empty(),
                !app.midi_editor_selected_channel_pressures.is_empty(),
                !app.midi_editor_selected_opaque_events.is_empty(),
            ];
            let selected_type_count = selected_types
                .into_iter()
                .filter(|selected| *selected)
                .count();
            if selected_type_count == 0 {
                return app.midi_editor_mode == super::MidiEditorMode::EventList
                    && app
                        .project
                        .midi_items()
                        .iter()
                        .find(|item| item.id() == item_id)
                        .is_some_and(|item| {
                            let cursors = super::view::midi_event_list_cursors(app, item);
                            cursors.contains(&super::MidiEditorEventListCursor::End)
                                && (app.midi_editor_event_list_cursor
                                    == Some(super::MidiEditorEventListCursor::End)
                                    || cursors.as_slice()
                                        == [super::MidiEditorEventListCursor::End])
                        });
            }
            if selected_type_count != 1 {
                return false;
            }
            if app.midi_editor_selected_opaque_events.is_empty() {
                true
            } else if app.midi_editor_selected_opaque_events.len() == 1 {
                app.project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .and_then(|item| {
                        app.midi_editor_selected_opaque_events
                            .iter()
                            .next()
                            .and_then(|index| item.opaque_events().get(*index as usize))
                    })
                    .is_some_and(|event| {
                        super::midi_opaque_event_properties_type_and_payload(&event.kind).is_some()
                    })
            } else {
                false
            }
        }
        CommandKind::AddProjectMarker => !project_edit_busy(app),
        CommandKind::InsertRegionFromTimeSelection => {
            !project_edit_busy(app) && app.timeline.time_selection.is_some()
        }
        CommandKind::DeleteProjectRegionNearCursor | CommandKind::EditProjectRegionNearCursor => {
            !project_edit_busy(app) && project_region_at_cursor(app).is_some()
        }
        CommandKind::LoopTime(_) => true,
        CommandKind::PreviousProjectMarker | CommandKind::NextProjectMarker => true,
        CommandKind::DeleteProjectMarkerNearCursor => {
            !project_edit_busy(app) && nearest_project_marker_to_delete(app).is_some()
        }
        CommandKind::ToggleSelectedProjectMarkers => {
            !project_edit_busy(app)
                && app
                    .project
                    .markers()
                    .iter()
                    .any(|marker| app.selected_render_markers.contains(&marker.id()))
        }
        CommandKind::ToggleSelectedProjectRegions => {
            !project_edit_busy(app)
                && app
                    .project
                    .regions()
                    .iter()
                    .any(|region| app.selected_render_regions.contains(&region.id()))
        }
        CommandKind::ToggleSelectedProjectRegionsAndMarkers => {
            !project_edit_busy(app)
                && (app
                    .project
                    .regions()
                    .iter()
                    .any(|region| app.selected_render_regions.contains(&region.id()))
                    || app
                        .project
                        .markers()
                        .iter()
                        .any(|marker| app.selected_render_markers.contains(&marker.id())))
        }
        CommandKind::MoveProjectMarkerSlot(_) => !project_edit_busy(app),
        CommandKind::ToggleItemProperties if app.item_properties_window_id.is_some() => true,
        CommandKind::OpenItemNudgeDialog => true,
        CommandKind::CloseItemNudgeDialog => app.item_nudge_window_id.is_some(),
        CommandKind::NudgeItemsLeft | CommandKind::NudgeItemsRight => !project_edit_busy(app),
        CommandKind::OpenItemProperties | CommandKind::ToggleItemProperties => {
            app.timeline.selected_item.is_some_and(|id| {
                app.project.audio_items().iter().any(|item| item.id() == id)
                    || app.project.midi_items().iter().any(|item| item.id() == id)
            })
        }
        CommandKind::DuplicateSelectedItem => {
            !project_edit_busy(app)
                && !app.timeline.selected_items.is_empty()
                && app.timeline.selected_items.iter().all(|item_id| {
                    app.project
                        .audio_items()
                        .iter()
                        .any(|item| item.id() == *item_id)
                        || app
                            .project
                            .midi_items()
                            .iter()
                            .any(|item| item.id() == *item_id)
                })
        }
        CommandKind::ToggleMuteSelectedItems => {
            !app.io_busy
                && !app.playback_busy()
                && !app.timeline.selected_items.is_empty()
                && app.timeline.selected_items.iter().all(|item_id| {
                    app.project
                        .audio_items()
                        .iter()
                        .any(|item| item.id() == *item_id)
                        || app
                            .project
                            .midi_items()
                            .iter()
                            .any(|item| item.id() == *item_id)
                })
                && (!app.timeline.selected_items.iter().any(|item_id| {
                    app.project
                        .midi_items()
                        .iter()
                        .any(|item| item.id() == *item_id)
                }) || !app.playback_active())
        }
        CommandKind::ImportAudio => {
            !project_edit_busy(app)
                && !app.project.tracks().is_empty()
                && app.media_store_path().is_some()
        }
        CommandKind::ImportMidiFile => !project_edit_busy(app),
        CommandKind::DuplicateSelectedAudioItem => {
            !project_edit_busy(app)
                && app.timeline.selected_items.len() == 1
                && app.timeline.selected_item.is_some_and(|item_id| {
                    app.project
                        .audio_items()
                        .iter()
                        .any(|item| item.id() == item_id)
                })
        }
        CommandKind::DuplicateSelectedMidiItem => {
            !project_edit_busy(app)
                && app.timeline.selected_items.len() == 1
                && app.timeline.selected_item.is_some_and(|item_id| {
                    app.project
                        .midi_items()
                        .iter()
                        .any(|item| item.id() == item_id)
                })
        }
        CommandKind::DeleteSelectedItems => {
            !project_edit_busy(app) && !app.timeline.selected_items.is_empty()
        }
        CommandKind::DeleteByFocus | CommandKind::DeleteByFocusNoPrompting => {
            if project_edit_busy(app) {
                false
            } else {
                match app.timeline.delete_focus {
                    DeleteFocusTarget::Arrange => !app.timeline.selected_items.is_empty(),
                    DeleteFocusTarget::TrackPanel => !app.selected_track_ids().is_empty(),
                    DeleteFocusTarget::Automation => app
                        .timeline
                        .selected_automation_point_delete_event()
                        .is_some(),
                }
            }
        }
        CommandKind::TrimItemsToSelectedArea => !project_edit_busy(app),
        CommandKind::SelectAllItems => true,
        CommandKind::SelectAllItemsInTrack => {
            !app.items_for_tracks(app.selected_track_ids()).is_empty()
        }
        CommandKind::SelectAllItemsInTimeSelection => {
            app.timeline.time_selection.is_some_and(|selection| {
                let track_ids = app.project.tracks().iter().map(|track| track.id());
                !app.items_in_time_selection(track_ids, selection).is_empty()
            })
        }
        CommandKind::SelectAllItemsOnSelectedTracksInTimeSelection => {
            app.timeline.time_selection.is_some_and(|selection| {
                !app.items_in_time_selection(app.selected_track_ids(), selection)
                    .is_empty()
            })
        }
        CommandKind::UnselectAllItems => !app.timeline.selected_items.is_empty(),
        CommandKind::InvertItemSelection => {
            !app.project.audio_items().is_empty() || !app.project.midi_items().is_empty()
        }
        CommandKind::SelectNextAdjacentNonOverlappingItem => {
            !app.timeline.selected_items.is_empty()
        }
        CommandKind::SelectPreviousAdjacentNonOverlappingItem => {
            !app.timeline.selected_items.is_empty()
        }
        CommandKind::SelectItemOnPreviousTrack | CommandKind::SelectItemOnNextTrack => {
            !app.timeline.selected_items.is_empty()
        }
        CommandKind::MoveEditCursorToPreviousItemEdge => app.nearest_item_edge(false).is_some(),
        CommandKind::MoveEditCursorToNextItemEdge => app.nearest_item_edge(true).is_some(),
        CommandKind::MoveEditCursorToStartOfItems | CommandKind::MoveEditCursorToEndOfItems => {
            !app.timeline.selected_items.is_empty()
        }
        CommandKind::SnapSelectedItemsLeft
        | CommandKind::SnapSelectedItemsRight
        | CommandKind::SnapSelectedItemsToNearestPoint => {
            !project_edit_busy(app) && !app.timeline.selected_items.is_empty()
        }
        CommandKind::SelectItemUnderMouseCursor
        | CommandKind::AddItemUnderMouseToSelection
        | CommandKind::ToggleSelectionOfItemUnderMouse => true,
        CommandKind::SplitSelectedItemsAtCursor => !project_edit_busy(app),
        CommandKind::SplitSelectedItemsAtTimeSelection => {
            !project_edit_busy(app) && app.can_split_selected_items_at_time_selection()
        }
        CommandKind::HealSplitsInItems => {
            !project_edit_busy(app) && !app.timeline.selected_items.is_empty()
        }
        CommandKind::AddTrack => !project_edit_busy(app),
        CommandKind::Track(TrackCommand::Routing) => track.is_some(),
        CommandKind::Track(TrackCommand::CycleFolderCompact) => {
            track.is_some_and(|track| track.folder)
        }
        CommandKind::Track(command) => {
            !project_edit_busy(app)
                && track.is_some_and(|track| match command {
                    TrackCommand::ToggleFolder => {
                        let target = &app.project.tracks()[track.index];
                        !target.is_frozen()
                            && (!target.is_folder()
                                || !app
                                    .project
                                    .tracks()
                                    .iter()
                                    .any(|candidate| candidate.parent_track() == Some(target.id())))
                    }
                    TrackCommand::Routing
                    | TrackCommand::CycleFolderCompact
                    | TrackCommand::Rename
                    | TrackCommand::TogglePhase
                    | TrackCommand::ToggleMute
                    | TrackCommand::ToggleSolo
                    | TrackCommand::ToggleRecordArm
                    | TrackCommand::Delete => true,
                    TrackCommand::MoveUp => track.index > 0,
                    TrackCommand::MoveDown => {
                        track.index
                            + app
                                .project
                                .track_subtree_len(app.project.tracks()[track.index].id())
                            < app.project.tracks().len()
                    }
                })
        }
        #[cfg(feature = "audio-device")]
        CommandKind::TogglePlayback => true,
        #[cfg(feature = "audio-device")]
        CommandKind::ToggleRepeat => true,
        #[cfg(feature = "audio-device")]
        CommandKind::TogglePlayStop => true,
        #[cfg(feature = "audio-device")]
        CommandKind::StopPlayback => true,
        #[cfg(feature = "audio-device")]
        CommandKind::PanicMidi => app.playback.is_some(),
    }
}

fn track_state(app: &App, track_id: TrackId) -> Option<TrackState> {
    app.project
        .tracks()
        .iter()
        .enumerate()
        .find(|(_, track)| track.id() == track_id)
        .map(|(index, track)| TrackState {
            index,
            muted: track.is_muted(),
            phase_inverted: track.is_phase_inverted(),
            solo: track.is_solo(),
            record_armed: track.is_record_armed(),
            folder: track.is_folder(),
        })
}

fn numbered_project_marker_action_id(slot: u8) -> &'static str {
    match slot {
        1 => "markers.add-or-move.01",
        2 => "markers.add-or-move.02",
        3 => "markers.add-or-move.03",
        4 => "markers.add-or-move.04",
        5 => "markers.add-or-move.05",
        6 => "markers.add-or-move.06",
        7 => "markers.add-or-move.07",
        8 => "markers.add-or-move.08",
        9 => "markers.add-or-move.09",
        10 => "markers.add-or-move.10",
        _ => "markers.add-or-move.invalid",
    }
}

fn command_id(kind: CommandKind) -> CommandId {
    match kind {
        CommandKind::NewProject => CommandId::NewProject,
        CommandKind::Quit => CommandId::Quit,
        CommandKind::OpenProject => CommandId::OpenProject,
        CommandKind::SaveProject => CommandId::SaveProject,
        CommandKind::SaveProjectAs => CommandId::SaveProjectAs,
        CommandKind::SaveProjectNewVersion => CommandId::SaveProjectNewVersion,
        CommandKind::OpenProjectTemplates => CommandId::OpenProjectTemplates,
        CommandKind::SaveProjectAsTemplate => CommandId::SaveProjectAsTemplate,
        CommandKind::SaveAllProjects => CommandId::SaveAllProjects,
        CommandKind::OpenRecentProjects => CommandId::OpenRecentProjects,
        CommandKind::CloseProject => CommandId::CloseProject,
        CommandKind::CloseAllProjects => CommandId::CloseAllProjects,
        CommandKind::ExportWav => CommandId::ExportWav,
        CommandKind::ExportProjectMidi => CommandId::ExportProjectMidi,
        CommandKind::OpenRenderQueue => CommandId::OpenRenderQueue,
        CommandKind::CancelOfflineRender => CommandId::CancelOfflineRender,
        CommandKind::OpenSettings => CommandId::OpenSettings,
        CommandKind::OpenAbout => CommandId::OpenAbout,
        CommandKind::OpenProjectSettings => CommandId::OpenProjectSettings,
        CommandKind::OpenSnapGridSettings => CommandId::OpenSnapGridSettings,
        CommandKind::ToggleSwingGrid => CommandId::ToggleSwingGrid,
        CommandKind::OpenActionList => CommandId::OpenActionList,
        CommandKind::Undo => CommandId::Undo,
        CommandKind::Redo => CommandId::Redo,
        CommandKind::ToggleMediaBrowserPanel => CommandId::ToggleMediaBrowserPanel,
        CommandKind::ToggleOfflineJobsPanel => CommandId::ToggleOfflineJobsPanel,
        CommandKind::ShowArrangement => CommandId::ShowArrangement,
        CommandKind::ShowMixer => CommandId::ShowMixer,
        CommandKind::OpenMarkerManager => CommandId::OpenMarkerManager,
        CommandKind::OpenRegionRenderMatrix => CommandId::OpenRegionRenderMatrix,
        CommandKind::ToggleProjectMarkersOnRuler => CommandId::ToggleProjectMarkersOnRuler,
        CommandKind::ToggleProjectRegionsOnRuler => CommandId::ToggleProjectRegionsOnRuler,
        CommandKind::ToggleProjectRegionsAndMarkersOnRuler => {
            CommandId::ToggleProjectRegionsAndMarkersOnRuler
        }
        CommandKind::ToggleMixerPanel => CommandId::ToggleMixerPanel,
        CommandKind::AddMidiItem => CommandId::AddMidiItem,
        CommandKind::ToggleMidiInlineEditors => CommandId::ToggleMidiInlineEditors,
        CommandKind::CloseMidiInlineEditors => CommandId::CloseMidiInlineEditors,
        CommandKind::MidiNotesUpSemitone => CommandId::MidiNotesUpSemitone,
        CommandKind::MidiNotesDownSemitone => CommandId::MidiNotesDownSemitone,
        CommandKind::MidiNotesUpOctave => CommandId::MidiNotesUpOctave,
        CommandKind::MidiNotesDownOctave => CommandId::MidiNotesDownOctave,
        CommandKind::MidiNotesUpSemitoneIgnoringScale => {
            CommandId::MidiNotesUpSemitoneIgnoringScale
        }
        CommandKind::MidiNotesDownSemitoneIgnoringScale => {
            CommandId::MidiNotesDownSemitoneIgnoringScale
        }
        CommandKind::MidiTransposeNotes => CommandId::MidiTransposeNotes,
        CommandKind::MidiEditorPianoRoll => CommandId::MidiEditorPianoRoll,
        CommandKind::MidiEditorEventList => CommandId::MidiEditorEventList,
        CommandKind::MidiEventProperties => CommandId::MidiEventProperties,
        CommandKind::AddProjectMarker => CommandId::AddProjectMarker,
        CommandKind::InsertRegionFromTimeSelection => CommandId::InsertRegionFromTimeSelection,
        CommandKind::DeleteProjectRegionNearCursor => CommandId::DeleteProjectRegionNearCursor,
        CommandKind::EditProjectRegionNearCursor => CommandId::EditProjectRegionNearCursor,
        CommandKind::LoopTime(command) => CommandId::LoopTime(command),
        CommandKind::PreviousProjectMarker => CommandId::PreviousProjectMarker,
        CommandKind::NextProjectMarker => CommandId::NextProjectMarker,
        CommandKind::DeleteProjectMarkerNearCursor => CommandId::DeleteProjectMarkerNearCursor,
        CommandKind::ToggleSelectedProjectMarkers => CommandId::ToggleSelectedProjectMarkers,
        CommandKind::ToggleSelectedProjectRegions => CommandId::ToggleSelectedProjectRegions,
        CommandKind::ToggleSelectedProjectRegionsAndMarkers => {
            CommandId::ToggleSelectedProjectRegionsAndMarkers
        }
        CommandKind::MoveProjectMarkerSlot(slot) => CommandId::MoveProjectMarkerSlot(slot),
        CommandKind::ImportAudio => CommandId::ImportAudio,
        CommandKind::ImportMidiFile => CommandId::ImportMidiFile,
        CommandKind::OpenItemProperties => CommandId::OpenItemProperties,
        CommandKind::ToggleItemProperties => CommandId::ToggleItemProperties,
        CommandKind::ToggleMuteSelectedItems => CommandId::ToggleMuteSelectedItems,
        CommandKind::OpenItemNudgeDialog => CommandId::OpenItemNudgeDialog,
        CommandKind::CloseItemNudgeDialog => CommandId::CloseItemNudgeDialog,
        CommandKind::NudgeItemsLeft => CommandId::NudgeItemsLeft,
        CommandKind::NudgeItemsRight => CommandId::NudgeItemsRight,
        CommandKind::SelectAllItems => CommandId::SelectAllItems,
        CommandKind::SelectAllItemsInTrack => CommandId::SelectAllItemsInTrack,
        CommandKind::SelectAllItemsInTimeSelection => CommandId::SelectAllItemsInTimeSelection,
        CommandKind::SelectAllItemsOnSelectedTracksInTimeSelection => {
            CommandId::SelectAllItemsOnSelectedTracksInTimeSelection
        }
        CommandKind::UnselectAllItems => CommandId::UnselectAllItems,
        CommandKind::InvertItemSelection => CommandId::InvertItemSelection,
        CommandKind::SelectNextAdjacentNonOverlappingItem => {
            CommandId::SelectNextAdjacentNonOverlappingItem
        }
        CommandKind::SelectPreviousAdjacentNonOverlappingItem => {
            CommandId::SelectPreviousAdjacentNonOverlappingItem
        }
        CommandKind::SelectItemOnPreviousTrack => CommandId::SelectItemOnPreviousTrack,
        CommandKind::SelectItemOnNextTrack => CommandId::SelectItemOnNextTrack,
        CommandKind::MoveEditCursorToPreviousItemEdge => {
            CommandId::MoveEditCursorToPreviousItemEdge
        }
        CommandKind::MoveEditCursorToNextItemEdge => CommandId::MoveEditCursorToNextItemEdge,
        CommandKind::MoveEditCursorToStartOfItems => CommandId::MoveEditCursorToStartOfItems,
        CommandKind::MoveEditCursorToEndOfItems => CommandId::MoveEditCursorToEndOfItems,
        CommandKind::SnapSelectedItemsLeft => CommandId::SnapSelectedItemsLeft,
        CommandKind::SnapSelectedItemsRight => CommandId::SnapSelectedItemsRight,
        CommandKind::SnapSelectedItemsToNearestPoint => CommandId::SnapSelectedItemsToNearestPoint,
        CommandKind::SelectItemUnderMouseCursor => CommandId::SelectItemUnderMouseCursor,
        CommandKind::AddItemUnderMouseToSelection => CommandId::AddItemUnderMouseToSelection,
        CommandKind::ToggleSelectionOfItemUnderMouse => CommandId::ToggleSelectionOfItemUnderMouse,
        CommandKind::DuplicateSelectedItem => CommandId::DuplicateSelectedItem,
        CommandKind::DuplicateSelectedAudioItem => CommandId::DuplicateSelectedAudioItem,
        CommandKind::DuplicateSelectedMidiItem => CommandId::DuplicateSelectedMidiItem,
        CommandKind::DeleteSelectedItems => CommandId::DeleteSelectedItems,
        CommandKind::DeleteByFocus => CommandId::DeleteByFocus,
        CommandKind::DeleteByFocusNoPrompting => CommandId::DeleteByFocusNoPrompting,
        CommandKind::TrimItemsToSelectedArea => CommandId::TrimItemsToSelectedArea,
        CommandKind::CopyLoopOfSelectedArea => CommandId::CopyLoopOfSelectedArea,
        CommandKind::EncloseMediaItemsInRazorEdit => CommandId::EncloseMediaItemsInRazorEdit,
        CommandKind::PasteItems => CommandId::PasteItems,
        CommandKind::SplitSelectedItemsAtCursor => CommandId::SplitSelectedItemsAtCursor,
        CommandKind::SplitSelectedItemsAtTimeSelection => {
            CommandId::SplitSelectedItemsAtTimeSelection
        }
        CommandKind::HealSplitsInItems => CommandId::HealSplitsInItems,
        CommandKind::AddTrack => CommandId::AddTrack,
        CommandKind::Track(command) => CommandId::SelectedTrack(command),
        #[cfg(feature = "audio-device")]
        CommandKind::TogglePlayback => CommandId::TogglePlayback,
        #[cfg(feature = "audio-device")]
        CommandKind::ToggleRepeat => CommandId::ToggleRepeat,
        #[cfg(feature = "audio-device")]
        CommandKind::TogglePlayStop => CommandId::TogglePlayStop,
        #[cfg(feature = "audio-device")]
        CommandKind::StopPlayback => CommandId::StopPlayback,
        #[cfg(feature = "audio-device")]
        CommandKind::PanicMidi => CommandId::PanicMidi,
    }
}

pub(super) fn project_edit_busy(app: &App) -> bool {
    app.io_busy
        || app.path_picker_busy
        || app.import_busy
        || app.audio_asset_management_busy
        || app.playback_busy()
        || app.playback_active()
}

pub(super) fn nearest_project_marker_to_delete(app: &App) -> Option<aaadaw_core::MarkerId> {
    let cursor_samples = match app.timeline.cursor_position() {
        crate::timeline::EditCursorPosition::Tick(tick) => {
            app.project.sample_at_tick_position(tick as f64).ok()?
        }
        crate::timeline::EditCursorPosition::Sample(sample) => sample as f64,
    };
    let radius_samples =
        f64::from(app.project.settings().sample_rate()) * PROJECT_MARKER_DELETE_RADIUS_SECONDS;
    app.project
        .markers()
        .iter()
        .filter_map(|marker| {
            let marker_samples = app
                .project
                .sample_at_tick_position(marker.position_ticks() as f64)
                .ok()?;
            let distance = (marker_samples - cursor_samples).abs();
            (distance < radius_samples).then_some((distance, marker.position_ticks(), marker.id()))
        })
        .min_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.value().cmp(&right.2.value()))
        })
        .map(|(_, _, marker_id)| marker_id)
}

/// REAPER 40615/40616 target the Region containing the edit cursor. In an
/// overlap the most recently started Region wins; equal starts prefer the
/// shorter Region, and identical bounds retain the earlier Region identity.
pub(super) fn project_region_at_cursor(app: &App) -> Option<aaadaw_core::RegionId> {
    let cursor = app.timeline.edit_cursor_tick_position(&app.project);
    app.project
        .regions()
        .iter()
        .filter(|region| {
            region.start_ticks() as f64 <= cursor && cursor < region.end_ticks() as f64
        })
        .min_by(|left, right| {
            right
                .start_ticks()
                .cmp(&left.start_ticks())
                .then_with(|| left.end_ticks().cmp(&right.end_ticks()))
                .then_with(|| left.id().value().cmp(&right.id().value()))
        })
        .map(|region| region.id())
}

fn project_file_busy(app: &App) -> bool {
    app.io_busy || app.path_picker_busy || app.import_busy || app.audio_asset_management_busy
}

fn recording_busy(_app: &App) -> bool {
    #[cfg(feature = "audio-device")]
    {
        _app.recording.is_some() || _app.recording_starting || _app.recording_stopping
    }
    #[cfg(not(feature = "audio-device"))]
    {
        false
    }
}

fn history_command_enabled(app: &App, track_mix_only: bool) -> bool {
    !app.io_busy
        && !app.path_picker_busy
        && !app.import_busy
        && !app.audio_asset_management_busy
        && !app.playback_busy()
        && (!app.playback_active() || track_mix_only)
}

#[cfg(test)]
mod shortcut_label_tests {
    use super::*;

    #[test]
    fn shortcut_labels_use_the_platform_modifier_name() {
        assert_eq!(
            format_shortcut_label_for_platform("Mod+Shift+S", true),
            "Cmd+Shift+S"
        );
        assert_eq!(
            format_shortcut_label_for_platform("Ctrl/Cmd+N", true),
            "Cmd+N"
        );
        assert_eq!(
            format_shortcut_label_for_platform("Mod+Shift+S", false),
            "Ctrl+Shift+S"
        );
        assert_eq!(
            format_shortcut_label_for_platform("Ctrl/Cmd+N", false),
            "Ctrl+N"
        );
        assert_eq!(
            format_shortcut_label_for_platform("Ctrl+StandardEnter; StandardHome", false),
            "Ctrl+Enter; Home"
        );
        assert_eq!(
            capture_binding("Space", Modifiers::SHIFT).unwrap(),
            "Shift+Space"
        );
    }
}

#[cfg(test)]
mod project_marker_command_tests {
    use super::*;

    #[test]
    fn marker_actions_use_reaper_default_keys_and_stable_action_ids() {
        let bindings = ShortcutBindings::new();
        let macros = [];
        assert_eq!(
            from_shortcut(&Key::Character("m"), Modifiers::NONE, &bindings, &macros),
            Some(CommandId::AddProjectMarker)
        );
        assert_eq!(
            from_shortcut(&Key::Character("r"), Modifiers::SHIFT, &bindings, &macros),
            Some(CommandId::InsertRegionFromTimeSelection)
        );
        assert_eq!(
            from_shortcut(&Key::Character("["), Modifiers::NONE, &bindings, &macros),
            Some(CommandId::PreviousProjectMarker)
        );
        assert_eq!(
            from_shortcut(&Key::Character("]"), Modifiers::NONE, &bindings, &macros),
            Some(CommandId::NextProjectMarker)
        );
        assert_eq!(
            command_kind_id(CommandKind::AddProjectMarker),
            "insert.project-marker"
        );
        assert_eq!(
            command_kind_id(CommandKind::InsertRegionFromTimeSelection),
            "regions.insert-from-time-selection"
        );
        assert_eq!(
            command_kind_id(CommandKind::PreviousProjectMarker),
            "navigation.previous-project-marker"
        );
        assert_eq!(
            command_kind_id(CommandKind::NextProjectMarker),
            "navigation.next-project-marker"
        );
        let manager_shortcut = Modifiers::CTRL
            .union(Modifiers::ALT)
            .union(Modifiers::SHIFT);
        assert_eq!(
            from_shortcut(&Key::Character("r"), manager_shortcut, &bindings, &macros),
            Some(CommandId::OpenMarkerManager)
        );
        assert_eq!(
            stable_id(CommandId::OpenMarkerManager).as_deref(),
            Some("view.marker-manager")
        );
        assert!(find(&App::default(), "region/marker manager").is_some());
        assert_eq!(
            stable_id(CommandId::OpenRegionRenderMatrix).as_deref(),
            Some("view.region-render-matrix")
        );
        assert!(find(&App::default(), "region render matrix").is_some());
    }

    #[test]
    fn numbered_marker_actions_use_reaper_slots_and_default_shortcuts() {
        let bindings = ShortcutBindings::new();
        let macros = [];
        let app = App::default();
        let slots = [
            (1, Key::Character("1"), "markers.add-or-move.01"),
            (2, Key::Character("2"), "markers.add-or-move.02"),
            (3, Key::Character("3"), "markers.add-or-move.03"),
            (4, Key::Character("4"), "markers.add-or-move.04"),
            (5, Key::Character("5"), "markers.add-or-move.05"),
            (6, Key::Character("6"), "markers.add-or-move.06"),
            (7, Key::Character("7"), "markers.add-or-move.07"),
            (8, Key::Character("8"), "markers.add-or-move.08"),
            (9, Key::Character("9"), "markers.add-or-move.09"),
            (10, Key::Character("0"), "markers.add-or-move.10"),
        ];
        for (slot, key, action_id) in slots {
            let command = CommandId::MoveProjectMarkerSlot(slot);
            assert_eq!(
                from_shortcut(&key, Modifiers::CTRL, &bindings, &macros),
                Some(command)
            );
            assert_eq!(
                command_kind_id(CommandKind::MoveProjectMarkerSlot(slot)),
                action_id
            );
            assert_eq!(find(&app, &format!("add marker {slot}")), Some(command));
        }
    }
}

#[cfg(test)]
mod macro_tests {
    use super::*;

    fn test_macro() -> ActionMacro {
        ActionMacro {
            id: 23,
            name: "Arrange and mix".to_owned(),
            steps: vec![
                "view.arrangement-workspace".to_owned(),
                "view.mixer-workspace".to_owned(),
            ],
        }
    }

    #[test]
    fn macro_shortcuts_validate_and_resolve_by_stable_id() {
        let macros = [test_macro()];
        let bindings = ShortcutBindings::from([("macro.23".to_owned(), "Mod+M".to_owned())]);
        let bindings = validate_bindings_with_macros(&bindings, &macros).unwrap();
        assert_eq!(bindings.get("macro.23").map(String::as_str), Some("Mod+M"));
        assert_eq!(
            from_shortcut(&Key::Character("m"), Modifiers::COMMAND, &bindings, &macros),
            Some(CommandId::Macro(23))
        );
        assert!(
            validate_bindings_with_macros(
                &ShortcutBindings::from([("macro.23".to_owned(), "Mod+Z".to_owned())]),
                &macros,
            )
            .is_err()
        );
        assert!(
            validate_bindings_with_macros(
                &ShortcutBindings::from([("macro.999".to_owned(), "Mod+M".to_owned())]),
                &macros,
            )
            .is_err()
        );
        assert!(
            validate_bindings_with_macros(
                &ShortcutBindings::from([
                    ("edit.undo".to_owned(), "Mod+M".to_owned()),
                    ("macro.23".to_owned(), "Mod+M".to_owned()),
                ]),
                &macros,
            )
            .is_err()
        );
    }

    #[test]
    fn macro_shortcuts_reject_conflicts_between_macros() {
        let first = test_macro();
        let second = ActionMacro {
            id: 24,
            name: "Mix and arrange".to_owned(),
            steps: first.steps.clone(),
        };
        let bindings = ShortcutBindings::from([
            ("macro.23".to_owned(), "Mod+M".to_owned()),
            ("macro.24".to_owned(), "Mod+M".to_owned()),
        ]);
        assert!(validate_bindings_with_macros(&bindings, &[first, second]).is_err());
    }

    #[test]
    fn macro_names_cannot_shadow_existing_action_search_names() {
        let mut action_macro = test_macro();
        action_macro.name = "save project".to_owned();
        assert!(validate_action_macros(vec![action_macro]).is_err());
    }

    #[test]
    fn destructive_actions_are_not_available_as_macro_steps() {
        assert!(!macro_step_ids().contains("item.delete-selected"));
        assert!(!macro_step_ids().contains("track.delete"));
    }
}

#[cfg(test)]
mod midi_inline_editor_command_tests {
    use super::*;

    #[test]
    fn reaper_inline_editor_action_uses_e_and_has_stable_open_close_ids() {
        let bindings = ShortcutBindings::new();
        assert_eq!(
            from_shortcut(&Key::Character("e"), Modifiers::NONE, &bindings, &[],),
            Some(CommandId::ToggleMidiInlineEditors)
        );
        assert_eq!(
            command_kind_id(CommandKind::ToggleMidiInlineEditors),
            "item.toggle-midi-inline-editors"
        );
        assert_eq!(
            command_kind_id(CommandKind::CloseMidiInlineEditors),
            "item.close-midi-inline-editors"
        );
        assert!(
            COMMANDS
                .iter()
                .any(|definition| definition.label == "Item: Open item inline editors")
        );
        assert!(
            COMMANDS
                .iter()
                .any(|definition| definition.label == "Item: Close item inline editors")
        );
    }
}

/// Stable registry identity, independent of labels and the current selection.
pub(super) fn stable_id(command: CommandId) -> Option<String> {
    if let CommandId::Macro(id) = command {
        return Some(macro_id(id));
    }
    COMMANDS
        .iter()
        .find(|definition| command_id(definition.kind) == command)
        .map(|definition| command_kind_id(definition.kind).to_owned())
}

pub(super) fn toggle_state(app: &App, command: CommandId) -> Option<bool> {
    match command {
        #[cfg(feature = "audio-device")]
        CommandId::ToggleRepeat => Some(app.repeat_enabled),
        CommandId::ToggleMediaBrowserPanel => Some(app.media_panel_dock.open),
        CommandId::ToggleOfflineJobsPanel => Some(app.offline_jobs_panel_open),
        CommandId::ToggleMixerPanel => Some(if app.is_mobile_main_window() {
            app.main_workspace == MainWorkspace::Mixer
        } else {
            app.media_panel_dock.mixer_open
        }),
        CommandId::ToggleProjectMarkersOnRuler => Some(app.project.show_project_markers()),
        CommandId::ToggleProjectRegionsOnRuler => Some(app.project.show_project_regions()),
        CommandId::ToggleProjectRegionsAndMarkersOnRuler => {
            Some(app.project.show_project_regions() && app.project.show_project_markers())
        }
        CommandId::LoopTime(LoopTimeCommand::ToggleLinked) => Some(app.timeline.loop_points_linked),
        CommandId::SelectedTrack(TrackCommand::ToggleFolder) => app
            .selected_track_id()
            .and_then(|id| track_state(app, id))
            .map(|track| track.folder),
        CommandId::SelectedTrack(TrackCommand::ToggleMute) => app
            .selected_track_id()
            .and_then(|id| track_state(app, id))
            .map(|track| track.muted),
        CommandId::SelectedTrack(TrackCommand::TogglePhase) => app
            .selected_track_id()
            .and_then(|id| track_state(app, id))
            .map(|track| track.phase_inverted),
        CommandId::SelectedTrack(TrackCommand::ToggleSolo) => app
            .selected_track_id()
            .and_then(|id| track_state(app, id))
            .map(|track| track.solo),
        CommandId::SelectedTrack(TrackCommand::ToggleRecordArm) => app
            .selected_track_id()
            .and_then(|id| track_state(app, id))
            .map(|track| track.record_armed),
        _ => None,
    }
}
