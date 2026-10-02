# Render MIDI instruments in the realtime graph

## Goal

Route sample-timed MIDI events to their track's prepared CLAP processor and mix its stereo output into JACK and PipeWire playback.

## Scope

- Add track-associated processors to `AudioRenderGraph`; compile MIDI/event scratch buffers before playback.
- Mix stereo instrument audio with PCM and apply track mute, solo, volume, and pan.
- Release active notes on transport stop and stop old processors on the audio thread before graph retirement; return stopped processors for control-thread deactivation.
- Cover rendered note timing, track controls, stop/retirement ownership, and both backend builds with a test instrument.

## Out of scope

Plugin discovery, project assignment UI, and piano-roll editing.

## Dependencies

[`docs/tickets/2026-10-02-clap-host-processor.md`](2026-10-02-clap-host-processor.md), completed.

## Acceptance

An assigned processor renders only its track's MIDI events at the scheduled frame offsets. Stereo output is mixed with audio items and obeys mute/solo/gain/pan. Stop and graph replacement do not leave notes sounding or run plugin teardown on the callback. The callback performs no host-side allocations, locks, or I/O.

## Verification

Use a Clack test instrument through `AudioRenderGraph`; run `cargo xtest`, strict workspace Clippy, and JACK/PipeWire feature builds.

## Status

In progress.
