# Non-loop MIDI Nudge edge targets

Reference: Linux REAPER `7.82/linux-x86_64`, Default 7 theme, Main action 41226.

The mixed-media fixture contains a non-loop Audio item at 2 s and a non-loop MIDI item at 6 s, each 10 s long. The MIDI source is 20 QN and contains a note, CC, and pitch bend. The [native probe](../../../scripts/reaper_parity/probe-item-nudge-mixed-nonloop-edge-nudge.lua) records item bounds, source length and offset, MIDI events, state-change count, and Undo label. Set `AAADAW_MIDI_SOURCE_OFFSET=0.1` to measure the positive-offset case, or `AAADAW_SOURCE_START_EVENTS=1` to place events at the source boundary.

With Nudge set to 250 milliseconds and Snap off, REAPER reports:

| Target | MIDI item result | Source result | Event result |
| --- | --- | --- | --- |
| Left Trim, Nudge right, source offset 0 | 6.25–16 s | Offset 0; source shrinks from 20 to 19.5 QN | Note, CC, and pitch bend move left by 480 PPQ |
| Left Trim, Nudge right, source offset 0, earliest event at 0 PPQ | 6.25–16 s | Offset advances to 0.25 s; source remains 20 QN | Events at 0/240 PPQ stay unchanged |
| Left Trim, Nudge right, source offset 0.1 s | 6.25–16 s | Offset advances to 0.35 s; source remains 20 QN | Events stay unchanged |
| Left Trim, Nudge left, source offset 0 | 5.75–16 s | Offset stays 0; source grows to 20.5 QN | Note, CC, and pitch bend move right by 480 PPQ |
| Left Edge, Nudge right | 6.25–16 s | Offset and source length stay unchanged | Events stay unchanged |
| End Position, Nudge right | 6.25–16.25 s | Offset and source length stay unchanged | Events stay unchanged |
| Right Trim, Nudge right | 6–16.25 s | Source grows to 20.5 QN | Events stay unchanged |

When a 0.1 s source offset is nudged left by 250 ms, REAPER crosses source zero by rebasing the MIDI data 289 PPQ to the right, retaining a 1 PPQ (0.000520833 s) source offset and growing the source to 20.301041667 QN. Each changed operation increments project state once with Undo label `Nudge items`; the baseline probe also records the leftward action that restores each target case.

The complete native records are [the source-offset-zero target matrix](fixtures/item-nudge-mixed-nonloop-edge-nudge-native.tsv), [the positive-offset/cross-zero snapshots](fixtures/item-nudge-mixed-nonloop-edge-nudge-offset-native.tsv), and [the source-start event case](fixtures/item-nudge-mixed-nonloop-edge-nudge-source-start-native.tsv). Actual REAPER windows are captured after [the 0.1 s source crosses zero](fixtures/item-nudge-mixed-nonloop-edge-nudge-offset-cross-zero-after.png) and with [events at source zero](fixtures/item-nudge-mixed-nonloop-edge-nudge-source-start-after.png).

AAADAW now routes Nudge and Set through distinct MIDI edge-edit modes. For non-loop Nudge Left Trim, it removes available source prefix only up to the earliest MIDI event, advances the take offset for any remaining positive trim, and rebases the source at zero for leftward expansion. The mixed-media regressions cover all four edge targets, note/CC/pitch-bend transforms, source-boundary events, the 1 PPQ cross-zero behavior, and one-step Undo/Redo. Set behavior continues through its existing transform. This closes the non-loop MIDI Nudge edge slice only; `ITEM-NUDGE-SET-001` and complete REAPER UI parity remain `in_progress`.

The rebuilt desktop executable (Build ID `ef6c95f6ab1f03277a9ac6f4f8949cca1a5372b1`, SHA-256 `a163ffa157fba44113c4c890aceea8e16f39c917a2469798883bcace71b335c3`) launched through the repository smoke helper on the bundled Xvfb and rendered the actual 1280×800 main window. Its screenshot matches the standard startup fixture (SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b`). In a separate live session, the visible Item menu opened Nudge/set items...; `xwininfo` confirmed the 1280×800 main window and 528×121 Nudge/Set dialog. The [live dialog capture](fixtures/aaadaw-nonloop-midi-edge-nudge-live.png) has SHA-256 `bc10e4f08dc3f21bfc5b9d3ec6233c3eb7180c5c5ff62c783438dcb516f5da7a`. The dialog screenshot has no selected item; the behavior is verified by the native probe and App regressions.

The source-zero target TSV SHA-256 is `810473f09e0334a175826e83b83c3cf0773e9658d50225973f2c85d45a9308d4`; the offset TSV is `9b18604e324c050631da8cd8fe4e7dbd9ee275333720cebe2695a8772e9e0cda`; the source-start TSV is `3ea4e7add6d9c3d3473ad1c319815b3d987072fd676e934f07cee43e7706557e`. Screenshot hashes are `4d3e28c6f20a11db5257caed6e5ff42c855cfdffcbdab10ab1ec4322c68b76c` and `69440dd29d536d865c27320f276285d4ddba3c930a91cfb8c077b90039fbd671` respectively.
