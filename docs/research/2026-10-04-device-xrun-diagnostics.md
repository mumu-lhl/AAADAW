# Device XRun diagnostics: JACK and PipeWire

## Findings

The engine pins `jack = 0.13.5` and `pipewire = 0.10` in `crates/aaadaw-engine/Cargo.toml`; the lockfile resolves PipeWire Rust to 0.10.1. JACK has a direct `NotificationHandler::xrun` callback: its docs define an xrun as a buffer under/overrun where data was missed. The callback does not promise non-realtime execution; the JACK wrapper separately labels `ProcessHandler::process` realtime and `thread_init` non-realtime. Treat `xrun` as realtime-sensitive and keep it to a bounded atomic counter update (no logging, locks, allocation, or UI work). Read that counter from the control/UI side. A relaxed `AtomicU64::fetch_add(1)` is consistent with the existing backend counters.

PipeWire Rust 0.10.1's `StreamEvents` has state, control, IO, parameter, buffer, process, and drained callbacks, but no xrun/underrun event or xrun count. The app connects both PipeWire streams with `RT_PROCESS`; its process callback therefore belongs to the realtime data path and should follow the same atomic-only rule. `Stream::time()` is explicitly RT-safe and returns monotonically increasing stream `ticks`, but its docs do not define skipped ticks as an xrun counter. A timing discontinuity can be measured as an observation, but should not be reported as a device XRun without a validated definition. Device/xrun attribution through a PipeWire graph monitor remains separate research.

Keep three signals separate:

- `PlaybackStats::underrun_samples`: the app's render graph could not supply queued PCM samples for playback; this is already shown in the transport.
- JACK backend XRuns: JACK explicitly notified this client of a missed buffer cycle.
- PipeWire timing observations or stream errors: distinct backend observations; the pinned per-stream API does not provide a direct XRun event.

These can happen independently or together. Do not merge them into the existing generic callback-error count or imply that PCM starvation proves a device XRun.

## Recommended next tracer bullet

Add a JACK-only cumulative backend-XRun counter, wire `NotificationHandler::xrun` to an atomic increment, expose it beside (but separately labeled from) the PCM underrun sample count, and test the counter-to-transport formatting without audio hardware. Do not synthesize a PipeWire XRun count from `ticks` in this slice. Follow up with a PipeWire-specific investigation only if the project needs an equivalent device-level signal and can define/test its semantics against a real graph.

## Primary sources

- Repository dependency versions: [`crates/aaadaw-engine/Cargo.toml`](../../crates/aaadaw-engine/Cargo.toml) and [`Cargo.lock`](../../Cargo.lock).
- JACK 0.13.5 [`NotificationHandler` / `xrun`](https://docs.rs/jack/0.13.5/jack/trait.NotificationHandler.html#tymethod.xrun), [`ProcessHandler`](https://docs.rs/jack/0.13.5/jack/trait.ProcessHandler.html), and [versioned source](https://github.com/RustAudio/rust-jack/blob/0.13.5/src/client/callbacks.rs).
- PipeWire Rust 0.10.1 [`StreamEvents` and callback builder](https://docs.rs/pipewire/0.10.1/pipewire/stream/struct.ListenerLocalCallbacks.html), [`Stream::time()`](https://docs.rs/pipewire/0.10.1/pipewire/stream/struct.Stream.html#method.time), and [versioned source](https://gitlab.freedesktop.org/pipewire/pipewire-rs/-/blob/0.10.1/pipewire/src/stream/mod.rs).
- PipeWire [`pw_time`](https://docs.pipewire.org/structpw__time.html) defines stream timing fields and their meanings; it does not define `ticks` as an XRun counter.
- Current app-side PCM underflow accounting: [`AudioRenderGraph::render_into`](../../crates/aaadaw-engine/src/lib.rs), [`jack_output.rs`](../../crates/aaadaw-engine/src/jack_output.rs), and [`pipewire_output.rs`](../../crates/aaadaw-engine/src/pipewire_output.rs).
