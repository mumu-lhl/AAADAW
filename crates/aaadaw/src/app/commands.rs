use super::action_macros::ActionMacro;
use super::{App, MainMenu, MainWorkspace, Message, PathPickerTarget};
use aaadaw_core::TrackId;
use iced::Task;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use std::collections::HashMap;

pub(crate) type ShortcutBindings = HashMap<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandId {
    NewProject,
    OpenProject,
    SaveProject,
    SaveProjectAs,
    ExportWav,
    CancelOfflineRender,
    OpenSettings,
    Undo,
    Redo,
    ToggleMediaBrowserPanel,
    ShowArrangement,
    ShowMixer,
    AddMidiItem,
    ImportAudio,
    DuplicateSelectedAudioItem,
    DuplicateSelectedMidiItem,
    DeleteSelectedItems,
    SplitSelectedItemsAtCursor,
    SplitSelectedItemsAtTimeSelection,
    AddTrack,
    SelectedTrack(TrackCommand),
    Track {
        track_id: TrackId,
        command: TrackCommand,
    },
    #[cfg(feature = "audio-device")]
    TogglePlayback,
    #[cfg(feature = "audio-device")]
    PanicMidi,
    Macro(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrackCommand {
    Rename,
    ToggleMute,
    ToggleSolo,
    ToggleRecordArm,
    MoveUp,
    MoveDown,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandKind {
    NewProject,
    OpenProject,
    SaveProject,
    SaveProjectAs,
    ExportWav,
    CancelOfflineRender,
    OpenSettings,
    Undo,
    Redo,
    ToggleMediaBrowserPanel,
    ShowArrangement,
    ShowMixer,
    AddMidiItem,
    ImportAudio,
    DuplicateSelectedAudioItem,
    DuplicateSelectedMidiItem,
    DeleteSelectedItems,
    SplitSelectedItemsAtCursor,
    SplitSelectedItemsAtTimeSelection,
    AddTrack,
    Track(TrackCommand),
    #[cfg(feature = "audio-device")]
    TogglePlayback,
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

#[derive(Clone, Copy)]
enum Shortcut {
    Command(char),
    CommandShift(char),
    Space,
}

impl Shortcut {
    fn config_label(self) -> String {
        match self {
            Self::Command(key) => format!("Mod+{}", key.to_ascii_uppercase()),
            Self::CommandShift(key) => {
                format!("Mod+Shift+{}", key.to_ascii_uppercase())
            }
            Self::Space => "Space".to_owned(),
        }
    }

    fn parse(value: &str) -> Result<Option<Self>, String> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }
        if value.eq_ignore_ascii_case("space") {
            return Ok(Some(Self::Space));
        }
        let parts = value.split('+').map(str::trim).collect::<Vec<_>>();
        if !(2..=3).contains(&parts.len()) {
            return Err("Use Mod+key, Mod+Shift+key, or Space".to_owned());
        }
        let has_mod = matches!(
            parts[0].to_ascii_lowercase().as_str(),
            "mod" | "ctrl" | "cmd"
        );
        let shifted = parts.len() == 3 && parts[1].eq_ignore_ascii_case("shift");
        if !has_mod || (parts.len() == 3 && !shifted) {
            return Err("Use Mod+key, Mod+Shift+key, or Space".to_owned());
        }
        let key = parts.last().copied().unwrap_or_default();
        let mut characters = key.chars();
        let Some(character) = characters.next() else {
            return Err("Shortcut key must be one letter".to_owned());
        };
        if !character.is_ascii_alphabetic() || characters.next().is_some() {
            return Err("Shortcut key must be one letter".to_owned());
        }
        Ok(Some(if shifted {
            Self::CommandShift(character.to_ascii_lowercase())
        } else {
            Self::Command(character.to_ascii_lowercase())
        }))
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
            Self::Space => modifiers == Modifiers::NONE && *key == Key::Named(Named::Space),
        }
    }
}

const OPEN_SHORTCUT: &[Shortcut] = &[Shortcut::Command('o')];
const NEW_PROJECT_SHORTCUT: &[Shortcut] = &[Shortcut::Command('n')];
const SAVE_SHORTCUT: &[Shortcut] = &[Shortcut::Command('s')];
const UNDO_SHORTCUT: &[Shortcut] = &[Shortcut::Command('z')];
const REDO_SHORTCUT: &[Shortcut] = &[Shortcut::CommandShift('z'), Shortcut::Command('y')];
#[cfg(feature = "audio-device")]
const PLAYBACK_SHORTCUT: &[Shortcut] = &[Shortcut::Space];

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
        label: "Save project as…",
        aliases: &["save project as", "save project as…", "save as", "save as…"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::ExportWav,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Render project to WAV…",
        aliases: &["render", "export audio", "bounce project", "render wav"],
        shortcuts: &[],
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::CancelOfflineRender,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Cancel WAV render",
        aliases: &["cancel render", "cancel export"],
        shortcuts: &[],
        destructive: false,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::OpenSettings,
        menu: Some(MainMenu::File),
        category: "File",
        label: "Settings…",
        aliases: &["settings", "preferences", "open settings"],
        shortcuts: &[],
        destructive: false,
        separator_before: true,
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
        kind: CommandKind::DuplicateSelectedMidiItem,
        menu: Some(MainMenu::Item),
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
        label: "Delete selected items",
        aliases: &["delete selected item", "delete items"],
        shortcuts: &[],
        destructive: true,
        separator_before: false,
    },
    CommandDefinition {
        kind: CommandKind::SplitSelectedItemsAtCursor,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Split items at edit cursor",
        aliases: &[
            "split at cursor",
            "split items at cursor",
            "split selected items at edit cursor",
        ],
        shortcuts: &[],
        destructive: false,
        separator_before: true,
    },
    CommandDefinition {
        kind: CommandKind::SplitSelectedItemsAtTimeSelection,
        menu: Some(MainMenu::Item),
        category: "Item",
        label: "Split items at time selection",
        aliases: &[
            "split at time selection",
            "split items at selection",
            "split selected items at time selection",
        ],
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
        label: "Play/stop",
        aliases: &["play", "stop", "toggle playback"],
        shortcuts: PLAYBACK_SHORTCUT,
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
];

pub(super) struct CommandEntry {
    pub(super) id: CommandId,
    pub(super) category: &'static str,
    pub(super) label: String,
    pub(super) shortcut: Option<String>,
    pub(super) enabled: bool,
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
            | CommandKind::ShowArrangement
            | CommandKind::ShowMixer
            | CommandKind::AddMidiItem
            | CommandKind::DuplicateSelectedAudioItem
            | CommandKind::DuplicateSelectedMidiItem
            | CommandKind::SplitSelectedItemsAtCursor
            | CommandKind::SplitSelectedItemsAtTimeSelection
            | CommandKind::AddTrack
            | CommandKind::Track(TrackCommand::ToggleMute)
            | CommandKind::Track(TrackCommand::ToggleSolo)
            | CommandKind::Track(TrackCommand::ToggleRecordArm)
            | CommandKind::Track(TrackCommand::MoveUp)
            | CommandKind::Track(TrackCommand::MoveDown)
    ) || {
        #[cfg(feature = "audio-device")]
        if kind == CommandKind::TogglePlayback {
            return true;
        }
        false
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

pub(super) fn shortcut_entries(app: &App) -> Vec<ShortcutEntry> {
    let mut entries = COMMANDS
        .iter()
        .map(|definition| {
            let id = command_kind_id(definition.kind);
            ShortcutEntry {
                id: id.to_owned(),
                label: definition.label.to_owned(),
                category: definition.category.to_owned(),
                binding: if app.shortcut_defaults_restored.contains(id) {
                    definition
                        .shortcuts
                        .iter()
                        .map(|shortcut| shortcut.config_label())
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    app.shortcut_binding_edits
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| config_binding_for(app, id, definition.shortcuts))
                }
                .replace("Mod+", "Ctrl/Cmd+"),
                default_binding: definition
                    .shortcuts
                    .iter()
                    .map(|shortcut| shortcut.config_label())
                    .collect::<Vec<_>>()
                    .join(", ")
                    .replace("Mod+", "Ctrl/Cmd+"),
            }
        })
        .collect::<Vec<_>>();
    entries.extend(app.action_macros.iter().map(|action_macro| {
        let id = macro_id(action_macro.id);
        ShortcutEntry {
            binding: app
                .shortcut_defaults_restored
                .contains(&id)
                .then(String::new)
                .unwrap_or_else(|| {
                    app.shortcut_binding_edits
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| config_binding_for(app, &id, &[]))
                })
                .replace("Mod+", "Ctrl/Cmd+"),
            id,
            label: action_macro.name.clone(),
            category: "Macros".to_owned(),
            default_binding: String::new(),
        }
    }));
    entries
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
    let mut resolved = HashMap::<String, String>::new();
    let mut normalized_bindings = ShortcutBindings::new();
    for definition in COMMANDS {
        let id = command_kind_id(definition.kind);
        let Some(value) = bindings.get(id) else {
            continue;
        };
        insert_normalized_binding(id, value, &mut resolved, &mut normalized_bindings)?;
    }
    for action_macro in macros {
        let id = macro_id(action_macro.id);
        let Some(value) = bindings.get(&id) else {
            continue;
        };
        insert_normalized_binding(&id, value, &mut resolved, &mut normalized_bindings)?;
    }
    for definition in COMMANDS {
        let id = command_kind_id(definition.kind);
        if bindings.contains_key(id) {
            continue;
        }
        for shortcut in definition.shortcuts {
            let normalized = shortcut.config_label();
            if let Some(other_id) = resolved.insert(normalized.clone(), id.to_owned())
                && other_id != id
            {
                return Err(format!("{normalized} conflicts with the default for {id}"));
            }
        }
    }
    Ok(normalized_bindings)
}

