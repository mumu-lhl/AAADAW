use aaadaw_app::{
    ClapPluginDescriptor, ClapPluginScanError, ClapPluginScanReport,
    scan_clap_plugins_with_inspector,
};
use aaadaw_engine::inspect_clap_plugin_entry;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const SCAN_COMMAND: &str = "__aaadaw_scan_clap_entry_v1";
const PROTOCOL_VERSION: u32 = 1;
const MAX_PATH_ARGUMENT_BYTES: usize = 8 * 1024;
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;
const SCAN_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const PROCESS_ERROR_PREFIX: &str = "CLAP scanner process:";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ScanResponse {
    version: u32,
    plugins: Vec<PluginDescriptor>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PluginDescriptor {
    plugin_id: String,
    name: String,
    vendor: Option<String>,
    features: Vec<String>,
}

pub(crate) fn scan_plugins(
    search_paths: &[PathBuf],
    skipped_errors: &[ClapPluginScanError],
) -> ClapPluginScanReport {
    scan_clap_plugins_with_inspector(search_paths, skipped_errors, inspect_entry)
}

fn inspect_entry(entry_path: &Path) -> Result<Vec<ClapPluginDescriptor>, String> {
    validate_path_argument(entry_path.as_os_str())?;
    let executable = std::env::current_exe()
        .map_err(|error| format!("{PROCESS_ERROR_PREFIX} could not locate AAADAW: {error}"))?;
    let response_file = tempfile::NamedTempFile::new().map_err(|error| {
        format!("{PROCESS_ERROR_PREFIX} could not create response file: {error}")
    })?;
    let response_path = response_file.into_temp_path();
    validate_path_argument(response_path.as_os_str())?;
    let mut command = Command::new(executable);
    command
        .arg(SCAN_COMMAND)
        .arg(entry_path)
        .arg(&response_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = run_child(&mut command)?;
    if !status.success() {
        return Err(format!("{PROCESS_ERROR_PREFIX} child exited with {status}"));
    }
    let metadata = fs::metadata(&response_path)
        .map_err(|error| format!("{PROCESS_ERROR_PREFIX} response missing: {error}"))?;
    if metadata.len() > MAX_RESPONSE_BYTES {
        return Err(format!(
            "{PROCESS_ERROR_PREFIX} response exceeded {MAX_RESPONSE_BYTES} bytes"
        ));
    }
    let response = fs::read(&response_path)
        .map_err(|error| format!("{PROCESS_ERROR_PREFIX} response read failed: {error}"))?;
    decode_response(entry_path, status, response)
}

fn validate_path_argument(path: &OsStr) -> Result<(), String> {
    if path.to_string_lossy().len() > MAX_PATH_ARGUMENT_BYTES {
        return Err(format!(
            "{PROCESS_ERROR_PREFIX} request path exceeds {MAX_PATH_ARGUMENT_BYTES} bytes"
        ));
    }
    Ok(())
}

fn decode_response(
    entry_path: &Path,
    status: std::process::ExitStatus,
    response_bytes: Vec<u8>,
) -> Result<Vec<ClapPluginDescriptor>, String> {
    if !status.success() {
        return Err(format!("{PROCESS_ERROR_PREFIX} child exited with {status}"));
    }
    if response_bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(format!(
            "{PROCESS_ERROR_PREFIX} response exceeded {MAX_RESPONSE_BYTES} bytes"
        ));
    }
    let response: ScanResponse = serde_json::from_slice(&response_bytes)
        .map_err(|error| format!("{PROCESS_ERROR_PREFIX} invalid response: {error}"))?;
    if response.version != PROTOCOL_VERSION {
        return Err(format!(
            "{PROCESS_ERROR_PREFIX} unsupported protocol version {}",
            response.version
        ));
    }
    if let Some(error) = response.error {
        if !response.plugins.is_empty() {
            return Err(format!(
                "{PROCESS_ERROR_PREFIX} response contains both plugins and an error"
            ));
        }
        return Err(error);
    }
    Ok(response
        .plugins
        .into_iter()
        .map(|plugin| ClapPluginDescriptor {
            entry_path: entry_path.to_owned(),
            plugin_id: plugin.plugin_id,
            name: plugin.name,
            vendor: plugin.vendor,
            features: plugin.features,
        })
        .collect())
}

fn run_child(command: &mut Command) -> Result<std::process::ExitStatus, String> {
    run_child_with_timeout(command, SCAN_TIMEOUT)
}

fn run_child_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<std::process::ExitStatus, String> {
    let mut child = command
        .spawn()
        .map_err(|error| format!("{PROCESS_ERROR_PREFIX} could not start: {error}"))?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{PROCESS_ERROR_PREFIX} timed out after {:.1} seconds",
                    timeout.as_secs_f64()
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{PROCESS_ERROR_PREFIX} wait failed: {error}"));
            }
        }
    };
    Ok(status)
}

