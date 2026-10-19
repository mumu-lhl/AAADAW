# MIDI Item Takes

REAPER 7.82 Default 7 stores multiple ordered MIDI takes on one media item. The native behavior measured for Main action 40642 is recorded in [the MIDI explode probe](fixtures/midi-explode-takes-in-place-native.tsv) and [the broader explode comparison](audio-explode-takes-in-place.md).

AAADAW now represents the active take index and alternate take snapshots in `MidiItemSnapshot`. Alternate snapshots retain source offset, loop metadata, notes, controllers, pitch bends, Program Changes, aftertouch, channel pressure, opaque Meta/SysEx events, and typed event order. Note IDs remain unique across all takes. SQLite schema 67 stores the active index separately and keeps alternate take metadata/events in `midi_item_takes` and `midi_take_events`; legacy single-take items load with active index 0.

Action 40642 can explode selected MIDI takes in take-list order. Mixed audio/MIDI selections run as one undoable project transaction; Undo and Redo restore all take contents. MIDI playback schedules the active take only. The active take continues to use the established event tables, so the callback and schedule representation do not allocate or inspect inactive take collections.

MIDI Item Properties and the Inspector can now select the active MIDI Take, and the Next/Previous actions switch selected MIDI items with Undo/Redo. MIDI Take creation and event editing are still unavailable in the interface. Splitting now partitions every Take in order, preserves the active index and per-Take event order, applies each Take's source offset, and records the replacement as one undoable project event.

REAPER 7.82 / Default 7 action 40012 was measured with active Take A and active Take B, nonzero per-Take offsets, and loop-source phase wrap. Both split items retain the ordered Take list and active index; Undo restores the selected original and Redo restores the two split items. AAADAW's Core and App regressions verify each Take's notes, offsets, event-order targets, right-segment selection, and one-step Undo/Redo. See the [multi-take split evidence](midi-item-multi-take-split.md).

The current Cranelift desktop binary was run under an isolated Xvfb display at 1280×800. The [four-state capture manifest](fixtures/midi-item-takes-aaadaw.tsv) records the fixture, binary, script, and before/after/Undo/Redo screenshot hashes. The overlapped takes share one Arrange range, so the image alone does not reveal the item count; Core/App assertions and native REAPER's item-count readback provide that check.

Focused verification:

- `crates/aaadaw-core/tests/midi_item_takes.rs` checks ordered explosion, per-Take split partitioning, loop-phase wrapping, event order, and Undo/Redo.
- `crates/aaadaw/src/app/tests.rs::cursor_split_preserves_multi_take_midi_and_selects_the_right_segment` checks the selected-item command path, right-segment selection, and Undo/Redo.
- `crates/aaadaw-storage/tests/midi_item_takes.rs` round-trips all modeled event kinds, opaque payloads, event order, and a nonzero active index.
- `crates/aaadaw-engine/tests/midi_schedule.rs::midi_playback_compiles_only_the_active_take` checks that inactive notes do not enter the playback plan.
- `crates/aaadaw/src/app/tests.rs::exploding_mixed_audio_and_midi_takes_is_one_undo_step` checks the registered action's mixed selection, one-step Undo/Redo, and deselection.

MIDI Take creation and event editing, more complete Take arrangement controls, Fixed Lanes, Comp Areas, and cross-project Take clipboard behavior remain open under `EDIT-TAKE-001`.

![AAADAW before MIDI take explosion](fixtures/midi-item-takes-aaadaw-before.png)

![AAADAW after MIDI take explosion](fixtures/midi-item-takes-aaadaw-after.png)

![AAADAW after Undo](fixtures/midi-item-takes-aaadaw-undo.png)

![AAADAW after Redo](fixtures/midi-item-takes-aaadaw-redo.png)
