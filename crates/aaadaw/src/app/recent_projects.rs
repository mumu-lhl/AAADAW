use super::{App, config_paths};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const VERSION: u32 = 1;
pub(super) const MAX_RECENT_PROJECTS: usize = 20;

#[derive(Debug, Serialize, Deserialize)]
struct RecentProjectList {
    version: u32,
    paths: Vec<PathBuf>,
}

fn load_path(path: &Path) -> Result<Vec<PathBuf>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let list: RecentProjectList =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if list.version != VERSION {
        return Err("Unsupported recent-project list version".to_owned());
    }
    Ok(normalize_paths(list.paths))
}

pub(super) fn load() -> Result<Vec<PathBuf>, String> {
    config_paths::config_file_path("recent-projects.json")
        .map_or_else(|| Ok(Vec::new()), |path| load_path(&path))
}

fn with_recent_path(paths: &[PathBuf], path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.file_name().is_none() {
        return Err("A recent project must name a file".to_owned());
    }
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut updated = Vec::with_capacity(paths.len().min(MAX_RECENT_PROJECTS) + 1);
    updated.push(path);
    updated.extend(paths.iter().cloned());
    Ok(normalize_paths(updated))
}

fn save(paths: &[PathBuf]) -> Result<(), String> {
    let config_path = config_paths::config_file_path("recent-projects.json")
        .ok_or_else(|| "No application configuration directory".to_owned())?;
    save_path(&config_path, paths)
}

fn normalize_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| path.file_name().is_some() && seen.insert(path.clone()))
        .take(MAX_RECENT_PROJECTS)
        .collect()
}

fn save_path(path: &Path, paths: &[PathBuf]) -> Result<(), String> {
    let list = RecentProjectList {
        version: VERSION,
        paths: normalize_paths(paths.to_vec()),
    };
    let bytes = serde_json::to_vec_pretty(&list).map_err(|error| error.to_string())?;
    config_paths::write_atomic(path, &bytes).map_err(|error| error.to_string())
}

impl App {
    pub(super) fn remember_recent_project(&mut self, path: &Path) {
        match with_recent_path(&self.recent_project_paths, path) {
            Ok(paths) => {
                self.recent_project_paths = paths;
                if let Err(error) = save(&self.recent_project_paths) {
                    tracing::warn!(path = %path.display(), %error, "could not persist recent project")
                }
            }
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "could not persist recent project")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_projects_round_trip_deduplicate_and_obey_the_limit() {
        let directory = tempfile::tempdir().unwrap();
        let list_path = directory.path().join("recent.json");
        let projects = (0..MAX_RECENT_PROJECTS + 2)
            .map(|index| directory.path().join(format!("project-{index}.aaadaw")))
            .collect::<Vec<_>>();
        save_path(&list_path, &projects).unwrap();
        let loaded = load_path(&list_path).unwrap();
        assert_eq!(loaded.len(), MAX_RECENT_PROJECTS);
        assert_eq!(loaded[0], projects[0]);

        save_path(&list_path, &[projects[0].clone(), projects[0].clone()]).unwrap();
        assert_eq!(load_path(&list_path).unwrap(), [projects[0].clone()]);
    }

    #[test]
    fn unsupported_recent_project_configuration_is_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let list_path = directory.path().join("recent.json");
        let invalid = br#"{"version":2,"paths":[]}"#;
        std::fs::write(&list_path, invalid).unwrap();

        assert!(load_path(&list_path).is_err());
        assert_eq!(std::fs::read(&list_path).unwrap(), invalid);
    }
}
