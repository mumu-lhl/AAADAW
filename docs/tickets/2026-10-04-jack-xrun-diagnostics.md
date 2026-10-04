# Show JACK device XRuns in playback diagnostics

## User scenario

A musician hears an output dropout and needs to tell whether JACK reported a missed device/server
buffer cycle or the app's media feeder ran out of PCM samples.

## Expected behavior

- Count JACK `NotificationHandler::xrun` events with an atomic increment only.
- Expose the cumulative count through JACK output stats and the existing playback stats refresh path.
- Show a compact `JACK XRuns: N` transport diagnostic only when nonzero and playback remains open.
- Keep JACK XRuns, PCM stream underrun samples, and backend callback errors distinct.
- Represent unsupported backend XRun reporting explicitly rather than displaying a fabricated zero.

## Validation

- Hardware-free tests exercise XRun counter accumulation and zero/nonzero transport formatting.
- Linux JACK feature CI compiles the notification handler.
- No claim is made that the diagnostic prevents XRuns or identifies a physical device cause.

## Scope

This ticket is JACK-only because the pinned PipeWire Rust 0.10.1 stream events expose no explicit
per-stream XRun callback or count. Do not infer XRuns from PipeWire timing ticks. Automatic buffer
size adjustment, PipeWire graph-level monitoring, and Master output limiting/protection remain
separate work. See [device XRun API research](../research/2026-10-04-device-xrun-diagnostics.md).
