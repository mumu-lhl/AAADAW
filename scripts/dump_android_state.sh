#!/bin/sh

set +e

adb shell pidof org.aaadaw.app
adb shell dumpsys activity activities | tail -n 200
adb shell dumpsys activity exit-info org.aaadaw.app | tail -n 120
adb shell dumpsys window | grep -E 'mCurrentFocus|mFocusedApp'
adb logcat -d -v threadtime -b all \
    | grep -Ei 'org\.aaadaw|AAADAW|System\.(out|err)|stderr|stdout|ActivityTaskManager|ActivityManager|AndroidRuntime|NativeActivity|Fatal signal|panic|ANR in' \
    | tail -n 700
printf '%s\n' '--- app-private files ---'
app_files="$(adb shell run-as org.aaadaw.app find . -type f 2>&1 | tr -d '\r')"
printf '%s\n' "$app_files"
printf '%s\n' "$app_files" \
    | while IFS= read -r file; do
        case "$file" in
            *.log)
                printf '%s\n' "--- $file ---"
                adb shell run-as org.aaadaw.app cat "$file"
                ;;
        esac
    done
