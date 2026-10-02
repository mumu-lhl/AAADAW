# Duplicate MIDI Item

## Goal

Duplicate a selected MIDI item as a new adjacent item with independent note IDs.

## Scope

- Add an atomic `DawAction` with undo/redo.
- Expose it in the Item menu, Actions search, and MIDI item context menu.
- Place the duplicate immediately after the source, matching Audio item duplication.

## Out of scope

Copy/paste, drag-copy, and duplicate-to-time-selection.

## Dependencies

Existing MIDI item model and Arrangement Item context menu.

## Acceptance

The duplicate preserves track, length, and note data; has fresh item/note IDs; and leaves the source unchanged. Overflow and missing-item errors do not alter state or history. One undo removes the duplicate and redo restores it.

## Verification

Core and app tests for content, IDs, overflow, command availability, and undo/redo; run `cargo xtest`.

## UI acceptance

The duplicate command appears only for a MIDI item selection and uses the same label and behavior in the Item menu, Actions search, and item context menu.

## Status

Complete.
