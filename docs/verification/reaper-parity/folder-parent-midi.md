# Folder Parent MIDI forwarding

Linux REAPER 7.82 / Default 7 was run with an isolated profile and an empty project. The project contains nested `Folder Parent > Nested Folder > MIDI Child` tracks and one C4 MIDI note. A test JSFX on the top folder counts incoming Note On and Note Off messages through REAPER's shared memory; the ReaScript triggers an offline render for each routing state.

| Native state | Parent Note On | Parent Note Off |
| --- | ---: | ---: |
| Both Parent sends enabled | 1 | 1 |
| MIDI Child Main Send disabled | 0 | 0 |
| Nested Folder Main Send disabled | 0 | 0 |

This confirms that REAPER forwards MIDI through each enabled folder Parent edge and that either Main Send switch blocks events from reaching the outer folder. The saved projects also show `ISBUS 1 1` for each folder, `ISBUS 2 -1` for the child, and the expected `MAINSEND 0/1` state. The event counters are read from the JSFX during native offline rendering; the silent render files are only used to drive the FX processing and are not treated as the measurement.

The [ReaScript probe](../../../scripts/reaper_parity/probe-folder-parent-midi.lua) and [counter JSFX](../../../scripts/reaper_parity/jsfx/aaadaw-folder-parent-midi-probe.jsfx) run against a disposable profile. Install the JSFX in that profile's `Effects` directory before launching REAPER so it is indexed at startup, create a writable empty probe-output directory, set `AAADAW_REAPER_PROBE_DIR` to it, then open the script with `-newinst -nosplash -new`. The probe removes only its own same-named WAV render outputs before each pass. The [native TSV](fixtures/folder-parent-midi-native.tsv) records the counts. Saved REAPER projects capture the all-enabled state, leaf-send-disabled state, and nested-folder-send-disabled state:

- [All Parent sends enabled](fixtures/folder-parent-midi-enabled-native.rpp), SHA-256 `3e5782de5e2c6378a3f75684a60ae17bbd5956f01290ec67f88c5745dcd82f1d`
- [MIDI Child Main Send disabled](fixtures/folder-parent-midi-leaf-disabled-native.rpp), SHA-256 `8c0fbc56e7d4e5d6c1223426b3a1ae13c1818b343c48118277ef047970c2ac49`
- [Nested Folder Main Send disabled](fixtures/folder-parent-midi-inner-disabled-native.rpp), SHA-256 `d011b390735efa11b6b2e2e8f5886f4a5b49b080b0c0bfc6c5cd795a0d4280b1`

Probe SHA-256 values: ReaScript `72a2d1bcedf4dfec74263b3ba1ba030c67ca8e18142dd0d22f1ac01786526b40`; JSFX `9034c241d04e1dec81bb526c4e9140f8626ff5e507fdd303db3b57838e599e6e`; TSV `e08ae653bcdb47aeca52d9a274a4f95809cfb98ac16a545a1048924d096c0425`.

AAADAW mirrors this nested routing and each Main Send boundary in `crates/aaadaw-engine/tests/midi_schedule.rs::folder_parent_midi_forwarding_obeys_each_main_send_switch`; the plan expansion remains outside the realtime callback. This fixture verifies MIDI routing behavior, not the routing window's visual parity.
