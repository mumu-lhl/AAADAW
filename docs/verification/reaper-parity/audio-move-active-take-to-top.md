# Move the active take to the top

## REAPER 7.82 reference

The reference is Linux REAPER 7.82 with the Default 7 theme. Main action 41380 is **Item: Move active takes to top** and has no default shortcut. With Take B at index 0 and Take A active at index 1, the action moves Take A to index 0 and keeps Take A active; Take B moves to index 1. Undo restores the prior order without changing the active source, and redo reapplies the new order. Running the action when the active take is already at index 0 does not add an undo step. The native probe readbacks and saved RPP states are listed in the [fixture manifest](fixtures/audio-move-active-take-to-top-aaadaw.tsv).

![REAPER 7.82 after action 41380, with Take A active at index 0](fixtures/audio-move-active-take-to-top-native-after.png)

## AAADAW implementation

The Core action moves the active take's index to zero and shifts earlier takes down one position. It preserves the active source and its mix properties, participates in snapshot persistence and Undo/Redo, and treats an already-top take as a history-free no-op. Since the active source does not change, the action does not rebuild the playback graph. Frozen render items are rejected; the desktop route also follows the normal project-edit busy guard.

The Action List and Item menu expose **Move active takes to top**, with no default shortcut. REAPER Main KeyMap action 41380 maps to the matching command. For a selected multi-take item, the Inspector exposes **Move active to top**. The paired live captures show Take A selected as Take 2/2 before the operation and Take 1/2 afterward, while preserving Take A as the active source.

![AAADAW before moving the active take](fixtures/audio-move-active-take-to-top-aaadaw-before.png)

![AAADAW after moving Take A to index 0](fixtures/audio-move-active-take-to-top-aaadaw-after.png)

The [capture script](../../../scripts/reaper_parity/capture-audio-move-active-take-to-top-aaadaw.sh) runs the desktop binary with an isolated XDG profile, a temporary copy of the project fixture, and a temporary shortcut to invoke the shortcut-free command. The manifest records the exact binary, screenshots, project fixture, test runs, and hashes. Screenshots verify the live window and take state, not audio audibility.

## Remaining parity work

`EDIT-TAKE-001` remains `in_progress`. Multi-item actions, recording-created takes, lane layouts, full Item Properties interactions, comp areas, MIDI-item take handling, and cross-project clipboard behavior remain open.
