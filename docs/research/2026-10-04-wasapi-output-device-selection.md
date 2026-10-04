# Persisted Windows output-device selection through CPAL 0.18.2

**Question.** Can aaadaw persist a non-default WASAPI output device without owning WASAPI/COM enumeration itself? Sources checked 2026-10-04; this note uses CPAL 0.18.2 source/API documentation and Microsoft’s Windows audio API documentation.

## Finding

Yes. CPAL 0.18.2 exposes `DeviceTrait::id()` and `HostTrait::device_by_id()`. On WASAPI, CPAL implements that ID with `IMMDevice::GetId()` and tags it with the `wasapi` host. `DeviceId` is serializable through its documented `Display`/`FromStr` representation, and CPAL specifically documents persisting it, then resolving it with `device_by_id()` on a later run. Its generic `device_by_id()` implementation re-enumerates devices and matches IDs; WASAPI’s enumerator currently includes active endpoints. This supports selecting a persisted, non-default output endpoint robustly across ordinary restarts and duplicate friendly names.

Persist `DeviceId::to_string()`, parse it at startup, then resolve it on a non-realtime control path. Confirm `supports_output()` before opening it. If parsing or lookup fails, report the saved device as unavailable and ask the user to choose another endpoint or explicitly choose “System Default”; do not silently switch a persisted explicit choice to the default.

## Limits and boundary

- **Names are labels, not keys.** WASAPI friendly names come from endpoint properties and need not be unique. Use the endpoint ID for identity; if labels collide in the picker, show additional description fields and/or a short ID suffix.
- **Persistence has a platform caveat.** CPAL says IDs should remain stable across runs, disconnects, and reboots “where possible,” not as an unconditional cross-driver/reinstall guarantee. Microsoft defines `IMMDevice::GetId` as the endpoint identifier used to reacquire that endpoint. Treat a missing ID as normal device loss and allow re-selection; do not infer identity from a matching name.
- **Hotplug is not a live-rebind feature.** CPAL’s WASAPI device list enumerates active endpoints at lookup time. CPAL does not expose a general add/remove notification API through `HostTrait`, and its default-endpoint monitor is distinct from a specific-device stream. Re-enumerate/reopen from a control thread after stream invalidation or a user retry. A selected endpoint that is unplugged may not resolve until it is active again; the app should show that state. Device notification behavior for specific-device streams needs Windows hardware/virtual-endpoint validation.
- **Keep the abstraction at CPAL for now.** Pass a `DeviceId` (or its persisted string) to the engine’s output-opening boundary and resolve through the selected CPAL host there. This keeps device identity and stream construction together while leaving COM ownership out of UI/settings code. Direct WASAPI is justified only if aaadaw needs first-class endpoint-added/removed/state notifications, role-specific Windows policy, or guarantees CPAL cannot provide.

## Primary sources

### CPAL 0.18.2

- [`DeviceId` persistence and round-trip example](https://github.com/RustAudio/cpal/blob/v0.18.2/src/lib.rs#L238-L327) — demonstrates `Display`, `FromStr`, and `host.device_by_id`; API docs say application code should persist IDs in this form.
- [`HostTrait::device_by_id`](https://github.com/RustAudio/cpal/blob/v0.18.2/src/traits.rs#L62-L71) — re-enumerates and matches the device ID; returns `None` if no match is found.
- [`DeviceTrait::id`](https://github.com/RustAudio/cpal/blob/v0.18.2/src/traits.rs#L155-L165) — unique on its host and intended to remain stable across runs/disconnections/reboots where possible; also exposes `supports_output`.
- [WASAPI ID implementation](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/device.rs#L500-L515) — uses `IMMDevice::GetId()` to construct a WASAPI `DeviceId`.
- [WASAPI endpoint enumeration](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/device.rs#L1252-L1277) — creates a collection using `EnumAudioEndpoints(eAll, DEVICE_STATE_ACTIVE)`.
- [WASAPI default stream endpoint monitor](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/stream.rs#L27-L45) — CPAL’s monitor is attached to default-device stream handling, not a general host-level device-list notification API.

### Microsoft Windows audio APIs

- [`IMMDevice::GetId`](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdevice-getid) — obtains an endpoint ID string.
- [`IMMDeviceEnumerator::GetDevice`](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-getdevice) — retrieves an endpoint by its ID string.
- [`IMMDeviceEnumerator::EnumAudioEndpoints`](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-enumaudioendpoints) — enumerates endpoints filtered by data flow and state.
- [`IMMNotificationClient`](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immnotificationclient) — Windows callbacks for endpoint add/remove/state and default-device changes; relevant if aaadaw later requires direct notifications.
