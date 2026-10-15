# Item Nudge/Set edge targets

This comparison covers Linux REAPER `7.82/linux-x86_64`, the Default 7 theme, and Main action 41226. REAPER's dialog exposes Set targets `left trim`, `left edge`, `end position`, and `right trim`. Native single-item and staggered Audio selections are now measured; MIDI multi-selection, other units, tempo cases, and Set/Snap combinations remain open.

## Native measurements

The Audio fixture is one selected loop-source item at 2 seconds with a 10-second length and zero source offset. Applying each Set target changed project state once and used the `Nudge items` Undo label:

| Target | Entered value | Position after | Length after | Source offset after |
| --- | ---: | ---: | ---: | ---: |
| Left Trim | 4 s | 4 s | 8 s | 2 s |
| Left Edge | 4 s | 4 s | 8 s | 0 s |
| End Position | 14 s | 4 s | 10 s | 0 s |
| Right Trim | 14 s | 2 s | 12 s | 0 s |

The left targets use the first selected item's left edge as their dialog anchor. End Position and Right Trim use the last selected item's right edge. The End Position dialog displays Relative Set; the other three edge targets do not. Snap to unit is visible for all four. The exact item states and undo/redo evidence are in the [Audio native fixture](fixtures/item-nudge-audio-set-edges-native.tsv).

A staggered Audio fixture selects two 10-second loop-source items at 2 and 6 seconds. With Relative Set off, each target places the corresponding edge of every selected item at the entered absolute time. At 4 seconds, Left Trim and Left Edge move both starts to 4 seconds, leaving lengths 8 and 12 seconds. Left Trim advances both source offsets by 2 seconds, including the item whose old start was after the target; Left Edge preserves both offsets. At 14 seconds, End Position makes both ends coincide at 14 seconds (both starts become 4 seconds), while Right Trim makes both ends coincide without changing their starts (lengths become 12 and 8 seconds). The exact rows and one-step End Position Undo/Redo are in the [staggered native fixture](fixtures/item-nudge-audio-set-edge-multiselect-native.tsv).

With Relative Set enabled for End Position, the entered value remains an absolute destination for the last selected item's right edge. Setting it to 14 seconds moves that edge from 16 to 14 seconds, then applies the resulting −2-second delta to every selected item: starts become 0 and 4 seconds, and both lengths stay 10 seconds. The field is not a raw delta input.

A mixed Audio/MIDI fixture starts both selected loop-source items at 2 seconds with 10-second lengths. Set → Left Trim to 4 seconds moves both to 4 seconds, reduces both lengths to 8 seconds, and advances both source offsets by 2 seconds. The MIDI note stays at the same PPQ range. Undo restores both items together and Redo reapplies the edit; each is one history step. See the [mixed native fixture](fixtures/item-nudge-set-edges-mixed-native.tsv).

## AAADAW behavior

AAADAW applies edge edits to selected Audio and MIDI items in one batch transaction. Absolute Left Trim advances each source offset by the absolute distance between its old start and the target, using each audio source's sample rate and the MIDI tempo map. Left Edge preserves source offsets. Absolute End Position places each item's end at the target without changing its length. Right Trim places each item's end at the target without moving its start. End Position Relative Set computes a shared signed delta from the last selected item's right edge to the absolute target. Snap rounds the target before these transforms. Invalid trims and unavailable source metadata reject the entire batch.

The regression suite compares the single-item mixed Audio/MIDI results above, verifies absolute target snapping, and checks all four Audio targets on a staggered selection against the native positions, lengths, and source offsets. It also verifies End Position Relative Set, atomic over-trim rejection, one-step Undo/Redo, and preserved MIDI events and selection.

## Evidence

