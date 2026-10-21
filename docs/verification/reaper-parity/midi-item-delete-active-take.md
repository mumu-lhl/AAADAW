# Delete the Active MIDI Take

Linux REAPER 7.82 / Default 7 maps Main action **40129**, `Take: Delete active take from items`, to both Audio and MIDI items. The native probe uses a selected A/B/C MIDI item with distinct notes and source offsets. Deleting active Take B removes only B, compacts the indices, and activates the preceding Take A. Deleting the first Take activates the new first Take; deleting the last Take activates its predecessor. Deleting a one-Take MIDI item removes the item. Each operation is one Undo/Redo step, and selection follows the native result. The probe reads each state back from REAPER and saves its RPP project.

AAADAW now applies the same operation to MIDI items through the shared Take command, Action List, Item menu, and Inspector. Deleting one of several Takes preserves the other Take snapshots, including names, source offsets, and MIDI events. Deleting the only Take removes the item. Selected Audio and MIDI items can be processed together as one project transaction. Frozen tracks and busy project operations reject the edit. MIDI take removal rebuilds the playback graph through the established project update path.

The native probe and state readback, native RPP files, REAPER screenshot, AAADAW project fixture, Cranelift build identity, and four live screenshots are catalogued in the [evidence manifest](fixtures/midi-delete-active-take-evidence.tsv). The REAPER screenshot shows `Take 1/2: Take A` after deleting active Take B. AAADAW screenshots show the Inspector's Take count and active name before and after the edit, after Undo, and after Redo.

Verification passed:

- Stable Nextest: AAADAW desktop **869/869**; Core and Storage **388/388** across 49 test binaries.
- Strict all-target Clippy for Core, Storage, and AAADAW with `-D warnings -A future-incompatible`.
- Formatting, capture-script shell syntax, parity-matrix structure, and `git diff --check`.
- Live REAPER and AAADAW desktop captures at 1280×800 under isolated Xvfb; screenshot hashes are in the manifest.

`EDIT-TAKE-001` remains `in_progress`. Audio deletion behavior is covered separately in [Delete the active audio take](audio-delete-active-take.md). Empty Audio Takes, recording-created Takes, other Take editing and arrangement controls, Fixed Lanes, Comp Areas, and cross-project Take clipboard behavior remain open.
