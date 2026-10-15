# MIDI Item Properties: Loop source

## REAPER 7.82 reference

In Linux REAPER `7.82/linux-x86_64` with the Default 7 theme, a newly created, selected two-second MIDI item opens the actual `Media Item Properties` window with F2. The native window tree reports 526×693, and its `Loop source` checkbox is checked by default. The 1280×800 root capture is [here](fixtures/midi-item-properties-loop-source-native.png); the [fixture TSV](fixtures/midi-item-properties-loop-source.tsv) records the build-independent REAPER version, theme, window size, and image hash.

## AAADAW behavior

AAADAW now shows `Loop source` in the MIDI Item Properties form and initializes it from the selected item. Turning it off applies `SetMidiItemLoopSource` through the existing project event/undo path. The change updates only the loop flag: it preserves the visible item bounds and stored source-loop length. Undo and Redo restore the flag while keeping a 960-tick source loop on a 1920-tick visible item. The App regression exercises that shorter source case. The Engine regression confirms that disabling the flag stops repeated MIDI events while preserving the source length.

The Apply path rejects this property change while playback is active and marks it for playback graph refresh when the audio-device backend is enabled. The edit does not add per-callback work or change the MIDI event data.

The rebuilt desktop binary (Build ID `731072fd5d787f4b86a53b0e680f01a7f5d59d69`, SHA-256 `a3b9012b630dd25abeba7f1fda6b3e6dd92c90f3ac7db86f7f0db28b421f8602`) passed the bundled-Xvfb startup smoke and rendered the 1280×800 main window. In a fresh isolated Xvfb :155 profile, I added a track and MIDI item through AAADAW, selected it, and opened Item Properties with F2. `xwininfo` reported `Media Item Properties` at 526×390. Its [live screenshot](fixtures/midi-item-properties-loop-source-aaadaw.png) shows the checked control next to the same MIDI item's name, position, length, and source offset.

## Remaining parity gaps

The screenshots confirm that the Loop source control is visible in both applications and enabled by default. AAADAW's MIDI properties window is still much shorter than REAPER's 526×693 window. Take timebase, playback rate and pitch, mix behavior, additional source controls, fades, and other native fields remain outside this slice; the full Item Properties window is not accepted as complete. A MIDI source-loop length display/edit control and recovery of historical loop length for empty trimmed items also remain open.

After the control change, the AAADAW App suite passed 585/585. The Engine suite passed 187/187 under nightly+Cranelift with bundled Xvfb available for CLAP GUI helper tests. Exact screenshot hashes, binary details, and window measurements are in the [fixture TSV](fixtures/midi-item-properties-loop-source.tsv). `ITEM-PROPERTIES-001`, `ITEM-NUDGE-SET-001`, and P4 remain `in_progress`.
