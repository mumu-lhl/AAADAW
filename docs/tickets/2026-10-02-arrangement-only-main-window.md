# Arrangement-only main window

## Goal

Keep the main window focused on one project in Arrange, with project and media commands discoverable from menus and panels.

## Scope

- Remove the Arrangement/Media/Project page tabs and full-page Media and Project views.
- Keep Media Browser as a dockable panel opened from View; keep project actions in File and Action Search in Actions.
- Add File > New Project to create a fresh empty project with the app's default settings. Preserve unsaved work by disabling the command while dirty, consistent with Open.
- Remove obsolete workspace state and commands while preserving existing menu and panel behavior.

## Out of scope

Mixer workspace, workspace presets, and new project templates.

## Dependencies

Existing menu commands, project replacement guards, and Media Browser dock.

## Acceptance

The app always opens and remains in Arrange; no child-page tabs appear. Media Browser still opens and hides from View, and all file, edit, and import operations remain reachable. New Project creates a clean untitled project and cannot discard a dirty project.

## Verification

App command/guard tests, `cargo xtest -p aaadaw`, and clippy.

## UI acceptance

Inspect the main window and Media Browser dock at 1280×800 and 900×620 in private Xvfb. The menu bar and Arrange should lead the hierarchy; removing tabs should give useful vertical space to timeline rows. Verify the dock does not replace or float above another host application's window.

## Status

Complete. `cargo xtest -p aaadaw` passed (162 workspace tests); clippy passed with one existing `too_many_arguments` warning in `timeline/renderer.rs`. Visual review passed at 1280×800 and 900×620 in private Xvfb; File/View menus and the docked Media Browser were checked.
