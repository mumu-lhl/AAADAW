#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
binary=${1:-"$repo_root/target/debug/aaadaw"}
output=${2:-"$repo_root/docs/verification/reaper-parity/fixtures/render-filename-pattern-aaadaw.png"}
runtime_root=${AAADAW_REAPER_RUNTIME:-"$repo_root/target/reaper782/root/usr/bin"}
if [[ ! -x "$runtime_root/Xvfb" ]]; then
  runtime_root=/tmp/aaadaw-reaper-782/root/usr/bin
fi
xdotool=${AAADAW_XDOTOOL:-"$runtime_root/xdotool"}
xvfb=${AAADAW_XVFB:-"$runtime_root/Xvfb"}

for utility in xwininfo import identify sha256sum; do
  command -v "$utility" >/dev/null || {
    printf 'Required screenshot utility is missing: %s\n' "$utility" >&2
    exit 2
  }
done
[[ -x "$binary" ]] || { printf 'AAADAW executable is missing: %s\n' "$binary" >&2; exit 2; }
[[ -x "$xvfb" ]] || { printf 'Xvfb executable is missing: %s\n' "$xvfb" >&2; exit 2; }
[[ -x "$xdotool" ]] || { printf 'xdotool executable is missing: %s\n' "$xdotool" >&2; exit 2; }

mkdir -p "$repo_root/target" "$(dirname -- "$output")"
run_dir=${AAADAW_CAPTURE_RUN_DIR:-}
if [[ -z "$run_dir" ]]; then
  run_dir=$(mktemp -d "$repo_root/target/aaadaw-render-wildcards.XXXXXX")
else
  mkdir -p "$run_dir"
fi
mkdir -p "$run_dir/config" "$run_dir/data" "$run_dir/state" "$run_dir/cache" "$run_dir/runtime"
chmod 700 "$run_dir/runtime"

display=${AAADAW_DISPLAY:-}
if [[ -n "$display" ]]; then
  DISPLAY="$display" xwininfo -root >/dev/null || {
    printf 'Configured X11 display is unavailable: %s\n' "$display" >&2
    exit 1
  }
else
  for number in $(seq 90 599); do
    candidate=":$number"
    if [[ -e "/tmp/.X$number-lock" || -S "/tmp/.X11-unix/X$number" ]]; then
      continue
    fi
    if DISPLAY="$candidate" xwininfo -root >/dev/null 2>&1; then
      continue
    fi
    display=$candidate
    break
  done
fi
[[ -n "$display" ]] || { printf 'No free X11 display was found.\n' >&2; exit 1; }

