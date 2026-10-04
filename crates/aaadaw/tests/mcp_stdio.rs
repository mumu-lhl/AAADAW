use aaadaw_core::{DawAction, Project};
use aaadaw_storage::ProjectStore;
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
    assert_eq!(midi["midi_item_count"], 0);
    assert_eq!(response_for(6)["error"]["code"], -32602);
    assert!(
        responses
            .iter()
            .any(|response| response["error"]["code"] == -32700)
    );
}
