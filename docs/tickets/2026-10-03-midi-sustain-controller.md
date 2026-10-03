# MIDI Sustain Controller Lane

## Goal

Let musicians edit sustain pedal events in a MIDI item and hear consistent pedal state through an assigned CLAP instrument during playback, seeking, and stop.

## Scope

- Store MIDI control-change points as item-relative controller/value/tick data, validated to MIDI 1.0 ranges.
- Edit CC64 in a piano-roll Sustain lane with snapped click-to-add, drag-to-move, and a right-click context menu with Delete.
- Persist controller points through the additive SQLite schema v8 migration.
- Schedule sample-offset CC events, chase the last CC64 value before the playhead, and release sustain on stop.

## Out of scope

Other controller lanes, controller recording, MIDI output ports, global Panic, and generalized controller reset policy.

## Dependencies

CLAP instrument playback, MIDI note chase (Issue #22), and the piano-roll editor.

## Acceptance

Sustain edits are undoable, survive save/reopen, and play at the intended sample offsets. Seeking into an active sustain range sends the pedal state before chased notes. Stopping releases held notes and sends pedal off. The audio callback uses precompiled schedules and preallocated storage.

## Verification

Core action/snapshot and split tests, schema migration and storage round-trip tests, MIDI schedule and CLAP instrument tests, piano-roll interaction tests, workspace tests, and strict Clippy.

## Status

Complete. `cargo xtest` passed all 241 workspace tests; strict workspace Clippy passed. The editor interaction test confirms add, drag, and context-menu deletion. The menu follows the right-click behavior in `DESIGN.md`.