pub(crate) fn run_helper(
    entry_path: &OsStr,
    response_path: &OsStr,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_path_argument(entry_path)?;
    validate_path_argument(response_path)?;
    let entry_path = Path::new(entry_path);
    // SAFETY: the parent scans entries from user-configured plug-in paths. The helper runs in a
    // separate process so a plug-in fault cannot terminate or hang the desktop process.
    let response = match unsafe { inspect_clap_plugin_entry(entry_path) } {
        Ok(plugins) => ScanResponse {
            version: PROTOCOL_VERSION,
            plugins: plugins.into_iter().map(PluginDescriptor::from).collect(),
            error: None,
        },
        Err(error) => ScanResponse {
            version: PROTOCOL_VERSION,
            plugins: Vec::new(),
            error: Some(error.to_string()),
        },
    };
    let response_file = File::create(response_path)?;
    let mut response_file = LimitedWriter::new(response_file, MAX_RESPONSE_BYTES);
    serde_json::to_writer(&mut response_file, &response)?;
    response_file.flush()?;
    Ok(())
}

struct LimitedWriter<W> {
    inner: W,
    remaining: u64,
}

impl<W> LimitedWriter<W> {
    fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
        }
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::other("scanner response limit exceeded"));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl From<ClapPluginDescriptor> for PluginDescriptor {
    fn from(plugin: ClapPluginDescriptor) -> Self {
        Self {
            plugin_id: plugin.plugin_id,
            name: plugin.name,
            vendor: plugin.vendor,
            features: plugin.features,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::ExitStatus;

    fn shell(script: &str) -> Command {
        #[cfg(unix)]
        let command = {
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            command
        };
        #[cfg(windows)]
        let command = {
            let mut command = Command::new("powershell");
            command.args(["-NoProfile", "-Command", script]);
            command
        };
        command
    }

    fn successful_status() -> ExitStatus {
        #[cfg(unix)]
        let status = Command::new("true").status().unwrap();
        #[cfg(windows)]
        let status = Command::new("cmd").args(["/C", "exit 0"]).status().unwrap();
        status
    }

    #[test]
    fn valid_response_decodes_and_maps_entry_path() {
        let response = ScanResponse {
            version: PROTOCOL_VERSION,
            plugins: vec![PluginDescriptor {
                plugin_id: "org.example.synth".to_owned(),
                name: "Synth".to_owned(),
                vendor: Some("Example".to_owned()),
                features: vec!["instrument".to_owned()],
            }],
            error: None,
        };
        let path = PathBuf::from("/plugins/synth.clap");
        let plugins = decode_response(
            &path,
            successful_status(),
            serde_json::to_vec(&response).unwrap(),
        )
        .unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].entry_path, path);
        assert_eq!(plugins[0].plugin_id, "org.example.synth");
    }

    #[test]
    fn invalid_and_mismatched_responses_are_rejected() {
        let path = Path::new("broken.clap");
        let malformed =
            decode_response(path, successful_status(), b"not-json".to_vec()).unwrap_err();
        assert!(malformed.contains("invalid response"));

        let wrong_version = ScanResponse {
            version: PROTOCOL_VERSION + 1,
            plugins: Vec::new(),
            error: None,
        };
        let error = decode_response(
            path,
            successful_status(),
            serde_json::to_vec(&wrong_version).unwrap(),
        )
        .unwrap_err();
        assert!(error.contains("unsupported protocol version"));
    }

    #[test]
    fn child_exit_and_timeout_become_process_errors() {
        let mut command = shell("exit 7");
        let status = run_child_with_timeout(&mut command, Duration::from_secs(1)).unwrap();
        assert!(!status.success());

        #[cfg(unix)]
        let mut command = shell("exec sleep 2");
        #[cfg(windows)]
        let mut command = shell("Start-Sleep -Seconds 2");
        let error = run_child_with_timeout(&mut command, Duration::from_millis(20)).unwrap_err();
        assert!(error.starts_with(PROCESS_ERROR_PREFIX));
        assert!(error.contains("timed out"));
    }

    #[cfg(unix)]
    #[test]
    fn signaled_child_does_not_take_down_the_parent() {
        use std::os::unix::process::ExitStatusExt;

        let mut command = shell("kill -SEGV $$");
        let status = run_child_with_timeout(&mut command, Duration::from_secs(1)).unwrap();
        assert_eq!(status.signal(), Some(11));
    }

    #[test]
    fn request_path_has_a_strict_size_limit() {
        let path = std::ffi::OsString::from("x".repeat(MAX_PATH_ARGUMENT_BYTES + 1));
        let error = validate_path_argument(&path).unwrap_err();
        assert!(error.contains("request path exceeds"));
    }

    #[test]
    fn response_writer_rejects_data_over_limit() {
        let mut output = LimitedWriter::new(Vec::new(), 3);
        assert_eq!(output.write(b"abc").unwrap(), 3);
        assert!(output.write(b"d").is_err());
        assert_eq!(output.inner, b"abc");
    }
}
