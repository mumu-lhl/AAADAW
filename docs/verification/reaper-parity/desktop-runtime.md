# AAADAW Linux desktop runtime verification

This smoke run starts the built desktop binary in a fresh X11 display and isolated XDG profile, waits for its real main window, and captures the display. It does not require an audio device. `WGPU_BACKEND=gl` selects the Mesa/OpenGL path used by the headless Xvfb host.

Run it from the repository root:

```sh
bash scripts/reaper_parity/run-aaadaw-desktop-smoke.sh [path/to/aaadaw] [output.png]
```

The script uses a system `Xvfb` when available. On the parity host, Xvfb is bundled with the isolated REAPER runtime rather than installed system-wide; the script discovers the extracted executable or accepts an explicit `AAADAW_XVFB=/path/to/Xvfb`. `xwininfo`, ImageMagick `import`, and `sha256sum` are also required. The XDG profile, app log, and Xvfb log are kept under ignored `target/aaadaw-desktop-smoke.*` for inspection.

On 2026-10-14 the latest `target/debug/aaadaw` (Build ID `b63071e4c3a3757051a9125dcafee4baa40453e9`, SHA-256 `59bc8bfa475f11edc45e8eff911e63b1b003148b481ffbf07838ea4d58b86e44`) launched on a new Xvfb display. The smoke script produced the [startup screenshot](fixtures/aaadaw-desktop-runtime-startup.png); `xwininfo` reported the actual main window as 1280×800. In a separate fresh display, XTest mouse events opened the actual Item menu and its `Nudge/set items...` action; the resulting X11 window tree contained a 528×121 `Nudge/Set Items` window. The [interaction screenshot](fixtures/aaadaw-desktop-runtime-nudge.png) shows that live window. Both screenshot hashes and run details are recorded in [the fixture manifest](fixtures/aaadaw-desktop-runtime.tsv).

The earlier failed relaunch followed a teardown of the old Xvfb session: `DISPLAY` was unset, no system Xvfb executable existed, and prior Xvfb/app processes were zombies. Launching against that stale environment could not create a client window. A fresh Xvfb process from the bundled REAPER runtime resolved the issue without changing system packages. This runtime proof is separate from feature acceptance; each parity item still needs its own behavior, screenshot, and regression evidence.
