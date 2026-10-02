# Track instrument assignment

## Goal

Let a MIDI track remember which CLAP instrument should render its notes.

## Scope

- Add an optional CLAP instrument reference to tracks, with an undoable assign/clear action.
- Persist the reference in project snapshots and an additive SQLite schema migration.
- Preserve projects without assigned instruments and reject malformed references safely.

## Out of scope

Plugin discovery, loading, audio processing, UI, and plugin private state.

## Dependencies

Accepted CLAP playback decision in ADR 0005.

## Acceptance

Assignment and clearing round-trip through undo/redo, snapshot restore, save/reopen, and migration from existing project schemas. Existing projects without assignments remain unchanged.

## Verification

Core action/snapshot tests and storage migration/round-trip tests with `cargo xtest`.

## Status

Complete. `cargo xtest -p aaadaw-core -p aaadaw-storage` passed (158 workspace tests); `cargo clippy -p aaadaw-core -p aaadaw-storage --all-targets` passed.
