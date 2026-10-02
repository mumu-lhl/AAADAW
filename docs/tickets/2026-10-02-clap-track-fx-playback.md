# CLAP track effects in playback

## Goal

Make enabled track CLAP effects process real project audio during JACK and PipeWire playback.

## Scope

- Activate configured effect slots before playback and compile their processors into the render graph.
- Keep each control-thread owner paired with its processor through seek graph replacement and playback shutdown.
- Report activation and processing setup failures through the existing playback status.
- Keep bypassed slots out of the active graph and preserve chain order.

## Out of scope

MIDI instrument routing, plugin parameter editing/state persistence, and out-of-process isolation.

## Dependencies

[`2026-10-02-track-fx-project-model.md`](2026-10-02-track-fx-project-model.md),
[`2026-10-02-clap-track-fx-processing.md`](2026-10-02-clap-track-fx-processing.md).

## Acceptance

- Playback renders project audio through enabled track effects in saved order; bypassed effects do not process audio.
- Effects activate outside the audio callback, and graph retirement returns stopped processors to their matching owners for deactivation.
- Playback preparation, graph replacement failure, and output shutdown do not leak active CLAP instances.
- Existing media playback behavior remains unchanged when no effects are assigned.

## Verification

Use a CLAP test effect through the application playback preparation and lifecycle interface. Run `cargo xtest`, strict Clippy, and JACK/PipeWire feature checks.

## UI acceptance

FX chain status reports effect activation failures in plain language; no dashboard-style surfaces are added.

## Status

Complete. Playback loads enabled effects from saved plugin references, installs them in ordered render-graph slots, and returns stopped processors to matching owners after graph replacement or shutdown. Preparation failures are reported before output starts. `cargo xtest --all-features` passed 201 workspace tests; strict Clippy passed with all targets and features.
