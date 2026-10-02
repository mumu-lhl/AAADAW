# Track FX chain editor

## Goal

Provide the REAPER-inspired per-track FX workflow defined in `DESIGN.md`.

## Scope

- Persist ordered CLAP plugins and enabled/bypassed state per track through `DawAction`.
- Add an FX control to each TCP row with clear empty/active/bypassed states.
- Open a track FX chain window with the ordered enabled list on the left, selected plugin GUI on the right, separate Add and Remove commands, and chain commands along the bottom. Add opens a separate plugin selector window populated from discovery results.
- Load/process compatible plugins on the control/audio paths with safe instance ownership and graph replacement.

## Out of scope

Plugin process isolation, VST3, sends/routing, and plugin private-state persistence beyond metadata required to reopen the chain.

## Dependencies

[`docs/tickets/2026-10-02-clap-plugin-discovery.md`](2026-10-02-clap-plugin-discovery.md), CLAP host processor and render graph, existing core Action and storage migration patterns.

## Acceptance

Users can add a discovered CLAP plugin to a track, select it, open its GUI, bypass/enable it, remove it, save/reopen the project, and hear the resulting chain. Invalid or missing plugins leave the project editable and report their status.

## Verification

Test ordered chain actions, undo/redo, persistence, plugin lifecycle and bypass processing; run the workspace test, lint, and backend build matrix.

## UI acceptance

The chain layout and plugin selector follow `DESIGN.md` at default and narrow window sizes. Run and inspect the real app in private Xvfb; verify Add and Remove remain distinct, the selector shows scanned plugins, enabled state is visible beyond color, and the selected plugin interface is usable.

## Status

Ready; plugin discovery is complete.
