# MIDI Editor Transpose MIDI dialog

Linux REAPER 7.82 / Default 7 exposes MIDI Editor action 40759, `Transpose notes...`, on the default `T` shortcut. The 311×264 native window defaults to `Selected notes`, semitones `0`, and `Snap to scale` off. The other scope is `All notes`; choosing it applies the entered pitch amount to unselected notes too. A `+2` semitone preview moved selected pitches 60/64/67 to 62/66/69 while leaving an unselected pitch 72 unchanged in Selected notes scope; All notes scope moved that pitch 72 to 74. Tick, duration, channel, velocity, and selection stayed unchanged.

The dialog previews pitch changes while it remains open. Entering `+5` under All notes immediately raised all four probe notes; Cancel restored their original pitches. The chosen scope remained `All notes` on the next invocation after Cancel, while the amount reset to `0`. The default-window and post-Cancel captures plus the event readbacks and repeatable Lua setup are in the [fixture manifest](fixtures/midi-note-transpose-dialog-ui.tsv).

AAADAW registers Action List action `midi-editor.transpose-notes` with default `T`. Its `Transpose…` MIDI Editor dialog previews scoped semitone changes in the Piano Roll without mutating the project. Cancel/Escape discard the preview; OK commits one pitch-only `EditMidiNotes` batch with one-step Undo/Redo. The scope persists while the App session is open, and each invocation starts at zero semitones.

The [AAADAW desktop capture](fixtures/midi-note-transpose-dialog-aaadaw.png) shows the real Cranelift-built Linux window with the dialog open at `All notes` and `+2`; Build ID `0a27d42e4b67484760aeb0a35c3638a9ea1dd336`. It confirms the actual dialog layout and rendered controls. The image is not used as proof of pitch-preview or commit behavior; those paths are covered by the App and Piano Roll tests below.

The REAPER controls `Snap to scale`, `From scale`, `To scale`, scale/key choices, and the octave offset are not implemented. Session scope is not persisted across application restarts, and range-boundary behavior for action 40759 still needs a native probe. `MIDI-NOTE-TRANSPOSE-DIALOG-001`, UI-019, and P7 remain `in_progress`.

Validation: the dialog App regression, Piano Roll preview mapping regression, full AAADAW App Nextest (772/772, 0 skipped), strict AAADAW all-target Clippy, formatting, diff checks, desktop startup/render, and fixture hash validation passed. The UI manifest includes the native REAPER captures and the separate AAADAW desktop capture.
