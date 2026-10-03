# Disk Full Save Recovery

## Goal

Prove a failed project save caused by exhausted SQLite capacity cannot replace or damage the last committed project, and that saving can resume after capacity returns.

## Scope

- Limit SQLite page growth and attempt a full-snapshot write with a large CLAP state blob.
- Check that `SQLITE_FULL` is reported and the old project remains loadable from the open store and after reopening.
- Restore the page limit and retry on the same live store; verify the replacement survives another reopen.

## Dependencies

`aaadaw-storage::ProjectStore` transactional snapshot replacement.

## Acceptance

The existing snapshot remains unchanged after a real SQLite full error, including after close and reopen. The same live store can retry successfully once the page limit is increased.

## Verification

Run the focused unit test, the storage test suite, and strict storage Clippy.

## Status

Complete. `cargo xtest -p aaadaw-storage` passed (233 workspace tests), and `cargo clippy -p aaadaw-storage --all-targets -- -D warnings` passed. The regression tests induced `SQLITE_FULL`, verified rollback before and after reopen, retried on the same connection, and verified the replacement after reopen.
