use directories::ProjectDirs;
use std::{env, fs, io, path::PathBuf};
use tracing_appender::{
    non_blocking::{NonBlocking, WorkerGuard},
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::EnvFilter;

const LOG_DIRECTORY_ENV: &str = "AAADAW_LOG_DIR";
const LOG_FILE_PREFIX: &str = "aaadaw";
const MAX_LOG_FILES: usize = 7;

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
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|error| {
        eprintln!("AAADAW logging: invalid RUST_LOG filter ({error}); using info");
        EnvFilter::new("info")
    });
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

fn create_file_appender(directory: &std::path::Path) -> Result<RollingFileAppender, String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create log directory: {error}"))?;
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(directory)
        .map_err(|error| format!("cannot create log files: {error}"))
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
}
