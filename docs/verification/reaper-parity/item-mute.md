# Media Item mute

Linux REAPER 7.82 / Default 7 records Main action 40175 as `Item properties: Toggle mute`. The existing [Media Item Properties reference](images/reaper-item-properties-volume.png) shows the `Mute` checkbox unchecked by default; the action catalog is in [the reference action list](linux-7.82/actions.tsv).

When an item is muted, REAPER also shows a red `M` badge in its Arrange item header. The repeatable [native ReaScript probe](../../../scripts/reaper_parity/probe-item-mute-visual.lua) creates one selected C4 MIDI item, sets its mute bit, and records the version/theme/transport state. The [native capture](fixtures/item-mute-native.png) and [focused crop](fixtures/item-mute-native-item-crop.png) show the badge.

## AAADAW implementation

AAADAW stores mute state on Audio and MIDI items. Schema 44 adds Audio mute and schema 45 adds MIDI mute; older projects migrate both states to off. Action 40175 applies one undoable transaction to Audio, MIDI, or mixed selections. The Audio Item Properties checkbox and MIDI Properties view use drafts with Apply/Cancel; MIDI Properties also edits Name, Position, Length, and Start in source in Time/Beats/H:M:S:F/Samples. Combined edits apply as one history step and preserve note data. The remaining MIDI timebase, take mix/rate/pitch, other source controls, and fade fields are not implemented.

For Audio, the realtime control publishes volume and mute together through one atomic value; the render callback applies zero gain without adding locks, allocations, or I/O. Offline rendering uses the same mute state. MIDI mute filters that item’s events out of the compiled realtime and offline event plans. The command marks the playback graph dirty, and MIDI-containing selections are disabled while playback is active so an existing plan cannot continue emitting the muted item. Snapshot restoration, SQLite migration, Undo/Redo, duplicate-state preservation, and realtime/offline scheduling have regression coverage.

The Arrange timeline cache now carries mute state for both item types, and the label layer draws a red `M` badge in the item header while enough width is visible. The reproducible [muted MIDI project generator](../../../crates/aaadaw-storage/examples/generate_item_mute_demo.rs), [AAADAW capture](fixtures/item-mute-aaadaw.png), and [focused crop](fixtures/item-mute-aaadaw-item-crop.png) preserve the result. The regression `timeline_cache_preserves_audio_and_midi_item_mute_states` verifies that both mute bits reach the drawing cache.

## Remaining parity and validation

Audio loop copy/paste carries the source mute state through Paste and Undo/Redo, but native copy behavior has not been re-probed. Native mixed-selection/runtime semantics, exact menu placement and enabled states, complete property-window geometry, and full visual alignment remain open. A playback-time native probe could not run because this container has no working audio device: REAPER reports JACK client creation failure and remains stopped. The capture manifest records native/AAADAW versions, binary identity, dimensions, and screenshot hashes in [`item-mute-visual.tsv`](fixtures/item-mute-visual.tsv).

The focused cache regression passed 1/1; the demo project generator compiles; full workspace Nextest passed 1390/1390 across 73 binaries under Xvfb :2307. Nightly Cranelift built the desktop binary used for the AAADAW capture. Stable workspace all-target Clippy with `-D warnings`, rustfmt, CRLF-aware diff check, matrix structure checks, and four screenshot hash checks passed. The earlier mute-model validation also covered 1066/1066 workspace tests with two X11-dependent CLAP floating-editor tests excluded. `ITEM-MUTE-001` remains `in_progress`.