fn insert_normalized_binding(
    id: &str,
    value: &str,
    resolved: &mut HashMap<String, String>,
    normalized_bindings: &mut ShortcutBindings,
) -> Result<(), String> {
    if value.trim().is_empty() {
        normalized_bindings.insert(id.to_owned(), String::new());
        return Ok(());
    }
    let shortcut =
        Shortcut::parse(value)?.ok_or_else(|| "Shortcut cannot be empty here".to_owned())?;
    let normalized = shortcut.config_label();
    if let Some(other_id) = resolved.insert(normalized.clone(), id.to_owned()) {
        return Err(format!(
            "{normalized} is already assigned to both {other_id} and {id}"
        ));
    }
    normalized_bindings.insert(id.to_owned(), normalized);
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
    if let Some(action_macro) = macros.iter().find(|action_macro| {
        let id = macro_id(action_macro.id);
        bindings
            .get(&id)
            .and_then(|binding| Shortcut::parse(binding).ok().flatten())
            .is_some_and(|shortcut| shortcut.matches(key, modifiers))
    }) {
        return Some(CommandId::Macro(action_macro.id));
    }
    COMMANDS.iter().find_map(|definition| {
        let id = command_kind_id(definition.kind);
        let custom = bindings
            .get(id)
            .and_then(|binding| Shortcut::parse(binding).ok().flatten());
        match custom {
            Some(shortcut) => shortcut.matches(key, modifiers),
            None if bindings.contains_key(id) => false,
            None => definition
                .shortcuts
                .iter()
                .any(|shortcut| shortcut.matches(key, modifiers)),
        }
        .then(|| command_id(definition.kind))
    })
}

