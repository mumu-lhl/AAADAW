use aaadaw_engine::{
    ClapInstrumentHelperProcess, ClapIpcConfig, ClapIpcMidiEvent, ClapIpcMidiKind,
    ClapIpcSubmitError,
};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
#[cfg(target_os = "linux")]
use x11rb::connection::Connection;
#[cfg(target_os = "linux")]
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt, EventMask, ExposeEvent, KeyButMask, KeyPressEvent,
};
#[cfg(target_os = "linux")]
use x11rb::rust_connection::RustConnection;

const SAMPLE_RATE: u32 = 48_000;
const BLOCK_FRAMES: usize = 16;
const EVENT_CAPACITY: usize = 8;

fn config() -> ClapIpcConfig {
    ClapIpcConfig::new(SAMPLE_RATE, BLOCK_FRAMES, EVENT_CAPACITY).unwrap()
}

fn helper_path() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_aaadaw-engine-test-helper"))
}

fn build_test_clap_plugin() -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("engine crate is part of the workspace");
    let manifest = workspace.join("tests/fixtures/clap-synth/Cargo.toml");
    let status = Command::new("cargo")
        .args([
            "build",
            "--package",
            "aaadaw-test-clap-plugin",
            "--manifest-path",
        ])
        .arg(&manifest)
        .current_dir(workspace)
        .status()
        .expect("Cargo should build the deterministic CLAP fixture");
    assert!(status.success(), "CLAP fixture build must succeed");

    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));
    let library_name = if cfg!(target_os = "windows") {
        "aaadaw_test_clap_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libaaadaw_test_clap_plugin.dylib"
    } else {
        "libaaadaw_test_clap_plugin.so"
    };
    let library = target.join("debug").join(library_name);
    assert!(
        library.is_file(),
        "built CLAP fixture exists at {library:?}"
    );
    library
}

fn midi_event(
    kind: ClapIpcMidiKind,
    frame_offset: u32,
    pitch: u8,
    velocity: u8,
    controller: u8,
) -> ClapIpcMidiEvent {
    ClapIpcMidiEvent {
        frame_offset,
        note_id: ClapIpcMidiEvent::NO_NOTE_ID,
        pitch_bend: ClapIpcMidiEvent::NO_PITCH_BEND,
        kind: kind as u8,
        pitch,
        velocity,
        controller,
        reserved: [0; 2],
    }
}

