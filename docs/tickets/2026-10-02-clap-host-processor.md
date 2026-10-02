# Build the minimal CLAP instrument processor

## Goal

Provide one tested engine module that loads a CLAP instrument off the audio callback and renders sample-timed note events into preallocated stereo audio buffers.

## Scope

- Inspect an entry for instrument descriptors and load a selected plugin ID.
- Activate at the project sample rate and callback frame limit, accepting a stereo output bus and no audio input bus.
- Convert scheduled Note On/Off events into ordered CLAP events and process without callback allocation or locks.
- Keep plugin instance ownership on the control thread; expose a processor handoff and return path that allows teardown off the realtime callback.
- Exercise load, process, failure, and teardown with a small Clack test instrument.

## Out of scope

Project track routing, live backend integration, parameter editing, plugin state, plugin GUI, and discovery across standard install paths.

## Dependencies

ADR 0005; `clack-host` 0.2 supports the workspace Rust 1.85 MSRV and uses MIT OR Apache-2.0.

## Acceptance

The module rejects non-instruments and unsupported audio-port layouts with actionable errors. Test note events arrive at exact frame offsets, stereo output is preserved, and processor ownership can move to/from an audio thread without dropping the instance there.

## Verification

Focused host tests, `cargo xtest -p aaadaw-engine`, and strict workspace Clippy.

## Status

Complete. Verified with `cargo xtest -p aaadaw-engine` (169 workspace tests passed) and strict workspace Clippy. The host tests cover plugin validation, sample-offset ordering, stereo output, and returning a stopped processor for control-thread teardown.
