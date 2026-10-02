# Reimport Changed Embedded Audio

## Goal

Let users refresh a selected Audio Item from its changed original source while keeping its timeline placement undoable.

## Scope

- Show a Reimport action in the selected Audio Item inspector only for a changed embedded source.
- Reuse background import progress/cancel handling to create a new immutable asset snapshot.
- Retarget the selected item through `DawAction::EditAudioItem`, preserving its start, source offset, and length.
- Keep the previous snapshot available for undo and refresh the source status and waveform.

## Out of scope

Bulk retargeting of every placement that shares the source.

## Dependencies

Audio source scanning, `AudioItemImportWorker`, and `EditAudioItem`.

## Acceptance

Reimport is available only for a selected item whose embedded source is `Changed`. Import success swaps only that placement to a new immutable media reference, preserves its timeline geometry, and can be undone/redone. Cancellation or errors leave the placement unchanged. Reimport is blocked for dirty projects, missing/untracked/unverified sources, active audio output, or another project operation.

## Verification

Test worker action construction, busy guards, undo/redo, source status, and cancellation; run `cargo xtest` and clippy.

## UI acceptance

Inspect the selected Audio Item in Arrangement and the changed-source state in the inspector at 1280×800 and 900×620 in private Xvfb. The action must be discoverable beside source status and must not disturb the rest of the inspector layout.

## Status

Complete: the selected-item Inspector offers Reimport for changed embedded sources. The background operation imports a new immutable snapshot, persists metadata, and returns an undoable `EditAudioItem` while preserving that item's timeline geometry.
