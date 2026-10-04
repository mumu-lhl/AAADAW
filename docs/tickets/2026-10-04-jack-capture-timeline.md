# Preserve JACK capture gaps on the recording timeline

## User scenario

A musician records an overdub through JACK and expects the captured take to retain its duration even
if the callback is delayed or the recording worker briefly falls behind. Dropped samples must never
be silently compressed into a shorter take.

## Current problem

The JACK callback previously discarded `ProcessScope::last_frame_time()`. The bounded capture queue
carried sample frames only, so the file worker treated blocks on either side of a callback gap as
contiguous. This shifted all later audio earlier in the take.

## Expected behavior

- Carry each JACK block's first server frame position and frame count through bounded callback-owned
  queue storage without allocation, locks, I/O, or blocking.
- Map the first captured block to the existing project recording anchor. Preserve forward gaps
  between subsequent blocks as silence in the WAV so item duration does not shrink.
- Reject timestamp regressions, invalid frame ranges, gaps longer than ten seconds, descriptor
  overflow, or sample overflow as a failed take. Never publish a silently shortened take.
- Preserve recovery segment frame counts and import behavior when silence crosses a WAV segment
  boundary.
- Leave PipeWire stream timing and physical input-latency compensation as separate follow-up work.

## Acceptance checks

- Hardware-free tests cover 44.1 kHz and 48 kHz, varying block lengths, short gaps, regression,
  excessive gaps, frame-position overflow, and both sample and descriptor overflow.
- Existing capture callback remains bounded and real-time safe.
- Run `cargo xtest -p aaadaw-engine -p aaadaw-app`, strict workspace Clippy, and formatting checks.
- Linux JACK feature CI compiles the backend; real-device loopback and latency calibration are not
  claimed by this ticket.

## Status

Implementation is in progress on the JACK timestamp and gap-preservation path. See
[recording latency research](../research/2026-10-03-recording-latency-compensation.md) for backend
clock and latency semantics.
