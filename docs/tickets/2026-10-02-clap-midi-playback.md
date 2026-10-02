# Play MIDI through one CLAP instrument

## Goal

Make MIDI tracks produce audio through a user-installed CLAP instrument during live playback.

## Scope

- Load and activate the assigned instrument on a control thread; process scheduled MIDI and stereo audio in the render path for both JACK and PipeWire.
- Keep plugin lifecycle and retirement safe across graph replacement and shutdown; no plugin discovery, loading, allocation, locking, or file I/O in the audio callback.
- Provide a usable track-level way to assign and clear an installed CLAP instrument, backed by `DawAction` and the existing persisted reference.
- Mix instrument output with audio items and honor track mute/solo.
- Report missing or failed instruments clearly and keep the project editable.

## Out of scope

Plugin private-state persistence, custom plugin windows, effects, multi-output instruments, and process isolation.

## Dependencies

ADR 0005, persisted `TrackInstrument`, `MidiEventPlan`, JACK/PipeWire render graph.

## Acceptance

A user can assign an installed instrument to a MIDI track and hear its MIDI items through JACK or PipeWire. Note events are delivered at their scheduled sample offsets; mute, solo, stop, seek, graph replacement, and shutdown do not leave stuck notes or destroy a plugin on the audio callback. Missing plugins do not prevent project editing.

## Verification

Exercise a small CLAP test instrument through the same load, schedule, process, graph-replacement, and teardown path; verify sample-accurate note offsets, output mixing, callback allocation/locking constraints, and backend builds. Run `cargo xtest`, strict workspace Clippy, and the application build.

## UI acceptance

Inspect the track instrument controls in the real application at 1280×800 and 900×620 in private Xvfb. The control must fit the Arrangement track workflow, expose assignment/clear/load status, and avoid adding a top-level page. Confirm visual hierarchy and control density against `DESIGN.md`.

## Status

In progress.
