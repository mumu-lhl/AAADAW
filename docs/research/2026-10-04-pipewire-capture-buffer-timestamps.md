# PipeWire capture buffer timestamps

## Finding

`Stream::time().ticks` is not a timestamp for the buffer just dequeued by an input process callback. In pipewire-rs 0.10.1 it wraps `pw_stream_get_time()` (or `_n` with `v0_3_50`), and the docs say stream time is updated each graph cycle, usually from the process callback. `ticks` is a monotonically increasing stream position, but the API does not associate that position with a particular dequeued `pw_buffer`. `now` is the monotonic time of that report; interpolation with `pw_stream_get_nsec()` estimates current stream position, not the buffer's first frame. Queueing, graph latency, and buffers from another cycle prevent treating a callback-time snapshot as the exact buffer start.

For per-buffer timing, negotiate SPA header metadata and inspect the metadata on each dequeued buffer. The metadata parameter is `SPA_PARAM_Meta` (`SPA_TYPE_OBJECT_ParamMeta`) with `SPA_PARAM_META_type = SPA_META_Header` and `SPA_PARAM_META_size = sizeof(struct spa_meta_header)`. `SPA_META_Header` is the metadata type; it is not a stream flag. In Rust, the corresponding accessor is `Buffer::find_meta::<spa::buffer::meta::MetaHeader>()`.

`MetaHeader` exposes:

- `pts: i64`: presentation timestamp in **nanoseconds**. Use successive valid PTS values and the negotiated sample rate to calculate frame deltas; do not treat `seq` as a frame counter.
- `seq: u64`: sequence number incremented at a media-specific frequency. It can help detect ordering/discontinuity but does not specify how many audio frames a buffer contains.
- `offset: u32`: offset in the current cycle, not an independently defined absolute stream-frame index.
- `flags`: notably `DISCONT` means this data is not continuous with the previous buffer; `GAP` marks media-neutral data; `CORRUPTED` warns data may be corrupted. The flags describe data state and do not supply a missing timestamp.

Header metadata is conditional, not guaranteed for every stream/server. A client can request it through the SPA metadata parameter, but the peer/driver must support and provide it. The 0.10.1 Rust API returns `Option` from `find_meta`; absence is explicitly representable. The current AAADAW stream passes only an audio format parameter to `connect`, so it does not request header metadata. Its callback can inspect metadata on the dequeued buffer without leaving `RT_PROCESS`, but it cannot assume the header exists.

Therefore, a deterministic gap-preserving PipeWire path is **not available from the current implementation or from `stream.time().ticks` alone**. It is implementable for connections that negotiate and actually provide valid `MetaHeader` PTS values: carry each buffer's PTS and frame count through the bounded queue, map PTS deltas to project frames, and fill forward gaps. If metadata is rejected, absent on a buffer, invalid, or marked discontinuous without a recoverable timestamp, the safe behavior is to invalidate/stop that take with an explicit timing-unavailable error. Falling back to callback order or `stream.time()` silently reintroduces possible time compression. Supporting PipeWire nodes that do not provide the header would require a separate source-specific timing contract; the public API does not promise an equivalent per-buffer position.

## Primary sources

- PipeWire `pw_time`: definitions and relationship among `now`, `rate`, `ticks`, and `delay`: <https://docs.pipewire.org/structpw__time.html>
- SPA `spa_meta_header`: fields, timestamp units, and metadata flags: <https://docs.pipewire.org/structspa__meta__header.html>
- SPA `spa_param_meta`: metadata type and size parameter fields: <https://docs.pipewire.org/structspa__param__meta.html>
- PipeWire stream connect and buffer processing APIs: <https://docs.pipewire.org/group__pw__stream.html>
- Official PipeWire audio capture example (process callback and dequeue): <https://docs.pipewire.org/audio-capture_8c-example.html>
- Version-pinned pipewire-rs 0.10.1 `Stream::time()` and `Stream::connect()` implementation/docs: <https://docs.rs/pipewire/0.10.1/src/pipewire/stream/mod.rs.html>
- Version-pinned libspa 0.10.1 `MetaHeader` fields, flag meanings, and metadata type: <https://docs.rs/libspa/0.10.1/src/libspa/buffer/meta.rs.html>
- Version-pinned pipewire-rs 0.10.1 buffer `find_meta()` implementation: <https://docs.rs/pipewire/0.10.1/src/pipewire/buffer.rs.html>

The repository resolves `pipewire` and `libspa` to 0.10.1 in `Cargo.lock`. In that source, `Stream::time()` is marked RT-safe and describes the per-cycle report; `Buffer::find_meta()` searches the current buffer's metadata and returns `None` if it is absent; `MetaHeader::pts()` documents nanoseconds and `seq()` documents media-specific increments. The stream's `connect()` API accepts arbitrary SPA pods, while the Rust wrapper does not provide a high-level ParamMeta builder.
