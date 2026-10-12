# Project WAV render bounds

Reference target: Linux REAPER 7.82, Default 7 theme, fresh factory configuration. This record proves AAADAW's current output behavior; it does not claim factory UI or behavior parity where the native render dialog has not yet been captured.

The Render window offers `Entire project` and `Time selection` bounds, a tail duration with up to millisecond precision, Mono/Stereo output channels, and an output-rate picker. Tail input is parsed as decimal seconds without floating-point conversion and converted to the nearest project sample. A time selection is converted from its start-inclusive/end-exclusive tempo-map ticks to project samples when the user starts the render. The rate picker offers the project rate or the eleven explicit rates measured in General Project Settings: 8,000, 11,025, 16,000, 22,050, 32,000, 44,100, 48,000, 88,200, 96,000, 176,400, and 192,000 Hz. The offline queue stores the sample bounds, tail duration, channel count, concrete output rate, format, and Master ceiling in the job, so later selection or control changes do not retarget queued work.

The graph renders at the project rate. When the selected output rate differs, a stateful 192-tap sinc converter runs on the offline worker and writes converted frames to the WAV writer; equal-rate exports bypass the extra converter. Mono output averages the stereo Master mix after conversion. Regression evidence covers 48 kHz to 44.1 kHz, exact output duration, packet-boundary invariance, selected range plus fractional tail, and Mono/Stereo channel headers/data for six linear PCM/IEEE float encodings plus 4-bit IMA ADPCM, 2-bit cADPCM, and G.711 u-Law. Native format menus and PCM8/PCM32/Float64/u-Law/IMA ADPCM/cADPCM outputs are recorded in [Render formats](render-formats.md); native Mono mix evidence for a centered case is recorded in [Render channels](render-channels.md).

Output supports Mono or Stereo; multichannel output remains incomplete. Master mix, selected-track stems, selected tracks via Master, and Master mix plus stems are partially implemented as documented in [Render Source](render-source.md); sample-peak normalization is partially implemented as documented in [Render Normalize](render-normalize.md). Tail precision is limited to milliseconds, and the rate preset list is reused from General rather than verified against the native Render list. Region/item/Razor bounds, render presets, metadata, loudness normalization, and limiting remain incomplete. The selected-range tail currently continues through later timeline material; that detail and the factory default tail duration still require comparison with REAPER. The controlled native channel probe does not cover the full Render window, so this atomic item remains `in_progress`.

WAV output is published only after a successful render. Cancellation and source/render errors drop the temporary file; the regression also verifies that an existing destination is unchanged when cancellation is already requested.

Focused evidence:

- `crates/aaadaw/src/app/audio_export.rs`: range controls resolve against the current project tempo map and missing selections cannot be submitted.
- `crates/aaadaw-app/tests/audio_playback.rs`: a non-zero selection starts on the correct source samples, outputs exactly the selected frame count plus a one-second tail, and keeps the project sample rate/stereo header.
- `crates/aaadaw-app/src/offline_render.rs`: range length validation and cancellation publication guard.

The parity matrix and progress record track the remaining native comparison and sample-rate controls separately.
