# Realtime rendering checks

## Reproducible local benchmark

Run `cargo bench -p aaadaw-engine --bench render_callback`. The harness warms each graph,
measures 256 render blocks, repeats each scenario seven times, and reports the median callback time
and estimated realtime multiple at 48 kHz. Project construction, queue filling, graph compilation,
and output allocation happen outside the timed section.

Scenarios cover 8 tracks at 128 frames, and 64 tracks at both 128 and 512 frames. Each track has
an active PCM stream. The current harness uses same-rate mono test samples, the built-in mixer,
and the Master sample-peak guard; it does not include CLAP plugin execution, media decoding,
device scheduling, or GUI load. Results are a local comparison aid, not a hardware-independent
performance guarantee. Do not add CI pass/fail thresholds based on wall-clock duration.

## Callback audit

Audited the render path through `JackProcessHandler::process`, the PipeWire stream `process`
listener and `ProcessData::process_bytes`, `AudioRenderGraph::render_into`, the PCM queue consumer,
track mixer, final Master guard, callback counters, and the CLAP processor calls.

| Realtime path | Prepared or bounded work | Work kept off the callback |
| --- | --- | --- |
| JACK process handler | Read the bounded command queue, move retired graphs into a bounded SPSC queue, render into preallocated scratch, copy output, update atomics. | Graph construction and command production are control-thread work. Retired graph destruction and processor deactivation are control-thread work. |
| PipeWire process listener | Dequeue a server buffer, validate its fixed output slice, render into preallocated scratch, encode stereo `f32`, set chunk metadata, update atomics. | Stream setup/negotiation and graph construction happen before playback. Callback-owned graphs are collected and destroyed by the control thread. |
| Render graph | Schedule into preallocated MIDI buffers, consume SPSC PCM, mix fixed topology, process prepared effects/instruments, run the in-place Master guard, and return value counters. | Media decode/refill, project compilation, processor loading/activation/deactivation, filesystem access, and UI diagnostics are outside the callback. |
| CLAP call | Host buffers and event storage are prepared before playback; host-side error values on processing paths borrow static messages. | Third-party plugin code executes inside the callback and must itself obey the CLAP realtime contract. In-process plugins can still allocate, block, or stall the host; the current host cannot enforce isolation. |

The engine integration test `realtime_allocation` installs a thread-local allocation counter and
checks 128 warmed `render_into` calls. Graph creation, queue filling, and test assertions are not
counted. This deterministic check covers allocations made on the rendering thread; it does not
measure callback duration, locks, I/O, other threads, or allocations performed inside an external
plugin. Backend feature checks compile JACK and PipeWire paths in CI, while physical-device XRun
and long-session profiling still require a supported audio device.

## Realtime error reporting

CLAP process failures are reported through static host messages because formatting third-party
error values on the callback can allocate. Detailed dynamic error formatting remains appropriate
for control-thread setup, discovery, state persistence, and teardown failures. The UI currently
surfaces callback error counts rather than formatting errors on the realtime thread.
