# WASAPI capture semantics through CPAL 0.18.2

**Scope.** Primary-source notes for Issue #52: input callback timestamps, Windows shared-mode capture, endpoint errors, and what CPAL can promise about stereo PCM input. Sources checked 2026-10-04. No capture hardware was available for empirical validation.

## CPAL callback timestamps

`InputCallbackInfo::timestamp()` returns an `InputStreamTimestamp` with two distinct `StreamInstant`s:

- `capture`: the time the input data was captured; CPAL's WASAPI backend defines this as the instant of the first sample in the packet, using `qpcPosition` returned by `IAudioCaptureClient::GetBuffer`.
- `callback`: the time associated with invocation of the user callback. In the WASAPI backend, CPAL obtains this from `IAudioClock::GetPosition` and its QPC position.

These are monotonic stream instants, not wall-clock timestamps. The crate's cross-platform contract says instants share a clock within one stream but their origins are **not guaranteed to be shared across streams**. Do not use CPAL's generic API contract to claim that capture and callback instants from separate streams are comparable. `callback - capture` is meaningful for a single callback and stream, but represents reported packet age at callback time, not a hardware-independent measurement of ADC-to-application latency.

CPAL calls the data callback while holding the WASAPI capture packet and releases that packet after the callback returns. The callback should copy/process into preallocated storage and return promptly; it must not retain the borrowed sample slice.

## Shared-mode format and stereo behavior

CPAL's WASAPI input stream initializes `IAudioClient` in shared mode with event-callback buffering. Its input-format discovery starts with `GetMixFormat`, preserves that format's channel count, tests candidate sample formats/sample rates using `IsFormatSupported`, and only reports formats that are natively supported. The backend explicitly enables `AUTOCONVERTPCM` for **output**, not capture.

Therefore CPAL does not promise a stereo capture config on every Windows endpoint. The number of channels it discovers is the endpoint mix format's channel count; CPAL's source comments that probing shared-mode formats with a different channel count fails on the tested API behavior, so it assumes that channel count is the supported count. Stereo input is available only when the endpoint reports a two-channel input configuration. If the device reports mono or a multichannel mix format, an application must not assume WASAPI/CPAL will downmix it to stereo; explicitly select a supported config or do a separately designed channel mapping step. Sample format and sample rate are also device/config-specific; capture probes exact formats and does not use the output SRC path.

The backend recognizes common PCM widths and IEEE float formats when mapping WASAPI's mix format, but only configurations accepted by `IsFormatSupported` are exposed for capture. The names “PCM input” and “stereo input” alone do not establish that a requested rate, bit depth, or channel count is accepted by a given driver.

## Device and capture errors

CPAL's default input device is monitored for default-endpoint changes through `IMMNotificationClient`. A default change does not rebind the running `IAudioClient`: if a replacement default exists, CPAL reports `StreamInvalidated`; if no default exists, it reports `DeviceNotAvailable`. A device invalidation HRESULT from WASAPI maps to `DeviceNotAvailable`; resource invalidation maps to `StreamInvalidated`. The app must handle the stream error outside the realtime data callback, stop/retire the old stream, re-enumerate/reopen on a control thread, and report unavailable input if reopening fails.

For capture packet discontinuities, CPAL checks WASAPI's `AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY` flag and emits `ErrorKind::Xrun`; it ignores that flag on the first post-start packet because WASAPI leaves the device position undefined there. This is evidence of a discontinuity report, not a guarantee that every hardware/driver overrun is detectable or that capture is lossless when no xrun is reported.

## Concrete implementation implications

1. Enumerate the default input or a saved device ID and inspect `supported_input_configs()`; require two channels only when stereo capture is a product requirement, and fail clearly if it is unavailable.
2. Select a supported input sample format/rate rather than assuming f32, 44.1 kHz, or stereo. Keep conversion/channel mapping explicit in the engine boundary.
3. Treat `InputStreamTimestamp::capture` as the packet's first-sample timestamp in CPAL's stream clock and `callback` as callback time. Preserve both when passing captured blocks to the recording timeline; do not substitute callback arrival time for capture time without documenting the changed latency semantics.
4. Keep the callback bounded and allocation-free; copy packet data only into preallocated buffers/queues. Route xrun and stream/device errors to control state with bounded communication.
5. Unit-test timestamp-to-frame math and stream error/reopen state transitions without hardware. Validate stereo, sample type/rate, unplug, default-device switch, and actual timestamp/latency behavior on Windows devices or a suitable virtual endpoint before claiming hardware support.

## Sources

### CPAL 0.18.2 (maintainer source and API docs)

- [`StreamInstant`, `InputStreamTimestamp`, and `InputCallbackInfo`](https://github.com/RustAudio/cpal/blob/v0.18.2/src/timestamp.rs#L3-L69) — monotonic clock caveat, callback/capture meanings, and host clocks (WASAPI uses QPC).
- [`InputCallbackInfo::timestamp`](https://github.com/RustAudio/cpal/blob/v0.18.2/src/timestamp.rs#L247-L255) — public accessor.
- [WASAPI packet capture and discontinuity handling](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/stream.rs#L797-L860) — calls `GetBuffer`, checks discontinuity, timestamps callback data, calls the user callback, then releases the packet.
- [WASAPI input timestamp construction](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/stream.rs#L917-L955) — CPAL's `qpcPosition` conversion and first-sample packet interpretation.
- [WASAPI stream invalidation errors](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/stream.rs#L730-L779) and [HRESULT mapping](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/mod.rs#L58-L81) — default-device change and WASAPI error categories.
- [Default input stream construction](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/device.rs#L117-L135) — notification monitor is attached to default input streams.
- [WASAPI input format discovery](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/device.rs#L609-L730) — mix-format channel count, format/rate probing, and no capture autoconversion.
- [Event-callback flag](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/device.rs#L55-L59) and [shared-mode capture initialization](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/device.rs#L830-L904) — event-driven shared mode and capture client setup.
- [CPAL callback realtime contract](https://github.com/RustAudio/cpal/blob/v0.18.2/src/lib.rs#L86-L109) — callback thread and restrictions.
- [`DeviceTrait` input configuration docs](https://docs.rs/cpal/0.18.2/cpal/trait.DeviceTrait.html) — supported input config and input stream creation APIs.

### Microsoft WASAPI documentation

- [`IAudioCaptureClient::GetBuffer`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer) — packet frames, device/QPC positions, and buffer flags.
- [Capture stream lifecycle](https://learn.microsoft.com/en-us/windows/win32/coreaudio/capturing-a-stream) — acquire/release capture packets and shared-mode capture flow.
- [`IAudioClient::GetMixFormat`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-getmixformat) — shared audio engine mix format.
- [`IAudioClient::IsFormatSupported`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-isformatsupported) and [`IAudioClient::Initialize`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize) — shared-mode format acceptance and initialization constraints.
- [WASAPI error codes](https://learn.microsoft.com/en-us/windows/win32/coreaudio/audclnt-xxx-error-codes) — device/resource invalidation HRESULT definitions.
- [`IMMNotificationClient`](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immnotificationclient) — endpoint add/remove/state/default-device notification interface.
- [`AUDCLNT_BUFFERFLAGS`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/ne-audioclient-_audclnt_bufferflags) — discontinuity and silent-packet flags.
