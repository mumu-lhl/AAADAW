use aaadaw_core::{DawAction, MidiNoteData, Project, VolumeAutomationPoint};
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
        .apply(DawAction::CreateBusTrack {
            index: 1,
            name: "Music Bus".to_owned(),
        })
        .unwrap();
    let bus_id = project.tracks()[1].id();
    project
        .apply(DawAction::SetTrackVolume {
            track_id,
            volume_db: -4.5,
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackPan {
            track_id,
            pan: 0.25,
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackMute {
            track_id,
            muted: true,
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackSolo {
            track_id,
            solo: true,
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackRecordArm {
            track_id,
            armed: true,
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackOutput {
            track_id,
            output_track: Some(bus_id),
        })
        .unwrap();
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
        json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {
                "name": "daw_quantize_midi_item",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": item_id.value(),
                    "grid_numerator": 1,
                    "grid_denominator": 16
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 13,
            "method": "tools/call",
            "params": {
                "name": "daw_set_volume_automation_point",
                "arguments": {"track_id": 1, "sample": 0, "gain_db": -3}
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 14,
            "method": "tools/call",
            "params": {
                "name": "daw_set_track_record_arm",
                "arguments": {"track_id": 1, "armed": true}
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 15,
            "method": "tools/call",
            "params": {
                "name": "daw_set_track_mix",
                "arguments": {"track_id": 1, "volume_db": -3}
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 16,
            "method": "tools/call",
            "params": {"name": "daw_undo", "arguments": {}}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 17,
            "method": "tools/call",
            "params": {"name": "daw_redo", "arguments": {}}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 18,
            "method": "tools/call",
            "params": {"name": "daw_set_tempo_point", "arguments": {"tick": 960, "bpm": 90}}
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
    assert!(
        response_for(7)["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool["name"] != "daw_set_volume_automation_point")
    );
    assert!(
        response_for(7)["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool["name"] != "daw_set_track_record_arm")
    );
    assert!(
        response_for(7)["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool["name"] != "daw_set_track_mix")
    );
    assert!(
        response_for(7)["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool["name"] != "daw_set_tempo_point")
    );
    assert!(
        response_for(7)["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| !matches!(tool["name"].as_str(), Some("daw_undo" | "daw_redo")))
    );
    let structure = response_for(4)["result"]["contents"][0]["text"]
        .as_str()
        .unwrap();
    let structure: Value = serde_json::from_str(structure).unwrap();
    assert_eq!(structure["tracks"][0]["id"], track_id.value());
    assert_eq!(structure["tracks"][0]["name"], "Keys");
    assert_eq!(structure["tracks"][0]["volume_db"], -4.5);
    assert_eq!(structure["tracks"][0]["pan"], 0.25);
    assert_eq!(structure["tracks"][0]["muted"], true);
    assert_eq!(structure["tracks"][0]["solo"], true);
    assert_eq!(structure["tracks"][0]["record_armed"], true);
    assert_eq!(structure["tracks"][0]["output_track_id"], bus_id.value());
    assert_eq!(structure["tracks"][1]["type"], "bus");
    assert_eq!(structure["tracks"][1]["output_track_id"], Value::Null);
    assert_eq!(structure["tempo_points"].as_array().unwrap().len(), 1);
    assert_eq!(structure["tempo_points"][0]["tick"], 0);
    assert_eq!(structure["tempo_points"][0]["bpm"], 120.0);
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
    assert_eq!(response_for(11)["result"]["isError"], true);
    assert_eq!(response_for(13)["result"]["isError"], true);
    assert_eq!(response_for(14)["result"]["isError"], true);
    assert_eq!(response_for(15)["result"]["isError"], true);
    assert_eq!(response_for(16)["result"]["isError"], true);
    assert_eq!(response_for(17)["result"]["isError"], true);
    assert_eq!(response_for(18)["result"]["isError"], true);
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
fn explicitly_authorized_mcp_sets_project_tempo_undoably_and_persistently() {
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("mcp-tempo-test.aaadaw");
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
    let call = |id, arguments| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": "daw_set_tempo_point", "arguments": arguments}
        })
    };
    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "aaadaw-tempo-test", "version": "0.1"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        call(3, json!({"tick": 960, "bpm": 90.0})),
        call(4, json!({"tick": 960, "bpm": 90.0})),
        call(5, json!({"tick": 960, "bpm": 100.0})),
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {"name": "daw_undo", "arguments": {}}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "resources/read",
            "params": {"uri": "daw://project/structure"}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {"name": "daw_redo", "arguments": {}}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "resources/read",
            "params": {"uri": "daw://project/structure"}
        }),
        call(10, json!({"tick": 1920, "bpm": -1.0})),
        call(11, json!({"tick": 1.5, "bpm": 120.0})),
        call(12, json!({"tick": 1920, "bpm": 120.0, "extra": true})),
        call(13, json!({"tick": u64::MAX, "bpm": 120.0})),
        call(14, json!({"bpm": 120.0})),
        call(15, json!({"tick": 1920})),
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
            .any(|tool| tool["name"] == "daw_set_tempo_point")
    );
    assert_eq!(
        response_for(3)["result"]["structuredContent"]["changed"],
        true
    );
    assert_eq!(response_for(3)["result"]["structuredContent"]["tick"], 960);
    assert_eq!(response_for(3)["result"]["structuredContent"]["bpm"], 90.0);
    assert_eq!(
        response_for(4)["result"]["structuredContent"]["changed"],
        false
    );
    assert_eq!(response_for(4)["result"]["structuredContent"]["tick"], 960);
    assert_eq!(response_for(4)["result"]["structuredContent"]["bpm"], 90.0);
    assert_eq!(
        response_for(5)["result"]["structuredContent"]["changed"],
        true
    );
    assert_eq!(
        response_for(6)["result"]["structuredContent"]["changed"],
        true
    );
    assert_eq!(
        response_for(8)["result"]["structuredContent"]["changed"],
        true
    );

    let tempo_points = |id| {
        let structure: Value = serde_json::from_str(
            response_for(id)["result"]["contents"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        structure["tempo_points"].as_array().unwrap().clone()
    };
    assert!(
        tempo_points(7)
            .iter()
            .any(|point| point["tick"] == 960 && point["bpm"] == 90.0)
    );
    assert!(
        tempo_points(9)
            .iter()
            .any(|point| point["tick"] == 960 && point["bpm"] == 100.0)
    );
    for id in [10, 11, 12, 13, 14, 15] {
        assert_eq!(response_for(id)["result"]["isError"], true);
    }

    let reopened = ProjectStore::load_read_only(&project_path).unwrap();
    let tempo_points = reopened.tempo_points().collect::<Vec<_>>();
    assert_eq!(tempo_points.len(), 2);
    assert!(tempo_points.iter().all(|(tick, _, _)| *tick != 1920));
    assert_eq!(reopened.tempo_at_tick(960), 100.0);
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

#[test]
fn explicitly_authorized_mcp_quantize_is_undoable_validated_and_persistent() {
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("mcp-quantize-test.aaadaw");
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
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 110,
                    duration: 120,
                    velocity: 100,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 510,
                    duration: 120,
                    velocity: 90,
                },
            ],
        })
        .unwrap();
    let original_note_ids = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.id().value())
        .collect::<Vec<_>>();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3840,
        })
        .unwrap();
    let boundary_item_id = project.midi_items()[1].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id: boundary_item_id,
            notes: vec![MidiNoteData {
                pitch: 67,
                tick: 3800,
                duration: 30,
                velocity: 80,
            }],
        })
        .unwrap();
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
                "clientInfo": {"name": "aaadaw-quantize-test", "version": "0.1"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "daw_quantize_midi_item",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": item_id.value(),
                    "grid_numerator": 1,
                    "grid_denominator": 16,
                    "strength": 0.5
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 30,
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
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "daw_quantize_midi_item",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": item_id.value(),
                    "grid_numerator": 1,
                    "grid_denominator": 16
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "daw_quantize_midi_item",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": item_id.value(),
                    "grid_numerator": 1,
                    "grid_denominator": 16
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "daw_quantize_midi_item",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": item_id.value(),
                    "grid_numerator": 1,
                    "grid_denominator": 7
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "daw_quantize_midi_item",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": item_id.value(),
                    "grid_numerator": 1,
                    "grid_denominator": 16,
                    "strength": 1.1
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {
                "name": "daw_quantize_midi_item",
                "arguments": {
                    "track_id": other_track_id.value(),
                    "item_id": item_id.value(),
                    "grid_numerator": 1,
                    "grid_denominator": 16
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {
                "name": "daw_quantize_midi_item",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": boundary_item_id.value(),
                    "grid_numerator": 1,
                    "grid_denominator": 16
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "daw_quantize_midi_item",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": audio_item_id.value(),
                    "grid_numerator": 1,
                    "grid_denominator": 16
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {
                "name": "daw_quantize_midi_item",
                "arguments": {
                    "track_id": u64::MAX,
                    "item_id": item_id.value(),
                    "grid_numerator": 1,
                    "grid_denominator": 16
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "tools/call",
            "params": {
                "name": "daw_quantize_midi_item",
                "arguments": {
                    "track_id": track_id.value(),
                    "item_id": u64::MAX,
                    "grid_numerator": 1,
                    "grid_denominator": 16
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
            .any(|tool| tool["name"] == "daw_quantize_midi_item")
    );
    let partial_result = &response_for(3)["result"]["structuredContent"];
    assert_eq!(response_for(3)["result"]["isError"], false);
    assert_eq!(partial_result["strength"], 0.5);
    assert_eq!(partial_result["changed_note_ids"], json!(original_note_ids));
    let partial_ticks = response_for(30)["result"]["structuredContent"]["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| note["tick"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(partial_ticks, [55, 495, 3800]);
    let full_result = &response_for(4)["result"]["structuredContent"];
    assert_eq!(response_for(4)["result"]["isError"], false);
    assert_eq!(full_result["strength"], 1.0);
    assert_eq!(full_result["changed_note_ids"], json!(original_note_ids));
    assert_eq!(response_for(5)["result"]["isError"], false);
    assert_eq!(
        response_for(5)["result"]["structuredContent"]["changed_note_ids"],
        json!([])
    );
    for id in [6, 7, 8, 9, 10, 11, 12] {
        assert_eq!(response_for(id)["result"]["isError"], true);
    }

    let reopened = ProjectStore::load_read_only(&project_path).unwrap();
    let midi_item = reopened
        .midi_items()
        .iter()
        .find(|item| item.id() == item_id)
        .unwrap();
    assert_eq!(midi_item.notes()[0].tick(), 0);
    assert_eq!(midi_item.notes()[0].pitch(), 60);
    assert_eq!(midi_item.notes()[0].duration(), 120);
    assert_eq!(midi_item.notes()[0].velocity(), 100);
    assert_eq!(midi_item.notes()[1].tick(), 480);
    let boundary_item = reopened
        .midi_items()
        .iter()
        .find(|item| item.id() == boundary_item_id)
        .unwrap();
    assert_eq!(boundary_item.notes()[0].tick(), 3800);
}

#[test]
fn explicitly_authorized_mcp_sets_volume_automation_points_and_persists_them() {
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("mcp-volume-automation-test.aaadaw");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Lead".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::SetTrackVolumeAutomation {
            track_id,
            points: vec![
                VolumeAutomationPoint::new(12_000, -3.0).unwrap(),
                VolumeAutomationPoint::new(48_000, -9.0).unwrap(),
            ],
        })
        .unwrap();
    let mut store = ProjectStore::open(&project_path).unwrap();
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
    let call = |id, sample: Value, gain_db: Value| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": "daw_set_volume_automation_point",
                "arguments": {"track_id": track_id.value(), "sample": sample, "gain_db": gain_db}
            }
        })
    };
    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "aaadaw-volume-test", "version": "0.1"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        call(3, json!(24_000), json!(-6.0)),
        call(4, json!(24_000), json!(-12.0)),
        call(5, json!(24_000), json!(-12.0)),
        call(6, json!(u64::MAX), json!(-1.0)),
        call(7, json!(-1), json!(-1.0)),
        call(8, json!(30_000), json!(6.1)),
        call(9, json!(30_000), json!("NaN")),
        json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "daw_set_volume_automation_point",
                "arguments": {"track_id": u64::MAX, "sample": 30_000, "gain_db": -1.0}
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
            .any(|tool| tool["name"] == "daw_set_volume_automation_point")
    );
    assert_eq!(
        response_for(3)["result"]["structuredContent"]["changed"],
        true
    );
    assert_eq!(
        response_for(3)["result"]["structuredContent"]["point_count"],
        3
    );
    assert_eq!(
        response_for(4)["result"]["structuredContent"]["changed"],
        true
    );
    assert_eq!(
        response_for(5)["result"]["structuredContent"]["changed"],
        false
    );
    for id in [6, 7, 8, 9, 10] {
        assert_eq!(response_for(id)["result"]["isError"], true);
    }

    let reopened = ProjectStore::load_read_only(&project_path).unwrap();
    assert_eq!(
        reopened.tracks()[0].volume_automation(),
        [
            VolumeAutomationPoint::new(12_000, -3.0).unwrap(),
            VolumeAutomationPoint::new(24_000, -12.0).unwrap(),
            VolumeAutomationPoint::new(48_000, -9.0).unwrap(),
        ]
    );
}

#[test]
fn explicitly_authorized_mcp_sets_track_record_arm_state() {
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("mcp-record-arm-test.aaadaw");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Vocal".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    let mut store = ProjectStore::open(&project_path).unwrap();
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
    let call = |id, track: Value, armed: Value| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": "daw_set_track_record_arm",
                "arguments": {"track_id": track, "armed": armed}
            }
        })
    };
    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "aaadaw-record-arm-test", "version": "0.1"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        call(3, json!(track_id.value()), json!(true)),
        call(4, json!(track_id.value()), json!(true)),
        call(5, json!(track_id.value()), json!(false)),
        call(6, json!(track_id.value()), json!("true")),
        call(7, json!(u64::MAX), json!(true)),
        json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {
                "name": "daw_set_track_record_arm",
                "arguments": {"track_id": track_id.value()}
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
            .any(|tool| tool["name"] == "daw_set_track_record_arm")
    );
    assert_eq!(
        response_for(3)["result"]["structuredContent"]["armed"],
        true
    );
    assert_eq!(
        response_for(3)["result"]["structuredContent"]["changed"],
        true
    );
    assert_eq!(
        response_for(4)["result"]["structuredContent"]["changed"],
        false
    );
    assert_eq!(
        response_for(5)["result"]["structuredContent"]["armed"],
        false
    );
    assert_eq!(
        response_for(5)["result"]["structuredContent"]["changed"],
        true
    );
    for id in [6, 7, 8] {
        assert_eq!(response_for(id)["result"]["isError"], true);
    }
    let reopened = ProjectStore::load_read_only(&project_path).unwrap();
    assert!(!reopened.tracks()[0].is_record_armed());
}

