# Nudge edge targets with Snap to unit

Reference: Linux REAPER `7.82/linux-x86_64`, Default 7 theme, Main action 41226.

## Native measurements

The mixed-media fixture selects a loop-source Audio item at 2 s and a loop-source MIDI item at 6 s, both 10 s long. The MIDI source contains a note, CC, and pitch bend. In Nudge mode, `by: 0.6 milliseconds` with Snap to unit enabled, clicking Nudge right rounds the first selected item position to 2.001 s and applies the resulting 1 ms delta to the whole selection:

| Target | Audio result | MIDI result | Source data |
| --- | --- | --- | --- |
| Left Trim | 2.001 s, 9.999 s | 6.001 s, 9.999 s | Both offsets advance to 1 ms; MIDI events stay unchanged |
| Left Edge | 2.001 s, 9.999 s | 6.001 s, 9.999 s | Offsets and MIDI events stay unchanged |
| End Position | 2.001 s, 10 s | 6.001 s, 10 s | Offsets and MIDI events stay unchanged |
| Right Trim | 2 s, 10.001 s | 6 s, 10.001 s | Offsets and MIDI events stay unchanged |

The complete state captures are in [the native TSV](fixtures/item-nudge-mixed-edge-snap-native.tsv); each target has an actual REAPER window capture. A second fixture shifts the Audio start to 2.0004 s and MIDI start to 6.0006 s. With `by: 0.8 milliseconds`, End Position and Right Trim apply a shared 0.6 ms delta, because snapping is anchored to the first selected item's start at 2.0004 s. This distinguishes group-anchor snapping from independently rounding every selected item's edge; values and screenshots are in [the anchor TSV](fixtures/item-nudge-mixed-edge-snap-anchor-native.tsv).

## AAADAW behavior

AAADAW applies the same rounded delta to the selected group for Left Trim, Left Edge, End Position, and Right Trim. It converts MIDI boundaries through the project tempo map and preserves Audio source-rate conversion, MIDI loop phase, event data, and selection. Each edit is one `Nudge items` transaction with Undo/Redo. The regression `item_nudge_edge_targets_support_snap_for_mixed_audio_midi_selection` covers all four targets with Snap enabled and disabled, mixed Audio/MIDI selection, MIDI note/CC/pitch-bend preservation, expected source offsets, selection, and Undo/Redo.

The new binary Build ID is `a3368be34af07e2a120c3f66440763b81052016a`, SHA-256 `3c2d09703677e0e63d86dd69726ffa210c08429e255321bf227519e30c46fd19`. It launched on bundled Xvfb `:90` and rendered the actual 1280×800 main window. The captured startup frame has SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b`, byte-identical to the existing [desktop smoke fixture](fixtures/aaadaw-nonloop-midi-edge-targets-default-smoke.png). In a second `:200` session, the same build opened Item → Nudge/set items... from the visible menu; `xwininfo` reported the actual 528×121 dialog. The [current-build dialog capture](fixtures/aaadaw-nudge-edge-snap-live.png) shows its initial Nudge/Position, 1 millisecond, and Snap-off state with no selection.

Focused nextest passed 1/1; the complete AAADAW App suite passed 628/628 with no skipped tests. App Clippy with `-D warnings`, rustfmt, and `git diff --check` passed. Only milliseconds have been measured for edge Snap; other Units, variable-tempo MIDI edge precision, and broader native Undo parity remain open. `ITEM-NUDGE-SET-001` stays `in_progress`.
