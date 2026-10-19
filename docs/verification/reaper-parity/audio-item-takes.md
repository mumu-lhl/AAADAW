# Audio item takes

## REAPER 7.82 reference

The native probe used Linux REAPER 7.82 with Default 7 and two WAV sources. Action 40639, **Take: Duplicate active take**, appends a copy after the existing take order and activates that new last take; a single-take item becomes a two-take item with index 1 active. For three selected items with starting take counts 2, 1, and 2, the action produces counts 3, 2, and 3, activates the appended take in each, and preserves all original take order. One real Undo restores all three starting states and one Redo reapplies the action. This multi-selection probe supersedes the old inference from the single-action fixture below: that earlier script changed a take source after duplication, so its final RPP could not establish where REAPER had inserted the copy. The multi-selection readback, before/after/undo/redo RPP files, and native screenshots are listed in the [multi-selection fixture manifest](fixtures/audio-duplicate-active-take-multi-aaadaw.tsv); its screenshots show the live Arrange with an evaluation panel partially occluding the left side, so the state readback and RPP are authoritative.

In the two-take project, action 40125 switches from index 1 to 0 and 40126 switches from 0 to 1. Switching is recorded in project history and undo/redo restores the prior active take. Action 40435 displays the takes in separate lanes when there is room. The saved RPP marks the active `<TAKE SEL>` block and stores each take's name, source, offset, rate, and mix properties in its own block.

The native readback, saved RPP, and screenshots of Arrange and Media Item Properties are in the [fixture set](fixtures/audio-multi-take-native.tsv). The Arrange capture shows the selected take lane and the `Take 2/2: Take A` label; Media Item Properties shows both take names and the active-take selector.

![REAPER 7.82 Default 7 with two audio takes shown in Arrange](fixtures/audio-multi-take-native-arrange.png)

![REAPER 7.82 Default 7 Media Item Properties with the take selector open](fixtures/audio-multi-take-native-properties-list.png)

## AAADAW implementation slice

Audio items now preserve an ordered active and inactive take list in Core snapshots and SQLite schema 66. Duplicate active take appends a copy after the existing order and activates it. Next/previous actions update the active take through `Project.apply(DawAction)`, so edits participate in undo and redo. Because the active take can point to a different media source, the desktop playback graph is marked dirty for rebuild after a switch; this remains off the audio callback thread.

The selected Audio Item Inspector shows the active take number and name, with Duplicate, Previous, Next, Move active to top, Crop to active take, and Delete active take controls. For a multi-selection it exposes selection-level Duplicate, Move, Crop, and Delete actions. Action List, the Item menu, and Inspector route 40639 over selected audio items in project order through one batch transaction; selected single-take items also receive a second take, and a frozen target rejects the whole action. The Action List registers the matching REAPER action names, and the REAPER KeyMap registry maps native action IDs 40639, 40125, 40126, 40057, 40603, 40129, 41380, and 40131 to these commands. Deleting a take preserves the remaining source and chooses the next take in order; deleting the only take removes the item. Action 41380 moves every selected multi-take audio item whose active take is below index 0 in one undoable transaction, leaving selected singleton and already-top items unchanged. Cropping removes inactive takes while preserving the active source and take properties. Action 40131 crops all selected multi-take audio items in one undoable transaction and leaves selected single-take items unchanged. See [Paste audio items as takes](audio-paste-as-takes.md), [Delete the active audio take](audio-delete-active-take.md), [Move the active take to the top](audio-move-active-take-to-top.md), and [Crop to the active take](audio-crop-to-active-take.md). Multi-selection Paste as takes, multi-selection take switching, and other take-list operations remain open.

The earlier live desktop capture used the Linux Stable LLVM binary at 1280×800; the current multi-selection four-state capture uses the Linux nightly Cranelift desktop binary at 1280×800. The selected-state screenshot shows `Take 2/2 · Take A`; the second capture follows the Next take control and shows `Take 1/2 · Take B`, the changed Arrange label, and the alternate source reference in the Inspector. The self-contained [AAADAW project](fixtures/audio-multi-take-aaadaw.aaadaw), [generator](fixtures/audio-multi-take-aaadaw-generator.rs), and [hash/test manifest](fixtures/audio-multi-take-aaadaw.tsv) accompany the captures. The current multi-selection capture verifies the Inspector action, all three Take counts and active sources, and one-step Undo/Redo in the live AAADAW window; its project and screenshot hashes are in the [multi-selection fixture manifest](fixtures/audio-duplicate-active-take-multi-aaadaw.tsv).

![AAADAW selected multi-take items before duplicating active takes](fixtures/audio-duplicate-active-take-multi-aaadaw-before.png)

![AAADAW after duplicating and activating a copy on each selected item](fixtures/audio-duplicate-active-take-multi-aaadaw-after.png)

![AAADAW after one Undo restores the original take counts](fixtures/audio-duplicate-active-take-multi-aaadaw-undo.png)

![AAADAW after one Redo reapplies duplication to all selected items](fixtures/audio-duplicate-active-take-multi-aaadaw-redo.png)

![AAADAW with Take A active and the take controls visible](fixtures/audio-multi-take-aaadaw-selected.png)

![AAADAW after Next take selects Take B](fixtures/audio-multi-take-aaadaw-switched.png)

This build reports `Audio unavailable in this build`; these screenshots verify the real window, selection, and take-switch UI, while playback audibility remains unverified.

Stable LLVM validation passed 847/847 AAADAW desktop tests and 373/373 Core/Storage tests; strict all-target Clippy, formatting, and diff checks passed. The nightly Cranelift desktop build launched for screenshot capture, while its full test harness remains blocked by the backend's unsupported `llvm.x86.sse2.pmulhu.w` intrinsic; the Stable LLVM suite passed in full.

## Remaining parity work

This does not complete `EDIT-TAKE-001`. Duplicate and delete now follow REAPER's multi-selection behavior in one transaction; Paste as takes still operates on one selected source/target pair. Recording-created takes, multi-selection take switching, other take-order operations, and cross-project clipboard behavior remain open. The REAPER `Show all takes (when room)` lane layout, take selector in a full Media Item Properties window, and more-than-two-take interaction still need implementation and native measurement. Fixed lanes, comp areas, and cross-item comp remain open P5 work. The matrix entry therefore remains `in_progress`.

The AAADAW screenshot, test results, binary identity, and artifact hashes are recorded alongside the native evidence in the fixture set.
