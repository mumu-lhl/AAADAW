# Transport audio stream underflow diagnostics

## Goal

Tell musicians when playback output contains silence because an audio item stream ran out of queued samples.

## Behavior

- Show the cumulative `PlaybackStats::underrun_samples` count in the transport while an output is open and the count is nonzero.
- Keep this stream-underflow count separate from backend callback errors and device-level JACK/PipeWire XRuns.
- Leave zero-count transport text compact.

## Verification

Passed locally: focused diagnostic formatting test, `cargo xtest` (246 tests), strict Clippy, formatting, and `git diff --check`.

Native Iced GUI rendering has not been visually reviewed in this environment.

## Status

Implementation complete; awaiting review and CI.
