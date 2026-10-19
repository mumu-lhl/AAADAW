use super::{commands::ShortcutBindings, config_paths::write_atomic, keyboard_config, shortcut};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

const HEADER: &str = "AAADAW_SHORTCUT_MAP\t1";
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Linux REAPER 7.82 Main-section actions with an exact AAADAW action counterpart.
/// Other REAPER actions are rejected instead of being silently discarded.
const REAPER_ACTIONS: &[(&str, u32, &str)] = &[
    ("actions.show-list", 40605, "Show action list"),
    ("file.quit", 40004, "File: Quit REAPER"),
    ("file.close-project", 40860, "Close current project tab"),
    ("file.close-all-projects", 40886, "File: Close all projects"),
    ("item.delete-selected", 40006, "Item: Remove items"),
    (
        "take.duplicate-active",
        40639,
        "Take: Duplicate active take",
    ),
    (
        "take.delete-active",
        40129,
        "Take: Delete active take from items",
    ),
    (
        "take.move-active-to-top",
        41380,
        "Item: Move active takes to top",
    ),
    ("take.switch-next", 40125, "Take: Switch items to next take"),
    (
        "take.paste-as-takes",
        40603,
        "Take: Paste as takes in items",
    ),
    (
        "take.switch-previous",
        40126,
        "Take: Switch items to previous take",
    ),
    (
        "item.split-at-cursor",
        40012,
        "Item: Split items at edit or play cursor (select right)",
    ),
    (
        "item.copy-loop-selected-area",
        40014,
        "Item: Copy loop of selected area of audio items",
    ),
    ("edit.copy-items", 40057, "Edit: Copy items/tracks"),
    ("audio.export-wav", 40015, "File: Render project to disk..."),
    ("file.settings", 40016, "Options: Preferences..."),
    (
        "grid.snap-settings",
        40071,
        "Options: Show snap/grid settings",
    ),
    ("file.project-settings", 40021, "File: Project settings..."),
    ("file.save-project-as", 40022, "File: Save project as..."),
    ("file.new-project", 40023, "File: New project"),
    ("file.open-project", 40025, "File: Open project"),
    ("file.save-project", 40026, "File: Save project"),
    ("edit.undo", 40029, "Edit: Undo"),
    ("edit.redo", 40030, "Edit: Redo"),
    ("transport.play-stop", 40044, "Transport: Play/stop"),
    (
        "item.split-at-selection",
        40061,
        "Item: Split items at time selection or razor edit",
    ),
    ("transport.toggle-playback", 40073, "Transport: Play/pause"),
    ("view.toggle-mixer", 40078, "View: Toggle mixer visible"),
    ("transport.toggle-repeat", 1068, "Transport: Toggle repeat"),
    ("track.add", 40001, "Track: Insert new track"),
    (
        "track.cycle-folder-compact",
        1042,
        "Track: Cycle folder collapsed state",
    ),
    ("track.delete", 40005, "Track: Remove tracks"),
    (
        "track.toggle-mute",
        6,
        "Track: Toggle mute for selected tracks",
    ),
    (
        "track.toggle-record-arm",
        9,
        "Track: Toggle record arm for selected tracks",
    ),
    (
        "track.toggle-solo",
        7,
        "Track: Toggle solo for selected tracks",
    ),
    ("transport.stop-playback", 1016, "Transport: Stop"),
    (
        "file.save-project-new-version",
        41895,
        "File: Save new version of project (automatically increment project name)",
    ),
    ("file.save-all-projects", 40897, "File: Save all projects"),
    (
        "file.save-project-as-template",
        40394,
        "File: Save project as template...",
    ),
    ("file.open-render-queue", 40929, "File: Open render queue"),
    (
        "file.export-project-midi",
        40849,
        "File: Export project MIDI...",
    ),
    ("grid.toggle-swing", 42304, "Grid: Toggle swing grid"),
    ("edit.paste-items", 42398, "Item: Paste items/tracks"),
    (
        "ruler.toggle-project-markers",
        43485,
        "Ruler: Show/hide all project markers",
    ),
    (
        "ruler.toggle-project-regions",
        43487,
        "Ruler: Show/hide all project regions",
    ),
    (
        "ruler.toggle-project-regions-and-markers",
        43489,
        "Ruler: Show/hide all project regions and markers",
    ),
    (
        "regions.delete-near-cursor",
        40615,
        "Markers: Delete region near cursor",
    ),
    (
        "regions.edit-near-cursor",
        40616,
        "Markers: Edit region near cursor",
    ),
    ("loop-time.go-to-start", 40632, "Go to start of loop"),
    ("loop-time.go-to-end", 40633, "Go to end of loop"),
    (
        "ruler.toggle-selected-project-markers",
        43484,
        "Ruler: Show/hide selected project markers",
    ),
    (
        "ruler.toggle-selected-project-regions",
        43486,
        "Ruler: Show/hide selected project regions",
    ),
    ("item.select-all", 40182, "Item: Select all items"),
    ("item.nudge-set", 41226, "Item edit: Nudge/Set..."),
    (
        "item.nudge-set.close",
        41227,
        "Item edit: Close nudge/Set dialog",
    ),
    (
        "item.nudge-left",
        41250,
        "Item edit: Nudge left by last nudge dialog settings",
    ),
    (
        "item.nudge-right",
        41249,
        "Item edit: Nudge right by last nudge dialog settings",
    ),
    (
        "item.select-all-in-track",
        40421,
        "Item: Select all items in track",
    ),
    (
        "item.select-all-in-time-selection",
        40717,
        "Item: Select all items in current time selection",
    ),
    (
        "item.unselect-all",
        40289,
        "Item: Unselect (clear selection of) all items",
    ),
    ("item.invert-selection", 41115, "Item: Invert selection"),
    (
        "item.select-next-adjacent",
        41127,
        "Item: Select next adjacent non-overlapping item",
    ),
    (
        "item.select-previous-adjacent",
        41128,
        "Item: Select previous adjacent non-overlapping item",
    ),
    (
        "item.select-previous-track",
        41139,
        "Item navigation: Select and move to item in previous track without changing track selection",
    ),
    (
        "item.select-next-track",
        41140,
        "Item navigation: Select and move to item in next track without changing track selection",
    ),
    (
        "item.navigate-cursor-left-edge",
        41167,
        "Item navigation: Move cursor left to nearest item edge",
    ),
    (
        "item.navigate-cursor-right-edge",
        41168,
        "Item navigation: Move cursor right to nearest item edge",
    ),
    (
        "item.navigate-cursor-selected-start",
        41173,
        "Item navigation: Move cursor to start of items",
    ),
    (
        "item.navigate-cursor-selected-end",
        41174,
        "Item navigation: Move cursor to end of items",
    ),
    ("item.snap-left", 41182, "Item: Snap items left"),
    ("item.snap-right", 41183, "Item: Snap items right"),
    (
        "item.snap-nearest",
        41184,
        "Item: Snap items to nearest snap point",
    ),
    (
        "item.select-under-mouse",
        40528,
        "Item: Select item under mouse cursor",
    ),
    (
        "item.add-under-mouse-to-selection",
        40529,
        "Item: Select item under mouse cursor (leaving other items selected)",
    ),
    (
        "item.toggle-under-mouse-selection",
        40530,
        "Item: Toggle selection of item under mouse cursor",
    ),
    ("item.toggle-mute", 40175, "Item properties: Toggle mute"),
    (
        "edit.delete-focused",
        40697,
        "Remove items/tracks/envelope points (depending on focus)",
    ),
    (
        "edit.delete-focused-no-prompting",
        40184,
        "Remove items/tracks/envelope points (depending on focus) - no prompting",
    ),
    (
        "item.trim-to-selection",
        40508,
        "Item: Trim items to selected area",
    ),
    (
        "item.enclose-media-in-razor-edit",
        42409,
        "Razor edit: Enclose media items, including space between items",
    ),
    ("item.heal-splits", 40548, "Item: Heal splits in items"),
    ("item.duplicate", 41295, "Item: Duplicate items"),
    (
        "item.select-all-on-selected-tracks-in-time-selection",
        40718,
        "Item: Select all items on selected tracks in current time selection",
    ),
    (
        "regions.insert-from-time-selection",
        40174,
        "Markers: Insert region from time selection",
    ),
    (
        "insert.project-marker",
        40157,
        "Markers: Insert marker at current position",
    ),
    (
        "markers.delete-near-cursor",
        40613,
        "Markers: Delete marker near cursor",
    ),
    (
        "navigation.previous-project-marker",
        40172,
        "Markers: Go to previous marker/project start",
    ),
    (
        "navigation.next-project-marker",
        40173,
        "Markers: Go to next marker/project end",
    ),
    ("loop-time.set-start", 40222, "Loop points: Set start point"),
    ("loop-time.set-end", 40223, "Loop points: Set end point"),
    (
        "loop-time.toggle-link",
        40621,
        "Options: Toggle loop points linked to time selection",
    ),
    (
        "loop-time.loop-to-time-selection",
        40622,
        "Time selection: Move loop points to time selection",
    ),
    (
        "loop-time.time-selection-to-loop",
        40623,
        "Time selection: Move time selection to loop points",
    ),
    (
        "loop-time.remove-loop-if-unlinked",
        40624,
        "Loop points: Remove (unselect) loop points, if not linked to time selection, ignoring lock state",
    ),
    (
        "loop-time.remove-time-selection-and-loop",
        40020,
        "Time selection: Remove (unselect) time selection and loop points",
    ),
    (
        "view.marker-manager",
        40326,
        "View: Show region/marker manager window",
    ),
    (
        "view.region-render-matrix",
        41888,
        "View: Show region render matrix window",
    ),
    (
        "track.toggle-phase",
        40282,
        "Track: Invert track polarity/phase",
    ),
    (
        "ruler.toggle-selected-project-regions-and-markers",
        43488,
        "Ruler: Show/hide selected project regions and markers",
    ),
    (
        "markers.add-or-move.01",
        40657,
        "Markers: Add/move marker 1 to play/edit cursor",
    ),
    (
        "markers.add-or-move.02",
        40658,
        "Markers: Add/move marker 2 to play/edit cursor",
    ),
    (
        "markers.add-or-move.03",
        40659,
        "Markers: Add/move marker 3 to play/edit cursor",
    ),
    (
        "markers.add-or-move.04",
        40660,
        "Markers: Add/move marker 4 to play/edit cursor",
    ),
    (
        "markers.add-or-move.05",
        40661,
        "Markers: Add/move marker 5 to play/edit cursor",
    ),
    (
        "markers.add-or-move.06",
        40662,
        "Markers: Add/move marker 6 to play/edit cursor",
    ),
    (
        "markers.add-or-move.07",
        40663,
        "Markers: Add/move marker 7 to play/edit cursor",
    ),
    (
        "markers.add-or-move.08",
        40664,
        "Markers: Add/move marker 8 to play/edit cursor",
    ),
    (
        "markers.add-or-move.09",
        40665,
        "Markers: Add/move marker 9 to play/edit cursor",
    ),
    (
        "markers.add-or-move.10",
        40656,
        "Markers: Add/move marker 10 to play/edit cursor",
    ),
];

