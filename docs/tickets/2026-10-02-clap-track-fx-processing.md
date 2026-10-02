# CLAP track FX processing

## Goal

Run enabled, ordered CLAP track processors through the realtime render graph.

## Scope

- Prepare effects off the audio callback and process preallocated stereo buffers in chain order.
- Preserve CLAP instance thread ownership while graphs are replaced or stopped.
- Report unsupported layouts and plugin failures without corrupting the project.
- Verify bypass is transparent and callback processing does not allocate or lock.

## Dependencies

[`2026-10-02-track-fx-project-model.md`](2026-10-02-track-fx-project-model.md), existing CLAP instrument processor and render graph.

## Acceptance

Audio items and MIDI instruments pass through enabled effects in project order; bypassed effects are skipped; graph replacement returns instances to their owner thread safely.

## Verification

Engine processing and lifecycle tests, workspace tests, and JACK/PipeWire feature builds.

## Status

Complete. Stereo CLAP effects load and process through preallocated buffers in ordered enabled chain slots; retired processors return to their matching owner. `cargo xtest` passed 184 workspace tests, and `cargo check -p aaadaw-engine --all-features` passed.
