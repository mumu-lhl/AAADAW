use super::config_paths::{config_file_path, write_atomic};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "clap-paths.conf";

pub(super) fn load() -> Result<Vec<PathBuf>, String> {
    let Some(path) = config_file_path(FILE_NAME) else {
        return Ok(Vec::new());
    };
    match std::fs::read_to_string(path) {
        Ok(contents) => parse(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn save(paths: &[PathBuf]) -> Result<(), String> {
    let Some(path) = config_file_path(FILE_NAME) else {
        return Err("no platform config directory is available".to_owned());
    };
    save_to(&path, paths).map_err(|error| error.to_string())
}

fn parse(contents: &str) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for (line_number, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if line.contains('\t') {
            return Err(format!("invalid CLAP path config line {}", line_number + 1));
        }
        let path = PathBuf::from(line);
        if paths.contains(&path) {
            continue;
        }
        paths.push(path);
    }
    Ok(paths)
}

fn save_to(path: &Path, paths: &[PathBuf]) -> io::Result<()> {
    let mut contents = Vec::new();
    for path in paths {
        let path = path.to_string_lossy();
        if path.contains(['\n', '\r', '\t']) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CLAP search paths cannot contain line breaks or tabs",
            ));
        }
        writeln!(&mut contents, "{path}")?;
    }
    write_atomic(path, &contents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn configured_paths_round_trip_and_duplicate_lines_are_ignored() {
        let path = std::env::temp_dir().join(format!(
            "aaadaw-clap-paths-{}-{}.conf",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let configured = vec![PathBuf::from("/first/path"), PathBuf::from("/second/path")];
        save_to(&path, &configured).unwrap();
        assert_eq!(
            parse(&std::fs::read_to_string(&path).unwrap()).unwrap(),
            configured
        );
        assert_eq!(
            parse("/first/path\n\n/first/path\n").unwrap(),
            vec![PathBuf::from("/first/path")]
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_path_record_is_rejected() {
        assert!(parse("/valid/path\tsecond-field\n").is_err());
    }
}
