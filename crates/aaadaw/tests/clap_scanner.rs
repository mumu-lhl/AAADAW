use std::process::Command;

#[test]
fn scanner_helper_returns_versioned_error_for_an_invalid_entry() {
    let directory = tempfile::tempdir().unwrap();
    let entry_path = directory.path().join("invalid.clap");
    let response_path = directory.path().join("response.json");
    std::fs::write(&entry_path, b"not a CLAP shared library").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_aaadaw"))
        .arg("__aaadaw_scan_clap_entry_v1")
        .arg(&entry_path)
        .arg(&response_path)
        .env("AAADAW_LOG_DIR", directory.path().join("logs"))
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let response: serde_json::Value =
        serde_json::from_slice(&std::fs::read(response_path).unwrap()).unwrap();
    assert_eq!(response["version"], 1);
    assert!(response["plugins"].as_array().unwrap().is_empty());
    assert!(response["error"].as_str().unwrap().contains("CLAP"));
}