pub(super) fn read(path: &Path) -> Result<ShortcutBindings, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("shortcut map must be a regular file".to_owned());
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err("shortcut map exceeds the 1 MiB size limit".to_owned());
    }
    let mut contents = String::new();
    std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(MAX_FILE_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|error| error.to_string())?;
    parse(&contents)
}

pub(super) fn write(path: &Path, bindings: &ShortcutBindings) -> Result<(), String> {
    let active_config_path = keyboard_config::config_path();
    write_to(path, bindings, active_config_path.as_deref())
}

pub(super) fn read_reaper(path: &Path) -> Result<ShortcutBindings, String> {
    let contents = read_bounded(path)?;
    parse_reaper(&contents)
}

pub(super) fn write_reaper(path: &Path, bindings: &ShortcutBindings) -> Result<(), String> {
    let active_config_path = keyboard_config::config_path();
    write_reaper_to(path, bindings, active_config_path.as_deref())
}

fn write_reaper_to(
    path: &Path,
    bindings: &ShortcutBindings,
    active_config_path: Option<&Path>,
) -> Result<(), String> {
    let contents = serialize_reaper(bindings)?;
    write_contents(path, &contents, active_config_path)
}

fn read_bounded(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("shortcut map must be a regular file".to_owned());
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err("shortcut map exceeds the 1 MiB size limit".to_owned());
    }
    let mut contents = String::new();
    std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(MAX_FILE_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|error| error.to_string())?;
    if contents.len() > MAX_FILE_BYTES as usize {
        return Err("shortcut map exceeds the 1 MiB size limit".to_owned());
    }
    Ok(contents)
}

