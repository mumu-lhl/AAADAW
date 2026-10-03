# Recording start anchor

## User scenario

A musician records an overdub while playback is running and expects the take to begin at the playhead position when capture actually starts.

## Root cause

`App::begin_recording` sampled the playhead, then waited for a durable recovery-manifest write and filesystem sync before enabling capture. Playback continued during that I/O, so the sample anchor could precede the first captured frames.

## Behavior

- Persist a provisional position before capture activation so crash recovery has a useful fallback.
- After that write completes, sample the current playhead and enable capture in the same UI/control update, without filesystem I/O between them.
- Queue the refined position to the recording writer, which durably updates the manifest off the realtime callback and without blocking capture activation.
- Until the refined position reaches disk, the manifest marks its saved sample as an estimate. Recovery warns when it must use that fallback; a take with no saved position is not silently placed at sample zero.

## Verification

The focused `aaadaw-app` tests verify delayed setup anchor refinement and ordered manifest persistence. JACK/PipeWire device timing and loopback latency are still unverified; see Issue #7 and the recording latency research note.

## Status

Implementation in progress for Issue #36.
