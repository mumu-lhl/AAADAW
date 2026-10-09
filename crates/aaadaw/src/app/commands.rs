use super::action_macros::ActionMacro;
use super::shortcut::{Shortcut, parse_bindings, serialize_bindings};
use super::{App, MainMenu, MainWorkspace, Message, PathPickerTarget};
use aaadaw_core::TrackId;
use iced::Task;
#[cfg(feature = "audio-device")]
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
    OpenActionList,
    Undo,
    Redo,
    ToggleMediaBrowserPanel,
    ToggleOfflineJobsPanel,
    ShowArrangement,
    ShowMixer,
    ToggleMixerPanel,
    AddMidiItem,
    ImportAudio,
    DuplicateSelectedItem,
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
    StopPlayback,
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
    OpenActionList,
    Undo,
    Redo,
    ToggleMediaBrowserPanel,
    ToggleOfflineJobsPanel,
    ShowArrangement,
    ShowMixer,
    ToggleMixerPanel,
    AddMidiItem,
    ImportAudio,
    DuplicateSelectedItem,
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
const ADD_TRACK_SHORTCUT: &[Shortcut] = &[Shortcut::character('t', Modifiers::COMMAND)];
const OPEN_SHORTCUT: &[Shortcut] = &[Shortcut::character('o', Modifiers::COMMAND)];
const NEW_PROJECT_SHORTCUT: &[Shortcut] = &[Shortcut::character('n', Modifiers::COMMAND)];
const SAVE_SHORTCUT: &[Shortcut] = &[Shortcut::character('s', Modifiers::COMMAND)];
const UNDO_SHORTCUT: &[Shortcut] = &[Shortcut::character('z', Modifiers::COMMAND)];
const REDO_SHORTCUT: &[Shortcut] = &[
    Shortcut::character('z', Modifiers::COMMAND.union(Modifiers::SHIFT)),
    Shortcut::character('y', Modifiers::COMMAND),
];
const DUPLICATE_ITEM_SHORTCUT: &[Shortcut] = &[Shortcut::character('d', Modifiers::COMMAND)];
const DELETE_ITEMS_SHORTCUT: &[Shortcut] = &[Shortcut::delete_backspace()];
const SPLIT_ITEMS_SHORTCUT: &[Shortcut] = &[Shortcut::character('s', Modifiers::NONE)];
#[cfg(feature = "audio-device")]
const PLAYBACK_SHORTCUT: &[Shortcut] = &[Shortcut::named(Named::Space, Modifiers::NONE)];
#[cfg(feature = "audio-device")]
const STOP_PLAYBACK_SHORTCUT: &[Shortcut] = &[Shortcut::named(Named::Space, Modifiers::SHIFT)];

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
        label: "Cancel active offline job",
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
        aliases: &["show offline jobs", "hide offline jobs", "render queue"],
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
        label: "Delete selected items",
        aliases: &["delete selected item", "delete items"],
        shortcuts: DELETE_ITEMS_SHORTCUT,
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
        shortcuts: SPLIT_ITEMS_SHORTCUT,
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
        shortcuts: ADD_TRACK_SHORTCUT,
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
        label: "Play/Pause",
        aliases: &["play", "pause", "toggle playback"],
        shortcuts: PLAYBACK_SHORTCUT,
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
            | CommandKind::ToggleOfflineJobsPanel
            | CommandKind::ShowArrangement
            | CommandKind::ShowMixer
            | CommandKind::ToggleMixerPanel
            | CommandKind::AddMidiItem
            | CommandKind::DuplicateSelectedItem
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
    matches!(
        kind,
        CommandKind::OpenActionList | CommandKind::AddTrack | CommandKind::ToggleMixerPanel
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
            // Adding a new factory binding must not invalidate older custom maps.
            if default_may_yield(definition.kind)
                && resolved.iter().any(|(bound, _)| shortcut.conflicts(*bound))
            {
                continue;
            }
            register_shortcut(*shortcut, id, &mut resolved)?;
        }
    }
    Ok(normalized_bindings)
}