#[test]
fn explicitly_authorized_mcp_sets_track_mix_as_one_persistent_edit() {
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("mcp-track-mix-test.aaadaw");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Lead".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    let mut store = ProjectStore::open(&project_path).unwrap();
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
    let call = |id, arguments| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": "daw_set_track_mix", "arguments": arguments}
        })
    };
    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "aaadaw-track-mix-test", "version": "0.1"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        call(
            3,
            json!({
                "track_id": track_id.value(),
                "volume_db": -6.0,
                "pan": 0.5,
                "muted": true,
                "solo": true
            }),
        ),
        call(
            4,
            json!({
                "track_id": track_id.value(),
                "volume_db": -6.0,
                "pan": 0.5,
                "muted": true,
                "solo": true
            }),
        ),
        call(5, json!({"track_id": track_id.value(), "pan": 0.25})),
        call(
            6,
            json!({"track_id": track_id.value(), "pan": 1.5, "muted": false}),
        ),
        call(7, json!({"track_id": u64::MAX, "volume_db": 2.0})),
        call(8, json!({"track_id": track_id.value()})),
        call(9, json!({"track_id": track_id.value(), "volume_db": 1e100})),
        call(10, json!({"track_id": track_id.value(), "muted": "false"})),
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
    assert!(tools.iter().any(|tool| tool["name"] == "daw_set_track_mix"));
    let initial = &response_for(3)["result"]["structuredContent"];
    assert_eq!(initial["changed"], true);
    assert_eq!(initial["volume_db"], -6.0);
    assert_eq!(initial["pan"], 0.5);
    assert_eq!(initial["muted"], true);
    assert_eq!(initial["solo"], true);
    assert_eq!(
        response_for(4)["result"]["structuredContent"]["changed"],
        false
    );
    let partial = &response_for(5)["result"]["structuredContent"];
    assert_eq!(partial["volume_db"], -6.0);
    assert_eq!(partial["pan"], 0.25);
    assert_eq!(partial["muted"], true);
    assert_eq!(partial["solo"], true);
    for id in [6, 7, 8, 9, 10] {
        assert_eq!(response_for(id)["result"]["isError"], true);
    }

    let reopened = ProjectStore::load_read_only(&project_path).unwrap();
    let track = &reopened.tracks()[0];
    assert_eq!(track.volume_db(), -6.0);
    assert_eq!(track.pan(), 0.25);
    assert!(track.is_muted());
    assert!(track.is_solo());
}