fn write_to(
    path: &Path,
    bindings: &ShortcutBindings,
    active_config_path: Option<&Path>,
) -> Result<(), String> {
    let contents = serialize(bindings);
    write_contents(path, &contents, active_config_path)
}

fn write_contents(
    path: &Path,
    contents: &str,
    active_config_path: Option<&Path>,
) -> Result<(), String> {
    if let Some(config_path) = active_config_path
        && normalized_path(path) == normalized_path(config_path)
    {
        return Err("cannot export a key map over AAADAW's active shortcut settings".to_owned());
    }
    write_atomic(path, contents.as_bytes()).map_err(|error| error.to_string())
}

fn normalized_path(path: &Path) -> std::path::PathBuf {
    if let Ok(path) = std::fs::canonicalize(path) {
        return path;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().unwrap_or_default();
    std::fs::canonicalize(parent)
        .unwrap_or_else(|_| parent.to_owned())
        .join(file_name)
}

fn parse(contents: &str) -> Result<ShortcutBindings, String> {
    if contents.len() > MAX_FILE_BYTES as usize {
        return Err("shortcut map exceeds the 1 MiB size limit".to_owned());
    }
    let mut lines = contents.lines();
    if lines.next() != Some(HEADER) {
        return Err("unsupported shortcut map header or version".to_owned());
    }

    let mut bindings = ShortcutBindings::new();
    for (index, line) in lines.enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line_number = index + 2;
        let Some((id, value)) = line.split_once('\t') else {
            return Err(format!("invalid shortcut map line {line_number}"));
        };
        if id.is_empty() || id.trim() != id || id.chars().any(char::is_whitespace) {
            return Err(format!(
                "invalid action ID on shortcut map line {line_number}"
            ));
        }
        if value.contains('\t') || value.contains('\n') || value.contains('\r') {
            return Err(format!("invalid shortcut value on line {line_number}"));
        }
        let parsed = shortcut::parse_bindings(value)
            .map_err(|error| format!("invalid shortcut on line {line_number}: {error}"))?;
        if bindings
            .insert(id.to_owned(), shortcut::serialize_bindings(&parsed))
            .is_some()
        {
            return Err(format!(
                "duplicate action ID on shortcut map line {line_number}"
            ));
        }
    }
    Ok(bindings)
}

