# Item Nudge/Set edge targets

This comparison covers Linux REAPER `7.82/linux-x86_64`, the Default 7 theme, and Main action 41226. REAPER's dialog exposes Set targets `left trim`, `left edge`, `end position`, and `right trim`. The implementation and acceptance item remain in progress while other units, tempo cases, and multi-selection combinations are measured.

## Native measurements

The Audio fixture is one selected loop-source item at 2 seconds with a 10-second length and zero source offset. Applying each Set target changed project state once and used the `Nudge items` Undo label:

| Target | Entered value | Position after | Length after | Source offset after |
| --- | ---: | ---: | ---: | ---: |
| Left Trim | 4 s | 4 s | 8 s | 2 s |
| Left Edge | 4 s | 4 s | 8 s | 0 s |
| End Position | 14 s | 4 s | 10 s | 0 s |
| Right Trim | 14 s | 2 s | 12 s | 0 s |

The left targets use the first selected item's left edge as their dialog anchor. End Position and Right Trim use the last selected item's right edge. The End Position dialog displays Relative Set; the other three edge targets do not. Snap to unit is visible for all four. The exact item states and undo/redo evidence are in the [Audio native fixture](fixtures/item-nudge-audio-set-edges-native.tsv).

A mixed Audio/MIDI fixture starts both selected loop-source items at 2 seconds with 10-second lengths. Set → Left Trim to 4 seconds moves both to 4 seconds, reduces both lengths to 8 seconds, and advances both source offsets by 2 seconds. The MIDI note stays at the same PPQ range. Undo restores both items together and Redo reapplies the edit; each is one history step. See the [mixed native fixture](fixtures/item-nudge-set-edges-mixed-native.tsv).

## AAADAW behavior

AAADAW applies edge edits to selected Audio and MIDI items in one batch transaction. Left Trim advances the source offset using each audio source's sample rate and the MIDI tempo map. Left Edge preserves source offsets. End Position moves items without changing their lengths. Right Trim changes lengths without moving starts. End Position Relative Set applies a signed delta from the last selected item's right edge, with Snap rounding the resulting edge. Invalid trims and unavailable source metadata reject the entire batch.

The regression suite compares the single-item mixed Audio/MIDI results above, verifies absolute target snapping, checks a staggered two-item selection against the first/last anchor labels, verifies End Position Relative Set, and checks atomic over-trim rejection. Tests also cover one-step Undo/Redo, preserved MIDI events and selection.

## Evidence

Native REAPER captures show the Set dialog and actual item state for [Left Trim before/after](fixtures/item-nudge-audio-set-edges-native-left-trim-before.png), [Left Edge before/after](fixtures/item-nudge-audio-set-edges-native-left-edge-before.png), [End Position before/after](fixtures/item-nudge-audio-set-edges-native-end-position-before.png), and [Right Trim before/after](fixtures/item-nudge-audio-set-edges-native-right-trim-before.png). The [mixed Audio/MIDI before](fixtures/item-nudge-set-edges-mixed-native-before.png), [after](fixtures/item-nudge-set-edges-mixed-native-after.png), and [Redo](fixtures/item-nudge-set-edges-mixed-native-redo.png) captures use the same REAPER version and theme.

The rebuilt AAADAW desktop binary (Build ID `c74dcb7cac16f9824f5ceba6bc586d221e49ff99`, SHA-256 `f70bf2cbd1a100ea2c4709a31255e544eecdbe5e7031347401b7a3f156d1e78d`) was launched on Xvfb `:102`. Its [live End Position screenshot](fixtures/aaadaw-set-edge-end-position-relative.png) shows the actual 528×121 dialog with Relative Set enabled. No item was selected in this capture, so it verifies the rendered controls only; the edge behavior is covered by app regressions and native fixtures.

SHA-256 for the key native captures:

- Left Trim after: `ac582c0e7d183f089c9f50f109dcf7be86952166192d865abd814ef3aa25ea12`
- Left Edge after: `9ce8e4ea0e0cfeab0c6fbd28c5727708d0ff22f73ec367d4ad2e72f8e75f4549`
- End Position before/after: `f878cbbdce46379157fc1a27860e6febc69b393a2724d43b2a8af5f9bee16e8a` / `a68d340c064b5ccc23bca7da74ee85d8587b307411c5a3bfbb51a8efccc8bdd7`
- Right Trim before/after: `e96ed7a8e8116037cd5372473564ba0f39e7fec6dfbe007dd5a5bc91a27315e3` / `d518fe4d7a830c53f328c2831d9c8ac2879aa380fd6b0002ea67d70d65ad2540`
- Mixed Left Trim before/after/Redo: `74cd362a4b04a83d5d6ca849bfc1d97104411dc9c665a0ca35852b218ffb077b` / `40f43ceafc14842c519d7835017588eee528b277feafa85bf93690b37b62c3ab` / `68f5f1aff4b8a4b31e8eeef81a8d5c539b710b20c0cf0e06ca24871582c6cd8c`

## Remaining parity work

The mixed native comparison currently covers Left Trim only. Native numeric results for staggered multi-selection, End Position Relative Set, non-default Units and Snap combinations, non-loop sources, variable-tempo MIDI conversion, playback state, and broader Undo behavior remain open. The tests prove AAADAW behavior for several of these inputs, but they do not replace the missing native comparisons. `ITEM-NUDGE-SET-001` therefore remains `in_progress`.
