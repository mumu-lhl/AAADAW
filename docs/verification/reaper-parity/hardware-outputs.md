# Audio Hardware Outputs

Reference: Linux REAPER 7.82 / x86_64 / Default 7. The native probe runs in an isolated profile and disposable project. It verifies the ReaScript routing category, route defaults, editable channel selectors, Send Mode values, and saved project state. It does not measure live device samples because the Xvfb environment has no physical or virtual audio output device.

## Native baseline

`CreateTrackSend(source, nil)` creates a Hardware Output and returns index `0`. `GetTrackNumSends(source, 2)` reports one Hardware Output; category `0` (ordinary Sends) remains zero. In the same project, category `1` reports one Receive for the Master path. This confirms that the Hardware Output belongs to its own route collection.

The created output defaults to `I_SRCCHAN=0`, `I_DSTCHAN=0` (stereo channels 1/2), `D_VOL=1`, `D_PAN=0`, `B_MUTE=0`, `B_PHASE=0`, `I_SENDMODE=0`, and `I_MIDIFLAGS=0`. The API accepts and reads back source channel values `0`, `1024`, `1025`, destination values `0`, `1024`, `1025`, and Send Mode values `0` through `3`. The meaning of each Send Mode is not inferred from the numeric readback; its tap point still needs an audio probe.

The native TSV, final RPP and a mono-channel-map RPP are reproducible with [`capture-hardware-output-native.sh`](../../scripts/reaper_parity/capture-hardware-output-native.sh). Their binary and source hashes are recorded in the [native evidence manifest](fixtures/hardware-output-native-evidence.tsv).

## AAADAW work still open

AAADAW has no Hardware Output route model or device-output sink. Its live CPAL callback currently negotiates a stereo stream and writes the Master mix. The first implementation target is a track-owned Hardware Output to the current device's channels 1/2, with independent source channel selection, gain, pan, mute, phase and send mode; it must bypass the track's Main Send switch, save/reopen, undo/redo, and reach the real-time device callback without allocation. Mono channel mapping must use the native 1/2 values above. Device outputs above channel 2, choosing a different physical device per route, and Wiring remain outside this first slice.

Before marking that slice complete, measure whether REAPER Hardware Outputs bypass Master FX/gain and how each Send Mode taps the source signal. Also verify main-send interaction, mute/solo behavior, and whether direct hardware output participates in offline render. The current probe establishes API defaults and saved state only; it does not claim audio-output parity or complete P3 routing support.
