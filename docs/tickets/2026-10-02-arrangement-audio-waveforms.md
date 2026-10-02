# Arrangement Audio Waveforms

## Goal

Show the source signal shape inside Audio Items so users can identify content while arranging.

## Scope

- Decode audio into min/max peak bins on background workers; keep a per-project in-memory cache.
- Render peaks clipped to each item's source range and timeline placement.
- Keep item bounds and selection state readable over the waveform.
- Load waveforms for existing project items and newly imported audio without blocking UI or audio callbacks.

## Out of scope

Persistent peak storage, multi-resolution pyramids, stereo channel display, and destructive editing.

## Dependencies

Audio asset storage, packet decoder, and Arrangement GPU renderer.

## Acceptance

Embedded and linked Audio Items show correct waveform data after background loading. Item splits and source offsets display the corresponding source section. Missing/unsupported media leaves a clear empty waveform area without blocking arrangement editing.

## Verification

Test peak generation across decoder packets, source-range clipping, sample-rate mapping, and stale project results; run `cargo xtest` and clippy.

## UI acceptance

Inspect 1280×800 and 900×620 with Audio selected and unselected. Peak color must remain subordinate to item selection, stay inside item bounds, and not obscure MIDI/track or time-selection states.

## Status

Complete.
