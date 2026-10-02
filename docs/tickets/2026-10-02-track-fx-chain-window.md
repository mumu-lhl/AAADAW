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

Complete. The track FX control opens a separate chain window with enabled/bypassed plugin rows, dedicated Add and Remove controls, and an Add picker backed by the scan results. The selected plugin GUI is embedded in the right pane; fixed-size editors expand the chain window to fit. X11/XWayland is required for native CLAP editor embedding. Some plugins open during attachment but reject the CLAP show callback, so the window keeps them attached and displays a compatibility note.

`cargo xtest` passed (188 workspace tests), strict app Clippy passed, and the real app was visually checked in private Xvfb with the Physics Guitar CLAP from `/home/mumulhl/.clap`. The editor appeared at its preferred 1080×560 size inside an automatically expanded 1380×650 chain window. The chain window is not configured as always-on-top.
