# Configurable Keyboard Shortcuts

## Goal

Let users change shortcuts for registered DAW commands and keep the bindings across launches.

## Scope

- Give commands stable IDs and keep menu labels, search entries, shortcut hints, and key dispatch on the same definitions.
- Add a compact shortcut editor in Project tools with reset, conflict detection, and clear-to-unbind behavior.
- Persist user bindings in the platform config directory without changing project files.

## Out of scope

Macro recording, key sequences, per-project profiles, and shortcut capture widgets.

## Dependencies

Existing command definitions, Project tools workspace, and global keyboard event handling.

## Acceptance

Edited bindings dispatch the same commands as menus and Actions search, update shortcut hints, survive restart, reject conflicts, and leave text-input-consumed keys untouched. Defaults work when no user config exists.

## Verification

Test parsing, conflicts, config round-trip, default/reset behavior, dispatch, and consumed events; run `cargo xtest` and clippy.

## UI acceptance

Shortcut fields remain readable in the Project workspace at 1280×800 and 900×620 and follow `DESIGN.md`'s compact desktop control styling.

## Status

Complete.
