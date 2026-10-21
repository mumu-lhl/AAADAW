#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
reaper=${AAADAW_REAPER_BIN:-"$repo_root/target/reaper-7.82/reaper_linux_x86_64/REAPER/reaper"}
fixtures=${1:-"$repo_root/docs/verification/reaper-parity/fixtures"}
sequence=${2:-left-right}
runtime_root=${AAADAW_XVFB_RUNTIME:-"$repo_root/target/reaper782/root/usr/bin"}
if [[ ! -x "$runtime_root/Xvfb" ]]; then runtime_root=/tmp/aaadaw-xvfb-apt/root/usr/bin; fi
xdotool=${AAADAW_XDOTOOL:-"$runtime_root/xdotool"}
xvfb=${AAADAW_XVFB:-"$runtime_root/Xvfb"}
source_offset=${AAADAW_REAPER_PROBE_SOURCE_OFFSET_SECONDS:--0.1}

for utility in import sha256sum xwininfo; do
  command -v "$utility" >/dev/null || {
    printf 'Required capture utility is missing: %s\n' "$utility" >&2
    exit 2
  }
done
[[ -x "$reaper" ]] || { printf 'REAPER binary is missing: %s\n' "$reaper" >&2; exit 2; }
[[ -x "$xdotool" ]] || { printf 'xdotool executable is missing: %s\n' "$xdotool" >&2; exit 2; }
[[ -x "$xvfb" ]] || { printf 'Xvfb executable is missing: %s\n' "$xvfb" >&2; exit 2; }
case "$sequence" in
  left-right|left|right) ;;
  *) printf 'Sequence must be left-right, left, or right: %s\n' "$sequence" >&2; exit 2 ;;
esac

mkdir -p "$fixtures"
run_dir=$(mktemp -d "$repo_root/target/reaper-midi-signed-nudge.XXXXXX")
mkdir -p "$run_dir/config" "$run_dir/output" "$run_dir/data" "$run_dir/state" "$run_dir/cache" "$run_dir/runtime"
chmod 700 "$run_dir/runtime"
display=
for number in $(seq 900 999); do
  candidate=":$number"
  if [[ ! -e "/tmp/.X$number-lock" && ! -S "/tmp/.X11-unix/X$number" ]]; then
    display=$candidate
    break
  fi
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
export AAADAW_REAPER_PROBE_SOURCE_OFFSET_SECONDS="$source_offset"
export XDG_CONFIG_HOME="$run_dir/config" XDG_DATA_HOME="$run_dir/data"
export XDG_STATE_HOME="$run_dir/state" XDG_CACHE_HOME="$run_dir/cache"
export XDG_RUNTIME_DIR="$run_dir/runtime"
export LD_LIBRARY_PATH="$repo_root/.rtk-lib:$runtime_root/../lib/x86_64-linux-gnu:/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
for _ in $(seq 1 100); do
  if xwininfo -display "$display" -root >/dev/null 2>&1; then break; fi
  sleep 0.1
done
"$reaper" -cfgfile "$run_dir/config/reaper.ini" -newinst -nosplash \
  "$repo_root/scripts/reaper_parity/probe-item-nudge-midi-nonloop.lua" \
  >"$run_dir/reaper.log" 2>&1 &
reaper_pid=$!

tsv="$run_dir/output/item-nudge-midi-nonloop.tsv"
command_file="$run_dir/output/item-nudge-midi-nonloop-command.txt"
for _ in $(seq 1 200); do [[ -s "$tsv" ]] && break; sleep 0.1; done
[[ -s "$tsv" ]] || { cat "$run_dir/reaper.log" >&2; exit 1; }
sleep 1

about_window=$("$xdotool" search --onlyvisible --name 'About REAPER' | head -1 || true)
if [[ -n "$about_window" ]]; then
  "$xdotool" mousemove 490 396 click 1
  sleep 0.5
fi
device_window=$("$xdotool" search --onlyvisible --name 'Error opening devices' | head -1 || true)
if [[ -n "$device_window" ]]; then
  "$xdotool" windowfocus --sync "$device_window" key Escape
  sleep 0.5
fi
nudge_window=$("$xdotool" search --onlyvisible --name 'Nudge/Set Items' | head -1 || true)
[[ -n "$nudge_window" ]] || { cat "$run_dir/reaper.log" >&2; exit 1; }
"$xdotool" windowmove "$nudge_window" 300 600
"$xdotool" windowraise "$nudge_window" windowfocus "$nudge_window"
"$xdotool" mousemove 445 625 click 1 key c Return
"$xdotool" mousemove 572 625 click 1 key ctrl+a type 250
sleep 0.5
import -display "$display" -window root "$fixtures/item-nudge-midi-negative-source-offset-native.png"

wait_for_label() {
  local label=$1
  for _ in $(seq 1 100); do
    if awk -F '\t' -v expected="$label" '$1==expected {found=1} END {exit !found}' "$tsv"; then
      return 0
    fi
    sleep 0.1
  done
  cat "$tsv"
  cat "$run_dir/reaper.log" >&2
  return 1
}
if [[ "$sequence" == left-right || "$sequence" == left ]]; then
  printf 'left\n' >"$command_file"
  wait_for_label left
fi
if [[ "$sequence" == left-right || "$sequence" == right ]]; then
  printf 'right\n' >"$command_file"
  wait_for_label right
fi
printf 'done\n' >"$command_file"
sleep 0.5
cp -- "$tsv" "$fixtures/item-nudge-midi-negative-source-offset-native.tsv"
sha256sum "$fixtures/item-nudge-midi-negative-source-offset-native.tsv"
sha256sum "$fixtures/item-nudge-midi-negative-source-offset-native.png"
printf 'native_run_dir\t%s\nnative_display\t%s\nsource_offset_seconds\t%s\nsequence\t%s\n' \
  "$run_dir" "$display" "$source_offset" "$sequence"
