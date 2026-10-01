# Desktop Arrangement Tickets

Scope: continue Phase 1.3 from the current code, following the UI audit in `DESIGN.md`.

## A1 — Replace the list with a spatial Arrangement

- **Goal:** Make Arrangement useful for locating and inspecting Audio/MIDI Items in musical time.
- **Scope:** Iced/wgpu viewport with ruler, track-aligned lanes and Item blocks; horizontal pan/zoom, vertical track scrolling, track/item selection and edit cursor; resizable TCP; right-click track context menu for selection, rename, mute/solo, reorder and delete; selected Item details remain editable in an Inspector.
- **Out of scope:** Item dragging, splitting, waveform peaks, piano roll, Mixer, and docking.
- **Dependencies:** None.
- **Acceptance:** Audio sample positions and MIDI ticks map through the Project timebase; ruler follows meter changes; lanes follow track order; selection and cursor have distinct visible states; the Item view has no fixed 200-object cap; zoom/pan reuse cached project geometry. TCP context operations target the right-clicked track, close after an action, and rename enters an editable state. Existing precise Item/note editing remains reachable in the Inspector and uses `DawAction`.
- **Verification:** `cargo xtest -p aaadaw`; relevant core/app/UI checks; format and diff checks. Run the real app and inspect default and narrow layouts when a graphical session is available.
- **Status:** Implementation and automated checks complete; real-window visual interaction review remains pending.

## A2 — Move selected Items with musical snapping

- **Goal:** Arrange audio and MIDI Items by dragging them in time and between tracks.
- **Scope:** Single and multi-Item selection, drag preview, 1/16-note snapping with a visible Snap toggle, track targeting, one undoable action per completed drag.
- **Out of scope:** Edge trimming, slip editing, time selection and copy-drag.
- **Dependencies:** A1.
- **Acceptance:** Audio Items retain source offsets and durations; MIDI Items retain notes and lengths; invalid drops do not change project state; undo/redo restores exact positions and tracks; JACK playback edit guard still applies.
- **Verification:** Focused Project/app tests and `cargo xtest -p aaadaw`; visually inspect drag preview, snapping, selection and narrow layout.
- **Status:** Complete. Automated checks pass; isolated Xvfb review confirmed drag preview, cross-track drop, snapping controls, and the 900×620 layout without activating a host window.

## A3 — Time selection and split at the edit cursor

- **Goal:** Define an editable time range and split Items at the edit cursor.
- **Scope:** Drag-created time selection, clear/adjust boundaries, split audio placements and MIDI note contents through undoable Project actions.
- **Out of scope:** Razor editing, fades, waveform editing and destructive source changes.
- **Dependencies:** A1, A2.
- **Acceptance:** A split preserves source continuity for audio and note timing for MIDI; invalid/empty splits are no-ops with useful status; each split is atomic and undoable as one step.
- **Verification:** Core transaction/undo tests, focused app tests, `cargo xtest -p aaadaw`, and visual interaction review.
- **Status:** Planned.

## A4 — Audible MIDI playback path decision and slice

- **Goal:** Make edited MIDI audible through a real instrument path.
- **Scope:** Resolve the Phase 0 instrument decision, then implement a minimal end-to-end playback slice through the engine and JACK path.
- **Out of scope:** Full CLAP browser/management UI, plugin state restore, and non-JACK backends.
- **Dependencies:** Existing MIDI scheduler and an explicit Phase 0 instrument choice; depends on A1 for visible MIDI placement.
- **Acceptance:** A saved MIDI Item produces audible notes during playback; mute/solo and stop/seek behavior are defined and tested; real-time callback work meets the audio-thread constraints.
- **Verification:** Engine/app tests, JACK-feature checks, `cargo xtest -p aaadaw --features jack-backend`, and an audio smoke test when a JACK device is available.
- **Status:** Blocked on the unresolved Phase 0 product/dependency decision.
