# Realtime rendering checks

## Reproducible local benchmark

Run `cargo bench -p aaadaw-engine --bench render_callback`. The harness prints the host OS/arch,
Rust compiler version, available thread count, build profile and workload configuration. It warms
each graph, measures 256 render blocks, repeats each scenario seven times, and reports the median
callback time and estimated realtime multiple at 48 kHz. Project construction, MIDI arrangement,
queue filling, graph compilation, and output allocation happen outside the timed section.

Scenarios cover 8 tracks at 128 frames, and 64 tracks at both 128 and 512 frames. Each track has
an active AudioItem-backed PCM stream; the two 64-track cases also schedule MIDI notes across
tracks. The current harness uses same-rate mono test samples, the built-in mixer, and the Master
sample-peak guard; it does not include CLAP plugin execution, media decoding, device scheduling,
or GUI load. Results are a local comparison aid, not a hardware-independent performance
guarantee. Do not add CI pass/fail thresholds based on wall-clock duration.

Example run in the development container (Linux x86_64, Rust 1.99.0, 2 available threads):

| Tracks | AudioItems | MIDI Items | Block frames | Median callback | Realtime multiple |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 8 | 0 | 128 | 1,654 ns | 1,612.7x |
| 64 | 64 | 64 | 128 | 12,338 ns | 216.1x |
| 64 | 64 | 64 | 512 | 53,518 ns | 199.3x |

Use a run from the target machine as its baseline; these container numbers are only an execution
example.

## Callback audit

Audited the render path through `JackProcessHandler::process`, the PipeWire stream `process`
listener and `ProcessData::process_bytes`, `AudioRenderGraph::render_into`, the PCM queue consumer,
track mixer, final Master guard, callback counters, and the CLAP processor calls.

| Realtime path | Prepared or bounded work | Work kept off the callback |
| --- | --- | --- |
| JACK process handler | Read the bounded command queue, move retired graphs into a bounded SPSC queue, render into preallocated scratch, copy output, update atomics. | Graph construction and command production are control-thread work. Retired graph destruction and processor deactivation are control-thread work. |
| PipeWire process listener | Dequeue a server buffer, validate its fixed output slice, render into preallocated scratch, encode stereo `f32`, set chunk metadata, update atomics. | Stream setup/negotiation and graph construction happen before playback. Callback-owned graphs are collected and destroyed by the control thread. |
| WASAPI output callback (via CPAL) | Read the bounded command queue, retire replaced graphs through a bounded SPSC queue, render into preallocated stereo scratch, convert to the negotiated PCM type, silence invalid or lost-device blocks, and update atomics. The requested block size is clamped to the device range and graph capacity; configurations without a reported buffer size or a viable range are rejected before playback. | Device/configuration negotiation and graph construction happen before playback. Retired graphs are collected and destroyed by the control thread. An output error marks the device unavailable; the UI stops transport and asks the user to close playback and reopen it after selecting a default device. Automatic rebinding is not attempted. |
| Render graph | While playing, schedule into preallocated MIDI buffers, consume timeline-stamped SPSC PCM, mix fixed topology, process prepared instruments/effects, run the in-place Master guard, and return value counters. On item underrun, publish the block end through a monotonic atomic resume position; discard stale queued frames with a fixed per-block cap. While stopped with an enabled input-monitor route, skip transport/MIDI/PCM advancement and run only the preallocated live-input track/effect/Master path. Monitor producer/consumer clones share queue cursors so graph swaps retain the active input connection. | Media decode/refill, project compilation, processor loading/activation/deactivation, filesystem access, and UI diagnostics are outside the callback. |
| CLAP call | Host buffers and event storage are prepared before playback; host-side error values on processing paths borrow static messages. | Third-party plugin code executes inside the callback and must itself obey the CLAP realtime contract. In-process plugins can still allocate, block, or stall the host; the current host cannot enforce isolation. |

Applying `ReplaceGraph` and `Shutdown` commands also runs in the JACK, PipeWire, and WASAPI callbacks.
These handlers send all-notes-off and stop active CLAP processors before retiring a graph; this can
call plugin code, including another process call to flush MIDI state. The CLAP host transition is
required on the audio thread, so it is an explicit exception to host-only bounded work. Plugins
must keep this transition realtime-safe too. A future asynchronous transition protocol would need
to preserve note release and safe processor ownership before moving this work off the callback.

The engine integration test `realtime_allocation` installs a thread-local allocation counter and
checks 128 warmed render calls with an active AudioItem PCM stream, scheduled MIDI, and Master
clamping. Graph creation, queue filling, and test assertions are not counted. This deterministic
check covers allocations made on the rendering thread; it does not measure callback duration,
locks, I/O, other threads, or allocations performed inside an external plugin. Backend feature
checks compile JACK and PipeWire paths in CI, while physical-device XRun and long-session profiling
still require a supported audio device.

## Realtime error reporting

CLAP process failures are reported through static host messages because formatting third-party
error values on the callback can allocate. The current callback discards per-error text and
increments a counter, so dynamic plugin details are not currently available in playback UI.
Detailed dynamic error formatting remains appropriate for control-thread setup, discovery, state
persistence, and teardown failures.
