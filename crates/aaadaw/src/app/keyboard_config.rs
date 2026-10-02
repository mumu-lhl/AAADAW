use super::commands::ShortcutBindings;
use super::config_paths::config_file_path;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "shortcuts.conf";

pub(super) fn load() -> Result<ShortcutBindings, String> {
    let Some(path) = config_path() else {
        return Ok(ShortcutBindings::new());
    };
    match std::fs::read_to_string(path) {
        Ok(contents) => parse(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(ShortcutBindings::new()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn save(bindings: &ShortcutBindings) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Err("no platform config directory is available".to_owned());
    };
    save_to(&path, bindings).map_err(|error| error.to_string())
}

pub(super) fn reset() -> Result<(), String> {
    let Some(path) = config_path() else {
        return Ok(());
    };
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn config_path() -> Option<PathBuf> {
    config_file_path(FILE_NAME)
}

fn parse(contents: &str) -> Result<ShortcutBindings, String> {
    let mut bindings = ShortcutBindings::new();
    for (line_number, line) in contents.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((id, binding)) = line.split_once('\t') else {
            return Err(format!("invalid shortcut config line {}", line_number + 1));
        };
        if id.trim().is_empty() || binding.contains('\t') {
            return Err(format!("invalid shortcut config line {}", line_number + 1));
        }
        if bindings.insert(id.to_owned(), binding.to_owned()).is_some() {
            return Err(format!("duplicate action ID on line {}", line_number + 1));
        }
    }
    Ok(bindings)
}

fn save_to(path: &Path, bindings: &ShortcutBindings) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let temporary = path.with_extension("conf.tmp");
    let mut entries = bindings.iter().collect::<Vec<_>>();
    entries.sort_unstable_by_key(|(key, _)| (*key).clone());
    let mut file = std::fs::File::create(&temporary)?;
    for (id, binding) in entries {
        writeln!(file, "{id}\t{binding}")?;
    }
    file.sync_all()?;
    drop(file);
    #[cfg(target_os = "windows")]
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_config_round_trips_in_sorted_order() {
        let path = std::env::temp_dir().join(format!(
            "aaadaw-shortcuts-{}-{}.conf",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let bindings = ShortcutBindings::from([
            ("edit.undo".to_owned(), "Mod+Z".to_owned()),
            ("file.save-project".to_owned(), "Mod+Shift+S".to_owned()),
        ]);
        save_to(&path, &bindings).unwrap();
        assert_eq!(
            parse(&std::fs::read_to_string(&path).unwrap()).unwrap(),
            bindings
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn malformed_shortcut_config_is_rejected() {
        assert!(parse("edit.undo Mod+Z\n").is_err());
        assert!(parse("edit.undo\tMod+Z\nedit.undo\tMod+Y\n").is_err());
    }
}
