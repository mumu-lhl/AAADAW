# Audio item takes

## REAPER 7.82 reference

The native probe used Linux REAPER 7.82 with Default 7 and two WAV sources. Action 40639, **Take: Duplicate active take**, adds a copy at list index 0 and leaves the original active at index 1. In the two-take project, action 40125 switches from index 1 to 0 and 40126 switches from 0 to 1. Switching is recorded in project history and undo/redo restores the prior active take. Action 40435 displays the takes in separate lanes when there is room. The saved RPP marks the active `<TAKE SEL>` block and stores each take's name, source, offset, rate, and mix properties in its own block.

The native readback, saved RPP, and screenshots of Arrange and Media Item Properties are in the [fixture set](fixtures/audio-multi-take-native.tsv). The Arrange capture shows the selected take lane and the `Take 2/2: Take A` label; Media Item Properties shows both take names and the active-take selector.

![REAPER 7.82 Default 7 with two audio takes shown in Arrange](fixtures/audio-multi-take-native-arrange.png)

![REAPER 7.82 Default 7 Media Item Properties with the take selector open](fixtures/audio-multi-take-native-properties-list.png)

## AAADAW implementation slice

Audio items now preserve an ordered active and inactive take list in Core snapshots and SQLite schema 66. Duplicate active take inserts a copy at index 0 while keeping the original active. Next/previous actions update the active take through `Project.apply(DawAction)`, so the edit participates in undo and redo. Because the active take can point to a different media source, the desktop playback graph is marked dirty for rebuild after a switch; this remains off the audio callback thread.

The selected Audio Item Inspector shows the active take number and name, with Duplicate, Previous, and Next controls. The Action List registers the matching REAPER action names, and the REAPER KeyMap registry now maps native action IDs 40639, 40125, and 40126 to these commands. This first UI slice enables actions for one selected Audio Item; multi-item batch switching is not implemented yet.

The live desktop capture used the current Cranelift-built Linux binary at 1280×800. The selected-state screenshot shows `Take 2/2 · Take A`; the second capture follows the Next take control and shows `Take 1/2 · Take B`, the changed Arrange label, and the alternate source reference in the Inspector. The self-contained [AAADAW project](fixtures/audio-multi-take-aaadaw.aaadaw), [generator](fixtures/audio-multi-take-aaadaw-generator.rs), and [hash/test manifest](fixtures/audio-multi-take-aaadaw.tsv) accompany the captures.

![AAADAW with Take A active and the take controls visible](fixtures/audio-multi-take-aaadaw-selected.png)

![AAADAW after Next take selects Take B](fixtures/audio-multi-take-aaadaw-switched.png)

This build reports `Audio unavailable in this build`; these screenshots verify the real window, selection, and take-switch UI, while playback audibility remains unverified.

Stable LLVM validation passed 845/845 AAADAW desktop tests and 369/369 Core/Storage tests; strict all-target Clippy, formatting, and diff checks passed. The nightly Cranelift desktop build launched for screenshot capture, while its full test harness remains blocked by the backend's unsupported `llvm.x86.sse2.pmulhu.w` intrinsic; the Stable LLVM suite passed in full.

## Remaining parity work

This does not complete `EDIT-TAKE-001`. Duplicating currently copies the active take's source; AAADAW does not yet add a distinct source as a take through Paste as takes or recording. The REAPER `Show all takes (when room)` lane layout, take selector in a full Media Item Properties window, take removal/reordering, multiple-selection behavior, and more than two-take navigation still need implementation and native measurement. Fixed lanes, comp areas, recording takes, and cross-item comp remain open P5 work. The matrix entry therefore remains `in_progress`.

The AAADAW screenshot, test results, binary identity, and artifact hashes are recorded alongside the native evidence in the fixture set.
