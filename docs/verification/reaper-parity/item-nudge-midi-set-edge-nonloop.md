# Non-loop MIDI Set edge targets on a staggered selection

This native comparison uses Linux REAPER `7.82/linux-x86_64`, the Default 7 theme, and Main action 41226. The unsaved project contains two selected MIDI items on one track, starting at 2 and 6 seconds. Each item is 10 seconds long, has a 20-quarter-note source with Loop source disabled, and contains notes at 960 and 2,880 PPQ. The shared [probe script](../../scripts/reaper_parity/probe-item-nudge-midi-set-edge-multiselect.lua) creates this non-loop case when `AAADAW_MIDI_PROBE_LOOP=0`; it also records item selection, state-change count, and the native Undo/Redo result.

With Mode `Set`, Target `Left Trim`, Snap off, and a 4-second target, REAPER aligns both left edges at 4 seconds. It rebases each non-loop MIDI source to keep its event positions valid:

| Item | Before start/length | After start/length | Source offset | Source length | Note PPQ after |
| --- | --- | --- | ---: | ---: | --- |
| 1 | 2 s / 10 s | 4 s / 8 s | 1.5 s | 19 QN | 0, 1,920 |
| 2 | 6 s / 10 s | 4 s / 12 s | 0 s | 24 QN | 4,800, 6,720 |

The first item advances its edge by 4 quarter notes. REAPER shifts its earliest event from 960 PPQ to zero, adds the remaining 3 quarter notes to `D_STARTOFFS`, and shortens the source by one quarter note. The second item's edge moves backward by 4 quarter notes; REAPER shifts its events forward by 4 quarter notes and extends the source by the same amount. Both items remain selected. Project state changes once with the Undo label `Nudge items`; Undo restores original bounds, offsets, source lengths, and note PPQ, and Redo reapplies the measured state.

The same fresh fixture measures the other three targets with Snap and Relative Set off:

| Target and value | Item 1 start/length; source offset/length | Item 2 start/length; source offset/length | Event/source behavior |
| --- | --- | --- | --- |
| Left Edge, 4 s | 4 s / 8 s; 0 s / 20 QN | 4 s / 12 s; 0 s / 20 QN | Source events and lengths stay unchanged |
| End Position, 14 s | 4 s / 10 s; 0 s / 20 QN | 4 s / 10 s; 0 s / 20 QN | Both ends meet at 14 s; event PPQ stays unchanged |
| Right Trim, 14 s | 2 s / 12 s; 0 s / 24 QN | 6 s / 8 s; 0 s / 16 QN | Each non-loop source length follows its visible right edge |

After Left Trim to 4 seconds, a subsequent Right Trim to 14 seconds leaves item 1 at 4–14 seconds with a 1.5-second source offset and 23 QN source, and item 2 at 4–14 seconds with zero offset and a 20 QN source. This confirms that Right Trim sets source length to source offset plus the new visible length. The native Undo/Redo snapshot for Right Trim restores and reapplies both item and source states in one `Nudge items` history step.

The complete before/after/Undo/Redo state is in the [native TSV](fixtures/item-nudge-midi-set-edge-nonloop-multiselect-native.tsv). Actual REAPER captures show the [baseline](fixtures/item-nudge-midi-set-edge-nonloop-multiselect-native-before.png), [Left Edge](fixtures/item-nudge-midi-set-edge-nonloop-multiselect-native-left-edge-after.png), [End Position](fixtures/item-nudge-midi-set-edge-nonloop-multiselect-native-end-position-after.png), and [Right Trim](fixtures/item-nudge-midi-set-edge-nonloop-multiselect-native-right-trim-after.png) windows. The previous [Left Trim capture](fixtures/item-nudge-midi-set-edge-nonloop-multiselect-native-left-trim-after.png) remains available, as does the [Right Trim after Left Trim capture](fixtures/item-nudge-midi-set-edge-nonloop-multiselect-native-right-trim-after-left-trim.png).

AAADAW applies placement and non-loop MIDI source changes in one `BatchTransaction`. Left Edge and End Position preserve MIDI source length; Right Trim now updates non-loop source length to source offset plus the new item length while preserving the stored events. The regression compares all three targets against the staggered native values and checks notes, CC, pitch bends, selection, and one-step Undo/Redo. Native mixed Audio/MIDI staggered selections, other units and Snap behavior, and variable-tempo precision remain open; this target slice does not close `ITEM-NUDGE-SET-001`.

The expanded native TSV has SHA-256 `c7e8f93d22c3eedb45d4cfd83a51b80074c5224914573409d645798b403a9f4b`. The new Left Edge, End Position, and Right Trim screenshots have SHA-256 `62724bd32dafa29838429f12d68586e0d2d215534d511a0b536fa67b827bee9f`, `e498828f55d1a7ed85c1b90e720fbced2bef8bb7a991b2a50aba8cea4f7378cd`, and `7b99c2ef7dddf2ed93228274edc6e551c43ebb3b3c04dc292ba372b91b68c514`, respectively. The after-Left-Trim Right Trim screenshot has SHA-256 `2d4ac551c6f23dec466a0bf38f6cd856f6c95d9e1e90e2faf9d8fc7ad3c37958`.
