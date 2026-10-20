# Move the Active MIDI Take to the Top

Linux REAPER 7.82 / Default 7 maps Main action **41380**, `Item: Move active takes to top`, to MIDI items as well as audio; it has no default shortcut. In a selection containing a two-Take item active at index 1, a one-Take item, a three-Take item active at index 2, and a two-Take item already active at index 0, the action moves only the active Takes below index 0. It preserves the active source, event data, and relative order of every other Take. The single-Take and already-top items remain unchanged. All selected items stay selected, one Undo restores the original order, and Redo reapplies it. Running the action after every active Take is at the top adds no undo step.

AAADAW now applies action 41380 to Audio, MIDI, and mixed Audio/MIDI selections. Core reorders MIDI Take indices while retaining each Take's source offset, loop settings, and events. The Action List and Item menu command work for selected MIDI items, and the Inspector exposes **Move active to top** / **Move selected to top**. Frozen targets are rejected; project busy states block the action. Reordering does not rebuild the playback graph because the active MIDI source and events stay the same.

The REAPER probe/readback, before/after/Undo/Redo/no-op RPP states, native screenshot, matching AAADAW project, Cranelift binary identity, and four live screenshots are in the [evidence manifest](fixtures/midi-move-active-take-to-top-evidence.tsv). The screenshots show the Inspector changing `Take 2/2 · Take A` to `Take 1/2 · Take A`, then restoring and reapplying that state.

Verification passed:

- Stable Nextest: AAADAW desktop **870/870**; Core and Storage **389/389** across 49 test binaries.
- Strict all-target Clippy for Core, Storage, and AAADAW with `-D warnings -A future-incompatible`.
- Formatting, both capture-script shell syntax checks, parity-matrix structure, and `git diff --check`.
- Live REAPER and AAADAW desktop captures at 1280×800 under isolated Xvfb; artifact SHA-256 values are in the manifest.

`EDIT-TAKE-001` remains `in_progress`; take recording, remaining Take arrangement controls, Fixed Lanes, Comp Areas, and cross-project Take clipboard behavior are still open.
