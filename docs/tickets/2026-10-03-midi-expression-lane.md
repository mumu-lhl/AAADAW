# MIDI Expression (CC11) Lane

## Goal

Let musicians draw and edit MIDI Expression events for phrase dynamics in the piano roll.

## Behavior

- Display a continuous CC11 lane using the shared controller lane interaction.
- Add snapped points, drag existing points, and delete them from the context menu.
- Preserve all notes and unrelated CC events; commit edits as one undoable project action.
- Reuse existing storage, playback scheduling, and seek chase for CC11.

## Verification

Passed locally: `cargo xtest` (245 tests), strict workspace Clippy, formatting check, and `git diff --check`. Focused tests cover CC11 lane editing, storage roundtrip, sample-offset scheduling, and seek-chase.

Native Iced GUI rendering has not been visually reviewed in this environment.

## Status

In progress.
