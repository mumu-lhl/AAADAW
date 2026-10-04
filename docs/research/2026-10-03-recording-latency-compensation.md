# Recording alignment: backend timing and latency

## Current project behavior

JACK capture blocks retain `ProcessScope::last_frame_time()` in a bounded descriptor ring. The control thread reads JACK's current frame time and pairs it with the project transport sample immediately before capture is enabled. The writer maps the first accepted block through that anchor and durably refines the recovery sidecar before writing that block. On stop, the app uses the same mapping for the imported take. The writer still writes forward gaps as silence and rejects regressions, impossible ranges, excessive gaps, and queue overflow. This maps JACK's capture clock to the project clock; it does not compensate physical input or output latency. PipeWire PTS and WASAPI capture time remain local to their input streams and are not yet paired with the project clock. Issue #36 still provides a provisional transport anchor if the process exits before the first mapped callback is persisted.

## Timing terms that must stay separate

| Quantity | Meaning | What it can establish |
| --- | --- | --- |
| Reported input/capture latency | Backend's estimate of delay from the physical input path to the capture port/stream, in frames or time. JACK exposes a port latency range for `Capture`; PipeWire `pw_time.delay` reports delay to device, including filters, and can be negative. | A latency correction only to the extent the backend, graph, and device report the path accurately. It is not a captured block's timestamp. |
| Reported output/playback latency | Delay from a playback port/stream through the output path to the device. JACK reports `Playback` latency; PipeWire's stream timing also exposes delay to device. | Needed to compare a recorded input against audio sent through the output path (e.g. a loopback test). It is not input latency and must not be added to every take blindly. |
| Capture frame position | The time-domain position of the first sample in a particular callback/buffer. JACK's process-cycle frame time plus in-cycle position can identify a cycle; PipeWire can expose stream time and, where negotiated/present, buffer header timing metadata. | Associates each captured block with an audio-server clock. Queue arrival order or callback invocation time alone is not a sample-accurate timestamp. |
| Stream time | A backend clock report mapping a monotonic timestamp to a stream frame/tick position and a device delay. PipeWire `pw_time` contains `now`, `rate`, `ticks`, `delay`, and queue fields; JACK offers cycle/frame time. | Maps clocks and block positions. It is distinct from the physical input latency estimate; do not treat `ticks` or `last_frame_time` as the latency correction itself. |

Latency ranges are not necessarily exact hardware measurements. JACK's latency API is a graph-wide contract: clients report their port latency and JACK propagates it across connections. PipeWire timing describes the stream/device path known to the graph. Interface converters, external digital devices, and unreported graph nodes can leave residual offset. The user may still need a calibrated recording offset.

## JACK capture placement policy (Issue #58)

The JACK adapter queries the connected left and right capture ports' `Capture` ranges on the setup/control path. Automatic correction is used only when both channels report the same precise nonzero range (`min == max`). A zero/default range, an ambiguous range, a missing port, or a channel mismatch is treated as unavailable: the take uses the user calibration value alone and the app reports that automatic capture-latency compensation is unavailable. This avoids presenting JACK's default zero as a trustworthy hardware measurement.

For the accepted value, the app subtracts the reported capture-path frames from the transport-to-capture placement anchor, then combines that signed sample correction with the existing user calibration before persistence. The audio samples are not shifted or resampled. The corrected anchor is the one written to recovery metadata and used for normal import. JACK defines capture latency as the time elapsed since samples read from a port buffer arrived at a terminal port; this is the input-path quantity relevant to the capture placement correction. The reported range still reflects the graph's report and may omit physical converter/device delay. Output latency remains separate and is not added to ordinary takes.

The JACK 2 API documents capture/playback latency ranges as graph-path values, defaults unreported port latency to zero, and says the range should be read after a port is connected (normally in the latency callback). The pinned `jack` Rust crate does not expose that callback, so this slice refreshes a bounded atomic snapshot from JACK's non-realtime port-connection notification and immediately after the explicit connections succeed. Each take reads that snapshot on the control path. Real-server timing and hardware accuracy still require Issue #7 validation.

## Deterministic clock-mapping slice

Issue #56 implements the first backend-independent sample-domain rule and the JACK adapter; its
math and recovery path are tested with synthetic timestamps and require no audio hardware:

