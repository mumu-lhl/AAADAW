use directories::ProjectDirs;
use std::{
    env, fs,
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::EnvFilter;

const LOG_DIRECTORY_ENV: &str = "AAADAW_LOG_DIR";
const LOG_FILE_PREFIX: &str = "aaadaw";
const MAX_LOG_FILES: usize = 7;
const MAX_LOG_FILE_BYTES: u64 = 10 * 1024 * 1024;

struct LoggingWriter {
    writer: NonBlocking,
    guard: WorkerGuard,
    destination: LogDestination,
    warning: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum LogDestination {
    File(PathBuf),
    Stderr,
}

/// Installs the process-wide subscriber and returns the guard that flushes its
/// non-blocking writer when the application exits.
pub fn initialize() -> WorkerGuard {
    let setup = make_writer(default_log_directory());
    if let Some(warning) = setup.warning.as_deref() {
        eprintln!("AAADAW logging: {warning}");
    }
    let (filter, filter_warning) = configured_filter(env::var_os("RUST_LOG").as_deref());
    if let Some(warning) = filter_warning {
        eprintln!("AAADAW logging: {warning}");
    }
    let LoggingWriter {
        writer,
        guard,
        destination,
        warning: _,
    } = setup;
    if let Err(error) = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init()
    {
        eprintln!("AAADAW logging: could not install tracing subscriber: {error}");
    } else if let LogDestination::File(directory) = destination {
        tracing::info!(log_directory = %directory.display(), "logging initialized");
    } else {
        tracing::info!("logging initialized on stderr fallback");
    }
    guard
}

fn default_log_directory() -> Result<PathBuf, String> {
    let override_directory = env::var_os(LOG_DIRECTORY_ENV).map(PathBuf::from);
    let platform_data_directory = ProjectDirs::from("org", "AAADAW", "AAADAW")
        .map(|directories| directories.data_local_dir().to_path_buf());
    choose_log_directory(override_directory, platform_data_directory)
}

fn choose_log_directory(
    override_directory: Option<PathBuf>,
    platform_data_directory: Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(directory) = override_directory.filter(|path| !path.as_os_str().is_empty()) {
        return Ok(directory);
    }
    platform_data_directory
        .map(|directory| directory.join("logs"))
        .ok_or_else(|| "no platform user data directory is available".to_owned())
}

fn make_writer(log_directory: Result<PathBuf, String>) -> LoggingWriter {
    match log_directory.and_then(|directory| {
        create_file_appender(&directory).map(|appender| (directory, appender))
    }) {
        Ok((directory, appender)) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            LoggingWriter {
                writer,
                guard,
                destination: LogDestination::File(directory),
                warning: None,
            }
        }
        Err(warning) => {
            let (writer, guard) = tracing_appender::non_blocking(io::stderr());
            LoggingWriter {
                writer,
                guard,
                destination: LogDestination::Stderr,
                warning: Some(format!("{warning}; using stderr fallback")),
            }
        }
    }
}

fn configured_filter(value: Option<&std::ffi::OsStr>) -> (EnvFilter, Option<String>) {
    let Some(value) = value else {
        return (EnvFilter::new("info"), None);
    };
    let Some(value) = value.to_str() else {
        return (
            EnvFilter::new("info"),
            Some("RUST_LOG is not valid Unicode; using info".to_owned()),
        );
    };
    match EnvFilter::try_new(value) {
        Ok(filter) => (filter, None),
        Err(error) => (
            EnvFilter::new("info"),
            Some(format!("invalid RUST_LOG filter ({error}); using info")),
        ),
    }
}

fn create_file_appender(directory: &Path) -> Result<BoundedRollingWriter, String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create log directory: {error}"))?;
    BoundedRollingWriter::open(directory, MAX_LOG_FILE_BYTES, MAX_LOG_FILES)
        .map_err(|error| format!("cannot create log files: {error}"))
}

struct BoundedRollingWriter {
    directory: PathBuf,
    file: File,
    file_bytes: u64,
    max_file_bytes: u64,
    max_files: usize,
}

