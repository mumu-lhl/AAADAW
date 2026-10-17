# Audio Item phase inversion

The existing Linux REAPER 7.82 / Default 7 Media Item Properties reference shows an `Invert phase` control in the Take properties section. The source image is [reaper-item-properties-volume.png](images/reaper-item-properties-volume.png), SHA-256 `8154716759e227d0ac95c9f73f53fa473b608524ec532b77b21703fd00a157a1`.

AAADAW now exposes the audio item's phase state in Item Properties and applies changes as an undoable Core action. The state is included in project snapshots and SQLite schema 46; projects migrating from earlier schemas default to normal polarity. Duplicated items retain the setting, while newly inserted and frozen items start at normal polarity.

Playback publishes volume, mute, and phase together through the existing item-volume atomic. The audio callback only reads that atomic and multiplies both channels by `-1.0` when phase is inverted; it performs no allocation, lock, or graph replacement. Newly compiled playback and offline-render graphs initialize from the saved project state. Loop-area copy/paste carries the phase bit with each source and restores it on undo/redo.

Focused verification:

- `crates/aaadaw-core/tests/audio_item_editing.rs::audio_item_phase_is_undoable_and_persisted_in_snapshots`
- `crates/aaadaw-engine/tests/item_volume.rs::live_item_phase_changes_polarity_without_replacing_the_audio_graph`
- `crates/aaadaw-app/tests/audio_playback.rs::inverted_audio_items_export_with_reversed_polarity`
- `crates/aaadaw-storage/tests/project_store.rs::audio_item_mix_roundtrips_and_schema_38_migrates_with_neutral_defaults`
- `crates/aaadaw/src/app/tests.rs::item_properties_mute_and_phase_apply_atomically_with_undo`
- `crates/aaadaw/src/app/tests.rs::copy_loop_of_selected_audio_area_pastes_source_sections_on_one_undo_step`

The stored REAPER screenshot establishes the control's presence, but this implementation run had no REAPER executable, Xvfb, or display server. No fresh native probe or AAADAW screenshot was captured, and the Item Properties geometry and surrounding Take controls remain incomplete. `ITEM-PROPERTIES-001` therefore remains `in_progress`.
