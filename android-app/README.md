# Android shell

This Gradle project packages the Rust `cdylib` as a NativeActivity APK. It is
currently a bring-up shell: project file access and audio-device support are
not implemented yet (tracked in [Issue #217](https://github.com/mumu-lhl/AAADAW/issues/217)).

Requirements: JDK 17, Android SDK platform 35, Android NDK, Rust targets
`aarch64-linux-android` and/or `x86_64-linux-android`, and `cargo-ndk`.

From the repository root, build the native library and copy it into the APK
source set for each ABI you build:

```sh
cargo ndk -t arm64-v8a -o android-app/app/src/main/jniLibs build --release -p aaadaw --lib
cd android-app
gradle assembleDebug
```

The manifest starts a NativeActivity and declares the shared object name. The
activity keeps orientation and window-size changes in the same instance so
winit can resize the Iced surface. The app does not currently request
microphone permission because Android recording is not implemented.
