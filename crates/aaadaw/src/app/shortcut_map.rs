use super::{commands::ShortcutBindings, config_paths::write_atomic, keyboard_config, shortcut};
use std::io::Read;
use std::path::Path;

const HEADER: &str = "AAADAW_SHORTCUT_MAP\t1";
const MAX_FILE_BYTES: u64 = 1024 * 1024;

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

fn write_to(
    path: &Path,
    bindings: &ShortcutBindings,
    active_config_path: Option<&Path>,
) -> Result<(), String> {
    if let Some(config_path) = active_config_path
        && normalized_path(path) == normalized_path(config_path)
    {
        return Err("cannot export a key map over AAADAW's active shortcut settings".to_owned());
    }
    let contents = serialize(bindings);
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