xvfb_pid=
app_pid=
cleanup() {
  if [[ -n "$app_pid" ]]; then kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true; fi
  if [[ -n "$xvfb_pid" ]]; then kill "$xvfb_pid" 2>/dev/null || true; wait "$xvfb_pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [[ -z "${AAADAW_DISPLAY:-}" ]]; then
  "$xvfb" "$display" -screen 0 1280x800x24 -ac -nolisten tcp >"$run_dir/xvfb.log" 2>&1 &
  xvfb_pid=$!
  display_ready=0
  for _ in $(seq 1 100); do
    if DISPLAY="$display" xwininfo -root >/dev/null 2>&1; then
      display_ready=1
      break
    fi
    sleep 0.1
  done
  if [[ "$display_ready" != 1 ]]; then
    cat "$run_dir/xvfb.log" >&2
    exit 1
  fi
fi

export DISPLAY="$display"
export WGPU_BACKEND=${WGPU_BACKEND:-gl}
export XDG_CONFIG_HOME="$run_dir/config"
export XDG_DATA_HOME="$run_dir/data"
export XDG_STATE_HOME="$run_dir/state"
export XDG_CACHE_HOME="$run_dir/cache"
export XDG_RUNTIME_DIR="$run_dir/runtime"
export LD_LIBRARY_PATH="$runtime_root/../lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
"$binary" >"$run_dir/aaadaw.log" 2>&1 &
app_pid=$!

main_window=
for _ in $(seq 1 150); do
  main_window=$("$xdotool" search --name AAADAW 2>/dev/null | head -1 || true)
  [[ -n "$main_window" ]] && break
  sleep 0.2
done
[[ -n "$main_window" ]] || { cat "$run_dir/aaadaw.log" >&2; exit 1; }
sleep 0.5
"$xdotool" windowmap --sync "$main_window"
"$xdotool" windowfocus --sync "$main_window" || true
"$xdotool" key --clearmodifiers --window "$main_window" ctrl+alt+r

render_window=
for _ in $(seq 1 100); do
  render_window=$("$xdotool" search --name 'Render project to WAV' 2>/dev/null | head -1 || true)
  [[ -n "$render_window" ]] && break
  sleep 0.2
done
[[ -n "$render_window" ]] || { cat "$run_dir/aaadaw.log" >&2; exit 1; }
sleep 0.5
insert_wildcard=${AAADAW_INSERT_WILDCARD:-}
if [[ "${AAADAW_OPEN_WILDCARDS:-0}" == 1 || -n "$insert_wildcard" ]]; then
  "$xdotool" mousemove --sync 575 260 click 1
  sleep 0.4
fi
scroll_wildcards=${AAADAW_SCROLL_WILDCARDS:-0}
if [[ -n "$insert_wildcard" ]]; then
  case "$insert_wildcard" in
    project) wildcard_x=163; wildcard_y=316 ;;
    title) wildcard_x=236; wildcard_y=316 ;;
    author) wildcard_x=310; wildcard_y=316 ;;
    user) wildcard_x=152; wildcard_y=350 ;;
    computer) wildcard_x=250; wildcard_y=350 ;;
    projectdirectory) wildcard_x=220; wildcard_y=385 ;;
    projectdirectorynumbered) wildcard_x=400; wildcard_y=385 ;;
    projectnotes) wildcard_x=178; wildcard_y=420 ;;
    projectnotesnamed) wildcard_x=343; wildcard_y=420 ;;
    bpm) wildcard_x=90; wildcard_y=455 ;;
    tempo) wildcard_x=165; wildcard_y=455 ;;
    playrate) wildcard_x=250; wildcard_y=455 ;;
    timesignature) wildcard_x=166; wildcard_y=490 ;;
    timesignum) wildcard_x=295; wildcard_y=490 ;;
    timesigdenom) wildcard_x=425; wildcard_y=490 ;;
    track) wildcard_x=89; wildcard_y=525 ;;
    tracknumber) wildcard_x=190; wildcard_y=525 ;;
    tracknamenumber|tracknameornumber) wildcard_x=347; wildcard_y=525 ;;
    seltrack) wildcard_x=466; wildcard_y=525 ;;
    region) wildcard_x=138; wildcard_y=560 ;;
    regionnumber) wildcard_x=248; wildcard_y=560 ;;
    regionnumberpadded) wildcard_x=410; wildcard_y=560 ;;
    regionname) wildcard_x=550; wildcard_y=560 ;;
    regionnamedvalue) wildcard_x=171; wildcard_y=595 ;;
    marker) wildcard_x=130; wildcard_y=630 ;;
    markernumber) wildcard_x=244; wildcard_y=630 ;;
    markernumberpadded) wildcard_x=410; wildcard_y=630 ;;
    markername) wildcard_x=550; wildcard_y=630 ;;
    markernamedvalue) wildcard_x=171; wildcard_y=665 ;;
    item) wildcard_x=117; wildcard_y=700 ;;
    itemnumber) wildcard_x=242; wildcard_y=700 ;;
    start) wildcard_x=126; wildcard_y=735 ;;
    end) wildcard_x=214; wildcard_y=735 ;;
    length) wildcard_x=307; wildcard_y=735 ;;
    startbeats) wildcard_x=115; wildcard_y=770 ;;
    endbeats) wildcard_x=220; wildcard_y=770 ;;
    lengthbeats) wildcard_x=335; wildcard_y=770 ;;
    starttc) wildcard_x=125; wildcard_y=805 ;;
    endtc) wildcard_x=218; wildcard_y=805 ;;
    startframes) wildcard_x=122; wildcard_y=840 ;;
    endframes) wildcard_x=238; wildcard_y=840 ;;
    lengthframes) wildcard_x=360; wildcard_y=840 ;;
    startseconds) wildcard_x=122; wildcard_y=875 ;;
    endseconds) wildcard_x=238; wildcard_y=875 ;;
    lengthseconds) wildcard_x=360; wildcard_y=875 ;;
    lenhh) wildcard_x=156; wildcard_y=910 ;;
    lenmm) wildcard_x=230; wildcard_y=910 ;;
    lenss) wildcard_x=304; wildcard_y=910 ;;
    lentt) wildcard_x=376; wildcard_y=910 ;;
    format) wildcard_x=134; wildcard_y=945 ;;
    samplerate) wildcard_x=215; wildcard_y=945 ;;
    sampleratek) wildcard_x=324; wildcard_y=945 ;;
    bitdepth) wildcard_x=436; wildcard_y=945 ;;
    channels) wildcard_x=102; wildcard_y=980 ;;
    fx) wildcard_x=90; wildcard_y=1013 ;;
    fxdelimited) wildcard_x=143; wildcard_y=1013 ;;
    trackslashes) wildcard_x=120; wildcard_y=1047 ;;
    folders) wildcard_x=260; wildcard_y=1047 ;;
    foldersdelimited) wildcard_x=350; wildcard_y=1047 ;;
    parent) wildcard_x=470; wildcard_y=1047 ;;
    date) wildcard_x=112; wildcard_y=1082 ;;
    time) wildcard_x=184; wildcard_y=1082 ;;
    datetime) wildcard_x=276; wildcard_y=1082 ;;
    year) wildcard_x=110; wildcard_y=1117 ;;
    year2) wildcard_x=180; wildcard_y=1117 ;;
    month) wildcard_x=255; wildcard_y=1117 ;;
    monthname) wildcard_x=350; wildcard_y=1117 ;;
    day) wildcard_x=470; wildcard_y=1117 ;;
    dayname) wildcard_x=545; wildcard_y=1117 ;;
    hour) wildcard_x=90; wildcard_y=1152 ;;
    hour12) wildcard_x=160; wildcard_y=1152 ;;
    ampm) wildcard_x=235; wildcard_y=1152 ;;
    minute) wildcard_x=310; wildcard_y=1152 ;;
    second) wildcard_x=400; wildcard_y=1152 ;;
    localtime) wildcard_x=510; wildcard_y=1152 ;;
    uniqueid) wildcard_x=180; wildcard_y=1187 ;;
    *) printf 'Unknown AAADAW_INSERT_WILDCARD token: %s\n' "$insert_wildcard" >&2; exit 2 ;;
  esac
  if [[ "$scroll_wildcards" == 0 ]] && (( wildcard_y > 720 )); then
    scroll_wildcards=$(((wildcard_y - 550) / 120))
    [[ "$scroll_wildcards" == 0 ]] && scroll_wildcards=1
  fi
  if [[ "$scroll_wildcards" =~ ^[1-9][0-9]*$ ]]; then
    "$xdotool" mousemove --sync 550 680 click --repeat "$scroll_wildcards" --delay 50 5
    sleep 0.3
  fi
  if [[ "$scroll_wildcards" =~ ^[1-9][0-9]*$ ]]; then
    wildcard_y=$((wildcard_y - 120 * scroll_wildcards))
  fi
  "$xdotool" mousemove --sync "$wildcard_x" "$wildcard_y" click 1
  sleep 0.4
