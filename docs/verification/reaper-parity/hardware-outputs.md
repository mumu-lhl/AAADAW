# Audio Hardware Outputs

Reference: Linux REAPER 7.82 / x86_64 / Default 7. The native probe runs in an isolated profile and disposable project. It verifies the ReaScript routing category, route defaults, editable channel selectors, Send Mode values, and saved project state. It does not measure live device samples because the Xvfb environment has no physical or virtual audio output device.

## Native baseline

`CreateTrackSend(source, nil)` creates a Hardware Output and returns index `0`. `GetTrackNumSends(source, 2)` reports one Hardware Output; category `0` (ordinary Sends) remains zero. In the same project, category `1` reports one Receive for the Master path. This confirms that the Hardware Output belongs to its own route collection.

The created output defaults to `I_SRCCHAN=0`, `I_DSTCHAN=0` (stereo channels 1/2), `D_VOL=1`, `D_PAN=0`, `B_MUTE=0`, `B_PHASE=0`, `I_SENDMODE=0`, and `I_MIDIFLAGS=0`. The API accepts and reads back source channel values `0`, `1024`, `1025`, destination values `0`, `1024`, `1025`, and Send Mode values `0` through `3`. REAPER's [ReaScript API](https://www.reaper.fm/sdk/reascript/reascripthelp.html) documents modes 0=post-fader, 1=pre-FX, 2=deprecated post-FX, and 3=post-FX; mode 8 (pre-receive) is unsupported for hardware outputs. This documents the control values, but the hardware-output signal behavior still needs a live audio probe.

The native TSV, final RPP and a mono-channel-map RPP are reproducible with [`capture-hardware-output-native.sh`](../../scripts/reaper_parity/capture-hardware-output-native.sh). Their binary and source hashes are recorded in the [native evidence manifest](fixtures/hardware-output-native-evidence.tsv).

## AAADAW implementation status

AAADAW now has a track-owned Hardware Output route collection that is independent of ordinary audio Sends and carries no MIDI route state. The Routing window can add/remove an output and edit its REAPER Send Mode, source/device channel pair, gain, pan, mute and phase. The current menu exposes modes 0, 1 and 3; compatibility mode 2 is preserved in project state and displayed as Post-FX without offering the deprecated value as a new choice. Modes 2 and 3 use the same post-FX buffer while retaining distinct serialized values. Routes are part of undo/redo and project snapshots; schema 74 persists them in `track_hardware_outputs`.

The real-time render graph accumulates direct outputs into preallocated stereo scratch and includes them in CPAL, JACK and PipeWire device callbacks. These routes continue when the track Main Send is disabled. They bypass the Master mix stage and enter the final output safety guard. Offline `render_into` deliberately excludes hardware outputs. Mono source and destination mapping uses the same channel-pair rules as normal sends. The callback path does not allocate for this feature.

The Routing window was opened in the built Linux desktop binary, a track Hardware Output was created, and the visible controls were checked in Xvfb. This screenshot verifies UI rendering and the available menu choices only; it is not evidence of live audio parity.

![AAADAW Hardware Output route controls](images/hardware-output-routing-aaadaw.png)

Screenshot SHA-256: `78008fd91e27e3b31305a9d48fc45159a5b35e84a76651ec43ca885e05f90c7d`.

The initial AAADAW Wiring canvas now displays Hardware Output paths; see [Track Wiring](routing-wiring.md). This is a first usable implementation slice, not verified REAPER parity. The native probe still has not measured live samples, so hardware Send Mode behavior, Master FX/gain behavior, Main Send interaction, mute/solo behavior and offline-render behavior remain subject to reference comparison. The native probe confirms route category, defaults and state serialization only. Device outputs above channel 2, choosing a different physical device per route, and native per-channel Wiring behavior remain open. Keep `ROUTE-HW-OUTPUT-001` in progress until those behavioral comparisons pass.
