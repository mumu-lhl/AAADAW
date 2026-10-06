use aaadaw_engine::{ClapInstrumentHelperProcess, ClapIpcConfig, ClapIpcSubmitError};
use std::io;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const SAMPLE_RATE: u32 = 48_000;
const BLOCK_FRAMES: usize = 16;
const EVENT_CAPACITY: usize = 8;

fn config() -> ClapIpcConfig {
    ClapIpcConfig::new(SAMPLE_RATE, BLOCK_FRAMES, EVENT_CAPACITY).unwrap()
}

fn helper_path() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_aaadaw-engine-test-helper"))
}

fn spawn(plugin_id: &str) -> io::Result<ClapInstrumentHelperProcess> {
    spawn_with_entry(plugin_id, Path::new("unused-test-plugin.clap"))
}

fn spawn_with_entry(plugin_id: &str, entry_path: &Path) -> io::Result<ClapInstrumentHelperProcess> {
    ClapInstrumentHelperProcess::spawn(helper_path(), entry_path, plugin_id, config(), None)
}

fn spawn_with_entry_and_state(
    plugin_id: &str,
    entry_path: &Path,
    state: &[u8],
) -> io::Result<ClapInstrumentHelperProcess> {
    ClapInstrumentHelperProcess::spawn(helper_path(), entry_path, plugin_id, config(), Some(state))
}

#[test]
fn helper_protocol_mismatch_after_launch_prevents_playback() {
    let error = spawn("test.mismatch")
        .err()
        .expect("helper must reject mismatch");

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("rejected startup"));
}

#[test]
fn helper_stall_after_handshake_is_detected_and_terminated() {
    let mut process = spawn("test.stall").expect("fake helper should complete startup");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !process.is_stalled(Duration::from_millis(30)) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }

    assert!(process.region().is_faulted());
    assert_eq!(process.region().fault_code(), 5);
}

#[test]
fn helper_crash_and_restart_recover_child_owned_slots() {
    let test_files = TempDir::new().unwrap();
    let marker_path = test_files.path().join("helper-started");
    let mut process =
        spawn_with_entry_and_state("test.crash-restart", &marker_path, b"initial state")
            .expect("fake helper should complete startup");
    process
        .region()
        .try_submit(1, 1, 0, &[], 8)
        .expect("helper should receive a request before crashing");

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut exit_status = None;
    while exit_status.is_none() && Instant::now() < deadline {
        exit_status = process.try_wait().unwrap();
        thread::sleep(Duration::from_millis(5));
    }
    let exit_status = exit_status.expect("helper should crash while owning its request");
    assert!(!exit_status.success());
    assert!(process.region().is_faulted());

    process
        .restart()
        .expect("supervisor should restart the helper");
    assert!(process.region().is_ready());
    for sequence in 2..=5 {
        process
            .region()
            .try_submit(1, sequence, sequence * 8, &[], 8)
            .expect("restart should recover every child-owned slot");
    }
    assert_eq!(
        process.region().try_submit(1, 6, 48, &[], 8),
        Err(ClapIpcSubmitError::SlotsFull)
    );

    let status = process
        .shutdown()
        .expect("restarted helper should shut down cleanly")
        .expect("shutdown should return the child status");
    assert!(status.success());
    assert_eq!(
        process.take_saved_state().unwrap(),
        Some(b"state after restart".to_vec())
    );
}

#[test]
fn orderly_shutdown_makes_helper_state_available_to_the_owner() {
    let mut process = spawn("test.state-shutdown").expect("state helper should complete startup");
    let status = process
        .shutdown()
        .expect("helper should shut down cleanly")
        .expect("shutdown should return the child status");

    assert!(status.success());
    assert_eq!(
        process
            .take_saved_state()
            .expect("saved state should be readable after shutdown"),
        Some(b"helper shutdown state".to_vec())
    );
}
