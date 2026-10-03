# Resume sustained MIDI notes after seeking

## User scenario

A musician seeks into a held MIDI note or stops and resumes playback in the middle of a note, and expects the assigned instrument to sound for the remaining duration.

## Behavior

- At the first non-empty block after playback starts or jumps, emit a note-on at offset zero for notes whose start is before the transport position and whose end is after it.
- Leave notes beginning exactly at the transport position to the regular event schedule, preventing duplicate note-ons.
- Preserve the scheduled note-off at the note's original end and filter muted/non-solo tracks during compilation.
- Keep callback lookup allocation-free using a prebuilt interval index and render-graph event storage.
- Do not consume a pending chase for an empty callback block.

## Verification

Engine tests cover active-range boundaries, muted/solo filtering, seek/start, stop/restart, zero-frame blocks, and the existing CLAP instrument release path. `cargo nextest run -p aaadaw-engine`, strict engine Clippy, and workspace nextest pass.

## Status

Implemented in Issue #22.
