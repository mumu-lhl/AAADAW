# Android app

This Gradle project packages the Rust `cdylib` as a NativeActivity APK. It
supports SAF project open/save, audio import, WAV export, AAudio playback and
recording, runtime microphone permission, a recording foreground service, and
a compact touch layout for screens below 720 logical pixels. SAF documents are
staged in app-private storage while open and copied back to the selected URI
after save/render.

Requirements: JDK 17, Android SDK platform 35, Android NDK, Rust target
`aarch64-linux-android`, optional `x86_64-linux-android` for emulator builds,
`cargo-ndk`, and Gradle 8.11 or newer.

From the repository root, build the native library and copy it into the APK
source set for each ABI you build:

```sh
ANDROID_JAR="$ANDROID_HOME/platforms/android-35/android.jar" cargo ndk -t arm64-v8a -P 26 -o android-app/app/src/main/jniLibs build --release -p aaadaw --lib --features android-backend
cd android-app
gradle assembleDebug
```

The manifest declares the NativeActivity, microphone permission, and recording
service. `MainActivity` streams local staged files to SAF URIs without loading
large exports into memory. Import Android ARM64 `.clap` libraries through
Settings; AAADAW scans and hosts imported plugins in-process. Native plugin code
can crash the app, so import trusted libraries only. Android external MIDI
input/output is not implemented yet. CI builds ARM64 and x86_64 APK libraries
and launches the app on an API 35 emulator through relaunch, larger font scale,
orientation changes, and screenshot capture. When an output stream is lost,
AAADAW attempts to reopen it and resume from the last reported sample; if the
route remains unavailable, playback stops safely. Input failure finalizes the
current take and requires recording to be started again. Validate SAF providers,
audio-route recovery, background recording, MIDI devices, system bars, screen
rotation, and font scaling on a physical ARM64 device before distributing a
release build.
