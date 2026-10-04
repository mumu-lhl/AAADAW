use aaadaw_core::{DawAction, MidiNoteData, Project};
use aaadaw_storage::{ProjectSessionLock, ProjectStore};
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn stdio_server_lists_and_reads_bounded_project_resources() {
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("mcp-test.aaadaw");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Keys".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3840,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 960,
                duration: 480,
                velocity: 100,
            }],
        })
        .unwrap();
    let store = ProjectStore::open(&project_path).unwrap();
    let mut store = store;
    store.save(&project).unwrap();
    store.close().unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_aaadaw"))
        .args([
            "mcp",
            "--stdio",
            "--project",
            project_path.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "aaadaw-test", "version": "0.1"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "resources/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "resources/templates/list"}),
        json!({"jsonrpc": "2.0", "id": 7, "method": "tools/list"}),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "resources/read",
            "params": {"uri": "daw://project/structure"}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "resources/read",
            "params": {"uri": format!("daw://project/track/{}/midi_summary", track_id.value())}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "resources/read",
            "params": {"uri": "daw://project/track/999999/midi_summary"}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {
                "name": "daw_scoped_query_notes",
                "arguments": {
                    "track_id": track_id.value(),
                    "start_tick": 0,
                    "end_tick": 3840
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {
                "name": "daw_create_track",
                "arguments": {"name": "Forbidden"}
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "daw_insert_midi_notes",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": item_id.value(),
                    "notes": [{"pitch": 64, "tick": 0, "duration": 480, "velocity": 90}]
                }
            }
        }),
        Value::String("{malformed json".to_owned()),
    ];
    {
        let stdin = child.stdin.as_mut().unwrap();
        for request in requests {
            if let Some(line) = request.as_str() {
                writeln!(stdin, "{line}").unwrap();
            } else {
                writeln!(stdin, "{}", request).unwrap();
            }
        }
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "MCP server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let response_for = |id| {
        responses
            .iter()
            .find(|response| response["id"] == id)
            .unwrap()
    };

    assert_eq!(response_for(1)["result"]["serverInfo"]["name"], "aaadaw");
    assert_eq!(
        response_for(2)["result"]["resources"][0]["uri"],
        "daw://project/structure"
    );
    assert_eq!(
        response_for(3)["result"]["resourceTemplates"][0]["uriTemplate"],
        "daw://project/track/{track_id}/midi_summary"
    );
    assert_eq!(
        response_for(7)["result"]["tools"][0]["name"],
        "daw_scoped_query_notes"
    );
    assert_eq!(
        response_for(7)["result"]["tools"].as_array().unwrap().len(),
        1
    );
    let structure = response_for(4)["result"]["contents"][0]["text"]
        .as_str()
        .unwrap();
    let structure: Value = serde_json::from_str(structure).unwrap();
    assert_eq!(structure["tracks"][0]["id"], track_id.value());
    assert_eq!(structure["tracks"][0]["name"], "Keys");
    let midi = response_for(5)["result"]["contents"][0]["text"]
        .as_str()
        .unwrap();
    let midi: Value = serde_json::from_str(midi).unwrap();
    assert_eq!(midi["track_id"], track_id.value());
    assert_eq!(midi["midi_item_count"], 1);
    assert_eq!(response_for(6)["error"]["code"], -32602);
    assert_eq!(response_for(8)["result"]["isError"], false);
    assert_eq!(
        response_for(8)["result"]["structuredContent"]["notes"][0]["tick"],
        960
    );
    assert_eq!(response_for(9)["result"]["isError"], true);
    assert_eq!(response_for(10)["result"]["isError"], true);
    // rmcp 3.5 skips malformed stdio lines and continues serving later requests.
    assert!(
        responses
            .iter()
            .all(|response| response["error"]["code"] != -32700)
    );
}

#[test]
fn explicitly_authorized_mcp_track_creation_persists_and_validates() {
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("mcp-write-test.aaadaw");
    let store = ProjectStore::open(&project_path).unwrap();
    store.close().unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_aaadaw"))
        .args([
            "mcp",
            "--stdio",
            "--project",
            project_path.to_str().unwrap(),
            "--write",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "aaadaw-write-test", "version": "0.1"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "daw_create_track", "arguments": {"name": "Lead Vocal"}}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {"name": "daw_create_track", "arguments": {"name": "  "}}
        }),
    ];
    {
        let stdin = child.stdin.as_mut().unwrap();
        for request in requests {
            writeln!(stdin, "{request}").unwrap();
        }
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "MCP writer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let response_for = |id| {
        responses
            .iter()
            .find(|response| response["id"] == id)
            .unwrap()
    };

    let tools = response_for(2)["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "daw_create_track"));
    assert_eq!(response_for(3)["result"]["isError"], false);
    assert_eq!(
        response_for(3)["result"]["structuredContent"]["name"],
        "Lead Vocal"
    );
    assert_eq!(response_for(4)["result"]["isError"], true);

    let project = ProjectStore::load_read_only(&project_path).unwrap();
    assert_eq!(project.tracks().len(), 1);
    assert_eq!(project.tracks()[0].name(), "Lead Vocal");
    assert_eq!(
        project.tracks()[0].id().value(),
        response_for(3)["result"]["structuredContent"]["track_id"]
            .as_u64()
            .unwrap()
    );
}

