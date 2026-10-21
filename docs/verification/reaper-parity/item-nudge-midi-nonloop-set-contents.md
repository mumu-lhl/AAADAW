# Absolute Set → Contents for non-loop MIDI

Linux REAPER `7.82/linux-x86_64` with `Default_7.0.ReaperThemeZip` was tested in a fresh unsaved project with transport stopped. The fixture was one selected, non-loop MIDI item at 2 seconds with a 5-second item length, a 10-quarter-note source, `D_STARTOFFS=0.1` seconds, and two notes. The real Nudge/Set dialog used Mode `Set`, Target `contents`, and Snap off.

The absolute target's relationship to the item start determines how REAPER stores the alignment:

| Target | `D_STARTOFFS` | MIDI source length | Event change |
| ---: | ---: | ---: | --- |
| 1.000 s, before the 2-second item start | 1.000 s | 10 quarter notes | Notes stay at 2,880–4,800 and 6,720–8,640 PPQ |
| 3.000 s, after the 2-second item start | 0 s | 12 quarter notes | Notes shift forward 1,920 PPQ to 4,800–6,720 and 8,640–10,560 PPQ |

In both cases the item remained at 2–7 seconds, Loop source stayed off, and selection was retained. One Undo after the 3-second Set restored the initial source offset, source length, and note positions; one Redo restored the Set result. The native TSV captures both target directions and the Undo/Redo state counts. The [probe script](../../../scripts/reaper_parity/probe-item-nudge-midi-nonloop-set-contents.lua) builds the fixture, records state, and waits while the actual dialog is operated.

AAADAW now follows these two native cases. When the target precedes the item start, it stores the positive MIDI source offset. When the target follows the item start, it shifts notes, controllers, and pitch bends together, extends the source length by the same tick delta, and resets the source offset to zero. The source edit is one `DawAction` inside the existing Set transaction, preserving event IDs and making the complete edit one-step Undo/Redo. The App regression checks both target directions, item bounds, selection, all three MIDI event types, note identities, source length, and Undo/Redo.

The actual REAPER captures show the configured dialog, item before and after Set, the one-second target case, and Undo/Redo: [dialog](fixtures/item-nudge-midi-nonloop-set-contents-native-dialog.png), [before](fixtures/item-nudge-midi-nonloop-set-contents-native-before.png), [after](fixtures/item-nudge-midi-nonloop-set-contents-native-after.png), [target before item start](fixtures/item-nudge-midi-nonloop-set-contents-native-target-1s.png), [Undo](fixtures/item-nudge-midi-nonloop-set-contents-native-undo.png), and [Redo](fixtures/item-nudge-midi-nonloop-set-contents-native-redo.png). The TSV SHA-256 is `a4d00fd335c2c4595e73b23bf83db7a861f5248742582ab3c953d643dabc1318`; the configured dialog screenshot SHA-256 is `e4cf5d5908a4741fb792c26b8d45a4c7b551e505bc564fa07a3e4111f54a6833`.

The rebuilt AAADAW binary (Build ID `c1e3833d58fbf8c1c257f9db9c8b2f25111dc592`, SHA-256 `69cfa5bc5e08ab5eb613c1602675caae5fedc64f59d7599607ccf05ed2dffc36`) passed the isolated-Xvfb startup smoke and rendered a 1280×800 main window; the startup screenshot hash matches the established fixture. A separate live window capture from the same binary shows Set → Contents with a 3-second target in the 528×121 dialog: [AAADAW dialog](fixtures/item-nudge-midi-nonloop-set-contents-aaadaw-dialog.png), SHA-256 `280754aa46de1ce83f755622da458b8fd395351dbf1b36e057fd2b29082248da`. That dialog capture has no selected item, so it validates the real window and control state; the source-edit behavior is covered by the App regression and native REAPER fixture.

These constant-tempo cases do not establish Set parity across tempo changes, other snap units, or mixed selections involving non-loop MIDI. `ITEM-NUDGE-SET-001` remains `in_progress`.
