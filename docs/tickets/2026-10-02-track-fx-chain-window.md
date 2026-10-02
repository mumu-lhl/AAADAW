# Track FX chain window and plugin selector

## Goal

Manage a track's CLAP chain in the REAPER-inspired workflow specified by `DESIGN.md`.

## Scope

- Add a TCP FX control with empty, active, and bypassed states.
- Open a compact chain window with ordered items and per-plugin enable controls on the left, selected plugin GUI on the right, and separate Add/Remove buttons below.
- Make Add open a second popup listing discovered plugins; selecting one inserts it through `DawAction`.
- Surface missing/incompatible plugin status while keeping chain items editable.

## Dependencies

[`2026-10-02-track-fx-project-model.md`](2026-10-02-track-fx-project-model.md), [`2026-10-02-clap-track-fx-processing.md`](2026-10-02-clap-track-fx-processing.md), plugin discovery.

## Acceptance

Plugin selection and chain management work at default and narrow sizes, including user CLAP instruments from `~/.clap`; the selected plugin's CLAP GUI is usable in the right pane.

## Verification

App tests and real-app visual inspection in private Xvfb; no window is forced always-on-top.

## Status

Blocked by the project model and processing tickets.
