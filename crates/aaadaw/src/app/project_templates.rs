use super::{App, config_paths};
use aaadaw_storage::ProjectSessionLock;
use std::path::{Path, PathBuf};

pub(super) struct WorkingCopy {
    pub(super) directory: PathBuf,
    pub(super) project_path: PathBuf,
}

impl Drop for WorkingCopy {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.directory)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                path = %self.directory.display(),
                %error,
                "could not remove project-template working copy"
            );
        }
    }
}

pub(super) fn directory() -> Option<PathBuf> {
    config_paths::config_file_path("project-templates.json")
        .and_then(|path| path.parent().map(|parent| parent.join("ProjectTemplates")))
}

fn normalized_path(path: &Path) -> PathBuf {
    if let Ok(path) = std::fs::canonicalize(path) {
        return path;
    }
    let Some(file_name) = path.file_name() else {
        return path.to_path_buf();
    };
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::canonicalize(parent)
        .map_or_else(|_| path.to_path_buf(), |parent| parent.join(file_name))
}

pub(super) fn is_within_directory(path: &Path, directory: &Path) -> bool {
    path.starts_with(directory)
        || normalized_path(path).starts_with(normalized_path(directory).as_path())
}

pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    left == right || normalized_path(left) == normalized_path(right)
}

fn is_template_project(
    template_source: Option<&Path>,
    project_path: Option<&Path>,
    template_directory: Option<&Path>,
) -> bool {
    template_source.is_some()
        || template_directory.is_some_and(|directory| {
            project_path.is_some_and(|path| is_within_directory(path, directory))
        })
}

pub(super) fn resolve_save_path(
    selected_path: &Path,
    template_directory: &Path,
) -> Result<PathBuf, String> {
    let selected_directory = selected_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let selected_directory = std::fs::canonicalize(selected_directory)
        .map_err(|error| format!("Could not access the selected folder: {error}"))?;
    let template_directory = std::fs::canonicalize(template_directory)
        .map_err(|error| format!("Could not access the ProjectTemplates folder: {error}"))?;
    if selected_directory != template_directory {
        return Err(format!(
            "Choose a template name inside {}",
            template_directory.display()
        ));
    }
    let Some(file_name) = selected_path.file_name() else {
        return Err("Choose a project template file name".to_owned());
    };
    let mut file_name = file_name.to_os_string();
    if !selected_path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("aaadaw"))
    {
        let mut normalized = PathBuf::from(file_name);
        normalized.set_extension("aaadaw");
        file_name = normalized.into_os_string();
    }
    Ok(template_directory.join(file_name))
}

fn is_project_template(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("aaadaw"))
}

pub(super) fn load() -> Result<Vec<PathBuf>, String> {
    let Some(directory) = directory() else {
        return Ok(Vec::new());
    };
    load_from_directory(&directory)
}

fn load_from_directory(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if is_project_template(&path) {
            paths.push(path);
        }
    }
    paths.sort_by_key(|path| {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        (name.to_lowercase(), name)
    });
    Ok(paths)
}

pub(super) fn create_working_copy(source: &Path) -> Result<(PathBuf, PathBuf), String> {
    if !is_project_template(source) {
        return Err(format!(
            "Project template {} is unavailable",
            source.display()
        ));
    }
    let _source_lock = ProjectSessionLock::acquire(source).map_err(|error| error.to_string())?;
    let directory = tempfile::Builder::new()
        .prefix("aaadaw-template-")
        .tempdir()
        .map_err(|error| error.to_string())?;
    let file_name = source
        .file_name()
        .ok_or_else(|| "Project template must have a file name".to_owned())?;
    let project_path = directory.path().join(file_name);
    std::fs::copy(source, &project_path).map_err(|error| error.to_string())?;
    let directory = directory.keep();
    let project_path = directory.join(file_name);
    Ok((directory, project_path))
}

impl App {
    pub(super) fn current_project_is_template(&self) -> bool {
        let template_directory = directory();
        is_template_project(
            self.project_template_source.as_deref(),
            self.project_path.as_deref(),
            template_directory.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_list_is_sorted_and_only_contains_aaadaw_files() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("Zulu.aaadaw"), b"template").unwrap();
        std::fs::write(directory.path().join("ALPHA.aaadaw"), b"template").unwrap();
        std::fs::write(directory.path().join("alpha.AAADAW"), b"template").unwrap();
        std::fs::write(directory.path().join("readme.txt"), b"not a template").unwrap();
        std::fs::create_dir(directory.path().join("folder.aaadaw")).unwrap();

        let paths = load_from_directory(directory.path()).unwrap();

        assert_eq!(
            paths
                .iter()
                .filter_map(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["ALPHA.aaadaw", "alpha.AAADAW", "Zulu.aaadaw"]
        );
    }

    #[test]
    fn template_working_copy_preserves_the_source_and_uses_the_template_name() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("Piano.aaadaw");
        std::fs::write(&source, b"template contents").unwrap();

        let (directory, project_path) = create_working_copy(&source).unwrap();
        let working_copy = WorkingCopy {
            directory,
            project_path,
        };
        assert_eq!(working_copy.project_path.file_name(), source.file_name());
        assert_ne!(working_copy.project_path, source);
        assert_eq!(
            std::fs::read(&working_copy.project_path).unwrap(),
            b"template contents"
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"template contents");
        let working_directory = working_copy.directory.clone();
        drop(working_copy);
        assert!(!working_directory.exists());
    }

    #[test]
    fn template_save_path_stays_in_the_template_folder_and_adds_the_project_extension() {
        let directory = tempfile::tempdir().unwrap();
        let selected = directory.path().join("New Session");

        assert_eq!(
            resolve_save_path(&selected, directory.path()).unwrap(),
            directory.path().join("New Session.aaadaw")
        );
        assert_eq!(
            resolve_save_path(&directory.path().join("Existing.AAADAW"), directory.path()).unwrap(),
            directory.path().join("Existing.AAADAW")
        );
    }

    #[test]
    fn template_save_path_rejects_a_different_folder_instead_of_redirecting_silently() {
        let directory = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();

        let error = resolve_save_path(&other.path().join("New Session.aaadaw"), directory.path())
            .expect_err("outside paths must not be silently redirected into ProjectTemplates");
        assert!(error.contains("Choose a template name inside"));
    }

    #[test]
    fn template_path_detection_canonicalizes_existing_files_and_keeps_unrelated_paths_out() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("Templates");
        std::fs::create_dir(&nested).unwrap();
        let project = nested.join("Session.aaadaw");
        std::fs::write(&project, b"project").unwrap();
        let outside = directory.path().join("Outside.aaadaw");
        std::fs::write(&outside, b"project").unwrap();

        assert!(is_within_directory(&project, &nested));
        assert!(!is_within_directory(&outside, &nested));
        assert!(same_path(&project, &nested.join("Session.aaadaw")));
    }

    #[test]
    fn direct_open_from_the_template_folder_is_still_a_template_project() {
        let directory = tempfile::tempdir().unwrap();
        let templates = directory.path().join("ProjectTemplates");
        std::fs::create_dir(&templates).unwrap();
        let template = templates.join("First.aaadaw");
        std::fs::write(&template, b"project").unwrap();
        let unrelated = directory.path().join("Project.aaadaw");

        assert!(is_template_project(None, Some(&template), Some(&templates)));
        assert!(!is_template_project(
            None,
            Some(&unrelated),
            Some(&templates)
        ));
        assert!(is_template_project(
            Some(&unrelated),
            None,
            Some(&templates)
        ));
    }
}
