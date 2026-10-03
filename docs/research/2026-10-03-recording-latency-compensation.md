# Recording alignment: backend timing and latency

## Current project behavior

The capture callbacks currently copy stereo samples into an SPSC queue with no frame timestamp (`crates/aaadaw-engine/src/capture.rs`). JACK's callback ignores its `ProcessScope`; PipeWire reads `spa_chunk` offset/size/stride but discards the buffer metadata and stream clock (`jack_input.rs`, `pipewire_input.rs`). The app places the take from one project-sample anchor, so it cannot account for input-to-callback delay, later block timing, or a gap in capture. Issue #36 fixes one concrete part: after the initial recovery-manifest sync, the app resamples the playhead immediately before enabling capture and queues a durable manifest correction on the writer thread. If a crash happens before that correction is flushed, recovery labels and warns that the older persisted position is only an estimate. Backend callback timestamps, discontinuity preservation, and hardware latency correction remain unimplemented.

## Timing terms that must stay separate

| Quantity | Meaning | What it can establish |
| --- | --- | --- |
| Reported input/capture latency | Backend's estimate of delay from the physical input path to the capture port/stream, in frames or time. JACK exposes a port latency range for `Capture`; PipeWire `pw_time.delay` reports delay to device, including filters, and can be negative. | A latency correction only to the extent the backend, graph, and device report the path accurately. It is not a captured block's timestamp. |
| Reported output/playback latency | Delay from a playback port/stream through the output path to the device. JACK reports `Playback` latency; PipeWire's stream timing also exposes delay to device. | Needed to compare a recorded input against audio sent through the output path (e.g. a loopback test). It is not input latency and must not be added to every take blindly. |
| Capture frame position | The time-domain position of the first sample in a particular callback/buffer. JACK's process-cycle frame time plus in-cycle position can identify a cycle; PipeWire can expose stream time and, where negotiated/present, buffer header timing metadata. | Associates each captured block with an audio-server clock. Queue arrival order or callback invocation time alone is not a sample-accurate timestamp. |
| Stream time | A backend clock report mapping a monotonic timestamp to a stream frame/tick position and a device delay. PipeWire `pw_time` contains `now`, `rate`, `ticks`, `delay`, and queue fields; JACK offers cycle/frame time. | Maps clocks and block positions. It is distinct from the physical input latency estimate; do not treat `ticks` or `last_frame_time` as the latency correction itself. |

Latency ranges are not necessarily exact hardware measurements. JACK's latency API is a graph-wide contract: clients report their port latency and JACK propagates it across connections. PipeWire timing describes the stream/device path known to the graph. Interface converters, external digital devices, and unreported graph nodes can leave residual offset. The user may still need a calibrated recording offset.

## Small deterministic implementation slice

Start with backend-independent sample-domain logic and synthetic timestamps; no audio hardware is needed to test the rule:

1. Represent a capture block with its first-frame timestamp in a monotonic backend frame clock, plus its frame count. Preserve that metadata through the bounded callback-to-worker queue; do not infer it later from queue drain time.
2. At record start, save a clock-to-project anchor: a backend frame position paired with the transport's project sample at that instant. Convert each block's first-frame timestamp to a project sample using this fixed anchor and the project sample rate. Keep the take's sample sequence contiguous in the file, but place/segment it according to timestamped positions if the clock reveals a gap.
3. Apply a separately named capture-latency compensation (plus an optional user calibration offset) to the mapped timeline position. Keep the sign and unit explicit; clamp/reject underflow rather than wrapping. Do not mix output latency into the input correction. For overdub/loopback calibration, compare both paths under an explicitly defined reference.
4. Unit-test the mapping with synthetic anchor/callback values at 44.1 and 48 kHz, variable callback sizes, a start delay, and reported positive/zero/negative offsets. An especially useful fixture starts capture 256 frames after the transport anchor and reports 128 frames of input latency; expected timeline placement is derived exactly from those declared inputs. Test gaps/overruns separately; a failed or dropped queue block must not silently compress time.

This first slice proves transport-to-capture clock mapping and signed sample arithmetic deterministically. A later backend adapter can supply real timestamps and latency reports; hardware loopback is needed to validate the complete physical path and calibration. Before using JACK timestamps across separately activated clients, confirm they share the same JACK server frame domain. For PipeWire, negotiate/request any metadata needed for per-buffer timestamps, or use a stream-time snapshot with well-defined cycle semantics; the current code requests only audio format and does not request header metadata.

## Backend/API evidence

The repository pins `jack = 0.13.5` and `pipewire = 0.10` in `crates/aaadaw-engine/Cargo.toml`; `Cargo.lock` resolves PipeWire Rust bindings to 0.10.1. `jack`'s `ProcessScope::last_frame_time()` is documented as the precise current-cycle start frame and `n_frames()` gives cycle size. Its `Port::get_latency_range()` documents capture/playback latency ranges and their callback context. PipeWire Rust 0.10.1's `stream::Time` wrapper documents `ticks` as monotonically increasing stream position and `delay` as delay to device including path filters; it exposes the stream time via `Stream::time()` behind the `v0_3_50` feature. The current dependency does not enable that feature, and the capture code does not call `time()`.

Primary references:

- JACK API, latency functions: <https://jackaudio.org/api/group__LatencyFunctions.html>
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
