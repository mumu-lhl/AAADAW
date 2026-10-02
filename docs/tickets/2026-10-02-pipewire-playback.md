# PipeWire Playback Backend

## Goal

Play the existing project render graph through a native Linux PipeWire output alongside JACK.

## Scope

- Add an optional PipeWire stereo output with explicit project-rate format negotiation, bounded callback buffers, silence on callback errors, lock-free transport/graph replacement, and off-thread graph reclamation.
- Integrate play, stop, seek, status, and error reporting into the existing app transport when built with `pipewire-backend`.
- Keep JACK available as its separate low-latency backend.

## Out of scope

Input/recording, device selection UI, sample-rate conversion, and Windows/macOS backends.

## Dependencies

`AudioRenderGraph`, media feeders, project sample clock, and current JACK transport behavior.

## Acceptance

The optional build compiles against system PipeWire, rejects incompatible render capacity before streaming, keeps the realtime callback allocation/lock/I/O free, preserves playback state across graph replacements, and lets the user play/stop/seek with status visible in the app.

## Verification

Test callback/queue behavior without hardware; run `cargo xtest`, clippy, and a silent PipeWire stream smoke check when an isolated server is available.

## UI acceptance

Transport controls describe the active backend, remain compact, and preserve the existing Arrange layout.

## Status

Complete.

## Implementation notes

- Uses the native `pipewire` Rust bindings behind `pipewire-backend`; builds with PipeWire system libraries and does not pull in the Linux ALSA dependency through CPAL.
- PipeWire objects live on a dedicated control thread. The process callback renders into preallocated buffers, uses SPSC transport/retirement queues, writes negotiated interleaved F32LE, and reports stream/callback errors through atomics.
- Feature selection is available in the transport when JACK and PipeWire are both compiled. The selected backend is session-local; JACK remains the default.
- The workspace test suite passed with `pipewire-backend` enabled (152 tests), feature builds and clippy passed. No isolated PipeWire server was configured, so a device stream smoke run was not attempted against the user's active audio session.
