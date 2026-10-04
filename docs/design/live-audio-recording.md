# Live audio recording ownership

The audio backend callback owns only an `AudioCaptureProducer` and an atomic control handle. A
bounded SPSC queue carries interleaved stereo `f32` frames to one recording worker. The callback
does not allocate, lock, wait, log, or access files. If the queue fills, the producer counts the
dropped frames, disables capture, and marks the take failed. A failed take cannot be re-armed.
JACK blocks also carry the server frame position of their first sample through a bounded descriptor
ring. The writer preserves short forward gaps as silence in the WAV; a regressing clock, invalid
block range, excessive gap, or descriptor overflow invalidates the take. Backends without capture
timestamps currently retain contiguous-queue behavior.
PipeWire capture requests per-buffer `SPA_META_Header` metadata and uses its PTS when available;
missing, corrupted, or unrepresentable timing invalidates that take rather than falling back to
callback order. `Stream::time().ticks` is not treated as the current buffer's first frame.

The recording worker owns the queue consumer and temporary PCM24 WAV segments. It drains frames in
fixed-size batches, sanitizes non-finite samples, and writes outside the realtime callback. Each
segment stays below the RIFF size limit; its header is patched only after that segment is complete,
then the file is synced and published from `.wav.part` to `.wav` in the project directory. This
keeps long takes in the existing decoder's supported PCM24 WAV format without one file exceeding
RIFF's 32-bit size fields.

Stop ordering is deliberate:

1. Atomically disable queue writes.
2. Stop and join the JACK or PipeWire input callback/thread.
3. Ask the file worker to drain the remaining queue, patch each WAV header, sync, and publish every
   segment in order.
4. Import the WAV segments into the project asset store in order.
5. Apply one `BatchTransaction` that places each segment contiguously on every track armed when
   recording began.
6. Remove the temporary WAV segments after their asset imports finish.

Any input, queue, file, or finalization failure invalidates the take and removes its temporary
segments. If importing a later segment fails, the app removes already imported assets that have no
persisted item references. Empty recordings are discarded. The successful project action is applied
only after storage has embedded every segment, so undo removes all placements together while the
embedded sources remain available for redo.
