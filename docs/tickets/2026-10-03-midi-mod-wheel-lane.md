# MIDI Modulation Wheel Lane

## Goal

Let musicians draw MIDI modulation-wheel (CC1) values in a piano roll and hear them through CLAP instruments.

## Scope

- Add a compact CC1 lane alongside the existing velocity and sustain lanes.
- Map click and drag height continuously to MIDI values `0..=127`; snap tick positions with the piano-roll grid.
- Reuse the existing controller action, storage, playback, seek chase, and undo/redo paths.
- Keep CC64 Sustain as its binary on/off lane.

## Out of scope

Other controller lanes, controller names/configuration, MIDI recording, and controller-specific interpolation.

## Acceptance

CC1 points can be added and dragged at a snapped tick and value, deleted from the right-click context menu, and edited as one undoable action. They survive save/reopen and reach compatible CLAP instruments at the scheduled sample offset and after seek chase. Sustain editing remains unchanged.

## Verification

Editor interaction and mapping tests, core/storage round-trip tests, MIDI schedule and CLAP render-graph tests, workspace tests, and strict Clippy.

## Status

Complete. `cargo xtest` passed all 242 workspace tests; strict workspace Clippy passed. Focused tests cover CC1 lane add/drag/delete, storage round-trip, scheduling/chase, and audible CLAP response at the requested sample offset. Native GUI rendering was not visually reviewed because Xvfb is unavailable and the package mirror returned HTTP 403.
