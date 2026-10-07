# Android app

This Gradle project packages the Rust `cdylib` as a NativeActivity APK. It
supports SAF project open/save, audio import, WAV export, AAudio playback and
recording, runtime microphone permission, foreground services for playback and
recording, and
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

The manifest declares the NativeActivity, microphone permission, and audio
foreground service types. The service keeps playback or recording active when
the Activity moves to the background, and updates its notification for each
active mode. Recording uses the microphone type; playback uses the media
playback type. `MainActivity` streams local staged files to SAF URIs without loading
large exports into memory. Import Android ARM64 `.clap` libraries through
Settings; AAADAW scans and hosts imported plugins in-process. Native plugin code
can crash the app, so import trusted libraries only. Android external MIDI
editor windows are not supported. Android's MIDI manager opens USB and paired
Bluetooth MIDI 1.0 ports. Refresh the External MIDI section in Audio settings
after connecting or pairing a device. While audio is running, live input reaches
the selected instrument and project MIDI is sent to connected outputs. The track
model currently uses MIDI channel 1, and live input is not recorded into MIDI
clips; attached-device behavior still needs validation. CI builds ARM64 and
x86_64 APK libraries and launches the app on an API 35 emulator through relaunch,
larger font scale, orientation changes, and screenshot capture. When an output stream is lost,
AAADAW attempts to reopen it and resume from the last reported sample; if the
route remains unavailable, playback stops safely. If an input route is lost
while recording, AAADAW closes the old stream and tries to reopen the selected
input. Captured frame timestamps preserve the interruption as silence in the
take. If reopening fails, AAADAW finalizes the audio captured before the route
loss; interruptions longer than ten seconds keep recoverable data and report a
timing error. Validate SAF providers,
audio-route recovery, background recording, MIDI devices, system bars, screen
rotation, and font scaling on a physical ARM64 device before distributing a
release build.