impl BoundedRollingWriter {
    fn open(directory: &Path, max_file_bytes: u64, max_files: usize) -> io::Result<Self> {
        let active_path = directory.join(format!("{LOG_FILE_PREFIX}.log"));
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&active_path)?;
        let file_bytes = file.metadata()?.len();
        let mut writer = Self {
            directory: directory.to_path_buf(),
            file,
            file_bytes,
            max_file_bytes: max_file_bytes.max(1),
            max_files: max_files.max(1),
        };
        if writer.file_bytes >= writer.max_file_bytes {
            writer.rotate()?;
        }
        Ok(writer)
    }

    fn rotate(&mut self) -> io::Result<()> {
        let active_path = self.directory.join(format!("{LOG_FILE_PREFIX}.log"));
        self.file.flush()?;
        if self.max_files > 1 {
            let oldest_path = self.backup_path(self.max_files - 1);
            remove_if_exists(&oldest_path)?;
            for index in (1..self.max_files - 1).rev() {
                let source = self.backup_path(index);
                if source.exists() {
                    let target = self.backup_path(index + 1);
                    remove_if_exists(&target)?;
                    fs::rename(source, target)?;
                }
            }
            if active_path.exists() {
                let newest_backup = self.backup_path(1);
                remove_if_exists(&newest_backup)?;
                fs::rename(&active_path, newest_backup)?;
            }
        } else {
            remove_if_exists(&active_path)?;
        }
        self.file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(active_path)?;
        self.file_bytes = 0;
        Ok(())
    }

    fn backup_path(&self, index: usize) -> PathBuf {
        self.directory
            .join(format!("{LOG_FILE_PREFIX}.{index}.log"))
    }
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

impl Write for BoundedRollingWriter {
    fn write(&mut self, mut buffer: &[u8]) -> io::Result<usize> {
        let original_len = buffer.len();
        while !buffer.is_empty() {
            if self.file_bytes >= self.max_file_bytes {
                self.rotate()?;
            }
            let available = (self.max_file_bytes - self.file_bytes) as usize;
            let chunk_len = buffer.len().min(available);
            self.file.write_all(&buffer[..chunk_len])?;
            self.file_bytes += chunk_len as u64;
            buffer = &buffer[chunk_len..];
        }
        Ok(original_len)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_log_directory_overrides_the_platform_data_directory() {
        let override_directory = PathBuf::from("portable/logs");
        let directory = choose_log_directory(
            Some(override_directory.clone()),
            Some(PathBuf::from("platform/data")),
        )
        .unwrap();
        assert_eq!(directory, override_directory);
    }

    #[test]
    fn default_log_directory_is_under_platform_user_data() {
        let directory =
            choose_log_directory(None, Some(PathBuf::from("user/data/AAADAW"))).unwrap();
        assert_eq!(directory, PathBuf::from("user/data/AAADAW/logs"));
    }

    #[test]
    fn unavailable_platform_directory_is_reported() {
        let error = choose_log_directory(None, None).unwrap_err();
        assert!(error.contains("no platform user data directory"));
    }

    #[test]
    fn log_directory_creation_failure_falls_back_to_stderr() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let blocking_file = temporary_directory.path().join("not-a-directory");
        fs::write(&blocking_file, "file").unwrap();

        let setup = make_writer(Ok(blocking_file.join("logs")));
        assert_eq!(setup.destination, LogDestination::Stderr);
        assert!(
            setup
                .warning
                .as_deref()
                .unwrap()
                .contains("using stderr fallback")
        );
    }

    #[test]
    fn missing_log_filter_uses_info_without_a_warning() {
        let (filter, warning) = configured_filter(None);
        assert_eq!(filter.to_string(), "info");
        assert!(warning.is_none());
    }

    #[test]
    fn valid_log_filter_is_used_without_a_warning() {
        let (filter, warning) = configured_filter(Some(std::ffi::OsStr::new("aaadaw=debug")));
        assert_eq!(filter.to_string(), "aaadaw=debug");
        assert!(warning.is_none());
    }

    #[test]
    fn invalid_log_filter_falls_back_to_info_with_a_warning() {
        let (filter, warning) = configured_filter(Some(std::ffi::OsStr::new("==invalid")));
        assert_eq!(filter.to_string(), "info");
        assert!(warning.unwrap().contains("using info"));
    }

    #[test]
    fn rolling_writer_bounds_file_size_and_retained_file_count() {
        let directory = tempfile::tempdir().unwrap();
        let mut writer = BoundedRollingWriter::open(directory.path(), 4, 3).unwrap();
        writer.write_all(b"abcdefghijk").unwrap();
        writer.flush().unwrap();

        let files = [
            directory.path().join("aaadaw.log"),
            directory.path().join("aaadaw.1.log"),
            directory.path().join("aaadaw.2.log"),
            directory.path().join("aaadaw.3.log"),
        ];
        assert!(files[..3].iter().all(|path| path.exists()));
        assert!(!files[3].exists());
        assert!(
            files[..3]
                .iter()
                .all(|path| fs::metadata(path).unwrap().len() <= 4)
        );
    }
}
