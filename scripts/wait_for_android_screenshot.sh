#!/bin/sh

set -eu

attempt=0
while [ "$attempt" -lt 30 ]; do
    adb shell screencap -p /data/local/tmp/aaadaw-portrait.png
    adb pull /data/local/tmp/aaadaw-portrait.png ./aaadaw-portrait.png
    if python3 scripts/check_android_screenshot.py aaadaw-portrait.png \
        > /tmp/aaadaw-screenshot-check.log 2>&1; then
        cat /tmp/aaadaw-screenshot-check.log
        exit 0
    fi
    sleep 3
    attempt=$((attempt + 1))
done

cat /tmp/aaadaw-screenshot-check.log >&2
exit 1
