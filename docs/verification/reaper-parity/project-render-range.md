# Project WAV render bounds

Reference target: Linux REAPER 7.82, Default 7 theme, fresh factory configuration. This record proves AAADAW's current output behavior; it does not claim factory UI or behavior parity where the native render dialog has not yet been captured.

The Render window now offers `Entire project` and `Time selection` bounds plus a whole-number tail duration. A time selection is converted from its start-inclusive/end-exclusive tempo-map ticks to project samples when the user starts the render. The offline queue stores those sample bounds, the tail value, output format, and Master ceiling in the job, so later selection or control changes do not retarget queued work. The render worker seeks to the captured start sample and writes the selected frame count followed by the configured tail. The existing default remains two seconds.

The current output remains stereo at the project's sample rate. Source selection, alternate sample rates/channels, tail sub-second input, regions, stems, render presets, metadata, normalization, and limiting are not implemented. The selected-range tail currently continues through later timeline material; that detail and the factory default tail duration still require comparison with REAPER. No `UI-028` native Render capture is committed yet, so this atomic item remains `in_progress`.

WAV output is published only after a successful render. Cancellation and source/render errors drop the temporary file; the regression also verifies that an existing destination is unchanged when cancellation is already requested.

Focused evidence:

- `crates/aaadaw/src/app/audio_export.rs`: range controls resolve against the current project tempo map and missing selections cannot be submitted.
- `crates/aaadaw-app/tests/audio_playback.rs`: a non-zero selection starts on the correct source samples, outputs exactly the selected frame count plus a one-second tail, and keeps the project sample rate/stereo header.
- `crates/aaadaw-app/src/offline_render.rs`: range length validation and cancellation publication guard.

The parity matrix and progress record track the remaining native comparison and sample-rate controls separately.