1. Represent a capture block with its first-frame timestamp in a monotonic backend frame clock, plus its frame count. Preserve that metadata through the bounded callback-to-worker queue; do not infer it later from queue drain time.
2. At record start, save a clock-to-project anchor: a backend frame position paired with the transport's project sample at that instant. Convert each block's first-frame timestamp to a project sample using this fixed anchor and the project sample rate. Keep the take's sample sequence contiguous in the file, but place/segment it according to timestamped positions if the clock reveals a gap.
3. Keep separately named capture-latency compensation (plus the optional user calibration offset) outside the clock mapping. Keep the sign and unit explicit; reject underflow rather than wrapping. Do not mix output latency into input correction. For overdub/loopback calibration, compare both paths under an explicitly defined reference.
4. Unit-test the mapping with synthetic anchor/callback values at 44.1 and 48 kHz, varying clock rates, nonzero initial callback delay, and arithmetic boundaries. Test gaps/overruns separately; a failed or dropped queue block must not silently compress time.

This slice proves transport-to-JACK-capture clock mapping and signed sample arithmetic deterministically. The app uses the same JACK server's shared frame domain across its separately activated input and output clients; actual devices and physical loopback have not been validated here. A later backend adapter can map PipeWire/WASAPI timestamps and then supply reliable device latency reports. For PipeWire, negotiate/request any metadata needed for per-buffer timestamps, or use a stream-time snapshot with well-defined cycle semantics; the current capture code requests header metadata but does not map its stream-relative PTS onto the playback clock.

## Backend/API evidence

The repository pins `jack = 0.13.5` and `pipewire = 0.10` in `crates/aaadaw-engine/Cargo.toml`; `Cargo.lock` resolves PipeWire Rust bindings to 0.10.1. `jack`'s `ProcessScope::last_frame_time()` is documented as the precise current-cycle start frame and `n_frames()` gives cycle size. Its `Port::get_latency_range()` documents capture/playback latency ranges and their callback context. PipeWire Rust 0.10.1's `stream::Time` wrapper documents `ticks` as monotonically increasing stream position and `delay` as delay to device including path filters; it exposes the stream time via `Stream::time()` behind the `v0_3_50` feature. The current dependency does not enable that feature, and the capture code does not call `time()`.

Primary references:

- JACK API, latency functions: <https://jackaudio.org/api/group__LatencyFunctions.html>
- JACK 2 API header, latency semantics and connected-port requirements: <https://github.com/jackaudio/jack2/blob/17959465a722225a36a8b612aed26764036f258e/common/jack/jack.h>
- JACK API, time functions: <https://jackaudio.org/api/group__TimeFunctions.html>
- JACK API, port functions: <https://jackaudio.org/api/group__PortFunctions.html>
- JACK Rust bindings 0.13.5, `ProcessScope` and client timing: <https://docs.rs/jack/0.13.5/jack/struct.ProcessScope.html> and <https://docs.rs/jack/0.13.5/jack/struct.Client.html>
- JACK Rust bindings 0.13.5, port latency range: <https://docs.rs/jack/0.13.5/jack/struct.Port.html#method.get_latency_range>
- PipeWire API, `pw_time` fields and meanings: <https://docs.pipewire.org/structpw__time.html>
- PipeWire API, streams and stream timing: <https://docs.pipewire.org/group__pw__stream.html>
- PipeWire SPA API, buffer chunk range/stride: <https://docs.pipewire.org/structspa__chunk.html>
- PipeWire SPA API, optional buffer header metadata: <https://docs.pipewire.org/structspa__meta__header.html>
- PipeWire capture example: <https://docs.pipewire.org/audio-capture_8c-example.html>
- Locked Rust wrapper source: `~/.cargo/registry/src/*/jack-0.13.5/src/client/client_impl.rs`, `jack-0.13.5/src/port/port_impl.rs`, and `pipewire-0.10.1/src/stream/mod.rs` (the `Time` fields and `Stream::time()` implementation).

## Realtime constraints

The timestamp and latency values must be available in the callback without locks, allocation, logging, or blocking. Use a bounded, preallocated queue for fixed-size block descriptors/data (or an equivalent bounded timestamp side channel); overflow must invalidate the take rather than silently lose alignment. Read or snapshot negotiated latency on a non-realtime/control path when the backend requires it, then publish a bounded atomic/fixed-size value for callback use. Keep clock conversion and placement arithmetic predictable; file writes, project mutations, UI updates, and device discovery remain off the realtime callback.
