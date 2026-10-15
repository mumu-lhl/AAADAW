# Non-loop MIDI Set → Left Trim on a staggered selection

This native comparison uses Linux REAPER `7.82/linux-x86_64`, the Default 7 theme, and Main action 41226. The unsaved project contains two selected MIDI items on one track, starting at 2 and 6 seconds. Each item is 10 seconds long, has a 20-quarter-note source with Loop source disabled, and contains notes at 960 and 2,880 PPQ. The shared [probe script](../../scripts/reaper_parity/probe-item-nudge-midi-set-edge-multiselect.lua) creates this non-loop case when `AAADAW_MIDI_PROBE_LOOP=0`; it also records item selection, state-change count, and the native Undo/Redo result.

With Mode `Set`, Target `Left Trim`, Snap off, and a 4-second target, REAPER aligns both left edges at 4 seconds. It rebases each non-loop MIDI source to keep its event positions valid:

| Item | Before start/length | After start/length | Source offset | Source length | Note PPQ after |
| --- | --- | --- | ---: | ---: | --- |
| 1 | 2 s / 10 s | 4 s / 8 s | 1.5 s | 19 QN | 0, 1,920 |
| 2 | 6 s / 10 s | 4 s / 12 s | 0 s | 24 QN | 4,800, 6,720 |

The first item advances its edge by 4 quarter notes. REAPER shifts its earliest event from 960 PPQ to zero, adds the remaining 3 quarter notes to `D_STARTOFFS`, and shortens the source by one quarter note. The second item's edge moves backward by 4 quarter notes; REAPER shifts its events forward by 4 quarter notes and extends the source by the same amount. Both items remain selected. Project state changes once with the Undo label `Nudge items`; Undo restores original bounds, offsets, source lengths, and note PPQ, and Redo reapplies the measured state.

The complete before/after/Undo/Redo state is in the [native TSV](fixtures/item-nudge-midi-set-edge-nonloop-multiselect-native.tsv). The [after screenshot](fixtures/item-nudge-midi-set-edge-nonloop-multiselect-native-left-trim-after.png) shows the actual REAPER window and overlapping selected items at the 4-second edge.

AAADAW applies the placement and non-loop MIDI source rebase in one `BatchTransaction`. Its regression compares both items' positions, lengths, source offsets and source lengths, and checks notes, CC, pitch bends, selection, and one-step Undo/Redo. Native mixed Audio/MIDI staggered selections, other Set targets, other units and Snap behavior, and variable-tempo precision remain open; this single measured scenario does not close `ITEM-NUDGE-SET-001`.

The native TSV has SHA-256 `9f93c93368de8d68df2e9a421033303c27da4d5e7d5f29e98c9f2f20c6c54500`. The after screenshot has SHA-256 `7be96941d61bc2e03b4d3e8798ee2fd3899b814287aa6ab5d0c876160f865c61`.
