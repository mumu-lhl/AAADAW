# Application logs and diagnostics

AAADAW initializes structured `tracing` logs before entering either the desktop UI or the MCP STDIO server. Logs use daily rotation and retain at most seven files. If the platform log directory cannot be created or opened, or the file sink later fails, AAADAW reports the problem on stderr and falls back to stderr logging; this does not prevent startup. MCP JSON-RPC responses remain on stdout.

## Log directory

The default directory is the application's per-user local data directory with a `logs` subdirectory:

- Linux: `$XDG_DATA_HOME/aaadaw/logs`, or `~/.local/share/aaadaw/logs` when `XDG_DATA_HOME` is not set.
- Windows: `%LOCALAPPDATA%\AAADAW\AAADAW\data\logs`.
- macOS: `~/Library/Application Support/org.AAADAW.AAADAW/logs`.

Set `AAADAW_LOG_DIR` to use a different directory, such as a portable or test location. The override names the log directory itself; AAADAW creates it when needed. Daily files use the `aaadaw.<UTC-date>.log` prefix pattern and retain at most seven files.

## Levels and realtime boundary

Set `RUST_LOG` to control the `tracing` filter. The default is `info`; for example, `RUST_LOG=aaadaw=debug,aaadaw_engine=info` enables more detail from the desktop shell without raising audio-engine logs.

Use structured fields and module targets rather than formatting state into ad-hoc strings. Use `error` for an operation that failed, `warn` for a recoverable problem or fallback, `info` for startup/shutdown and important backend lifecycle events, and `debug` for lower-level control-path details. Avoid routine per-frame events. Audio callbacks and other realtime paths must never log: format messages and write diagnostics only on a non-realtime control or worker thread.
