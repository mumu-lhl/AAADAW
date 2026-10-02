# Settings category navigation

## Goal

Make the Settings utility easy to navigate as settings categories grow.

## Scope

- Add a fixed left category list and a right detail pane in the independent Settings window.
- Put keyboard shortcuts in their own category and keep capture, clear, conflict feedback, and saving behavior.
- Add a per-Action Restore Default control. For actions with no default shortcut, restore leaves that action unassigned.
- Keep a global Restore All Defaults command as a separate convenience.

## Out of scope

New shortcut types, key sequences, and per-project bindings.

## Dependencies

Existing Settings window, shared Action command definitions, and persisted shortcut mappings.

## Acceptance

Switching categories changes only the right pane and preserves unsaved edits. Restoring one Action changes only its binding to the built-in default (including an intentionally unassigned default); all other bindings remain untouched. Capture and conflict handling remain functional.

## Verification

App tests for category selection and individual reset semantics; `cargo xtest -p aaadaw` and clippy.

## UI acceptance

Inspect the Settings window at 1280×800 and 900×620 in private Xvfb. The left category rail and right action list must remain legible and aligned, with capture, Clear, and Default controls discoverable without a dashboard or card layout.

## Status

Complete: Settings now has a left category rail with a clickable Keyboard Shortcuts category and a right editor showing the complete command list, grouped by Action category. Each Action can clear its current binding or stage restoration of its built-in default, including the unassigned default; Restore All Defaults and Save remain available. Future preference pages can use the same category-to-detail layout without adding empty sections.

Verification: `cargo xtest -p aaadaw` passed (163 tests), strict workspace Clippy passed, and the Settings window was inspected in private Xvfb at 1280×800 and 900×620.
