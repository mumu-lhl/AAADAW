# Selectable Arrange Snap Grid

## Goal

Let users choose the musical grid used by Arrange snapping.

## Scope

- Keep the Snap on/off toggle separate from a neighboring grid selector; default to 1/16.
- Offer common note values plus dotted and triplet subdivisions.
- Apply the selected grid consistently to item dragging and time-selection edges.
- Keep Shift-drag as a temporary snap bypass.

## Out of scope

MIDI note quantization, project-specific snap presets, and non-musical time grids.

## Dependencies

Existing meter-aware timeline tick mapping, item drag preview, time selection, and toolbar.

## Acceptance

Changing the grid changes drag and time-selection snap positions without changing Snap's enabled state. Grid changes survive project tempo and meter changes. Snap bypass and undo behavior remain unchanged.

## Verification

Test grid intervals and snap positions across supported note, dotted, and triplet values; run `cargo xtest -p aaadaw` and strict workspace Clippy.

## UI acceptance

Inspect the Arrange toolbar at 1280×800 and 900×620 in private Xvfb. The selected grid must be readable beside Snap, and the controls must remain usable at 900 px without obscuring Arrange.

## Status

Complete.

## Completion notes

- Verified all grid intervals and drag/selection snap behavior, including Shift bypass, with `cargo xtest -p aaadaw` (166 tests passed).
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo build -p aaadaw` passed.
- Inspected the Snap and Grid controls at 1280×800 and 900×620 in private Xvfb; the selector and full dotted/triplet labels remain readable at both sizes.