fn register_shortcut(
    shortcut: Shortcut,
    id: &str,
    resolved: &mut Vec<(Shortcut, String)>,
) -> Result<(), String> {
    if shortcut.is_reserved() {
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
    resolved: &mut Vec<(Shortcut, String)>,
    normalized_bindings: &mut ShortcutBindings,
) -> Result<(), String> {
    let shortcuts = parse_bindings(value)?;
    for shortcut in &shortcuts {
        register_shortcut(*shortcut, id, resolved)?;
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
    binding
        .replace("Ctrl/Cmd+", &modifier)
        .replace("Mod+", &modifier)
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
        CommandKind::OpenProject => "file.open-project",
        CommandKind::SaveProject => "file.save-project",
        CommandKind::SaveProjectAs => "file.save-project-as",
        CommandKind::ExportWav => "audio.export-wav",
        CommandKind::CancelOfflineRender => "audio.cancel-render",
        CommandKind::OpenSettings => "file.settings",
        CommandKind::OpenActionList => "actions.show-list",
        CommandKind::Undo => "edit.undo",
        CommandKind::Redo => "edit.redo",
        CommandKind::ToggleMediaBrowserPanel => "view.media-browser-panel",
        CommandKind::ToggleOfflineJobsPanel => "view.offline-jobs-panel",
        CommandKind::ShowArrangement => "view.arrangement-workspace",
        CommandKind::ShowMixer => "view.mixer-workspace",
        CommandKind::ToggleMixerPanel => "view.toggle-mixer",
        CommandKind::AddMidiItem => "insert.midi-item",
        CommandKind::ImportAudio => "insert.import-audio",
        CommandKind::DuplicateSelectedItem => "item.duplicate",
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
        CommandKind::StopPlayback => "transport.stop-playback",
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
        CommandId::ExportWav => Message::OpenRenderWindow,
        CommandId::CancelOfflineRender => Message::CancelOfflineRender,
        CommandId::OpenSettings => Message::OpenSettings,
        CommandId::OpenActionList => Message::OpenActionList,
        CommandId::Undo => Message::Undo,
        CommandId::Redo => Message::Redo,
        CommandId::ToggleMediaBrowserPanel => Message::ToggleMediaBrowserPanel,
        CommandId::ToggleOfflineJobsPanel => Message::ToggleOfflineJobsPanel,
        CommandId::ShowArrangement => Message::ShowMainWorkspace(MainWorkspace::Arrangement),
        CommandId::ShowMixer => Message::ShowMainWorkspace(MainWorkspace::Mixer),
        CommandId::ToggleMixerPanel => Message::ToggleMixerPanel,
        CommandId::AddMidiItem => Message::AddMidiItem,
        CommandId::ImportAudio => Message::PickPath(PathPickerTarget::ImportAudioToProject),
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
        CommandId::StopPlayback => Message::StopPlayback,
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
        CommandKind::NewProject | CommandKind::OpenProject => !project_edit_busy(app),
        CommandKind::SaveProject => !project_file_busy(app),
        CommandKind::OpenSettings | CommandKind::OpenActionList => true,
        CommandKind::SaveProjectAs => !project_edit_busy(app),
        CommandKind::ExportWav => {
            app.media_store_path().is_some()
                && !recording_busy(app)
                && app.offline_job_submission_allowed()
        }
        CommandKind::CancelOfflineRender => app.offline_render_busy,
        CommandKind::Undo => {
            (app.project.can_undo() || app.track_mix_commit_at.is_some())
                && history_command_enabled(
                    app,
                    app.project.can_undo_track_mix() || app.track_mix_commit_at.is_some(),
                )
        }
        CommandKind::Redo => {
            app.project.can_redo()
                && app.track_mix_commit_at.is_none()
                && history_command_enabled(app, app.project.can_redo_track_mix())
        }
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
        CommandKind::ImportAudio => {
            !project_edit_busy(app)
                && !app.project.tracks().is_empty()
                && app.media_store_path().is_some()
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
        CommandKind::OpenActionList => CommandId::OpenActionList,
        CommandKind::Undo => CommandId::Undo,
        CommandKind::Redo => CommandId::Redo,
        CommandKind::ToggleMediaBrowserPanel => CommandId::ToggleMediaBrowserPanel,
        CommandKind::ToggleOfflineJobsPanel => CommandId::ToggleOfflineJobsPanel,
        CommandKind::ShowArrangement => CommandId::ShowArrangement,
        CommandKind::ShowMixer => CommandId::ShowMixer,
        CommandKind::ToggleMixerPanel => CommandId::ToggleMixerPanel,
        CommandKind::AddMidiItem => CommandId::AddMidiItem,
        CommandKind::ImportAudio => CommandId::ImportAudio,
        CommandKind::DuplicateSelectedItem => CommandId::DuplicateSelectedItem,
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
        CommandKind::StopPlayback => CommandId::StopPlayback,
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
            capture_binding("Space", Modifiers::SHIFT).unwrap(),
            "Shift+Space"
        );
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
        CommandId::ToggleMediaBrowserPanel => Some(app.media_panel_dock.open),
        CommandId::ToggleOfflineJobsPanel => Some(app.offline_jobs_panel_open),
        CommandId::ToggleMixerPanel => Some(if app.is_mobile_main_window() {
            app.main_workspace == MainWorkspace::Mixer
        } else {
            app.media_panel_dock.mixer_open
        }),
        CommandId::SelectedTrack(TrackCommand::ToggleMute) => app
            .selected_track_id()
            .and_then(|id| track_state(app, id))
            .map(|track| track.muted),
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
