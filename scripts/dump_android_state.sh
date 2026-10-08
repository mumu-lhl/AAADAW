#!/bin/sh

set +e

adb shell pidof org.aaadaw.app
adb shell dumpsys activity activities | tail -n 200
adb shell dumpsys window | grep -E 'mCurrentFocus|mFocusedApp'
adb logcat -d -v threadtime -b all \
    | grep -E 'org\.aaadaw|ActivityTaskManager|ActivityManager|AndroidRuntime|NativeActivity|Fatal signal|panic|ANR in' \
    | tail -n 700
adb shell run-as org.aaadaw.app sh -c 'for file in $(find files -type f -name "aaadaw*"); do echo "$file"; cat "$file"; done'