pub(super) fn capture_binding(key: &str, modifiers: Modifiers) -> Result<String, String> {
    let candidate = if key.eq_ignore_ascii_case("space") && modifiers == Modifiers::NONE {
        "Space".to_owned()
    } else if modifiers == Modifiers::COMMAND {
        format!("Mod+{key}")
    } else if modifiers == (Modifiers::COMMAND | Modifiers::SHIFT) {
        format!("Mod+Shift+{key}")
    } else {
        return Err("Use Ctrl/Cmd with a letter, optionally Shift, or press Space".to_owned());
    };
    Shortcut::parse(&candidate)?
        .map(Shortcut::config_label)
        .ok_or_else(|| "That key cannot be used as a shortcut".to_owned())
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
    config_binding_for(app, id, defaults).replace("Mod+", "Ctrl/Cmd+")
}

fn config_binding_for(app: &App, id: &str, defaults: &[Shortcut]) -> String {
    let custom = app
        .shortcut_bindings
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(binding) = custom.get(id) {
        return binding.clone();
    }
    defaults
        .iter()
        .map(|shortcut| shortcut.config_label())
        .collect::<Vec<_>>()
        .join(", ")
}

fn command_kind_id(kind: CommandKind) -> &'static str {
    match kind {
        CommandKind::NewProject => "file.new-project",
        CommandKind::OpenProject => "file.open-project",
        CommandKind::SaveProject => "file.save-project",
        CommandKind::SaveProjectAs => "file.save-project-as",
        CommandKind::ExportWav => "audio.export-wav",
        CommandKind::CancelOfflineRender => "audio.cancel-render",
        CommandKind::OpenSettings => "file.settings",
        CommandKind::Undo => "edit.undo",
        CommandKind::Redo => "edit.redo",
        CommandKind::ToggleMediaBrowserPanel => "view.media-browser-panel",
        CommandKind::ShowArrangement => "view.arrangement-workspace",
        CommandKind::ShowMixer => "view.mixer-workspace",
        CommandKind::AddMidiItem => "insert.midi-item",
        CommandKind::ImportAudio => "insert.import-audio",
        CommandKind::DuplicateSelectedAudioItem => "item.duplicate-audio",
        CommandKind::DuplicateSelectedMidiItem => "item.duplicate-midi",
        CommandKind::DeleteSelectedItems => "item.delete-selected",
        CommandKind::SplitSelectedItemsAtCursor => "item.split-at-cursor",
        CommandKind::SplitSelectedItemsAtTimeSelection => "item.split-at-selection",
        CommandKind::AddTrack => "track.add",
        CommandKind::Track(TrackCommand::Rename) => "track.rename",
        CommandKind::Track(TrackCommand::ToggleMute) => "track.toggle-mute",
        CommandKind::Track(TrackCommand::ToggleSolo) => "track.toggle-solo",
        CommandKind::Track(TrackCommand::ToggleRecordArm) => "track.toggle-record-arm",
        CommandKind::Track(TrackCommand::MoveUp) => "track.move-up",
        CommandKind::Track(TrackCommand::MoveDown) => "track.move-down",
        CommandKind::Track(TrackCommand::Delete) => "track.delete",
        #[cfg(feature = "audio-device")]
        CommandKind::TogglePlayback => "transport.toggle-playback",
        #[cfg(feature = "audio-device")]
        CommandKind::PanicMidi => "transport.panic-midi",
    }
}

