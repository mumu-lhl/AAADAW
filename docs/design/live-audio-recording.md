# Live audio recording ownership

The audio backend callback owns only an `AudioCaptureProducer` and an atomic control handle. A
bounded SPSC queue carries interleaved stereo `f32` frames to one recording worker. The callback
does not allocate, lock, wait, log, or access files. If the queue fills, the producer counts the
dropped frames, disables capture, and marks the take failed. A failed take cannot be re-armed.

The recording worker owns the queue consumer and the temporary PCM24 WAV file. It drains frames in
fixed-size batches, sanitizes non-finite samples, and writes outside the realtime callback. The WAV
header is initially reserved and patched only after a complete, non-empty take has drained. The
worker syncs the finalized file and renames it from `.wav.part` to `.wav` in the project directory.
The RIFF size limit is checked before every write; an oversized take fails and its partial file is
removed. PCM24 was selected because the existing media decoder reads it and it preserves normal
studio capture resolution without adding a new container parser.

Stop ordering is deliberate:

1. Atomically disable queue writes.
2. Stop and join the JACK or PipeWire input callback/thread.
3. Ask the file worker to drain the remaining queue, patch the WAV header, sync, and publish.
4. Import the completed WAV into the project asset store.
5. Apply one `BatchTransaction` that places the shared asset on every track armed when recording
   began.
6. Remove the temporary WAV after the asset import finishes.

Any input, queue, file, or finalization failure invalidates the take and removes its temporary
file. Empty recordings are discarded. The successful project action is applied only after storage
has embedded the full asset, so undo removes all placements together while the embedded source
remains available for redo.
