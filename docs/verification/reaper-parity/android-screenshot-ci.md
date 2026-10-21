# Android screenshot acquisition CI

On commit `62781e8`, Rust CI run 38065960181 passed Linux tests/Clippy, Windows, macOS, and both Android builds. The separate emulator launch job 114254221144 failed before the screenshot validator printed a result. The log shows `- waiting for device -`, a live AAADAW process (1475), and its foreground activity in the diagnostic dump. This identifies the capture stage, but does not prove the rendered surface passed validation.

The screenshot loop previously used `set -e` with unguarded `adb shell screencap` and `adb pull`, so either transient command failure bypassed all 30 retries. Both commands are now guarded with individual 10-second bounds; validation runs only after both succeed. Existing OCR, foreground, rotation, and app-private data checks are retained.

Local proof: shell syntax check passes. An isolated executable stub makes the first capture fail, then verifies exactly three ADB calls (failed capture, successful capture, successful pull) and one validator call before success. Remote emulator verification remains pending on the next workflow run; this is not an Android product acceptance result.

Remote follow-up: commit `052a942987c0f3aba7d6f06eafc985e10e167e3b` passed Rust CI run 38067687384 (including emulator launch), Portable desktop archives 38067687370, and Native installers 38067687366. This closes the reported CI capture failure; it does not replace full Android feature acceptance.