pub(super) fn dispatch(app: &mut App, command: CommandId) -> Task<Message> {
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
        CommandId::NewProject => Message::NewProject,
        CommandId::OpenProject => Message::OpenProject,
        CommandId::SaveProject => Message::SaveProject,
        CommandId::SaveProjectAs => Message::PickPath(PathPickerTarget::SaveProject),
        CommandId::ExportWav => Message::PickPath(PathPickerTarget::ExportWav),
        CommandId::CancelOfflineRender => Message::CancelOfflineRender,
        CommandId::OpenSettings => Message::OpenSettings,
        CommandId::Undo => Message::Undo,
        CommandId::Redo => Message::Redo,
        CommandId::ToggleMediaBrowserPanel => Message::ToggleMediaBrowserPanel,
        CommandId::ShowArrangement => Message::ShowMainWorkspace(MainWorkspace::Arrangement),
        CommandId::ShowMixer => Message::ShowMainWorkspace(MainWorkspace::Mixer),
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
        CommandId::SplitSelectedItemsAtCursor => Message::SplitSelectedItemsAtCursor,
        CommandId::SplitSelectedItemsAtTimeSelection => Message::SplitSelectedItemsAtTimeSelection,
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
                TrackCommand::ToggleRecordArm => Message::ToggleRecordArm(track_id),
                TrackCommand::MoveUp => Message::MoveTrack(track_id, -1),
                TrackCommand::MoveDown => Message::MoveTrack(track_id, 1),
                TrackCommand::Delete => Message::DeleteTrack(track_id),
            }
        }
        #[cfg(feature = "audio-device")]
        CommandId::TogglePlayback => Message::TogglePlayback,
        #[cfg(feature = "audio-device")]
        CommandId::PanicMidi => Message::PanicMidi,
        CommandId::Macro(_) => unreachable!("macros are dispatched before built-in commands"),
    };
    app.update(message)
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
            let entry = entry_for(app, definition, command_id, selected_track);
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
    solo: bool,
    record_armed: bool,
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
        CommandKind::NewProject => !project_edit_busy(app) && !app.is_dirty(),
        CommandKind::OpenProject => !project_edit_busy(app) && !app.is_dirty(),
        CommandKind::SaveProject => !project_file_busy(app),
        CommandKind::OpenSettings => true,
        CommandKind::SaveProjectAs => !project_edit_busy(app),
        CommandKind::ExportWav => {
            app.project_path.is_some()
                && !project_file_busy(app)
                && !recording_busy(app)
                && !app.offline_render_busy
        }
        CommandKind::CancelOfflineRender => app.offline_render_busy,
        CommandKind::Undo => history_command_enabled(
            app,
            app.project.can_undo_track_mix() || app.track_mix_commit_at.is_some(),
        ),
        CommandKind::Redo => {
            app.track_mix_commit_at.is_none()
                && history_command_enabled(app, app.project.can_redo_track_mix())
        }
        CommandKind::ToggleMediaBrowserPanel => true,
        CommandKind::ShowArrangement | CommandKind::ShowMixer => true,
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
        CommandKind::SplitSelectedItemsAtCursor => {
            !project_edit_busy(app) && app.can_split_selected_items_at_cursor()
        }
        CommandKind::SplitSelectedItemsAtTimeSelection => {
            !project_edit_busy(app) && app.can_split_selected_items_at_time_selection()
        }
        CommandKind::AddTrack => !project_edit_busy(app),
        CommandKind::Track(command) => {
            !project_edit_busy(app)
                && track.is_some_and(|track| match command {
                    TrackCommand::Rename
                    | TrackCommand::ToggleMute
                    | TrackCommand::ToggleSolo
                    | TrackCommand::ToggleRecordArm
                    | TrackCommand::Delete => true,
                    TrackCommand::MoveUp => track.index > 0,
                    TrackCommand::MoveDown => track.index + 1 < app.project.tracks().len(),
                })
        }
        #[cfg(feature = "audio-device")]
        CommandKind::TogglePlayback => true,
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
            solo: track.is_solo(),
            record_armed: track.is_record_armed(),
        })
}

