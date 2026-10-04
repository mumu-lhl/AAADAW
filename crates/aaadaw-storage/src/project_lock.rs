use fs4::FileExt;
use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

/// Exclusive lock held for the lifetime of an open project session.
///
/// The lock file is intentionally retained after release. Removing it could let
/// two processes lock different inodes while opening the same project.
#[derive(Debug)]
pub struct ProjectSessionLock {
    _file: File,
    _identity_file: Option<File>,
}

impl ProjectSessionLock {
    pub fn acquire(project_path: impl AsRef<Path>) -> io::Result<Self> {
        let project_path = project_path.as_ref();
        let lock_path = lock_path(project_path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)?;
        FileExt::try_lock(&file).map_err(|error| match error {
            fs4::TryLockError::WouldBlock => io::Error::new(
                io::ErrorKind::WouldBlock,
                format!(
                    "project is already open in another AAADAW session: {}",
                    lock_path.display()
                ),
            ),
            fs4::TryLockError::Error(error) => error,
        })?;
        let identity_file = project_path
            .exists()
            .then(|| identity_lock_file(project_path))
            .transpose()?;
        Ok(Self {
            _file: file,
            _identity_file: identity_file,
        })
    }

    /// Adds the inode lock after a newly created project file has been saved.
    pub fn lock_file_identity(&mut self, project_path: impl AsRef<Path>) -> io::Result<()> {
        if self._identity_file.is_none() {
            self._identity_file = Some(identity_lock_file(project_path.as_ref())?);
        }
        Ok(())
    }
}

fn lock_path(project_path: &Path) -> io::Result<PathBuf> {
    let canonical_project_path = if project_path.exists() {
        project_path.canonicalize()?
    } else {
        let parent = project_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let file_name = project_path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "project path must name a file")
        })?;
        parent.canonicalize()?.join(file_name)
    };
    let mut lock_path = canonical_project_path.into_os_string();
    lock_path.push(".lock");
    Ok(PathBuf::from(lock_path))
}

fn identity_lock_file(project_path: &Path) -> io::Result<File> {
    let identity = file_id::get_file_id(project_path)?;
    let identity_name = match identity {
        file_id::FileId::Inode {
            device_id,
            inode_number,
        } => format!("inode-{device_id:016x}-{inode_number:016x}.lock"),
        file_id::FileId::LowRes {
            volume_serial_number,
            file_index,
        } => format!("file-{volume_serial_number:08x}-{file_index:016x}.lock"),
        file_id::FileId::HighRes {
            volume_serial_number,
            file_id,
        } => format!("file-{volume_serial_number:016x}-{file_id:032x}.lock"),
    };
    let lock_directory = lock_directory()?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_directory.join(identity_name))?;
    FileExt::try_lock(&file).map_err(|error| match error {
        fs4::TryLockError::WouldBlock => io::Error::new(
            io::ErrorKind::WouldBlock,
            "project is already open through another path",
        ),
        fs4::TryLockError::Error(error) => error,
    })?;
    Ok(file)
}

fn lock_directory() -> io::Result<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from)
                .map(|home| home.join(".cache"))
        })
        .unwrap_or_else(|| {
            use std::hash::{Hash, Hasher};

            let user = std::env::var_os("USER").or_else(|| std::env::var_os("USERNAME"));
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            user.hash(&mut hasher);
            std::env::temp_dir().join(format!("aaadaw-user-{:016x}", hasher.finish()))
        });
    let directory = base.join("aaadaw").join("project-locks");
    std::fs::create_dir_all(&directory)?;
    #[cfg(unix)]
    std::fs::set_permissions(
        &directory,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )?;
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::ProjectSessionLock;

    #[test]
    fn a_project_session_lock_excludes_another_lock_handle() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("session.aaadaw");
        let lock = ProjectSessionLock::acquire(&project).unwrap();

        let error = match ProjectSessionLock::acquire(&project) {
            Ok(_) => panic!("second project session unexpectedly acquired the lock"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        drop(lock);

        ProjectSessionLock::acquire(project).unwrap();
    }

    #[test]
    fn project_path_aliases_share_one_session_lock() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("session.aaadaw");
        std::fs::write(&project, b"project placeholder").unwrap();
        let alias = directory.path().join(".").join("session.aaadaw");
        let lock = ProjectSessionLock::acquire(&project).unwrap();

        let error = match ProjectSessionLock::acquire(&alias) {
            Ok(_) => panic!("path alias unexpectedly acquired a second project lock"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        drop(lock);

        ProjectSessionLock::acquire(alias).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn hard_link_aliases_share_one_session_lock() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("session.aaadaw");
        std::fs::write(&project, b"project placeholder").unwrap();
        let hard_link = directory.path().join("hard-link.aaadaw");
        std::fs::hard_link(&project, &hard_link).unwrap();
        let lock = ProjectSessionLock::acquire(&project).unwrap();

        let error = match ProjectSessionLock::acquire(&hard_link) {
            Ok(_) => panic!("hard link unexpectedly acquired a second project lock"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        drop(lock);

        ProjectSessionLock::acquire(hard_link).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_new_project_session_can_add_its_file_identity_lock_after_save() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("new-session.aaadaw");
        let mut lock = ProjectSessionLock::acquire(&project).unwrap();
        std::fs::write(&project, b"saved project placeholder").unwrap();
        lock.lock_file_identity(&project).unwrap();
        let hard_link = directory.path().join("new-session-link.aaadaw");
        std::fs::hard_link(&project, &hard_link).unwrap();

        let error = match ProjectSessionLock::acquire(&hard_link) {
            Ok(_) => panic!("hard link unexpectedly acquired a second project lock"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    }

    #[cfg(unix)]
    #[test]
    fn project_symlinks_share_the_target_session_lock() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("session.aaadaw");
        std::fs::write(&project, b"project placeholder").unwrap();
        let symlink = directory.path().join("session-link.aaadaw");
        std::os::unix::fs::symlink(&project, &symlink).unwrap();
        let lock = ProjectSessionLock::acquire(&project).unwrap();

        let error = match ProjectSessionLock::acquire(&symlink) {
            Ok(_) => panic!("project symlink unexpectedly acquired a second lock"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        drop(lock);

        ProjectSessionLock::acquire(symlink).unwrap();
    }
}
