# Track FX chain editor

## Goal

Deliver the REAPER-inspired per-track CLAP workflow defined in `DESIGN.md`.

## Scope

Coordinate the project model, realtime processing, and editor slices below.

## Out of scope

Plugin process isolation, VST3, sends/routing, parameter automation, and plugin private-state persistence beyond metadata required to reopen a chain.

## Dependencies

CLAP discovery, CLAP host processor and render graph, existing core Action and storage migration patterns.

## Acceptance

Users can add a discovered CLAP plugin to a track, select it, open its GUI, bypass/enable it, remove it, save/reopen the project, and hear the resulting chain. Invalid or missing plugins leave the project editable and report their status.

## Implementation tickets

- [`2026-10-02-track-fx-project-model.md`](2026-10-02-track-fx-project-model.md): ordered chain state, Actions, undo/redo, and storage.
- [`2026-10-02-clap-track-fx-processing.md`](2026-10-02-clap-track-fx-processing.md): realtime effect processing and safe CLAP lifecycle.
- [`2026-10-02-track-fx-chain-window.md`](2026-10-02-track-fx-chain-window.md): TCP entry, chain window, plugin selector popup, and plugin GUI.

## Verification

Run the workspace test, lint, backend build matrix, and visual review once all slices are complete.

## Status

In progress.
