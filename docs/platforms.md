# Desktop development and build notes

AAADAW currently verifies Linux, Windows, and Apple Silicon macOS builds in
GitHub Actions. It also builds unsigned Ubuntu 24.04 x86_64 `.deb`, Windows
x86_64 MSI, and macOS arm64 `.app` validation packages, plus Linux/Windows
portable archives. These artifacts are not signed public
releases and do not bundle audio services or drivers. Device-driver behavior
and audio latency still need validation on real hardware (see Issue #7 for
Linux capture).

## Linux

Install the stable Rust toolchain with `rustfmt` and `clippy`. The default build
does not enable a device backend. Linux builds also need the GLib development
package for the isolated CLAP helper's GLib main-context servicing
(`libglib2.0-dev` on Debian/Ubuntu). This services GLib-integrated plug-in UI
events and CLAP main-thread callbacks. The isolated CLAP helper also supports
`clap.posix-fd-support` for plug-ins that integrate their main-thread event work
through registered POSIX descriptors. Neither service runs arbitrary Qt, Xlib,
or other toolkit event loops:

```sh
cargo run -p aaadaw
cargo build --release -p aaadaw
```

For JACK controls and playback, install the JACK development headers
(`libjack-jackd2-dev` on Debian/Ubuntu) and enable the backend:

```sh
cargo run -p aaadaw --features aaadaw/jack-backend
cargo build --release -p aaadaw --features aaadaw/jack-backend
```

For PipeWire, install the package providing `libpipewire-0.3` development files
and use `aaadaw/pipewire-backend`. To expose both backends, enable both feature
names. A running server and a working audio route are required for playback.
The Linux build enables X11 and Wayland Iced shells; the host needs a compatible
display server and graphics stack to launch the desktop application.

## Windows

Install the stable Rust MSVC toolchain and Visual Studio Build Tools with the
Desktop development with C++ workload. The portable build and the WASAPI build
are checked in CI:

```powershell
cargo run -p aaadaw
cargo check --workspace --all-targets --features aaadaw/wasapi-backend
cargo build --release -p aaadaw --features aaadaw/wasapi-backend
```

The WASAPI feature enables device playback and recording when a Windows audio
device is available. CI tests policy and device-enumeration failure behavior;
it cannot validate a physical device or its driver.

## macOS

Use a current stable Rust toolchain. The CoreAudio backend is provided by CPAL
and uses the system's native output and input devices:

```sh
cargo check --workspace --all-targets --features aaadaw/coreaudio-backend
cargo run -p aaadaw --features aaadaw/coreaudio-backend
cargo build --release -p aaadaw --features aaadaw/coreaudio-backend
```

Audio Settings lists available input and output devices. The selected device is
saved by CPAL device ID; choosing **System default** explicitly clears the
saved ID. If a saved device is disconnected or unavailable, AAADAW reports that
state and does not silently switch to another device. Close and reopen playback
to apply an output change; an active recording keeps its connected input. Mono
inputs, including a built-in microphone that exposes only one channel, are
recorded centered to both project channels. macOS requests microphone
permission when recording first needs input.

The `macos-15` Apple Silicon CI job compiles all targets with CoreAudio and
tests device-enumeration recovery and backend policy. The native installer job
assembles an unsigned `AAADAW.app`, validates its `Info.plist`, launches the
bundled executable for `--version`, and uploads a zip artifact. CI does not
exercise physical CoreAudio devices, microphone permission prompts, playback
quality, or latency. The bundle is unsigned and not notarized; Gatekeeper-ready
distribution remains a release task.

## Packaging status

CI creates the validation packages described in [native installer notes](native-installers.md)
and the unsigned macOS app bundle above. There is no code-signing, notarization,
automatic update, or bundle of native audio services. Keep the chosen backend
feature and its required system libraries explicit when distributing a binary;
see [the roadmap](../ROADMAP.md) for remaining release gates.
