use std::process::Command;

#[test]
fn version_flag_runs_headlessly() {
    let output = Command::new(env!("CARGO_BIN_EXE_aaadaw"))
        .arg("--version")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("AAADAW {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn help_flag_lists_supported_invocations_without_starting_desktop() {
    let output = Command::new(env!("CARGO_BIN_EXE_aaadaw"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("aaadaw mcp --stdio --project <path.aaadaw> [--write]"));
    assert!(stdout.contains("--version"));
    assert!(output.stderr.is_empty());
}
