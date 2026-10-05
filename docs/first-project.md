# Your first AAADAW project

This walkthrough uses the bundled two-bar melody session to show the main audio-editing path. The project embeds its audio, so it opens without an extra media file or plug-in. Make a copy before editing the checked-in example.

- [Download the example project](../examples/first-project/first-project.aaadaw)
- [Download the melody WAV](../examples/first-project/melody.wav) to try the import steps with a new project

## Open and save a copy

1. Open AAADAW and choose **File → Open Project…**.
2. Select `first-project.aaadaw`.
3. Choose **File → Save Project As…** and save a working copy in your music-project folder.

The Arrangement opens with one audio track and one four-second item. The melody is embedded in the project, while new projects can embed audio as you import it.

## Import and edit audio

1. Choose **File → New Project** and save it with **File → Save Project As…**.
2. Click **+ Track** in the Arrangement toolbar. Select the new track by clicking its track header.
3. Choose **Insert → Import Audio…**, select `melody.wav`, and confirm. The file is copied into the project; the new item is placed on the selected track at the edit cursor.
4. Drag the item body to move it. The Arrangement toolbar's **Snap On/Off** button enables grid snapping; the **Grid** menu selects the grid size.
5. Drag either item edge inward to trim it. To split, click the timeline ruler to place the edit cursor, select the item, then choose **Item → Split items at edit cursor**.
6. Use **Edit → Undo** or **Edit → Redo** to check and restore an edit.

## Play and mix

The transport stays at the bottom of the main window. Press **Space** or click **Play** to play/pause. **Stop** returns to the playback start; **Restart** returns to the project start. Clicking the ruler moves the edit cursor. To seek the playhead while stopped, enter a sample position in the transport's **Sample** field and click **Seek**.

Adjust track volume and pan in its track controls. The Master output is protected by a configurable sample-peak ceiling; this is not a loudness meter or true-peak limiter.

Press **Ctrl+S** to save. Close and reopen the working copy to confirm the item and mix settings persist.

## Default shortcuts

| Action | Linux | Windows |
| --- | --- | --- |
| New project | Ctrl+N | Ctrl+N |
| Open project | Ctrl+O | Ctrl+O |
| Save project | Ctrl+S | Ctrl+S |
| Undo | Ctrl+Z | Ctrl+Z |
| Redo | Ctrl+Shift+Z or Ctrl+Y | Ctrl+Shift+Z or Ctrl+Y |
| Play/pause | Space | Space |

Change or restore bindings in **File → Settings… → Keyboard Shortcuts**. The app uses platform command modifiers; the table shows their current Linux and Windows labels. Keyboard shortcuts only run when a text field or another control has not consumed the key.

## Audio devices and recording

The Ubuntu package and Linux portable build include JACK and PipeWire support. A running audio server and a working route are required; AAADAW does not install or start either service. Choose the backend and available device in **File → Settings… → Audio**. The Windows packages use WASAPI and the devices available to the current Windows user.

To record, click the track's **R** control to arm it, then click **Record** in the transport. Recording requires an available input device. CI verifies backend builds and failure handling, but real-device recording and driver latency still need manual verification; see [Issue #7](https://github.com/mumu-lhl/AAADAW/issues/7).

## Troubleshooting and current limits

- **No playback output:** confirm the audio server is running on Linux, select the intended backend/device in Audio settings, and verify the server or system mixer routes AAADAW's stereo output to speakers or headphones. Check the transport status for device or callback errors.
- **Recording will not start:** confirm the track is armed and that the selected backend exposes two input channels. AAADAW reports an unavailable input without inserting a take.
- **Missing linked media:** open the Media Browser and use its missing-link repair action to select the source file again. Embedded media does not depend on its original path.
- **Missing CLAP plug-in:** install a compatible CLAP build in a scanned plug-in directory, rescan, and reopen the track's plug-in controls. Projects retain plug-in references and state, but a missing plug-in cannot process audio.
- **Plug-in stability:** CLAP entry scanning uses a helper process. Loaded plug-ins still run inside AAADAW with the user's permissions; a plug-in can crash or stall the application. Only load plug-ins you trust.

Current native packages target Ubuntu 24.04 x86_64 and Windows x86_64. They are unsigned validation packages, not an automatic update or public release channel. VST3 hosting, Android support, and loaded plug-in process isolation are not implemented. See [native installer notes](native-installers.md), [portable build notes](portable-builds.md), and [platform support](platforms.md) for the verified build scope.