#[test]
fn an_mcp_writer_refuses_a_project_locked_by_another_session() {
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("mcp-lock-test.aaadaw");
    let store = ProjectStore::open(&project_path).unwrap();
    store.close().unwrap();
    let _session_lock = ProjectSessionLock::acquire(&project_path).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_aaadaw"))
        .args([
            "mcp",
            "--stdio",
            "--project",
            project_path.to_str().unwrap(),
            "--write",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already open"));
    let project = ProjectStore::load_read_only(project_path).unwrap();
    assert!(project.tracks().is_empty());
}

#[test]
fn explicitly_authorized_mcp_midi_note_insertion_is_atomic_and_persistent() {
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("mcp-midi-write-test.aaadaw");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Keys".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Other".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    let other_track_id = project.tracks()[1].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3840,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "test-media".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 44_100,
        })
        .unwrap();
    let audio_item_id = project.audio_items()[0].id();
    let store = ProjectStore::open(&project_path).unwrap();
    let mut store = store;
    store.save(&project).unwrap();
    store.close().unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_aaadaw"))
        .args([
            "mcp",
            "--stdio",
            "--project",
            project_path.to_str().unwrap(),
            "--write",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "aaadaw-midi-write-test", "version": "0.1"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "daw_insert_midi_notes",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": item_id.value(),
                    "notes": [
                        {"pitch": 60, "tick": 0, "duration": 480, "velocity": 100},
                        {"pitch": 64, "tick": 960, "duration": 480, "velocity": 92}
                    ]
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "daw_insert_midi_notes",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": item_id.value(),
                    "notes": [
                        {"pitch": 67, "tick": 1920, "duration": 480, "velocity": 88},
                        {"pitch": 70, "tick": 3800, "duration": 100, "velocity": 80}
                    ]
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "daw_insert_midi_notes",
                "arguments": {
                    "track_id": other_track_id.value(),
                    "item_id": item_id.value(),
                    "notes": [{"pitch": 67, "tick": 1920, "duration": 480, "velocity": 80}]
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {
                "name": "daw_insert_midi_notes",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": item_id.value(),
                    "notes": [{"pitch": 128, "tick": 1920, "duration": 480, "velocity": 80}]
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "daw_insert_midi_notes",
                "arguments": {
                    "track_id": u64::MAX,
                    "item_id": item_id.value(),
                    "notes": [{"pitch": 67, "tick": 1920, "duration": 480, "velocity": 80}]
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "daw_insert_midi_notes",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": u64::MAX,
                    "notes": [{"pitch": 67, "tick": 1920, "duration": 480, "velocity": 80}]
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {
                "name": "daw_insert_midi_notes",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": audio_item_id.value(),
                    "notes": [{"pitch": 67, "tick": 1920, "duration": 480, "velocity": 80}]
                }
            }
        }),
    ];
    {
        let stdin = child.stdin.as_mut().unwrap();
        for request in requests {
            writeln!(stdin, "{request}").unwrap();
        }
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "MCP writer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let response_for = |id| {
        responses
            .iter()
            .find(|response| response["id"] == id)
            .unwrap()
    };
    let tools = response_for(2)["result"]["tools"].as_array().unwrap();
    assert!(
        tools
            .iter()
            .any(|tool| tool["name"] == "daw_insert_midi_notes")
    );
    assert_eq!(
        response_for(3)["result"]["isError"],
        false,
        "{}",
        response_for(3)
    );
    let note_ids = response_for(3)["result"]["structuredContent"]["note_ids"]
        .as_array()
        .unwrap();
    assert_eq!(note_ids.len(), 2);
    assert_ne!(note_ids[0], note_ids[1]);
    assert_eq!(response_for(4)["result"]["isError"], true);
    assert_eq!(response_for(5)["result"]["isError"], true);
    assert_eq!(response_for(6)["result"]["isError"], true);
    assert_eq!(response_for(7)["result"]["isError"], true);
    assert_eq!(response_for(8)["result"]["isError"], true);
    assert_eq!(response_for(9)["result"]["isError"], true);

    let reopened = ProjectStore::load_read_only(&project_path).unwrap();
    let midi_item = reopened
        .midi_items()
        .iter()
        .find(|item| item.id() == item_id)
        .unwrap();
    assert_eq!(midi_item.notes().len(), 2);
    assert_eq!(
        midi_item.notes()[0].id().value(),
        note_ids[0].as_u64().unwrap()
    );
    assert_eq!(
        midi_item.notes()[1].id().value(),
        note_ids[1].as_u64().unwrap()
    );
}
