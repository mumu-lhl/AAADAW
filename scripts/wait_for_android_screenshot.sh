#!/bin/sh

set -eu

printf 'Waiting for a rendered Android app screenshot\n' > /tmp/aaadaw-screenshot-check.log
attempt=0
while [ "$attempt" -lt 30 ]; do
    if timeout 10s adb shell screencap -p /data/local/tmp/aaadaw-portrait.png \
        && timeout 10s adb pull /data/local/tmp/aaadaw-portrait.png ./aaadaw-portrait.png \
        && python3 scripts/check_android_screenshot.py aaadaw-portrait.png \
            > /tmp/aaadaw-screenshot-check.log 2>&1; then
        cat /tmp/aaadaw-screenshot-check.log
        exit 0
    fi
    sleep 3
    attempt=$((attempt + 1))
done

cat /tmp/aaadaw-screenshot-check.log >&2
exit 1
