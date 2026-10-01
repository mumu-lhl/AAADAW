use super::{App, Message, project_path_from_query, run_blocking};
use aaadaw_core::{Project, ProjectSnapshot};
use aaadaw_storage::ProjectStore;
use iced::Task;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(super) fn open_project(app: &mut App) -> Task<Message> {
    #[cfg(feature = "jack-backend")]
    if app.playback.is_some() {
        app.status = "Close JACK output before opening another project".to_owned();
        return Task::none();
    }
    if app.io_busy {
        app.status = "Wait for current project operation to finish".to_owned();
        return Task::none();
    }
    if app.is_dirty() {
        app.status = "Save current project before opening another".to_owned();
        return Task::none();
    }
    let Some(path) = project_path_from_query(&app.project_path_query) else {
        app.status = "Enter a project file path first".to_owned();
        return Task::none();
    };
    app.io_busy = true;
    app.status = format!("Opening {}…", path.display());
    let message_path = path.clone();
    Task::perform(
        run_blocking("aaadaw-project-open", move || load_project_file(path)),
        move |result| Message::ProjectLoaded(message_path, Arc::new(Mutex::new(Some(result)))),
    )
}

pub(super) fn save_project(app: &mut App, save_as: Option<PathBuf>) -> Task<Message> {
    if app.io_busy {
        app.status = "Wait for current project operation to finish".to_owned();
        return Task::none();
    }
    let Some((path, can_overwrite)) = resolve_save_target(
        app.project_path.as_deref(),
        &app.project_path_query,
        save_as,
    ) else {
        app.status = "Enter a project file path first".to_owned();
        return Task::none();
    };
    let revision = app.revision;
    let snapshot = app.project.snapshot();
    app.io_busy = true;
    app.status = format!("Saving {}…", path.display());
    let message_path = path.clone();
    Task::perform(
        run_blocking("aaadaw-project-save", move || {
            save_project_file(path, snapshot, can_overwrite)
        }),
        move |result| Message::ProjectSaved(message_path, revision, result),
    )
}

pub(super) fn resolve_save_target(
    current: Option<&Path>,
    query: &str,
    save_as: Option<PathBuf>,
) -> Option<(PathBuf, bool)> {
    if let Some(path) = save_as {
        let can_overwrite = current.is_some_and(|current| current == path.as_path());
        return Some((path, can_overwrite));
    }
    if let Some(path) = current {
        return Some((path.to_path_buf(), true));
    }
    project_path_from_query(query).map(|path| (path, false))
}

pub(super) fn load_project_file(path: PathBuf) -> Result<Project, String> {
    if !path.is_file() {
        return Err(format!("project file {} does not exist", path.display()));
    }
    let store = ProjectStore::open(&path).map_err(|error| error.to_string())?;
    let project = store.load().map_err(|error| error.to_string());
    let close = store.close().map_err(|error| error.to_string());
    let project = project?;
    close?;
    Ok(project)
}

pub(super) fn save_project_file(
    path: PathBuf,
    snapshot: ProjectSnapshot,
    can_overwrite: bool,
) -> Result<(), String> {
    if path == Path::new(":memory:") {
        return Err("project path must name a file".to_owned());
    }
    if !can_overwrite && path.exists() {
        return Err("file exists; open it before saving to that path".to_owned());
    }
    let project = Project::from_snapshot(snapshot).map_err(|error| error.to_string())?;
    let mut store = ProjectStore::open(&path).map_err(|error| error.to_string())?;
    let save = store.save(&project).map_err(|error| error.to_string());
    let close = store.close().map_err(|error| error.to_string());
    save?;
    close?;
    Ok(())
}
