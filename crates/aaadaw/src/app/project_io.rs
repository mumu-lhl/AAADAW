use super::messages::SharedProjectSessionLock;
use super::{App, Message, project_path_from_query, run_blocking};
use aaadaw_core::{Project, ProjectSnapshot};
use aaadaw_storage::{ArrangementViewState, ProjectSessionLock, ProjectStore};
use iced::Task;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(super) fn open_project(app: &mut App) -> Task<Message> {
    #[cfg(feature = "audio-device")]
    if app.playback.is_some() {
        app.status = format!(
            "Close {} output before opening another project",
            app.playback_name()
        );
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
    if app.project_path.as_deref() == Some(path.as_path()) && app.project_lock.is_some() {
        app.status = format!("{} is already open", path.display());
        return Task::none();
    }
    app.io_busy = true;
    app.status = format!("Opening {}…", path.display());
    let message_path = path.clone();
    Task::perform(
        run_blocking("aaadaw-project-open", move || load_project_session(path)),
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
    #[cfg(feature = "audio-device")]
    let plugin_state_warning = app.persist_clap_plugin_states().err();
    #[cfg(not(feature = "audio-device"))]
    let plugin_state_warning = None;
    let revision = app.revision;
    let snapshot = app.project.snapshot();
    let arrangement_view_state = app.timeline.arrangement_view_state(&app.project);
    let reuse_session_lock =
        app.project_path.as_deref() == Some(path.as_path()) && app.project_lock.is_some();
    app.io_busy = true;
    app.status = format!("Saving {}…", path.display());
    let message_path = path.clone();
    Task::perform(
        run_blocking("aaadaw-project-save", move || {
            let mut new_lock = if reuse_session_lock {
                None
            } else {
                Some(ProjectSessionLock::acquire(&path).map_err(|error| error.to_string())?)
            };
            save_project_session_file(
                path.clone(),
                snapshot,
                arrangement_view_state,
                can_overwrite,
                new_lock.as_mut(),
            )?;
            #[cfg(target_os = "android")]
            crate::android_platform::sync_saf_document(&path)?;
            Ok(new_lock)
        }),
        move |result| {
            let (result, lock) = match result {
                Ok(lock) => (Ok(()), lock),
                Err(error) => (Err(error), None),
            };
            Message::ProjectSaved(
                message_path,
                revision,
                result,
                plugin_state_warning,
                SharedProjectSessionLock::new(lock),
            )
        },
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

#[cfg(test)]
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

pub(super) fn load_project_session(
    path: PathBuf,
) -> Result<(Project, Option<ArrangementViewState>, ProjectSessionLock), String> {
    let lock = ProjectSessionLock::acquire(&path).map_err(|error| error.to_string())?;
    let store = ProjectStore::open(&path).map_err(|error| error.to_string())?;
    let project = store.load().map_err(|error| error.to_string())?;
    let arrangement_view_state = store
        .load_arrangement_view_state()
        .map_err(|error| error.to_string())?;
    store.close().map_err(|error| error.to_string())?;
    Ok((project, arrangement_view_state, lock))
}

#[cfg(test)]
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

pub(super) fn save_project_session_file(
    path: PathBuf,
    snapshot: ProjectSnapshot,
    arrangement_view_state: ArrangementViewState,
    can_overwrite: bool,
    new_session_lock: Option<&mut ProjectSessionLock>,
) -> Result<(), String> {
    if path == Path::new(":memory:") {
        return Err("project path must name a file".to_owned());
    }
    if !can_overwrite && path.exists() {
        return Err("file exists; open it before saving to that path".to_owned());
    }
    let project = Project::from_snapshot(snapshot).map_err(|error| error.to_string())?;
    if let Some(new_session_lock) = new_session_lock {
        if can_overwrite {
            return Err("cannot replace a project without its session lock".to_owned());
        }
        return save_new_project_session_file(
            path,
            &project,
            &arrangement_view_state,
            new_session_lock,
        );
    }
    if !can_overwrite && path.exists() {
        return Err("file exists; open it before saving to that path".to_owned());
    }
    let mut store = ProjectStore::open(&path).map_err(|error| error.to_string())?;
    let save = store
        .save_with_arrangement_view_state(&project, &arrangement_view_state)
        .map_err(|error| error.to_string());
    let close = store.close().map_err(|error| error.to_string());
    save?;
    close?;
    Ok(())
}

fn save_new_project_session_file(
    path: PathBuf,
    project: &Project,
    arrangement_view_state: &ArrangementViewState,
    new_session_lock: &mut ProjectSessionLock,
) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary_path = tempfile::Builder::new()
        .prefix(".aaadaw-save-")
        .tempfile_in(parent)
        .map_err(|error| error.to_string())?
        .into_temp_path();
    let mut store = ProjectStore::open(&temporary_path).map_err(|error| error.to_string())?;
    if let Err(error) = new_session_lock.lock_file_identity(&temporary_path) {
        let _ = store.close();
        return Err(error.to_string());
    }
    let save = store
        .save_with_arrangement_view_state(project, arrangement_view_state)
        .map_err(|error| error.to_string());
    let close = store.close().map_err(|error| error.to_string());
    save?;
    close?;
    temporary_path
        .persist_noclobber(&path)
        .map_err(|error| error.error.to_string())?;
    Ok(())
}
