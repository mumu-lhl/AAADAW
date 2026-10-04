use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(super) fn config_file_path(file_name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    root.map(|root| root.join("aaadaw").join(file_name))
}

pub(super) fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(contents)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_replacement_keeps_the_existing_destination() {
        let parent = std::env::temp_dir().join(format!(
            "aaadaw-atomic-config-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let destination = parent.join("settings");
        std::fs::create_dir_all(&destination).unwrap();
        let sentinel = destination.join("preserve-me");
        std::fs::write(&sentinel, b"existing settings").unwrap();

        assert!(write_atomic(&destination, b"replacement").is_err());
        assert_eq!(std::fs::read(sentinel).unwrap(), b"existing settings");

        let _ = std::fs::remove_dir_all(parent);
    }
}