fn render_helper_block(
    process: &ClapInstrumentHelperProcess,
    sequence: u64,
    events: &[ClapIpcMidiEvent],
) -> [[f32; 2]; 16] {
    process
        .audio_port()
        .try_submit(1, sequence, sequence * 16, events, 16)
        .expect("fixture helper accepts a bounded MIDI block");
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut output = [[0.0; 2]; 16];
    loop {
        if process
            .audio_port()
            .try_read_response(1, sequence, sequence * 16, &mut output)
        {
            return output;
        }
        assert!(
            Instant::now() < deadline,
            "CLAP helper should render its block"
        );
        thread::sleep(Duration::from_millis(1));
    }
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
fn state_restore_startup_failure_is_distinguishable_from_plugin_load_failure() {
    let error = spawn("test.state-restore-failure")
        .err()
        .expect("helper must reject state restore failure");
    assert!(error.to_string().contains("state restore"));
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
    for sequence in 2..=13 {
        process
            .region()
            .try_submit(1, sequence, sequence * 8, &[], 8)
            .expect("restart should recover every child-owned slot");
    }
    assert_eq!(
        process.region().try_submit(1, 14, 112, &[], 8),
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

#[test]
fn orderly_shutdown_reports_no_state_for_a_stateless_helper() {
    let mut process =
        spawn("test.stateless-shutdown").expect("stateless helper should complete startup");
    process
        .shutdown()
        .expect("stateless helper should shut down cleanly");

    assert_eq!(process.take_saved_state().unwrap(), None);
    assert_eq!(process.take_saved_state().unwrap(), None);
}

#[test]
fn dynamically_loaded_child_plugin_renders_midi_and_roundtrips_saved_state() {
    let fixture = build_test_clap_plugin();
    let mut process = ClapInstrumentHelperProcess::spawn(
        helper_path(),
        &fixture,
        "test.dynamic-clap",
        config(),
        None,
    )
    .expect("helper should load the dynamic CLAP fixture");

    let events = [
        midi_event(ClapIpcMidiKind::ControllerChange, 0, 0, 96, 7),
        midi_event(
            ClapIpcMidiKind::NoteOn,
            0,
            60,
            100,
            ClapIpcMidiEvent::NO_CONTROLLER,
        ),
    ];
    let output = render_helper_block(&process, 0, &events);
    let expected = 96.0 / 127.0;
    assert!((output[0][0] - expected).abs() < 0.001);
    assert!((output[15][1] - expected).abs() < 0.001);

    let saved_state = process
        .save_plugin_state()
        .expect("plugin state should save through the helper")
        .expect("fixture plugin exposes persistent state");
    assert_eq!(saved_state, [96]);
    process.shutdown().unwrap();
    assert_eq!(
        process.take_saved_state().unwrap(),
        Some(saved_state.clone())
    );

    let mut restored = ClapInstrumentHelperProcess::spawn(
        helper_path(),
        &fixture,
        "test.dynamic-clap",
        config(),
        Some(&saved_state),
    )
    .expect("helper should restore saved CLAP state before activation");
    let note_on = [midi_event(
        ClapIpcMidiKind::NoteOn,
        0,
        60,
        100,
        ClapIpcMidiEvent::NO_CONTROLLER,
    )];
    let restored_output = render_helper_block(&restored, 0, &note_on);
    assert!((restored_output[0][0] - expected).abs() < 0.001);
    restored.shutdown().unwrap();
    restored.take_saved_state().unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn floating_editor_can_close_reopen_and_leave_instrument_audio_running() {
    let fixture = build_test_clap_plugin();
    let mut process = ClapInstrumentHelperProcess::spawn(
        helper_path(),
        &fixture,
        "test.dynamic-clap",
        config(),
        None,
    )
    .expect("helper should load the dynamic CLAP fixture");
    wait_for_gui_status(&process, 0);
    for expected_status in [1, 1, 0, 0, 1, 0] {
        let open = expected_status == 1;
        let sequence = process
            .request_gui(open, 0)
            .expect("GUI request uses its own bounded control field");
        wait_for_gui_request(&process, sequence, expected_status);
    }
    let events = [midi_event(
        ClapIpcMidiKind::NoteOn,
        0,
        60,
        100,
        ClapIpcMidiEvent::NO_CONTROLLER,
    )];
    let output = render_helper_block(&process, 0, &events);
    assert!(output.iter().any(|frame| frame[0] > 0.0));
    process.shutdown().unwrap();
    process.take_saved_state().unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn clap_posix_fd_callbacks_run_on_helper_main_thread_while_audio_continues() {
    let fixture = build_test_clap_plugin();
    let mut process = ClapInstrumentHelperProcess::spawn(
        helper_path(),
        &fixture,
        "test.dynamic-clap",
        config(),
        None,
    )
    .expect("helper should load the dynamic CLAP fixture");
    wait_for_gui_status(&process, 0);

    let sequence = process
        .request_gui(true, 0)
        .expect("GUI request uses its own bounded control field");
    wait_for_gui_request(&process, sequence, 1);
    let (x11, window) = find_fd_fixture_window();
    x11.send_event(
        false,
        window,
        EventMask::EXPOSURE,
        ExposeEvent {
            response_type: x11rb::protocol::xproto::EXPOSE_EVENT,
            sequence: 0,
            window,
            x: 0,
            y: 0,
            width: 320,
            height: 200,
            count: 0,
        },
    )
    .unwrap()
    .check()
    .unwrap();

    let note_on = [midi_event(
        ClapIpcMidiKind::NoteOn,
        0,
        60,
        100,
        ClapIpcMidiEvent::NO_CONTROLLER,
    )];
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut audio_sequence = 0;
    let mut output = [[0.0; 2]; 16];
    while Instant::now() < deadline {
        output = render_helper_block(&process, audio_sequence, &note_on);
        audio_sequence += 1;
        if (0.49..0.53).contains(&output[0][0]) {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        (0.49..0.53).contains(&output[0][0]),
        "the X11 Expose event should reach the non-GLib plugin event handler"
    );

    let root = x11.setup().roots[0].root;
    x11.send_event(
        false,
        window,
        EventMask::KEY_PRESS,
        KeyPressEvent {
            response_type: x11rb::protocol::xproto::KEY_PRESS_EVENT,
            detail: 38,
            sequence: 0,
            time: x11rb::CURRENT_TIME,
            root,
            event: window,
            child: 0,
            root_x: 0,
            root_y: 0,
            event_x: 12,
            event_y: 12,
            state: KeyButMask::default(),
            same_screen: true,
        },
    )
    .unwrap()
    .check()
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        output = render_helper_block(&process, audio_sequence, &note_on);
        audio_sequence += 1;
        if (0.74..0.78).contains(&output[0][0]) {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        (0.74..0.78).contains(&output[0][0]),
        "X11 input should trigger the plugin's GLib-integrated event source"
    );

    x11.destroy_window(window).unwrap().check().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while process.gui_status() != 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        process.gui_status(),
        0,
        "user window close reaches CLAP host"
    );
    let output = render_helper_block(&process, audio_sequence, &[]);
    assert!(output.iter().all(|frame| (0.74..0.78).contains(&frame[0])));
    process.shutdown().unwrap();
    process.take_saved_state().unwrap();
}

#[cfg(target_os = "linux")]
fn find_fd_fixture_window() -> (RustConnection, u32) {
    let (connection, screen_index) = x11rb::connect(None).expect("test client connects to Xvfb");
    let screen = &connection.setup().roots[screen_index];
    let children = connection
        .query_tree(screen.root)
        .unwrap()
        .reply()
        .unwrap()
        .children;
    let window = children
        .into_iter()
        .find(|window| {
            connection
                .get_property(false, *window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 64)
                .unwrap()
                .reply()
                .is_ok_and(|property| property.value == b"AAADAW CLAP fd fixture")
        })
        .expect("fixture editor should publish its X11 window name");
    (connection, window)
}

#[cfg(target_os = "windows")]
#[test]
fn unsupported_editor_status_does_not_stop_instrument_audio() {
    let fixture = build_test_clap_plugin();
    let mut process = ClapInstrumentHelperProcess::spawn(
        helper_path(),
        &fixture,
        "test.dynamic-clap",
        config(),
        None,
    )
    .expect("helper should load the dynamic CLAP fixture");
    process.request_gui(true, 0).unwrap();
    wait_for_gui_status(&process, 2);
    let events = [midi_event(
        ClapIpcMidiKind::NoteOn,
        0,
        60,
        100,
        ClapIpcMidiEvent::NO_CONTROLLER,
    )];
    let output = render_helper_block(&process, 0, &events);
    assert!(output.iter().any(|frame| frame[0] > 0.0));
    process.shutdown().unwrap();
    process.take_saved_state().unwrap();
}

fn wait_for_gui_status(process: &ClapInstrumentHelperProcess, expected: u32) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while process.gui_status() != expected && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(process.gui_status(), expected);
}

#[cfg(target_os = "linux")]
fn wait_for_gui_request(process: &ClapInstrumentHelperProcess, sequence: u64, expected: u32) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !process.region().gui_request_completed(sequence) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(process.region().gui_request_completed(sequence));
    assert_eq!(process.gui_status(), expected);
}

#[test]
fn active_helper_state_can_be_saved_without_stopping_its_process() {
    let mut process = spawn("test.live-state").expect("state helper should complete startup");
    let state = process
        .save_plugin_state()
        .expect("the helper should pause between blocks and save state");
    assert_eq!(state, Some(b"live helper state".to_vec()));
    assert!(process.region().is_ready());

    process
        .shutdown()
        .expect("helper should still shut down after a live state save");
    assert_eq!(
        process.take_saved_state().unwrap(),
        Some(b"final helper state".to_vec())
    );
}
