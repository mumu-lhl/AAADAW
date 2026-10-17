# MIDI Editor selected-note position and end quantize

Linux REAPER 7.82 / Default 7 was probed with MIDI Editor action 40729 (`Quantize notes position and end to grid`). On the default `1/32` grid (120 ticks at PPQ 960), selected notes move both endpoints to the nearest grid line: `100–220` becomes `120–240`, and `350–430` becomes `360–480`. A short selected note at `900–970` would collapse both endpoints to the same line, so REAPER keeps one full grid interval and returns `960–1080`. The unselected `550–670` note stays unchanged; channel and velocity are preserved. The action-specific event readback, projects, and probe are listed in the [fixture manifest](fixtures/midi-note-end-quantize-ui.tsv).

AAADAW adds `Q pos+end` to the MIDI Editor toolbar. It uses the current editor grid and processes only selected note IDs. A single `MidiNotesQuantized` project event records both tick and duration changes, so one Undo restores both endpoints and Redo reapplies them. Core and app tests cover the native endpoint results, the minimum one-grid duration, unselected-note and channel/velocity preservation, and Undo/Redo.

The [REAPER Piano Roll capture](fixtures/midi-note-quantize-reaper-native.png) shows the `1/32` editor grid and the final note layout produced by the native action; the action-specific before/after ticks are recorded in the TSV. The [AAADAW live-window capture](fixtures/midi-note-end-quantize-aaadaw.png) shows the new toolbar command beside the existing selected-note quantize command. Its Cranelift desktop binary had ELF Build ID `b3d8d140ee86c079c427beb934e7d2cd6fa4baf8`.

The partial Quantize Events dialog and its remaining scope/operation gaps are documented in [MIDI Editor Quantize Events dialog](midi-note-quantize-dialog.md). Native strength output, swing-grid intersection, general MIDI event quantization, and Humanize remain open. This slice implements REAPER's direct action 40729 only.

Full workspace Nextest passed **1409 tests across 74 binaries**. Strict workspace all-target Clippy (`-D warnings`), formatting, `git diff --check`, the 105-row/15-column unique-ID parity matrix check, and all 8 fixture manifest hashes passed.
