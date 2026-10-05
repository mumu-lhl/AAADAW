//! Control-thread ownership for one supervised CLAP instrument helper process.

use crate::{ClapIpcConfig, ClapIpcMapping, ClapIpcRegion};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const HELPER_COMMAND: &str = "--clap-instrument-helper";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(500);

/// Owns the shared mapping and child process for one isolated instrument.
///
/// Create, inspect, and stop this value from the control thread. Its mapping methods are bounded
/// and callback-safe; process management methods are not.
pub struct ClapInstrumentHelperProcess {
    mapping: ClapIpcMapping,
    child: Option<Child>,
    executable: PathBuf,
    entry_path: PathBuf,
    plugin_id: String,
    config: ClapIpcConfig,
    observed_heartbeat: u64,
    heartbeat_changed_at: Instant,
}

impl ClapInstrumentHelperProcess {
    /// Starts the current AAADAW executable in helper mode and waits for plugin activation.
    pub fn spawn(
        executable: &Path,
        entry_path: &Path,
        plugin_id: &str,
        config: ClapIpcConfig,
    ) -> io::Result<Self> {
        let mapping = ClapIpcMapping::create(config)?;
        // SAFETY: the mapping owner keeps this fixed-size private file alive until the child exits.
        let mapping_path = unsafe { mapping.path() }.ok_or_else(|| {
            io::Error::other("new CLAP helper mapping has no private backing path")
        })?;
        let child = spawn_helper(executable, &mapping_path, entry_path, plugin_id, config)?;
        let mut process = Self {
            mapping,
            child: Some(child),
            executable: executable.to_path_buf(),
            entry_path: entry_path.to_path_buf(),
            plugin_id: plugin_id.to_owned(),
            config,
            observed_heartbeat: 0,
            heartbeat_changed_at: Instant::now(),
        };
        if let Err(error) = process.wait_until_ready() {
            process.terminate_child();
            return Err(error);
        }
        Ok(process)
    }

    /// Returns the shared region for bounded callback-side submission and response reads.
    pub fn region(&self) -> &ClapIpcRegion {
        self.mapping.region()
    }

    /// Returns the helper's process exit status when it has exited.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        let status = child.try_wait()?;
        if status.is_some() && !self.mapping.region().is_shutdown() {
            self.mapping.region().recover_after_helper_exit();
            if !self.mapping.region().is_faulted() {
                self.mapping.region().mark_faulted(6);
            }
        }
        Ok(status)
    }

    /// Returns true and faults this instrument if its helper stops progressing past `timeout`.
    pub fn is_stalled(&mut self, timeout: Duration) -> bool {
        if !self.mapping.region().is_ready() {
            return self.mapping.region().is_faulted();
        }
        let heartbeat = self.mapping.region().helper_heartbeat();
        if heartbeat != self.observed_heartbeat {
            self.observed_heartbeat = heartbeat;
            self.heartbeat_changed_at = Instant::now();
            return false;
        }
        let stalled = self.heartbeat_changed_at.elapsed() >= timeout;
        if stalled {
            self.mapping.region().mark_faulted(5);
            if let Some(child) = self.child.as_mut()
                && child.try_wait().ok().flatten().is_none()
            {
                let _ = child.kill();
            }
        }
        stalled
    }

    /// Restarts a confirmed-exited helper using the same mapping and plugin reference.
    pub fn restart(&mut self) -> io::Result<()> {
        if self.child.is_some() && self.try_wait()?.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "CLAP helper must exit before it can be restarted",
            ));
        }
        self.child.take();
        // SAFETY: this owner retains the fixed-size backing file throughout helper restart.
        let mapping_path = unsafe { self.mapping.path() }
            .ok_or_else(|| io::Error::other("CLAP helper mapping no longer has a backing path"))?;
        self.mapping.region().begin_startup();
        let child = match spawn_helper(
            &self.executable,
            &mapping_path,
            &self.entry_path,
            &self.plugin_id,
            self.config,
        ) {
            Ok(child) => child,
            Err(error) => {
                self.mapping.region().mark_faulted(7);
                return Err(error);
            }
        };
        self.child = Some(child);
        self.observed_heartbeat = self.mapping.region().helper_heartbeat();
        self.heartbeat_changed_at = Instant::now();
        if let Err(error) = self.wait_until_ready() {
            self.terminate_child();
            if !self.mapping.region().is_faulted() {
                self.mapping.region().mark_faulted(7);
            }
            return Err(error);
        }
        Ok(())
    }

    /// Signals orderly shutdown, then reaps or terminates the helper on this control thread.
    pub fn shutdown(mut self) -> io::Result<Option<ExitStatus>> {
        self.mapping.region().mark_shutdown();
        let Some(mut child) = self.child.take() else {
            return Ok(None);
        };
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(Some(status));
            }
            if Instant::now() >= deadline {
                child.kill()?;
                return child.wait().map(Some);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_until_ready(&mut self) -> io::Result<()> {
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        loop {
            if self.mapping.region().is_ready() {
                return Ok(());
            }
            if self.mapping.region().is_faulted() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "CLAP helper rejected startup with fault code {}",
                        self.mapping.region().fault_code()
                    ),
                ));
            }
            if let Some(status) = self.try_wait()? {
                return Err(io::Error::other(format!(
                    "CLAP helper exited during startup with status {status}"
                )));
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "CLAP helper did not complete startup handshake in time",
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn terminate_child(&mut self) {
        self.mapping.region().mark_shutdown();
        if let Some(mut child) = self.child.take() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
            self.mapping.region().recover_after_helper_exit();
        }
    }
}

fn spawn_helper(
    executable: &Path,
    mapping_path: &Path,
    entry_path: &Path,
    plugin_id: &str,
    config: ClapIpcConfig,
) -> io::Result<Child> {
    Command::new(executable)
        .arg(HELPER_COMMAND)
        .arg(mapping_path)
        .arg(entry_path)
        .arg(plugin_id)
        .arg(config.sample_rate.to_string())
        .arg(config.max_block_frames.to_string())
        .arg(config.event_capacity.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
}

impl Drop for ClapInstrumentHelperProcess {
    fn drop(&mut self) {
        self.terminate_child();
    }
}
