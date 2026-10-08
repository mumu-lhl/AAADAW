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

## Android

The Android app uses NativeActivity with the Iced/winit event loop and an
Android-specific compact touch layout below 720 logical pixels. Project open
and save use the Storage Access Framework (SAF): documents are staged in the
app's private storage while open, and saved project/WAV files are copied back
to the user-selected URI. Audio import is staged before it enters the project.
The Android audio feature uses CPAL's AAudio backend for playback and capture;
recording asks for microphone permission at runtime and starts a microphone
foreground service with an ongoing notification. Playback starts a media
playback foreground service, so Android can keep the audio stream alive when
the Activity moves to the background. While either mode is active, the shared
service holds a partial wake lock and releases it when both modes end. If an
output stream is lost while playing, AAADAW closes it and tries to reopen
playback at the last reported sample. If no output is available, playback stops
with an error.
If an input route fails while recording, AAADAW closes the old stream and tries
to reopen the selected input. The capture writer preserves the outage as silence
between timestamped audio blocks. If reopening fails, AAADAW finalizes the audio
captured before the route loss. Gaps longer than ten seconds fail the timing
check and leave the recoverable recording data available for recovery.

On phone-sized windows, Media Browser, Settings, Tempo/Meter Map, the track FX
chain, and the CLAP picker use single-panel navigation in the main window. The
Back action returns to the prior panel while retaining the project, selected
track, edit cursor, and playback state. Native CLAP editor windows remain
unsupported on Android.

Build the ARM64 native library and debug APK with JDK 17, Android SDK platform
35, Android NDK, and the `aarch64-linux-android` Rust target:

```sh
ANDROID_JAR="$ANDROID_HOME/platforms/android-35/android.jar" cargo ndk -t arm64-v8a -P 26 -o android-app/app/src/main/jniLibs build --release -p aaadaw --lib --features android-backend
cd android-app
gradle assembleDebug
```

The APK targets ARM64 phones/tablets and x86_64 emulators. Import an Android ARM64 `.clap` library
through Settings to copy it into app-private storage. Android scans and hosts
these plugins in-process; plugin code can crash the app, so install only trusted
libraries. Android plugin editor windows are not supported.

Android's MIDI manager opens USB and paired Bluetooth MIDI 1.0 ports. While an
audio output is open, input reaches the selected instrument track and scheduled
project MIDI is sent to connected output ports. The current track model is
channel-agnostic, so events use MIDI channel 1; live input is not recorded into
MIDI clips. Refresh the External MIDI section in Audio settings after connecting
or pairing a device. Attached-device MIDI behavior still needs validation.

GitHub Actions builds ARM64 and x86_64 native libraries, assembles the APK, and
launches it on an API 35 x86_64 emulator, including relaunch, larger font scale,
orientation changes, and screenshot capture. This checks packaging and the
small-screen shell, not physical audio routing or SAF-provider behavior.
Physical-device checks remain open for SAF providers, route recovery behavior,
background recording, MIDI devices, system bars, and screen/font scaling.

## Packaging status

CI creates the validation packages described in [native installer notes](native-installers.md)
and the unsigned macOS app bundle above. There is no code-signing, notarization,
automatic update, or bundle of native audio services. Keep the chosen backend
feature and its required system libraries explicit when distributing a binary;
see [the roadmap](../ROADMAP.md) for remaining release gates.
