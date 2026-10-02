# CLAP plugin paths and discovery

## Goal

Let users configure multiple CLAP search paths and inspect discovered plugins from Settings.

## Scope

- Add a CLAP Plugins settings category with add/remove path and rescan controls.
- Persist user paths in application preferences; seed standard platform paths and `CLAP_PATH` on first launch.
- Recursively find `.clap` files/bundles, inspect them off the UI thread, and show instruments/effects plus per-entry scan errors.
- Deduplicate canonical paths and plugin IDs without hiding failures from other entries.

## Out of scope

Plugin isolation, loading a plugin into a project, and editing plugin parameters.

## Dependencies

Existing Settings category navigation and `clack` entry inspection.

## Acceptance

Users can add several directories, remove a path, rescan, and distinguish discovered plugins from failed entries. Standard CLAP paths and `CLAP_PATH` are used on first launch. Scanning does not block Settings interaction; configured paths survive restart.

## Verification

Test preference persistence, recursive/bundle path enumeration, duplicate handling, and scan errors. Run `cargo xtest`, strict workspace Clippy, and build the app.

## UI acceptance

At 1280×800 and 900×620, the Settings sidebar remains stable and CLAP Plugins shows all configured paths, scan status, and a scrollable catalog without dashboard-style cards. Inspect the real app in private Xvfb.

## Status

Complete. `cargo xtest` passed (179 workspace tests), strict workspace Clippy and the app build passed. Private Xvfb visual review at 760×620 and 640×460 passed; scanning `/home/mumulhl/.clap` found both Physics instruments without errors.
