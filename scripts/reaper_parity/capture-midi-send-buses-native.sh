#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
reaper=${AAADAW_REAPER_BIN:-"$repo_root/target/reaper-7.82/reaper_linux_x86_64/REAPER/reaper"}
fixtures=${1:-"$repo_root/docs/verification/reaper-parity/fixtures"}
runtime_root=${AAADAW_XVFB_RUNTIME:-"$repo_root/target/reaper782/root/usr/bin"}
if [[ ! -x "$runtime_root/Xvfb" ]]; then runtime_root=/tmp/aaadaw-xvfb-apt/root/usr/bin; fi
xdotool=${AAADAW_XDOTOOL:-"$runtime_root/xdotool"}
xvfb=${AAADAW_XVFB:-"$runtime_root/Xvfb"}

for utility in import identify sha256sum xwininfo; do
  command -v "$utility" >/dev/null || { printf 'Required screenshot utility is missing: %s\n' "$utility" >&2; exit 2; }
done
[[ -x "$reaper" ]] || { printf 'REAPER binary is missing: %s\n' "$reaper" >&2; exit 2; }
[[ -x "$xdotool" ]] || { printf 'xdotool executable is missing: %s\n' "$xdotool" >&2; exit 2; }
[[ -x "$xvfb" ]] || { printf 'Xvfb executable is missing: %s\n' "$xvfb" >&2; exit 2; }

mkdir -p "$fixtures"
run_dir=$(mktemp -d "$repo_root/target/reaper-midi-send-buses.XXXXXX")
profile=${AAADAW_REAPER_CAPTURE_PROFILE:-"$run_dir"}
mkdir -p "$profile/config/Effects/JSFX" "$run_dir/output" "$profile/data" "$profile/state" "$profile/cache" "$profile/runtime"
cp -- "$repo_root/scripts/reaper_parity/jsfx/aaadaw-midi-send-bus-probe.jsfx" \
  "$profile/config/Effects/JSFX/aaadaw-midi-send-bus-probe.jsfx"
cp -- "$repo_root/scripts/reaper_parity/jsfx/aaadaw-midi-send-bus-source-probe.jsfx" \
  "$profile/config/Effects/JSFX/aaadaw-midi-send-bus-source-probe.jsfx"
chmod 700 "$profile/runtime"
display=
for number in $(seq 800 899); do
  candidate=":$number"
  if [[ ! -e "/tmp/.X$number-lock" && ! -S "/tmp/.X11-unix/X$number" ]]; then display=$candidate; break; fi
done
[[ -n "$display" ]] || { printf 'No free X11 display was found.\n' >&2; exit 1; }

xvfb_pid=
reaper_pid=
cleanup() {
  if [[ -n "$reaper_pid" ]]; then kill "$reaper_pid" 2>/dev/null || true; wait "$reaper_pid" 2>/dev/null || true; fi
  if [[ -n "$xvfb_pid" ]]; then kill "$xvfb_pid" 2>/dev/null || true; wait "$xvfb_pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

"$xvfb" "$display" -screen 0 1280x800x24 -ac -nolisten tcp >"$run_dir/xvfb.log" 2>&1 &
xvfb_pid=$!
export DISPLAY="$display" AAADAW_REAPER_PROBE_DIR="$run_dir/output"
export XDG_CONFIG_HOME="$profile/config" XDG_DATA_HOME="$profile/data"
export XDG_STATE_HOME="$profile/state" XDG_CACHE_HOME="$profile/cache" XDG_RUNTIME_DIR="$profile/runtime"
export LD_LIBRARY_PATH="$repo_root/.rtk-lib:$runtime_root/../lib/x86_64-linux-gnu:/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
for _ in $(seq 1 100); do
  if xwininfo -display "$display" -root >/dev/null 2>&1; then break; fi
  sleep 0.1
done
"$reaper" -cfgfile "$profile/config/reaper.ini" -newinst -nosplash \
  "$repo_root/scripts/reaper_parity/probe-midi-send-buses.lua" >"$run_dir/reaper.log" 2>&1 &
reaper_pid=$!
sleep 4
"$xdotool" mousemove 490 396 click 1 || true

for _ in $(seq 1 200); do
  if [[ -s "$run_dir/output/midi-send-buses-capture-complete" ]]; then break; fi
  modal=$("$xdotool" search --onlyvisible --name 'Error opening devices' | head -1 || true)
  if [[ -n "$modal" ]]; then
    "$xdotool" mousemove 20 175 click 1
    sleep 0.2
    "$xdotool" mousemove 435 205 click 1
  fi
  sleep 0.1
done
[[ -s "$run_dir/output/midi-send-buses-capture-complete" ]] || {
  cat "$run_dir/reaper.log" >&2
  import -display "$display" -window root "$run_dir/native-probe-timeout.png" || true
  printf 'REAPER did not complete the MIDI bus probe.\n' >&2
  exit 1
}
sleep 8
"$xdotool" mousemove 490 396 click 1
sleep 0.5
"$xdotool" mousemove 490 396 click 1
sleep 0.5
main_window=$("$xdotool" search --onlyvisible --name 'REAPER v7.82' | head -1 || true)
[[ -n "$main_window" ]] || { printf 'REAPER window was not found after probe completion.\n' >&2; exit 1; }
"$xdotool" windowmove "$main_window" 0 0
"$xdotool" windowsize "$main_window" 1280 800
sleep 0.5
"$xdotool" mousemove 211 645 click 1
sleep 1
import -display "$display" -window root "$fixtures/midi-send-buses-native.png"
identify "$fixtures/midi-send-buses-native.png"
sha256sum "$fixtures/midi-send-buses-native.png"
for suffix in native.tsv actions.tsv after.rpp; do
  source="$run_dir/output/midi-send-buses-$suffix"
  target="$fixtures/midi-send-buses-$suffix"
  [[ -s "$source" ]] || { printf 'Probe artifact is missing: %s\n' "$source" >&2; exit 1; }
  cp -- "$source" "$target"
  sha256sum "$target"
done
printf 'native_run_dir\t%s\nnative_display\t%s\n' "$run_dir" "$display"