fn command_id(kind: CommandKind) -> CommandId {
    match kind {
        CommandKind::NewProject => CommandId::NewProject,
        CommandKind::OpenProject => CommandId::OpenProject,
        CommandKind::SaveProject => CommandId::SaveProject,
        CommandKind::SaveProjectAs => CommandId::SaveProjectAs,
        CommandKind::ExportWav => CommandId::ExportWav,
        CommandKind::CancelOfflineRender => CommandId::CancelOfflineRender,
        CommandKind::OpenSettings => CommandId::OpenSettings,
        CommandKind::Undo => CommandId::Undo,
        CommandKind::Redo => CommandId::Redo,
        CommandKind::ToggleMediaBrowserPanel => CommandId::ToggleMediaBrowserPanel,
        CommandKind::ShowArrangement => CommandId::ShowArrangement,
        CommandKind::ShowMixer => CommandId::ShowMixer,
        CommandKind::AddMidiItem => CommandId::AddMidiItem,
        CommandKind::ImportAudio => CommandId::ImportAudio,
        CommandKind::DuplicateSelectedAudioItem => CommandId::DuplicateSelectedAudioItem,
        CommandKind::DuplicateSelectedMidiItem => CommandId::DuplicateSelectedMidiItem,
        CommandKind::DeleteSelectedItems => CommandId::DeleteSelectedItems,
        CommandKind::SplitSelectedItemsAtCursor => CommandId::SplitSelectedItemsAtCursor,
        CommandKind::SplitSelectedItemsAtTimeSelection => {
            CommandId::SplitSelectedItemsAtTimeSelection
        }
        CommandKind::AddTrack => CommandId::AddTrack,
        CommandKind::Track(command) => CommandId::SelectedTrack(command),
        #[cfg(feature = "audio-device")]
        CommandKind::TogglePlayback => CommandId::TogglePlayback,
        #[cfg(feature = "audio-device")]
        CommandKind::PanicMidi => CommandId::PanicMidi,
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

fn key_matches_character(key: &Key<&str>, expected: char) -> bool {
    matches!(key, Key::Character(character) if character.eq_ignore_ascii_case(&expected.to_string()))
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
        let bindings = ShortcutBindings::from([("macro.23".to_owned(), "Ctrl+M".to_owned())]);
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
