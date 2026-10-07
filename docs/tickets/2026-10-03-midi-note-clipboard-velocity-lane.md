# MIDI note clipboard and velocity lane

## User scenario

A musician duplicates a selected MIDI motif and balances its dynamics in the piano roll without rebuilding notes or editing Inspector fields one at a time.

## Behavior

- Copy normalizes the selected phrase to its earliest note while preserving each note's pitch, duration, velocity, and relative tick spacing.
- Paste uses the visible piano-roll edit cursor and the shared snap grid. Clicking the time ruler moves the cursor; if no cursor was set, Copy places it after the copied phrase and carries that default target when switching Items. Repeated pastes advance by the phrase span. Snap Off allows an unsnapped target. Notes that do not fit are rejected without changing the project.
- Duplicate is a separate action that clones the selected notes after their current range without changing the clipboard.
- Paste allocates fresh note IDs and selects the new group.
- The velocity lane draws one bar per note. Hover reveals its numeric velocity; selection and hover have distinct colors. Dragging any selected handle applies one shared velocity delta and clamps each result to 1–127.
- Copy and paste are available from the editor toolbar and Ctrl/Cmd+C/V while the piano roll has focus. A paste and a completed velocity drag each use the regular undoable project actions.

## Verification

App tests cover phrase geometry, fresh IDs, repeated paste placement, selecting pasted notes, and undo/redo of both grouped paste and multi-note velocity changes. Piano-roll tests cover relative velocity deltas and lower/upper bounds. Actual Iced window review at normal and narrow sizes remains an environment-dependent manual check.
