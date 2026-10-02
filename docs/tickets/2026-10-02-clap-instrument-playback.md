# CLAP instrument assignment and playback

## Goal

Play MIDI notes on tracks with their assigned CLAP instrument, then pass instrument audio through the track's ordered FX chain.

## Scope

- Add a track context action that selects a scanned CLAP instrument or clears the current assignment through `DawAction`.
- Load assigned instruments before playback, install their processors into the prepared graph, and report missing/incompatible plugins clearly.
- Preserve instrument owners across graph replacement and shut them down on the control thread.
- Route instrument output through enabled track effects and respect mute/solo.

## Out of scope

Piano roll, live keyboard input, plugin parameter/state editing, and process isolation.

## Dependencies

[`2026-10-02-track-instrument-assignment.md`](2026-10-02-track-instrument-assignment.md),
[`2026-10-02-clap-render-graph.md`](2026-10-02-clap-render-graph.md),
[`2026-10-02-clap-track-fx-playback.md`](2026-10-02-clap-track-fx-playback.md).

## Acceptance

- A user can assign or clear an instrument from a track context menu; edits are undoable.
- Project MIDI events render through the matching track's instrument and then its enabled FX slots in order.
- Seek replacement, plugin activation failure, output setup failure, and shutdown do not leave active plugin instances behind.
- Existing audio-only tracks render the same when no instrument is assigned.

## Verification

Use the built-in CLAP test synth/effect through render graph installation and retirement. Run `cargo xtest --all-features` and strict Clippy.

## UI acceptance

The track context menu exposes instrument assignment without adding a new workspace page. The selection window filters to instruments and shows the current assignment; clear is a distinct action. Check the actual app window against `DESIGN.md` after implementation.

## Status

Complete. The track context menu assigns/clears scanned CLAP instruments through undoable actions. Assigned synths render project MIDI before the ordered track FX chain, and instrument instances return to matching owners after graph retirement and shutdown. `cargo xtest --all-features` passed 204 workspace tests; strict Clippy and default-feature checking passed. The assignment picker and track context menu were inspected in an isolated Xvfb session.