fn serialize(bindings: &ShortcutBindings) -> String {
    let mut entries = bindings.iter().collect::<Vec<_>>();
    entries.sort_unstable_by_key(|(id, _)| (*id).clone());
    let mut output = format!("{HEADER}\n");
    for (id, value) in entries {
        output.push_str(id);
        output.push('\t');
        output.push_str(value);
        output.push('\n');
    }
    output
}

fn parse_reaper(contents: &str) -> Result<ShortcutBindings, String> {
    if contents.len() > MAX_FILE_BYTES as usize {
        return Err("shortcut map exceeds the 1 MiB size limit".to_owned());
    }
    let mut shortcuts = HashMap::<String, Vec<shortcut::Shortcut>>::new();
    for (index, line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let record = line.split('#').next().unwrap_or_default().trim();
        if record.is_empty() {
            continue;
        }
        let mut fields = record.split_whitespace();
        if fields.next() != Some("KEY") {
            return Err(format!(
                "unsupported REAPER key map record on line {line_number}"
            ));
        }
        let values = fields.collect::<Vec<_>>();
        if values.len() != 4 {
            return Err(format!("invalid REAPER KEY record on line {line_number}"));
        }
        let parse_number = |value: &str, field: &str| {
            value
                .parse::<u32>()
                .map_err(|_| format!("invalid REAPER {field} on key map line {line_number}"))
        };
        let flags = parse_number(values[0], "modifier flags")?;
        let key_code = parse_number(values[1], "virtual-key code")?;
        let action_id = parse_number(values[2], "action ID")?;
        let section_id = parse_number(values[3], "section ID")?;
        if section_id != 0 {
            return Err(format!(
                "REAPER section {section_id} is not supported (only Main section 0 is supported)"
            ));
        }
        let Some((action, _, _)) = REAPER_ACTIONS
            .iter()
            .find(|(_, native_id, _)| *native_id == action_id)
        else {
            return Err(format!(
                "REAPER action ID {action_id} has no AAADAW action mapping"
            ));
        };
        let binding = shortcut::Shortcut::from_reaper_key(flags, key_code)
            .map_err(|error| format!("invalid REAPER key map line {line_number}: {error}"))?;
        let entries = shortcuts.entry((*action).to_owned()).or_default();
        if entries.contains(&binding) {
            return Err(format!(
                "duplicate REAPER shortcut on key map line {line_number}"
            ));
        }
        entries.push(binding);
    }
    Ok(shortcuts
        .into_iter()
        .map(|(action, bindings)| (action, shortcut::serialize_bindings(&bindings)))
        .collect())
}