Native REAPER captures show the Set dialog and actual item state for [Left Trim before/after](fixtures/item-nudge-audio-set-edges-native-left-trim-before.png), [Left Edge before/after](fixtures/item-nudge-audio-set-edges-native-left-edge-before.png), [End Position before/after](fixtures/item-nudge-audio-set-edges-native-end-position-before.png), and [Right Trim before/after](fixtures/item-nudge-audio-set-edges-native-right-trim-before.png). The [staggered Left Trim result](fixtures/item-nudge-audio-set-edges-multiselect-native-left-trim-after.png) shows the two-item native selection after applying the 4-second target. The [mixed Audio/MIDI before](fixtures/item-nudge-set-edges-mixed-native-before.png), [after](fixtures/item-nudge-set-edges-mixed-native-after.png), and [Redo](fixtures/item-nudge-set-edges-mixed-native-redo.png) captures use the same REAPER version and theme.

The rebuilt AAADAW desktop binary (Build ID `c74dcb7cac16f9824f5ceba6bc586d221e49ff99`, SHA-256 `f70bf2cbd1a100ea2c4709a31255e544eecdbe5e7031347401b7a3f156d1e78d`) was launched on Xvfb `:102`. Its [live End Position screenshot](fixtures/aaadaw-set-edge-end-position-relative.png) shows the actual 528×121 dialog with Relative Set enabled. No item was selected in this capture, so it verifies the rendered controls only; the edge behavior is covered by app regressions and native fixtures.

After the staggered-edge correction, Build ID `027235a78569b58fbf6072409f0a74d65336d0c3` (nightly Cranelift, SHA-256 `b6f8ce725ab437fab8f212d0d79e5709a4ff0176a8923243fd0e2414a7e319fa`) launched the actual 1280×800 main window on bundled Xvfb `:90`. Its [current-build startup capture](fixtures/aaadaw-set-edge-multiselect-desktop-smoke.png) is byte-identical to the standard startup fixture; the [runtime record](desktop-runtime.md) documents the isolated XDG profile and OpenGL backend.

The staggered native TSV has SHA-256 `63d4b9c15e74ab3a52244f8f410b2ad777783cef1a88d8597fe64b955122ed4a`; the [Left Trim after screenshot](fixtures/item-nudge-audio-set-edges-multiselect-native-left-trim-after.png) has SHA-256 `020fd820df938bf5858f16a698cf20ae6ebe9e73ba4dc3b09ae1e7896a96be38`.

SHA-256 for the key native captures:

- Left Trim after: `ac582c0e7d183f089c9f50f109dcf7be86952166192d865abd814ef3aa25ea12`
- Left Edge after: `9ce8e4ea0e0cfeab0c6fbd28c5727708d0ff22f73ec367d4ad2e72f8e75f4549`
- End Position before/after: `f878cbbdce46379157fc1a27860e6febc69b393a2724d43b2a8af5f9bee16e8a` / `a68d340c064b5ccc23bca7da74ee85d8587b307411c5a3bfbb51a8efccc8bdd7`
- Right Trim before/after: `e96ed7a8e8116037cd5372473564ba0f39e7fec6dfbe007dd5a5bc91a27315e3` / `d518fe4d7a830c53f328c2831d9c8ac2879aa380fd6b0002ea67d70d65ad2540`
- Mixed Left Trim before/after/Redo: `74cd362a4b04a83d5d6ca849bfc1d97104411dc9c665a0ca35852b218ffb077b` / `40f43ceafc14842c519d7835017588eee528b277feafa85bf93690b37b62c3ab` / `68f5f1aff4b8a4b31e8eeef81a8d5c539b710b20c0cf0e06ca24871582c6cd8c`

## Remaining parity work

The staggered native fixture uses Audio only; native staggered MIDI and mixed-media selections, non-default Units and Snap combinations, non-loop sources, variable-tempo MIDI edge conversion, playback state, and broader Undo behavior remain open. The tests prove AAADAW behavior for several inputs, but they do not replace the missing native comparisons. `ITEM-NUDGE-SET-001` therefore remains `in_progress`.
