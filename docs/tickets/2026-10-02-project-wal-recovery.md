# Project WAL Recovery and Copy

## Goal

Verify that SQLite recovers committed project state after an abrupt process exit and that a normally closed project is a portable single file.

## Scope

- Exercise WAL recovery from a child process that exits without closing its `ProjectStore`.
- Verify close checkpoints sidecar data so the project can be copied and reopened independently.

## Dependencies

`aaadaw-storage::ProjectStore` WAL mode and checkpoint-on-close behavior.

## Acceptance

The child exits with committed project changes still in its WAL; reopening the project restores the exact snapshot. After normal close, copying only the `.aaadaw` file and reopening the copy restores the exact snapshot.

## Verification

Run the storage integration tests with `cargo xtest` and clippy.

## Status

Complete. `cargo xtest -p aaadaw-storage` passed (153 workspace tests); `cargo clippy -p aaadaw-storage --all-targets` passed.
