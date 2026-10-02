# Arrangement Item Context Menu

## Goal

Expose common Audio/MIDI item operations at the pointer through the Arrangement context menu.

## Scope

- Right-click an item to open a compact context menu at the pointer.
- Right-clicking an unselected item selects it; right-clicking a selected item keeps the multi-selection.
- Reuse existing item commands and selection-aware availability.
- Close the menu after an action, Escape, or clicking elsewhere.

## Out of scope

Item edge editing, slip/stretch, and new project actions.

## Dependencies

A3.2 split commands; existing shared command definitions and Track context menu.

## Acceptance

Audio and MIDI items expose applicable duplicate/delete/split commands. Multi-selection remains intact when its selected item opens the menu. Each command has the same undo behavior as its menu/Actions entry.

## Verification

Test target hit detection, selection preservation, command availability, dismissal, and action routing; run `cargo xtest`.

## UI acceptance

Inspect at 1280×800 and 900×620. The popup must stay inside the Arrangement viewport, use compact DAW menu styling, and avoid obscuring the item under the pointer more than necessary.

## Status

Complete.
