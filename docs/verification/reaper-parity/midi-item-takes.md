# MIDI Item Takes

REAPER 7.82 Default 7 stores multiple ordered MIDI takes on one media item. The native behavior measured for Main action 40642 is recorded in [the MIDI explode probe](fixtures/midi-explode-takes-in-place-native.tsv) and [the broader explode comparison](audio-explode-takes-in-place.md).

AAADAW now represents the active take index and alternate take snapshots in `MidiItemSnapshot`. Alternate snapshots retain source offset, loop metadata, notes, controllers, pitch bends, Program Changes, aftertouch, channel pressure, opaque Meta/SysEx events, and typed event order. Note IDs remain unique across all takes. SQLite schema 67 stores the active index separately and keeps alternate take metadata/events in `midi_item_takes` and `midi_take_events`; legacy single-take items load with active index 0.

Action 40642 can explode selected MIDI takes in take-list order. Mixed audio/MIDI selections run as one undoable project transaction; Undo and Redo restore all take contents. MIDI playback schedules the active take only. The active take continues to use the established event tables, so the callback and schedule representation do not allocate or inspect inactive take collections.

The current model is intentionally ahead of its editing UI: users cannot yet create, switch, or edit MIDI take lists through the interface. `SplitMidiItem` rejects multi-take MIDI items until its partitioning logic can preserve every take's source range, note identities, event lists, and same-tick order. This avoids silent data loss while the splitter is still single-take-only.

The current Cranelift desktop binary was run under an isolated Xvfb display at 1280×800. The [four-state capture manifest](fixtures/midi-item-takes-aaadaw.tsv) records the fixture, binary, script, and before/after/Undo/Redo screenshot hashes. The overlapped takes share one Arrange range, so the image alone does not reveal the item count; Core/App assertions and native REAPER's item-count readback provide that check.

Focused verification:

- `crates/aaadaw-core/tests/midi_item_takes.rs` checks ordered explosion, Undo/Redo, and safe rejection of multi-take splits.
- `crates/aaadaw-storage/tests/midi_item_takes.rs` round-trips all modeled event kinds, opaque payloads, event order, and a nonzero active index.
- `crates/aaadaw-engine/tests/midi_schedule.rs::midi_playback_compiles_only_the_active_take` checks that inactive notes do not enter the playback plan.
- `crates/aaadaw/src/app/tests.rs::exploding_mixed_audio_and_midi_takes_is_one_undo_step` checks the registered action's mixed selection, one-step Undo/Redo, and deselection.

MIDI take creation/switching/editing, take-aware splitting, visual take selection, and a pixel-level native/AAADAW comparison remain open under `EDIT-TAKE-001`.

![AAADAW before MIDI take explosion](fixtures/midi-item-takes-aaadaw-before.png)

![AAADAW after MIDI take explosion](fixtures/midi-item-takes-aaadaw-after.png)

![AAADAW after Undo](fixtures/midi-item-takes-aaadaw-undo.png)

![AAADAW after Redo](fixtures/midi-item-takes-aaadaw-redo.png)
