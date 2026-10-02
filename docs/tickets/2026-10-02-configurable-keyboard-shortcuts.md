# Configurable Keyboard Shortcuts

## Goal

Let users change shortcuts for registered DAW commands and keep the bindings across launches.

## Scope

- Give commands stable IDs and keep menu labels, search entries, shortcut hints, and key dispatch on the same definitions.
- Move shortcut management into a small Settings window opened from the main menu.
- Replace shortcut text fields with press-to-record controls; support clear-to-unbind, restore defaults, and conflict feedback.
- Persist user bindings in the platform config directory without changing project files.

## Out of scope

Macro recording, key sequences, per-project profiles, and shortcut schemes.

## Dependencies

Existing command definitions, Project tools workspace, and global keyboard event handling.

## Acceptance

The Settings command opens a compact independent window without replacing the active workspace. Shortcut bindings are listed there and can only be changed by pressing a key combination; each can be cleared, all defaults can be restored, and conflicts are explained without replacing the existing binding. Updated bindings dispatch the same commands as menus and Actions search, update shortcut hints, survive restart, and do not trigger while text inputs own keyboard focus.

## Verification

Test key capture, conflicts, config round-trip, default/reset behavior, dispatch, and consumed events; run `cargo xtest` and clippy.

## UI acceptance

Inspect the Settings window at 1280×800 and 900×620 in private Xvfb. It should read as a compact native utility window, keep command names and current bindings aligned, clearly show capture/conflict/cleared states, and return to the existing main workspace after closing.

## Status

In progress: persistent bindings and dispatch exist, but the editor still uses text fields in Project tools and does not meet the updated Settings-window and key-capture requirements.
