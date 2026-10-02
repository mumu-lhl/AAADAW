# Docked Media Browser

## Goal

Keep Arrange visible while importing or repairing project audio sources.

## Scope

- Add a resizable Media Browser pane beside Arrangement with an in-pane hide control.
- Open/close it from View and Actions commands; retain its width while hidden during the session.
- Reflow import, source scan/pack, and relink controls for a narrow dock.

## Out of scope

Floating windows, multi-pane tabs, saved layouts, and docking the Project workspace.

## Dependencies

Arrangement and Media workspace, shared command definitions, and Iced 0.14 pane grid.

## Acceptance

The panel remains usable beside Arrangement; import and source actions keep their existing worker and `DawAction` paths. Closing/reopening keeps its width, switching workspace preserves its visibility state, and project state changes still use the public core interface.

## Verification

Test open/close, workspace changes, resize state, and command availability; run `cargo xtest` and clippy.

## UI acceptance

Inspect Arrange with the dock open at 1280×800 and 900×620. The Arrange canvas remains primary, controls fit without horizontal clipping, and the panel boundary is draggable.

## Status

Complete.
