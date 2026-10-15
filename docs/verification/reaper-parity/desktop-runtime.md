# AAADAW Linux desktop runtime verification

This smoke run starts the built desktop binary in a fresh X11 display and isolated XDG profile, waits for its real main window, and captures the display. It does not require an audio device. `WGPU_BACKEND=gl` selects the Mesa/OpenGL path used by the headless Xvfb host.

Run it from the repository root when an executable has already been built:

```sh
bash scripts/reaper_parity/run-aaadaw-desktop-smoke.sh [path/to/aaadaw] [output.png]
```

To rebuild the Linux desktop binary and capture it in one command, run:

```sh
bash scripts/reaper_parity/build-and-smoke-aaadaw-desktop.sh [output.png]
```

The build helper links the executable with debug sections stripped, stages the managed host's `target/runtime-pkgconfig` path when present, and removes only AAADAW incremental/link scratch files if the workspace has less than 1 GiB free. Dependency debug information and runtime code generation are unchanged.

The script uses a system `Xvfb` when available. On the parity host, Xvfb is bundled with the isolated REAPER runtime rather than installed system-wide; the script discovers the extracted executable or accepts an explicit `AAADAW_XVFB=/path/to/Xvfb`. `xwininfo`, ImageMagick `import`, and `sha256sum` are also required. The XDG profile, app log, and Xvfb log are kept under ignored `target/aaadaw-desktop-smoke.*` for inspection.

On 2026-10-14 the post-EOF, pre-signed-offset `target/debug/aaadaw` (Build ID `969fe3e9997570899c03c004f8c2941506c012e4`, SHA-256 `7410ee3a4d34bfc579f09f941df3aafef4b168ff68032c2b613de53e715809fe`) launched on a fresh Xvfb display. The smoke script produced the same [startup screenshot](fixtures/aaadaw-desktop-runtime-startup.png) (SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b`); `xwininfo` reported the actual main window as 1280×800. After the signed-offset changes, the new binary (Build ID `e1ab6146acaad9e9ea2d808e1f3261199529c792`, SHA-256 `0cdb64ab78668b631e6cec8b5b6a97db3f69387de364ee496fcf2a65fcfaa105`) also launched on Xvfb :90 and rendered a 1280×800 main window. Its screenshot is [here](fixtures/aaadaw-desktop-runtime-signed-offset.png); pixels match the startup fixture. In a separate fresh display, an earlier build was driven with XTest mouse events to open the actual Item menu and its `Nudge/set items...` action; the resulting X11 window tree contained a 528×121 `Nudge/Set Items` window. The [interaction screenshot](fixtures/aaadaw-desktop-runtime-nudge.png) shows that live window. All GUI screenshot hashes and run details are recorded in [the fixture manifest](fixtures/aaadaw-desktop-runtime.tsv).

The earlier failed relaunch followed a teardown of the old Xvfb session: `DISPLAY` was unset, no system Xvfb executable existed, and prior Xvfb/app processes were zombies. Launching against that stale environment could not create a client window. A fresh Xvfb process from the bundled REAPER runtime resolved the issue without changing system packages. This runtime proof is separate from feature acceptance; each parity item still needs its own behavior, screenshot, and regression evidence.

After the bounded-source-section coordinate correction, the rebuilt desktop binary (Build ID `dab8bd42de48bc34d83479ed2607137c64ab4b7a`, SHA-256 `f70ba20a0bc8675f6465b0bdd8c2b47c3264489162bc9703ea2f2b93481b5272`) launched on Xvfb `:90`; the smoke script found the actual AAADAW main window and captured 1280×800. Its screenshot is pixel-identical to the startup fixture (SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b`). The [fixture manifest](fixtures/aaadaw-desktop-runtime.tsv) records this rebuild separately from earlier runs.

On 2026-10-14, after MIDI source-loop, Contents Nudge, and per-item tempo-map conversion changes, the new desktop binary (Build ID `0b3b3dfe381801986c1a53db45b9f66e6f7aa900`, SHA-256 `6f0345d4001c61d44cd0356759c7176117045ea58d5c359dcdf5a4f45edfc1f1`) was rebuilt with `build-and-smoke-aaadaw-desktop.sh`. The script launched the real main window on a fresh Xvfb display at 1280×800; the screenshot again matches the startup fixture byte-for-byte (SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b`). This verifies startup after the source changes, not MIDI Editor or Nudge window visuals.

On 2026-10-15, the updated loop-source editor build (Build ID `108001ad22857fcc7f5ce81960203f5ec333c693`, SHA-256 `929def7e6e1d7835a25c89a726af78969f3f328a7d9f9f118a2fd80b69247401`) again launched the real 1280×800 main window on Xvfb :90 with the standard startup screenshot hash. The same binary was driven through its visible controls on Xvfb :94: create a MIDI item, add C4, extend the item from 3840 to 11520 ticks, open Piano roll, and fit the notes. `xwininfo` reported the actual MIDI Editor window as 1000×620. The [looped Piano roll capture](fixtures/midi-editor-loop-source-aaadaw.png) shows three repetitions of the same source note; its SHA-256 is `e9e28b4f7ea6309ffe983b2b43486dc0870888291c3ea91f75bd389ac2ab2c7a`. This feature capture used zero source offset; the nonzero wrapped-offset reference and test are recorded separately in [MIDI Editor loop-source verification](midi-editor-loop-source.md).
