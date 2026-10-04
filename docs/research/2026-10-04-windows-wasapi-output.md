# Windows shared-mode output: CPAL or direct WASAPI

**Scope.** Recommend a low-risk Rust path for Issue #50: render the existing engine graph to the Windows default playback endpoint in shared mode, recover when that endpoint changes or disappears, and keep the audio callback suitable for realtime work. Sources checked on 2026-10-04; only project code and first-party crate/Microsoft sources were used.

## Recommendation

Use CPAL 0.18.2's Windows WASAPI host for the first output backend. It already provides default-device selection, shared-mode format negotiation/conversion, event-driven output, a high-priority callback thread, and default endpoint change notification. It does **not** transparently rebind a running stream after a default-device change: its error callback reports `StreamInvalidated` (or `DeviceNotAvailable` if no default remains). Treat this as a request to stop/retire the old stream and reopen the current default device on the engine's non-realtime control thread. This is substantially less custom WASAPI/COM code than a direct implementation and gives the app a clear recovery boundary.

Choose the `cpal` default output device and obtain its default configuration, then request the project rate only if it is among the device's reported supported ranges. The CPAL WASAPI backend uses event-callback shared mode and `AUTOCONVERTPCM` with default-quality SRC for output; its WASAPI format probing includes the rates its Media Foundation resampler supports (8–384 kHz). Render the graph into a preallocated `f32` scratch buffer, then copy/convert to the callback's sample type without allocation, locks, logging, or blocking. Do not rely on a fixed callback frame count; use the provided buffer length.

This recommendation assumes OS shared-mode conversion is acceptable for ordinary playback. If aaadaw later needs exact endpoint format control, per-device policy, or richer notification/state handling than “stream invalidated, reopen,” direct WASAPI is the right lower-level seam to evaluate.

## Trade-offs

| Concern | CPAL WASAPI backend | `wasapi` crate directly |
|---|---|---|
| Default render endpoint | `HostTrait::default_output_device()`; CPAL marks default endpoints and watches the default flow. | `DeviceEnumerator::get_default_device(Direction::Render)`; role-specific APIs are also available. |
| Removal/default changes | The default stream installs `IMMNotificationClient` monitoring. A changed default signals the stream loop; CPAL does not rebind the `IAudioClient`, so report an error and reopen externally. Specific-device streams do not get the default-device monitor. | The crate exposes device-added/removed/state/default-device callbacks. The application must decide how to coordinate notifications with stream teardown and replacement. |
| Format/rate | Query supported ranges and select the project rate where supported. WASAPI output initialization requests shared mode with `AUTOCONVERTPCM` and `SRC_DEFAULT_QUALITY`; backend code probes common rates within the MF resampler range. | Explicit `WaveFormat`, `is_supported`, and `StreamMode::EventsShared { autoconvert, .. }` give tighter control and make policy the application's responsibility. The crate's example checks the desired format before initialization. |
| Realtime behavior | CPAL documents a dedicated high-priority callback thread on modern backends and explicitly prohibits allocation, locks, blocking, I/O, and calls back into CPAL in the data callback. Callback-size requests are approximate; consume the actual slice length. | It exposes WASAPI event-driven buffer operations rather than CPAL's data-callback contract. The crate example waits for events and writes buffers itself; its example allocates a `Vec` per write, so it is demonstration code, not a realtime-safe callback template. aaadaw would own thread priority, preallocation, buffer access, shutdown, and callback/control separation. |
| Rust 1.85 | CPAL 0.18.2 declares `rust-version = "1.85"`, matching the workspace MSRV. | `wasapi` 0.25.0 declares Rust 1.85 for the library. Its README notes `cargo test`/the `record_application` example need Rust 1.88 because of a dev/example dependency; that does not affect using the library as a dependency. |

## Implementation outline