fn serialize_reaper(bindings: &ShortcutBindings) -> Result<String, String> {
    let mut entries = bindings.iter().collect::<Vec<_>>();
    entries.sort_unstable_by_key(|(id, _)| (*id).clone());
    let mut output = String::new();
    for (id, value) in entries {
        let Some((_, action_id, action_name)) =
            REAPER_ACTIONS.iter().find(|(action, _, _)| action == id)
        else {
            return Err(format!(
                "AAADAW action {id} has no REAPER 7.82 Main-section mapping"
            ));
        };
        let bindings = shortcut::parse_bindings(value)
            .map_err(|error| format!("invalid shortcut for {id}: {error}"))?;
        if bindings.is_empty() {
            return Err(format!(
                "REAPER key maps cannot express cleared shortcuts for {id}"
            ));
        }
        for binding in bindings {
            let (flags, key_codes) = binding
                .reaper_key_codes()
                .map_err(|error| format!("cannot export {id}: {error}"))?;
            let label = binding
                .config_label()
                .replace("Mod+", "Ctrl+")
                .replace("Super+", "Logo+");
            for key_code in key_codes {
                output.push_str(&format!(
                    "KEY {flags} {key_code} {action_id} 0\t\t # Main : {label} : {action_name}\n"
                ));
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_map_round_trips_sorted_overrides_and_explicitly_cleared_defaults() {
        let bindings = ShortcutBindings::from([
            ("track.add".to_owned(), "Alt+F12; Mod+Alt+7".to_owned()),
            ("edit.undo".to_owned(), String::new()),
            ("file.save-project".to_owned(), "Mod+S".to_owned()),
        ]);
        let encoded = serialize(&bindings);
        assert!(encoded.starts_with("AAADAW_SHORTCUT_MAP\t1\nedit.undo\t\n"));
        assert_eq!(parse(&encoded).unwrap(), bindings);
    }

    #[test]
    fn malformed_or_unsupported_maps_are_rejected_before_application() {
        for contents in [
            "edit.undo\tMod+Z\n",
            "AAADAW_SHORTCUT_MAP\t2\nedit.undo\tMod+Z\n",
            "AAADAW_SHORTCUT_MAP\t1\nedit.undo Mod+Z\n",
            "AAADAW_SHORTCUT_MAP\t1\nedit.undo\tMod+Z\nedit.undo\tMod+Y\n",
            "AAADAW_SHORTCUT_MAP\t1\nedit.undo\tHyper+Z\n",
            "AAADAW_SHORTCUT_MAP\t1\nedit.undo\tF1;\n",
            "AAADAW_SHORTCUT_MAP\t1\n\tMod+Z\n",
        ] {
            assert!(
                parse(contents).is_err(),
                "accepted malformed map {contents:?}"
            );
        }
    }

    #[test]
    fn reaper_782_key_records_round_trip_observed_main_section_modifiers() {
        let contents = concat!(
            "KEY 29 123 40029 0\t\t # Main : Ctrl+Alt+Shift+F12 : Edit: Undo\n",
            "KEY 17 122 40029 0\t\t # Main : Alt+F11 : Edit: Undo\n",
            "KEY 5 122 40029 0\t\t # Main : Shift+F11 : Edit: Undo\n",
            "KEY 9 122 40029 0\t\t # Main : Ctrl+F11 : Edit: Undo\n",
            "KEY 33 122 40029 0\t\t # Main : Logo+F11 : Edit: Undo\n",
        );
        let bindings = parse_reaper(contents).unwrap();
        assert_eq!(
            bindings["edit.undo"],
            "Mod+Alt+Shift+F12; Alt+F11; Shift+F11; Mod+F11; Super+F11"
        );
        assert_eq!(serialize_reaper(&bindings).unwrap(), contents);
    }

    #[test]
    fn reaper_main_catalog_maps_additional_exact_actions() {
        let contents = concat!(
            "KEY 1 113 40605 0\t\t # Main : F2 : Show action list\n",
            "KEY 9 67 40057 0\t\t # Main : Ctrl+C : Edit: Copy items/tracks\n",
            "KEY 1 115 40886 0\t\t # Main : F4 : File: Close all projects\n",
            "KEY 1 114 40860 0\t\t # Main : F3 : Close current project tab\n",
            "KEY 1 116 40071 0\t\t # Main : F5 : Options: Show snap/grid settings\n",
            "KEY 1 121 40639 0\t\t # Main : F10 : Take: Duplicate active take\n",
            "KEY 9 84 40603 0\t\t # Main : Ctrl+T : Take: Paste as takes in items\n",
            "KEY 1 122 40125 0\t\t # Main : F11 : Take: Switch items to next take\n",
            "KEY 1 123 40126 0\t\t # Main : F12 : Take: Switch items to previous take\n",
            "KEY 1 117 40001 0\t\t # Main : F6 : Track: Insert new track\n",
            "KEY 1 118 1042 0\t\t # Main : F7 : Track: Cycle folder collapsed state\n",
            "KEY 1 119 40005 0\t\t # Main : F8 : Track: Remove tracks\n",
            "KEY 1 77 6 0\t\t # Main : M : Track: Toggle mute for selected tracks\n",
            "KEY 1 82 9 0\t\t # Main : R : Track: Toggle record arm for selected tracks\n",
            "KEY 1 83 7 0\t\t # Main : S : Track: Toggle solo for selected tracks\n",
            "KEY 1 120 1016 0\t\t # Main : F9 : Transport: Stop\n",
        );
        let bindings = parse_reaper(contents).unwrap();
        assert_eq!(bindings.len(), 16);
        for (action, key) in [
            ("actions.show-list", "F2"),
            ("file.close-project", "F3"),
            ("file.close-all-projects", "F4"),
            ("grid.snap-settings", "F5"),
            ("track.add", "F6"),
            ("track.cycle-folder-compact", "F7"),
            ("track.delete", "F8"),
            ("track.toggle-mute", "M"),
            ("track.toggle-record-arm", "R"),
            ("track.toggle-solo", "S"),
            ("transport.stop-playback", "F9"),
            ("take.duplicate-active", "F10"),
            ("take.switch-next", "F11"),
            ("take.switch-previous", "F12"),
            ("edit.copy-items", "Mod+C"),
            ("take.paste-as-takes", "Mod+T"),
        ] {
            assert_eq!(bindings[action], key, "{action}");
        }
        assert_eq!(serialize_reaper(&bindings).unwrap(), contents);
    }

    #[test]
    fn reaper_main_mapping_table_round_trips_all_unique_native_action_ids() {
        assert_eq!(REAPER_ACTIONS.len(), 109);
        let mut action_ids = REAPER_ACTIONS
            .iter()
            .map(|(_, action_id, _)| *action_id)
            .collect::<Vec<_>>();
        action_ids.sort_unstable();
        assert!(action_ids.windows(2).all(|pair| pair[0] != pair[1]));

        let bindings = REAPER_ACTIONS
            .iter()
            .map(|(id, _, _)| ((*id).to_owned(), "Mod+Alt+F12".to_owned()))
            .collect::<ShortcutBindings>();
        let serialized = serialize_reaper(&bindings).unwrap();
        assert_eq!(parse_reaper(&serialized).unwrap(), bindings);
    }

    #[test]
    fn reaper_key_map_rejects_sections_actions_directives_and_unknown_flags() {
        for contents in [
            "KEY 9 90 40029 32060\n",
            "KEY 9 90 999999 0\n",
            "KEY 2 90 40029 0\n",
            "SCR 4 0 RS1abc \"Custom action\" action.lua\n",
            "KEY 9 90 40029\n",
        ] {
            assert!(
                parse_reaper(contents).is_err(),
                "accepted unsupported REAPER map {contents:?}"
            );
        }
    }

    #[test]
    fn reaper_export_refuses_unmapped_actions_cleared_keys_and_unrepresentable_keys() {
        assert!(
            serialize_reaper(&ShortcutBindings::from([(
                "unmapped.action".to_owned(),
                "Mod+Z".to_owned()
            )]))
            .is_err()
        );
        assert!(
            serialize_reaper(&ShortcutBindings::from([(
                "edit.undo".to_owned(),
                String::new()
            )]))
            .is_err()
        );
        assert!(
            serialize_reaper(&ShortcutBindings::from([(
                "edit.undo".to_owned(),
                "F35".to_owned()
            )]))
            .is_err()
        );
    }

    #[test]
    fn reaper_export_refuses_to_replace_active_shortcut_config() {
        let parent = std::env::temp_dir().join(format!(
            "aaadaw-reaper-keymap-active-config-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        std::fs::create_dir_all(&parent).unwrap();
        let active_config = parent.join("shortcuts.conf");
        std::fs::write(&active_config, b"active shortcut settings").unwrap();
        let bindings = ShortcutBindings::from([("edit.undo".to_owned(), "Mod+Z".to_owned())]);

        assert!(write_reaper_to(&active_config, &bindings, Some(&active_config)).is_err());
        assert_eq!(
            std::fs::read(&active_config).unwrap(),
            b"active shortcut settings"
        );
        let _ = std::fs::remove_dir_all(parent);
    }

    #[test]
    fn write_atomically_preserves_the_previous_file_on_failure() {
        let parent = std::env::temp_dir().join(format!(
            "aaadaw-keymap-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let destination = parent.join("map.aaadawkeymap");
        let bindings = ShortcutBindings::from([("edit.undo".to_owned(), "Mod+Z".to_owned())]);
        write(&destination, &bindings).unwrap();
        assert_eq!(read(&destination).unwrap(), bindings);

        std::fs::remove_file(&destination).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let sentinel = destination.join("preserve-me");
        std::fs::write(&sentinel, b"existing file").unwrap();
        assert!(write(&destination, &bindings).is_err());
        assert_eq!(std::fs::read(sentinel).unwrap(), b"existing file");
        let _ = std::fs::remove_dir_all(parent);
    }

    #[test]
    fn export_refuses_to_replace_the_active_shortcut_config() {
        let parent = std::env::temp_dir().join(format!(
            "aaadaw-keymap-active-config-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        std::fs::create_dir_all(&parent).unwrap();
        let active_config = parent.join("shortcuts.conf");
        std::fs::write(&active_config, b"active shortcut settings").unwrap();
        let bindings = ShortcutBindings::from([("edit.undo".to_owned(), "Mod+Z".to_owned())]);

        assert!(write_to(&active_config, &bindings, Some(&active_config)).is_err());
        assert_eq!(
            std::fs::read(&active_config).unwrap(),
            b"active shortcut settings"
        );
        let _ = std::fs::remove_dir_all(parent);
    }

    #[test]
    fn import_size_limit_is_checked_before_parsing() {
        let contents = format!("{HEADER}\n{}", "#".repeat(MAX_FILE_BYTES as usize));
        assert!(parse(&contents).is_err());
    }
}
