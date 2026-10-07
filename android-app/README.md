# Android app

This Gradle project packages the Rust `cdylib` as a NativeActivity APK. It
supports SAF project open/save, audio import, WAV export, AAudio playback and
recording, runtime microphone permission, a recording foreground service, and
a compact touch layout for screens below 720 logical pixels. SAF documents are
staged in app-private storage while open and copied back to the selected URI
after save/render.

Requirements: JDK 17, Android SDK platform 35, Android NDK, Rust target
`aarch64-linux-android`, `cargo-ndk`, and Gradle 8.11 or newer.

From the repository root, build the native library and copy it into the APK
source set for each ABI you build:

```sh
cargo ndk -t arm64-v8a -p 26 -o android-app/app/src/main/jniLibs build --release -p aaadaw --lib --features android-backend
cd android-app
gradle assembleDebug
```

The manifest declares the NativeActivity, microphone permission, and recording
service. `MainActivity` streams local staged files to SAF URIs without loading
large exports into memory. Android external MIDI device support and Android
CLAP plugin scanning/hosting are not implemented. Validate file providers,
audio routes, background recording, system bars, and screen/font scaling on a
physical ARM64 device before distributing a release build.