1. Add CPAL behind `cfg(target_os = "windows")`; keep the existing Linux backends unchanged.
2. Open the default output on a control thread. Choose a supported config at the project sample rate and an output format the backend can fill; fail with a useful configuration error if no range accepts the rate.
3. Keep render graph ownership and all scratch/conversion storage prepared before `play()`. The callback only renders the current block and copies/converts samples into CPAL's supplied buffer.
4. On stream error, publish a bounded control event and silence/retire the stream. Re-enumerate/reopen outside the callback; do not call CPAL setup or destroy the render graph from the realtime callback.
5. Test the error/reopen state machine without audio hardware, and add Windows device tests/manual checks for unplug, default-device switch, 44.1/48 kHz, and supported sample types. Real endpoint behavior still needs a Windows machine or virtual endpoint.

## Sources

### CPAL (maintainer repository and crate documentation)

- [CPAL 0.18.2 manifest](https://github.com/RustAudio/cpal/blob/v0.18.2/Cargo.toml) — version, edition, and declared Rust 1.85 MSRV.
- [CPAL callback contract](https://github.com/RustAudio/cpal/blob/v0.18.2/src/lib.rs#L86-L109) — dedicated high-priority thread on modern backends and realtime restrictions.
- [WASAPI default stream error semantics](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/stream.rs#L730-L779) — CPAL explicitly notes it does not rebind `IAudioClient`; the notification results in `StreamInvalidated` or `DeviceNotAvailable`.
- [WASAPI endpoint monitor](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/stream.rs#L27-L45) and [default-device stream setup](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/device.rs#L137-L157) — `IMMNotificationClient` monitor is attached for default-device streams.
- [WASAPI format probing](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/device.rs#L188-L206) and [shared-mode output conversion](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/device.rs#L609-L665) — `IsFormatSupported`, output autoconversion, and supported-rate discovery.
- [WASAPI stream initialization](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/device.rs#L970-L982) — event callback, default-quality SRC, and `AUTOCONVERTPCM` flags.
- [CPAL buffer-size contract](https://github.com/RustAudio/cpal/blob/v0.18.2/src/lib.rs#L363-L388) — requested buffer sizes are approximate; use the callback's delivered buffer length.
- [CPAL `DeviceTrait` docs](https://docs.rs/cpal/0.18.2/cpal/traits/trait.DeviceTrait.html) — default and supported output configuration API.

### `wasapi` crate (maintainer repository and crate documentation)

- [`wasapi` 0.25.0 manifest](https://github.com/HEnquist/wasapi-rs/blob/0.25.0/Cargo.toml) and [README](https://github.com/HEnquist/wasapi-rs/blob/master/README.md) — shared/exclusive modes, notification support, and 1.85 library MSRV / 1.88 example-test caveat.
- [Shared-mode default-output example](https://github.com/HEnquist/wasapi-rs/blob/master/examples/playsine.rs#L50-L123) — explicit default render selection, format probe, shared event mode, and render client setup.
- [Event wait and writes in the example](https://github.com/HEnquist/wasapi-rs/blob/master/examples/playsine.rs#L125-L158) — buffer filling and event-driven loop (example uses per-write allocation, so do not copy that allocation into a realtime callback).
- [Device notification example](https://github.com/HEnquist/wasapi-rs/blob/master/examples/device_notifications.rs) — added, removed, state, and default endpoint notifications; callback registration lifetime.
- [`AudioClient` API docs](https://docs.rs/wasapi/0.25.0/wasapi/struct.AudioClient.html) — direct API for mix format, support probing, initialization, and render client access.

### Microsoft WASAPI documentation

- [Shared-mode streams](https://learn.microsoft.com/en-us/windows/win32/coreaudio/shared-mode) — shared clients coexist through the audio engine.
- [`IAudioClient::GetMixFormat`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-getmixformat) — obtains the audio engine's shared-mode mix format.
- [`IAudioClient::IsFormatSupported`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-isformatsupported) — shared-mode format negotiation and closest-match behavior.
- [`IAudioClient::Initialize`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize) and [stream flags](https://learn.microsoft.com/en-us/windows/win32/coreaudio/audclnt-streamflags-xxx-constants) — shared-mode initialization, event callback, and PCM conversion flags.
- [`IMMNotificationClient`](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immnotificationclient) — endpoint add/remove/state/default-device notifications.
- [Rendering a stream](https://learn.microsoft.com/en-us/windows/win32/coreaudio/rendering-a-stream) — obtain/release render buffers according to the event-driven stream lifecycle.
