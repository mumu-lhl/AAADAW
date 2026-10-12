# Project WAV render bounds

Reference target: Linux REAPER 7.82, Default 7 theme, fresh factory configuration. This record proves AAADAW's current output behavior; it does not claim factory UI or behavior parity where the native render dialog has not yet been captured.

The Render window offers `Entire project` and `Time selection` bounds, a tail duration with up to millisecond precision, and an output-rate picker. Tail input is parsed as decimal seconds without floating-point conversion and converted to the nearest project sample. A time selection is converted from its start-inclusive/end-exclusive tempo-map ticks to project samples when the user starts the render. The rate picker offers the project rate or the eleven explicit rates measured in General Project Settings: 8,000, 11,025, 16,000, 22,050, 32,000, 44,100, 48,000, 88,200, 96,000, 176,400, and 192,000 Hz. The offline queue stores the sample bounds, tail duration, concrete output rate, format, and Master ceiling in the job, so later selection or control changes do not retarget queued work.

The graph renders at the project rate. When the selected output rate differs, a stateful 192-tap sinc converter runs on the offline worker and writes converted frames to the WAV writer; equal-rate exports bypass the extra converter. Regression evidence covers 48 kHz to 44.1 kHz, exact one-second output duration, packet-boundary invariance, and the selected range plus tail in the final WAV header/data.

Output is still stereo, tail precision is limited to milliseconds, and the rate preset list is reused from General rather than verified against the native Render list. Source selection, regions/stems/other bounds, render presets, metadata, normalization, and limiting are not implemented. The selected-range tail currently continues through later timeline material; that detail and the factory default tail duration still require comparison with REAPER. No `UI-028` native Render capture is committed yet, so this atomic item remains `in_progress`.

WAV output is published only after a successful render. Cancellation and source/render errors drop the temporary file; the regression also verifies that an existing destination is unchanged when cancellation is already requested.

Focused evidence:

- `crates/aaadaw/src/app/audio_export.rs`: range controls resolve against the current project tempo map and missing selections cannot be submitted.
- `crates/aaadaw-app/tests/audio_playback.rs`: a non-zero selection starts on the correct source samples, outputs exactly the selected frame count plus a one-second tail, and keeps the project sample rate/stereo header.
- `crates/aaadaw-app/src/offline_render.rs`: range length validation and cancellation publication guard.

The parity matrix and progress record track the remaining native comparison and sample-rate controls separately.
