# MIDI piano-roll editing

## User scenario

A musician opens a MIDI item and edits notes by pitch and musical time, then plays the result through the track's assigned instrument.

## Current problem

MIDI notes could only be edited through an Inspector list and per-note nudge buttons. Those controls exposed the domain actions but made note placement and duration slow and difficult to read.

## Expected behavior

Open a selected MIDI item in an independent piano-roll window with a musical ruler, pitch keyboard, visible notes, and clear selection. Insert notes by clicking the grid; move selected notes by dragging; resize a note from its right edge; delete selected notes; and keep changes in the normal project action history.

## Acceptance

- Open the piano roll from the MIDI item context menu, the Inspector, or by double-clicking the item.
- Show a meter-aware bar/beat ruler, pitch keyboard, 1/16 grid, and notes in the visible pitch/time range.
- Click an empty cell to insert a note at the nearest 1/16 with grid-length duration and velocity 96.
- Click to select; Ctrl/Cmd-click toggles selection; drag moves selected notes; drag the right edge to resize. Each completed edit is a single undoable transaction.
- Delete removes the selected note set. Project core validation remains authoritative for pitch, duration, and item bounds.
- Keep piano-roll keyboard focus separate from main-window shortcuts and keep the editor usable at 900×620 and 1280×800.

## Dependencies

Existing MIDI note actions, project history, meter-aware timebase, and assigned CLAP instrument playback.

## Verification

Coordinate/snap mapping (including a non-default 480 PPQ project), modifier selection, and action/undo behavior are covered by deterministic tests. `cargo xtest -p aaadaw`, strict app Clippy, and real Iced-window interaction are required.

## Status

Complete. The selected MIDI item opens from its context menu, Inspector, or arrangement double-click. The piano roll supports click insertion, modifier selection, selected-note movement, right-edge resize, deletion, horizontal pan/zoom, and octave scrolling; note edits go through `DawAction` and one batch transaction per gesture.

Verification: `cargo xtest -p aaadaw` passed (200 tests) and strict app Clippy passed before review follow-ups. The final PPQ and modifier-selection changes passed GitHub Actions run 36 at commit `c71b6bb`: formatting, the Linux JACK backend check, Clippy, and nextest all succeeded. The actual Iced window was opened in Xvfb at 1280×800 and 900×620; double-click entry, visible note insertion, drag-move, and drag-resize were exercised. High-DPI scaling and keyboard-only navigation were not specifically reviewed. The GUI environment was isolated and did not use the user's audio devices or project files.
