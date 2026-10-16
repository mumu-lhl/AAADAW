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
run_dir=$(mktemp -d "$repo_root/target/aaadaw-render-wildcards.XXXXXX")
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
case "$insert_wildcard" in
  format|samplerate|sampleratek|bitdepth|channels)
    [[ "$scroll_wildcards" == 0 ]] && scroll_wildcards=2
    ;;
  fx|fxdelimited)
    [[ "$scroll_wildcards" == 0 ]] && scroll_wildcards=3
    ;;
  trackslashes|folders|foldersdelimited|parent)
    [[ "$scroll_wildcards" == 0 ]] && scroll_wildcards=4
    ;;
esac
if [[ "$scroll_wildcards" =~ ^[1-9][0-9]*$ ]]; then
  "$xdotool" mousemove --sync 550 680 click --repeat "$scroll_wildcards" --delay 50 5
  sleep 0.3
fi
if [[ -n "$insert_wildcard" ]]; then
  case "$insert_wildcard" in
    project) wildcard_x=163; wildcard_y=316 ;;
    title) wildcard_x=236; wildcard_y=316 ;;
    author) wildcard_x=310; wildcard_y=316 ;;
    projectdirectory) wildcard_x=220; wildcard_y=350 ;;
    projectdirectorynumbered) wildcard_x=400; wildcard_y=350 ;;
    projectnotes) wildcard_x=178; wildcard_y=385 ;;
    projectnotesnamed) wildcard_x=343; wildcard_y=385 ;;
    bpm) wildcard_x=90; wildcard_y=420 ;;
    tempo) wildcard_x=165; wildcard_y=420 ;;
    timesignature) wildcard_x=166; wildcard_y=455 ;;
    timesignum) wildcard_x=295; wildcard_y=455 ;;
    timesigdenom) wildcard_x=425; wildcard_y=455 ;;
    track) wildcard_x=89; wildcard_y=490 ;;
    tracknumber) wildcard_x=190; wildcard_y=490 ;;
    tracknamenumber|tracknameornumber) wildcard_x=347; wildcard_y=490 ;;
    seltrack) wildcard_x=466; wildcard_y=490 ;;
    region) wildcard_x=138; wildcard_y=525 ;;
    regionnumber) wildcard_x=248; wildcard_y=525 ;;
    regionnumberpadded) wildcard_x=410; wildcard_y=525 ;;
    regionname) wildcard_x=550; wildcard_y=525 ;;
    regionnamedvalue) wildcard_x=171; wildcard_y=560 ;;
    marker) wildcard_x=130; wildcard_y=595 ;;
    markernumber) wildcard_x=244; wildcard_y=595 ;;
    markernumberpadded) wildcard_x=410; wildcard_y=595 ;;
    markername) wildcard_x=550; wildcard_y=595 ;;
    markernamedvalue) wildcard_x=171; wildcard_y=630 ;;
    item) wildcard_x=117; wildcard_y=665 ;;
    itemnumber) wildcard_x=242; wildcard_y=665 ;;
    start) wildcard_x=126; wildcard_y=700 ;;
    end) wildcard_x=214; wildcard_y=700 ;;
    length) wildcard_x=307; wildcard_y=700 ;;
    startbeats) wildcard_x=115; wildcard_y=735 ;;
    endbeats) wildcard_x=220; wildcard_y=735 ;;
    lengthbeats) wildcard_x=335; wildcard_y=735 ;;
    starttc) wildcard_x=125; wildcard_y=770 ;;
    endtc) wildcard_x=218; wildcard_y=770 ;;
    startframes) wildcard_x=122; wildcard_y=805 ;;
    endframes) wildcard_x=238; wildcard_y=805 ;;
    lengthframes) wildcard_x=360; wildcard_y=805 ;;
    startseconds) wildcard_x=122; wildcard_y=840 ;;
    endseconds) wildcard_x=238; wildcard_y=840 ;;
    lengthseconds) wildcard_x=360; wildcard_y=840 ;;
    lenhh) wildcard_x=156; wildcard_y=875 ;;
    lenmm) wildcard_x=230; wildcard_y=875 ;;
    lenss) wildcard_x=304; wildcard_y=875 ;;
    lentt) wildcard_x=376; wildcard_y=875 ;;
    format) wildcard_x=134; wildcard_y=910 ;;
    samplerate) wildcard_x=215; wildcard_y=910 ;;
    sampleratek) wildcard_x=324; wildcard_y=910 ;;
    bitdepth) wildcard_x=436; wildcard_y=910 ;;
    channels) wildcard_x=102; wildcard_y=945 ;;
    fx) wildcard_x=90; wildcard_y=978 ;;
    fxdelimited) wildcard_x=143; wildcard_y=978 ;;
    trackslashes) wildcard_x=120; wildcard_y=1012 ;;
    folders) wildcard_x=260; wildcard_y=1012 ;;
    foldersdelimited) wildcard_x=350; wildcard_y=1012 ;;
    parent) wildcard_x=470; wildcard_y=1012 ;;
    *) printf 'Unknown AAADAW_INSERT_WILDCARD token: %s\n' "$insert_wildcard" >&2; exit 2 ;;
  esac
  if [[ "$scroll_wildcards" =~ ^[1-9][0-9]*$ ]]; then
    wildcard_y=$((wildcard_y - 120 * scroll_wildcards))
  fi
  "$xdotool" mousemove --sync "$wildcard_x" "$wildcard_y" click 1
  sleep 0.4
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