#[test]
fn mcp_undo_and_redo_apply_only_session_local_edits_and_persist() {
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("mcp-undo-redo-test.aaadaw");
    let mut store = ProjectStore::open(&project_path).unwrap();
    store.save(&Project::new()).unwrap();
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
                "clientInfo": {"name": "aaadaw-undo-redo-test", "version": "0.1"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "daw_undo", "arguments": {}}}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "daw_redo", "arguments": {}}}),
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {"name": "daw_create_track", "arguments": {"name": "Take"}}
        }),
        json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {"name": "daw_undo", "arguments": {}}}),
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "resources/read",
            "params": {"uri": "daw://project/structure"}
        }),
        json!({"jsonrpc": "2.0", "id": 8, "method": "tools/call", "params": {"name": "daw_redo", "arguments": {}}}),
        json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "resources/read",
            "params": {"uri": "daw://project/structure"}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {"name": "daw_undo", "arguments": {"unexpected": true}}
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
    assert!(tools.iter().any(|tool| tool["name"] == "daw_undo"));
    assert!(tools.iter().any(|tool| tool["name"] == "daw_redo"));
    assert_eq!(
        response_for(3)["result"]["structuredContent"]["changed"],
        false
    );
    assert_eq!(
        response_for(4)["result"]["structuredContent"]["changed"],
        false
    );
    assert_eq!(response_for(5)["result"]["isError"], false);
    assert_eq!(
        response_for(6)["result"]["structuredContent"]["changed"],
        true
    );
    let after_undo = serde_json::from_str::<Value>(
        response_for(7)["result"]["contents"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(after_undo["tracks"], json!([]));
    assert_eq!(
        response_for(8)["result"]["structuredContent"]["changed"],
        true
    );
    let after_redo = serde_json::from_str::<Value>(
        response_for(9)["result"]["contents"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(after_redo["tracks"][0]["name"], "Take");
    assert_eq!(response_for(10)["result"]["isError"], true);

    let reopened = ProjectStore::load_read_only(&project_path).unwrap();
    assert_eq!(reopened.tracks().len(), 1);
    assert_eq!(reopened.tracks()[0].name(), "Take");
}
