//! Control-thread ownership for one supervised CLAP instrument helper process.

use crate::clap_ipc::{
    clear_helper_state, read_helper_state, read_saved_helper_state, write_helper_state,
};
use crate::{ClapIpcAudioPort, ClapIpcConfig, ClapIpcMapping, ClapIpcRegion};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::{NamedTempFile, TempPath};

const HELPER_COMMAND: &str = "--clap-instrument-helper";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(500);
const STATE_SAVE_TIMEOUT: Duration = Duration::from_secs(2);
static NEXT_HELPER_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

enum SavedStateCache {
    Unread,
    Available(Option<Vec<u8>>),
    Consumed,
}

/// Owns the shared mapping and child process for one isolated instrument.
///
/// Create, inspect, and stop this value from the control thread. Its mapping methods are bounded
/// and callback-safe; process management methods are not.
pub struct ClapInstrumentHelperProcess {
    instance_id: u64,
    mapping: ClapIpcMapping,
    child: Option<Child>,
    executable: PathBuf,
    entry_path: PathBuf,
    plugin_id: String,
    config: ClapIpcConfig,
    state_input: TempPath,
    state_output: TempPath,
    saved_state: SavedStateCache,
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
        state: Option<&[u8]>,
    ) -> io::Result<Self> {
        Self::spawn_with_timeout(
            executable,
            entry_path,
            plugin_id,
            config,
            state,
            STARTUP_TIMEOUT,
        )
    }

    fn spawn_with_timeout(
        executable: &Path,
        entry_path: &Path,
        plugin_id: &str,
        config: ClapIpcConfig,
        state: Option<&[u8]>,
        startup_timeout: Duration,
    ) -> io::Result<Self> {
        let mapping = ClapIpcMapping::create(config)?;
        let state_input = NamedTempFile::new()?.into_temp_path();
        write_helper_state(&state_input, state)?;
        let state_output_file = NamedTempFile::new()?;
        let state_output = state_output_file.into_temp_path();
        clear_helper_state(&state_output)?;
        // SAFETY: the mapping owner keeps this fixed-size private file alive until the child exits.
        let mapping_path = unsafe { mapping.path() }.ok_or_else(|| {
            io::Error::other("new CLAP helper mapping has no private backing path")
        })?;
        let child = spawn_helper(
            executable,
            &mapping_path,
            entry_path,
            plugin_id,
            config,
            &state_input,
            &state_output,
        )?;
        let mut process = Self {
            instance_id: NEXT_HELPER_INSTANCE_ID.fetch_add(1, Ordering::Relaxed),
            mapping,
            child: Some(child),
            executable: executable.to_path_buf(),
            entry_path: entry_path.to_path_buf(),
            plugin_id: plugin_id.to_owned(),
            config,
            state_input,
            state_output,
            saved_state: SavedStateCache::Unread,
            observed_heartbeat: 0,
            heartbeat_changed_at: Instant::now(),
        };
        if let Err(error) = process.wait_until_ready(startup_timeout) {
            process.terminate_child();
            return Err(error);
        }
        Ok(process)
    }

    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// Returns the operating-system process ID while the helper is running.
    pub fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    pub fn config(&self) -> ClapIpcConfig {
        self.config
    }

    /// Returns the shared region for bounded callback-side submission and response reads.
    pub fn region(&self) -> &ClapIpcRegion {
        self.mapping.region()
    }

    /// Clones bounded audio access without transferring child-process ownership to the graph.
    pub fn audio_port(&self) -> ClapIpcAudioPort {
        self.mapping.audio_port()
    }

    /// Queues an editor open/close command without waiting on plugin GUI code.
    pub fn request_gui(&self, open: bool, parent: u64) -> io::Result<u64> {
        self.mapping
            .region()
            .request_gui(open, parent)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "a CLAP editor command is still pending",
                )
            })
    }

    /// Returns the last helper-reported editor lifecycle status.
    pub fn gui_status(&self) -> u32 {
        self.mapping.region().gui_status()
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
        if self.child.is_some() {
            match self.take_saved_state() {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }
        self.child.take();
        self.saved_state = SavedStateCache::Unread;
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
            &self.state_input,
            &self.state_output,
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
        if let Err(error) = self.wait_until_ready(STARTUP_TIMEOUT) {
            self.terminate_child();
            if !self.mapping.region().is_faulted() {
                self.mapping.region().mark_faulted(7);
            }
            return Err(error);
        }
        Ok(())
    }

    /// Returns the helper's saved CLAP state after it has exited and carries it into restarts.
    pub fn take_saved_state(&mut self) -> io::Result<Option<Vec<u8>>> {
        if let Some(child) = self.child.as_mut()
            && child.try_wait()?.is_none()
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "CLAP helper must exit before its state can be read",
            ));
        }
        match std::mem::replace(&mut self.saved_state, SavedStateCache::Consumed) {
            SavedStateCache::Available(state) => return Ok(state),
            SavedStateCache::Consumed => return Ok(None),
            SavedStateCache::Unread => {}
        }
        let state = read_saved_helper_state(&self.state_output, &self.state_input)?;
        write_helper_state(&self.state_input, state.as_deref())?;
        clear_helper_state(&self.state_output)?;
        Ok(state)
    }

    /// Requests a state snapshot while the helper audio worker is paused between process blocks.
    pub fn save_plugin_state(&mut self) -> io::Result<Option<Vec<u8>>> {
        if let Some(child) = self.child.as_mut()
            && child.try_wait()?.is_none()
        {
            let request = self.mapping.region().request_state_save().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "CLAP state save is already pending",
                )
            })?;
            let deadline = Instant::now() + STATE_SAVE_TIMEOUT;
            loop {
                if self.mapping.region().state_save_is_complete(request) {
                    if !self.mapping.region().state_save_succeeded() {
                        return Err(io::Error::other("CLAP helper could not save plugin state"));
                    }
                    let state = read_helper_state(&self.state_output)?;
                    write_helper_state(&self.state_input, state.as_deref())?;
                    clear_helper_state(&self.state_output)?;
                    return Ok(state);
                }
                if child.try_wait()?.is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "CLAP helper exited during state save",
                    ));
                }
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "CLAP helper state save timed out",
                    ));
                }
                thread::sleep(Duration::from_millis(1));
            }
        }
        self.take_saved_state()
    }

    /// Signals orderly shutdown, then reaps or terminates the helper on this control thread.
    /// Call [`Self::take_saved_state`] afterward to retrieve state written during shutdown.
    pub fn shutdown(&mut self) -> io::Result<Option<ExitStatus>> {
        self.mapping.region().mark_shutdown();
        let Some(mut child) = self.child.take() else {
            return Ok(None);
        };
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        loop {
            if let Some(status) = child.try_wait()? {
                if status.success() {
                    self.saved_state = SavedStateCache::Available(self.take_saved_state()?);
                }
                return Ok(Some(status));
            }
            if Instant::now() >= deadline {
                child.kill()?;
                let status = child.wait()?;
                self.mapping.region().recover_after_helper_exit();
                return Ok(Some(status));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_until_ready(&mut self, timeout: Duration) -> io::Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.mapping.region().is_ready() {
                return Ok(());
            }
            if self.mapping.region().is_faulted() {
                let fault_code = self.mapping.region().fault_code();
                let message = if fault_code == 3 {
                    "CLAP helper rejected startup because saved plugin state restore failed (fault code 3)".to_owned()
                } else {
                    format!("CLAP helper rejected startup with fault code {fault_code}")
                };
                return Err(io::Error::new(io::ErrorKind::InvalidData, message));
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
    state_input: &Path,
    state_output: &Path,
) -> io::Result<Child> {
    Command::new(executable)
        .arg(HELPER_COMMAND)
        .arg(mapping_path)
        .arg(entry_path)
        .arg(plugin_id)
        .arg(config.sample_rate.to_string())
        .arg(config.max_block_frames.to_string())
        .arg(config.event_capacity.to_string())
        .arg(state_input)
        .arg(state_output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
}

impl Drop for ClapInstrumentHelperProcess {
    fn drop(&mut self) {
        self.terminate_child();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn fake_helper(contents: &str) -> TempPath {
        let helper = NamedTempFile::new().unwrap();
        fs::write(
            helper.path(),
            format!("#!/bin/sh\nulimit -c 0\n{contents}\n"),
        )
        .unwrap();
        fs::set_permissions(helper.path(), fs::Permissions::from_mode(0o700)).unwrap();
        helper.into_temp_path()
    }

    fn config() -> ClapIpcConfig {
        ClapIpcConfig::new(48_000, 16, 8).unwrap()
    }

    #[test]
    fn helper_exit_during_startup_is_reported_and_reaped() {
        let helper = fake_helper("exit 23");
        let error = ClapInstrumentHelperProcess::spawn_with_timeout(
            &helper,
            Path::new("unused-plugin.clap"),
            "test.plugin",
            config(),
            None,
            Duration::from_secs(1),
        )
        .err()
        .expect("fake helper must exit before the startup handshake");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(error.to_string().contains("exited during startup"));
        assert!(error.to_string().contains("23"));
    }

    #[test]
    fn helper_startup_stall_times_out_and_terminates_child() {
        let helper = fake_helper("exec sleep 5");
        let started_at = Instant::now();
        let error = ClapInstrumentHelperProcess::spawn_with_timeout(
            &helper,
            Path::new("unused-plugin.clap"),
            "test.plugin",
            config(),
            None,
            Duration::from_millis(25),
        )
        .err()
        .expect("fake helper must time out before the startup handshake");

        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started_at.elapsed() < Duration::from_secs(1));
    }
}