elif [[ "${AAADAW_OPEN_WILDCARDS:-0}" == 1 && "$scroll_wildcards" =~ ^[1-9][0-9]*$ ]]; then
  # Allow a scrolled-picker screenshot without selecting a token and closing it.
  "$xdotool" mousemove --sync 550 680 click --repeat "$scroll_wildcards" --delay 50 5
  sleep 0.3
fi
if [[ "${AAADAW_TOGGLE_ALLOW_SLASH:-0}" == 1 ]]; then
  [[ "${AAADAW_OPEN_WILDCARDS:-0}" == 1 ]] || {
    printf 'AAADAW_TOGGLE_ALLOW_SLASH requires AAADAW_OPEN_WILDCARDS=1.\n' >&2
    exit 2
  }
  [[ "$scroll_wildcards" =~ ^[1-9][0-9]*$ ]] || {
    printf 'AAADAW_TOGGLE_ALLOW_SLASH requires a scrolled picker showing Options.\n' >&2
    exit 2
  }
  "$xdotool" mousemove --sync 80 495 click 1
  sleep 0.3
fi
xwininfo -id "$render_window" >"$run_dir/render-window.txt"
import -display "$display" -window root "$output"
printf 'display\t%s\n' "$display"
printf 'build_id\t%s\n' "$(readelf -n "$binary" | sed -n 's/.*Build ID: //p' | head -1)"
printf 'binary_sha256\t%s\n' "$(sha256sum "$binary" | cut -d ' ' -f 1)"
printf 'render_window\t%s\n' "$(cat "$run_dir/render-window.txt" | rg 'Width:|Height:' | tr '\n' ' ')"
printf 'screenshot\t%s\n' "$output"
printf 'screenshot_sha256\t%s\n' "$(sha256sum "$output" | cut -d ' ' -f 1)"
printf 'run_logs\t%s\n' "$run_dir"
