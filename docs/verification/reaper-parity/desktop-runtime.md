# AAADAW Linux desktop runtime verification

This smoke run starts the built desktop binary in a fresh X11 display and isolated XDG profile, waits for its real main window, and captures the display. It does not require an audio device. `WGPU_BACKEND=gl` selects the Mesa/OpenGL path used by the headless Xvfb host.

Run it from the repository root:

```sh
bash scripts/reaper_parity/run-aaadaw-desktop-smoke.sh [path/to/aaadaw] [output.png]
```

The script uses a system `Xvfb` when available. On the parity host, Xvfb is bundled with the isolated REAPER runtime rather than installed system-wide; the script discovers the extracted executable or accepts an explicit `AAADAW_XVFB=/path/to/Xvfb`. `xwininfo`, ImageMagick `import`, and `sha256sum` are also required. The XDG profile, app log, and Xvfb log are kept under ignored `target/aaadaw-desktop-smoke.*` for inspection.

On 2026-10-14 the post-EOF, pre-signed-offset `target/debug/aaadaw` (Build ID `969fe3e9997570899c03c004f8c2941506c012e4`, SHA-256 `7410ee3a4d34bfc579f09f941df3aafef4b168ff68032c2b613de53e715809fe`) launched on a fresh Xvfb display. The smoke script produced the same [startup screenshot](fixtures/aaadaw-desktop-runtime-startup.png) (SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b`); `xwininfo` reported the actual main window as 1280×800. After the signed-offset changes, the new binary (Build ID `e1ab6146acaad9e9ea2d808e1f3261199529c792`, SHA-256 `0cdb64ab78668b631e6cec8b5b6a97db3f69387de364ee496fcf2a65fcfaa105`) also launched on Xvfb :90 and rendered a 1280×800 main window. Its screenshot is [here](fixtures/aaadaw-desktop-runtime-signed-offset.png); pixels match the startup fixture. In a separate fresh display, an earlier build was driven with XTest mouse events to open the actual Item menu and its `Nudge/set items...` action; the resulting X11 window tree contained a 528×121 `Nudge/Set Items` window. The [interaction screenshot](fixtures/aaadaw-desktop-runtime-nudge.png) shows that live window. All GUI screenshot hashes and run details are recorded in [the fixture manifest](fixtures/aaadaw-desktop-runtime.tsv).

The earlier failed relaunch followed a teardown of the old Xvfb session: `DISPLAY` was unset, no system Xvfb executable existed, and prior Xvfb/app processes were zombies. Launching against that stale environment could not create a client window. A fresh Xvfb process from the bundled REAPER runtime resolved the issue without changing system packages. This runtime proof is separate from feature acceptance; each parity item still needs its own behavior, screenshot, and regression evidence.

After the bounded-source-section coordinate correction, the rebuilt desktop binary (Build ID `dab8bd42de48bc34d83479ed2607137c64ab4b7a`, SHA-256 `f70ba20a0bc8675f6465b0bdd8c2b47c3264489162bc9703ea2f2b93481b5272`) launched on Xvfb `:90`; the smoke script found the actual AAADAW main window and captured 1280×800. Its screenshot is pixel-identical to the startup fixture (SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b`). The [fixture manifest](fixtures/aaadaw-desktop-runtime.tsv) records this rebuild separately from earlier runs.
