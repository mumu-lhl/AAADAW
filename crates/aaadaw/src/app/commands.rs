use super::{App, MainMenu, Message, PathPickerTarget, WorkspacePage};
use aaadaw_core::TrackId;
use iced::Task;
#[cfg(feature = "jack-backend")]
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandId {
    OpenProject,
    SaveProject,
    SaveProjectAs,
    Undo,
    Redo,
    Workspace(WorkspacePage),
    AddMidiItem,
    ImportAudio,
    DuplicateSelectedAudioItem,
    DeleteSelectedItems,
    AddTrack,
    SelectedTrack(TrackCommand),
    Track {
        track_id: TrackId,
        command: TrackCommand,
    },
    #[cfg(feature = "jack-backend")]
    TogglePlayback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrackCommand {
    Rename,
    ToggleMute,
    ToggleSolo,
    MoveUp,
    MoveDown,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandKind {
    OpenProject,
    SaveProject,
    SaveProjectAs,
    Undo,
    Redo,
    Workspace(WorkspacePage),
    AddMidiItem,
    ImportAudio,
    DuplicateSelectedAudioItem,
    DeleteSelectedItems,
    AddTrack,
    Track(TrackCommand),
    #[cfg(feature = "jack-backend")]
    TogglePlayback,
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

#[derive(Clone, Copy)]
enum Shortcut {
    Command(char),
    CommandShift(char),
    #[cfg(feature = "jack-backend")]
    Space,
}

impl Shortcut {
    fn label(self) -> &'static str {
        match self {
            Self::Command('o') => "Ctrl/Cmd+O",
            Self::Command('s') => "Ctrl/Cmd+S",
            Self::Command('z') => "Ctrl/Cmd+Z",
            Self::Command('y') => "Ctrl/Cmd+Y",
            Self::CommandShift('z') => "Ctrl/Cmd+Shift+Z",
            #[cfg(feature = "jack-backend")]
            Self::Space => "Space",
            Self::Command(_) | Self::CommandShift(_) => "",
        }
    }

    fn matches(self, key: &Key<&str>, modifiers: Modifiers) -> bool {
        match self {
            Self::Command(character) => {
                modifiers == Modifiers::COMMAND && key_matches_character(key, character)
            }
            Self::CommandShift(character) => {
                modifiers == (Modifiers::COMMAND | Modifiers::SHIFT)
                    && key_matches_character(key, character)
            }
            #[cfg(feature = "jack-backend")]
            Self::Space => modifiers == Modifiers::NONE && *key == Key::Named(Named::Space),
        }
    }
}

const OPEN_SHORTCUT: &[Shortcut] = &[Shortcut::Command('o')];
const SAVE_SHORTCUT: &[Shortcut] = &[Shortcut::Command('s')];
const UNDO_SHORTCUT: &[Shortcut] = &[Shortcut::Command('z')];
const REDO_SHORTCUT: &[Shortcut] = &[Shortcut::CommandShift('z'), Shortcut::Command('y')];
#[cfg(feature = "jack-backend")]
const PLAYBACK_SHORTCUT: &[Shortcut] = &[Shortcut::Space];

const COMMANDS: &[CommandDefinition] = &[
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
        label: "Save project as…",
        aliases: &["save project as", "save project as…", "save as", "save as…"],
        shortcuts: &[],
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
        kind: CommandKind::Workspace(WorkspacePage::Arrangement),
        menu: Some(MainMenu::View),
        category: "View",
        label: "Arrangement",
        aliases: &["view arrangement"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Workspace(WorkspacePage::Media),
        menu: Some(MainMenu::View),
        category: "View",
        label: "Media",
        aliases: &["view media"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::Workspace(WorkspacePage::Project),
        menu: Some(MainMenu::View),
        category: "View",
        label: "Project",
        aliases: &["view project"],
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
        kind: CommandKind::DuplicateSelectedAudioItem,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Duplicate selected audio item",
        aliases: &[],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::DeleteSelectedItems,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Delete selected items",
        aliases: &["delete selected item", "delete items"],
        shortcuts: &[],
        destructive: true,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::AddTrack,
        menu: Some(MainMenu::Track),
        category: "Track",
        label: "Add track",
        aliases: &["create track"],
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
    #[cfg(feature = "jack-backend")]
    CommandDefinition {
        kind: CommandKind::TogglePlayback,
        menu: None,
        category: "Transport",
        label: "Play/stop",
        aliases: &["play", "stop", "toggle playback"],
        shortcuts: PLAYBACK_SHORTCUT,
        destructive: false,
        separator_before: false,
    },
];

pub(super) struct CommandEntry {
    pub(super) id: CommandId,
    pub(super) category: &'static str,
    pub(super) label: String,
    pub(super) shortcut: Option<String>,
    pub(super) enabled: bool,
    pub(super) destructive: bool,
    pub(super) separator_before: bool,
    aliases: &'static [&'static str],
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

pub(super) fn for_menu(app: &App, menu: MainMenu) -> Vec<CommandEntry> {
    entries(app)
        .into_iter()
        .filter(|entry| entry.section == Some(menu))
        .map(|entry| entry.command)
        .collect()
}

pub(super) fn for_actions_menu(app: &App) -> Vec<CommandEntry> {
    entries(app)
        .into_iter()
        .map(|entry| entry.command)
        .collect()
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

pub(super) fn from_shortcut(key: &Key<&str>, modifiers: Modifiers) -> Option<CommandId> {
    COMMANDS.iter().find_map(|definition| {
        definition
            .shortcuts
            .iter()
            .any(|shortcut| shortcut.matches(key, modifiers))
            .then(|| command_id(definition.kind))
    })
}

pub(super) fn dispatch(app: &mut App, command: CommandId) -> Task<Message> {
    let message = match command {
        CommandId::OpenProject => Message::OpenProject,
        CommandId::SaveProject => Message::SaveProject,
        CommandId::SaveProjectAs => Message::PickPath(PathPickerTarget::SaveProject),
        CommandId::Undo => Message::Undo,
        CommandId::Redo => Message::Redo,
        CommandId::Workspace(page) => Message::SelectWorkspace(page),
        CommandId::AddMidiItem => Message::AddMidiItem,
        CommandId::ImportAudio => Message::PickPath(PathPickerTarget::ImportAudioToProject),
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
        CommandId::DeleteSelectedItems => Message::DeleteSelectedItems,
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
                TrackCommand::Rename => Message::BeginTrackNameEdit(track_id),
                TrackCommand::ToggleMute => Message::ToggleMute(track_id),
                TrackCommand::ToggleSolo => Message::ToggleSolo(track_id),
                TrackCommand::MoveUp => Message::MoveTrack(track_id, -1),
                TrackCommand::MoveDown => Message::MoveTrack(track_id, 1),
                TrackCommand::Delete => Message::DeleteTrack(track_id),
            }
        }
        #[cfg(feature = "jack-backend")]
        CommandId::TogglePlayback => Message::TogglePlayback,
    };
    app.update(message)
}

struct ResolvedEntry {
    command: CommandEntry,
    section: Option<MainMenu>,
}

fn entries(app: &App) -> Vec<ResolvedEntry> {
    let selected_track = app
        .selected_track_id()
        .and_then(|track_id| track_state(app, track_id));
    COMMANDS
        .iter()
        .map(|definition| {
            let command_id = match definition.kind {
                CommandKind::Track(command) => CommandId::SelectedTrack(command),
                kind => command_id(kind),
            };
            let entry = entry_for(app, definition, command_id, selected_track);
            ResolvedEntry {
                command: entry,
                section: definition.menu,
            }
        })
        .collect()
}

#[derive(Clone, Copy)]
struct TrackState {
    index: usize,
    muted: bool,
    solo: bool,
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
        _ => definition.label,
    };
    CommandEntry {
        id,
        category: definition.category,
        label: label.to_owned(),
        shortcut: (!definition.shortcuts.is_empty()).then(|| {
            definition
                .shortcuts
                .iter()
                .map(|shortcut| shortcut.label())
                .collect::<Vec<_>>()
                .join(", ")
        }),
        enabled: command_enabled(app, definition.kind, track),
        destructive: definition.destructive,
        separator_before: definition.separator_before,
        aliases: definition.aliases,
    }
}

fn command_enabled(app: &App, kind: CommandKind, track: Option<TrackState>) -> bool {
    match kind {
        CommandKind::OpenProject => !project_edit_busy(app) && !app.is_dirty(),
        CommandKind::SaveProject => !project_file_busy(app),
        CommandKind::SaveProjectAs | CommandKind::Undo | CommandKind::Redo => {
            !project_edit_busy(app)
        }
        CommandKind::Workspace(_) => true,
        CommandKind::AddMidiItem => !project_edit_busy(app) && !app.project.tracks().is_empty(),
        CommandKind::ImportAudio => {
            !project_edit_busy(app)
                && !app.project.tracks().is_empty()
                && app.project_path.is_some()
        }
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
        CommandKind::DeleteSelectedItems => {
            !project_edit_busy(app) && !app.timeline.selected_items.is_empty()
        }
        CommandKind::AddTrack => !project_edit_busy(app),
        CommandKind::Track(command) => {
            !project_edit_busy(app)
                && track.is_some_and(|track| match command {
                    TrackCommand::Rename
                    | TrackCommand::ToggleMute
                    | TrackCommand::ToggleSolo
                    | TrackCommand::Delete => true,
                    TrackCommand::MoveUp => track.index > 0,
                    TrackCommand::MoveDown => track.index + 1 < app.project.tracks().len(),
                })
        }
        #[cfg(feature = "jack-backend")]
        CommandKind::TogglePlayback => true,
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
            solo: track.is_solo(),
        })
}

fn command_id(kind: CommandKind) -> CommandId {
    match kind {
        CommandKind::OpenProject => CommandId::OpenProject,
        CommandKind::SaveProject => CommandId::SaveProject,
        CommandKind::SaveProjectAs => CommandId::SaveProjectAs,
        CommandKind::Undo => CommandId::Undo,
        CommandKind::Redo => CommandId::Redo,
        CommandKind::Workspace(page) => CommandId::Workspace(page),
        CommandKind::AddMidiItem => CommandId::AddMidiItem,
        CommandKind::ImportAudio => CommandId::ImportAudio,
        CommandKind::DuplicateSelectedAudioItem => CommandId::DuplicateSelectedAudioItem,
        CommandKind::DeleteSelectedItems => CommandId::DeleteSelectedItems,
        CommandKind::AddTrack => CommandId::AddTrack,
        CommandKind::Track(command) => CommandId::SelectedTrack(command),
        #[cfg(feature = "jack-backend")]
        CommandKind::TogglePlayback => CommandId::TogglePlayback,
    }
}

fn project_edit_busy(app: &App) -> bool {
    app.io_busy
        || app.path_picker_busy
        || app.import_busy
        || app.audio_asset_management_busy
        || app.playback_busy()
        || app.playback_active()
}

fn project_file_busy(app: &App) -> bool {
    app.io_busy || app.path_picker_busy || app.import_busy || app.audio_asset_management_busy
}

fn key_matches_character(key: &Key<&str>, expected: char) -> bool {
    matches!(key, Key::Character(character) if character.eq_ignore_ascii_case(&expected.to_string()))
}
