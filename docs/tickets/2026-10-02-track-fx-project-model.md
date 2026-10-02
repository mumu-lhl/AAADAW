# Track FX project model

## Goal

Persist an ordered per-track CLAP chain with undoable enable/bypass and order edits.

## Scope

- Add validated plugin references and enabled state to Track and ProjectSnapshot.
- Change the chain through `DawAction`; undo/redo preserves order and bypass states.
- Add SQLite migration and round-trip coverage, retaining existing instrument assignments.

## Dependencies

CLAP plugin discovery.

## Acceptance

Projects reopen with the same ordered plugin IDs, paths, and enabled flags; older schemas migrate to an empty chain; invalid references are rejected atomically.

## Verification

Core and storage tests via `cargo xtest -p aaadaw-core -p aaadaw-storage`.

## Status

Complete. The workspace verification run passed 181 tests, including ordered-chain undo/redo, invalid snapshot, and schema round-trip/migration coverage.
