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
run_dir=$(mktemp -d "$repo_root/target/reaper-mixed-audio-midi-implode.XXXXXX")
mkdir -p "$run_dir/output"

read -r -a target_kinds <<< "${AAADAW_REAPER_CAPTURE_TARGETS:-audio midi}"

for target_kind in "${target_kinds[@]}"; do
  [[ "$target_kind" == audio || "$target_kind" == midi ]] || {
    printf 'Unsupported mixed-media target kind: %s\n' "$target_kind" >&2
    exit 2
  }
  prefix="mixed-audio-midi-implode-$target_kind-target"
  profile=${AAADAW_REAPER_CAPTURE_PROFILE:-"$run_dir/profiles/$target_kind"}
  mkdir -p "$profile/config" "$profile/data" "$profile/state" "$profile/cache" "$profile/runtime"
  chmod 700 "$profile/runtime"

  display=
  for number in $(seq 800 899); do
    candidate=":$number"
    if [[ ! -e "/tmp/.X$number-lock" && ! -S "/tmp/.X11-unix/X$number" ]]; then display=$candidate; break; fi
  done
  [[ -n "$display" ]] || { printf 'No free X11 display was found.\n' >&2; exit 1; }

  xvfb_pid=
  reaper_pid=
  cleanup_display() {
    if [[ -n "$reaper_pid" ]]; then kill "$reaper_pid" 2>/dev/null || true; wait "$reaper_pid" 2>/dev/null || true; fi
    if [[ -n "$xvfb_pid" ]]; then kill "$xvfb_pid" 2>/dev/null || true; wait "$xvfb_pid" 2>/dev/null || true; fi
  }
  trap 'cleanup_display' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  "$xvfb" "$display" -screen 0 1280x800x24 -ac -nolisten tcp >"$run_dir/$target_kind-xvfb.log" 2>&1 &
  xvfb_pid=$!
  export DISPLAY="$display" AAADAW_REAPER_PROBE_DIR="$run_dir/output"
  export AAADAW_REAPER_PROBE_MEDIA_A="$repo_root/examples/first-project/melody.wav"
  export AAADAW_REAPER_PROBE_TARGET="$target_kind"
  export XDG_CONFIG_HOME="$profile/config" XDG_DATA_HOME="$profile/data"
  export XDG_STATE_HOME="$profile/state" XDG_CACHE_HOME="$profile/cache" XDG_RUNTIME_DIR="$profile/runtime"
  export LD_LIBRARY_PATH="$repo_root/.rtk-lib:$runtime_root/../lib/x86_64-linux-gnu:/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

  for _ in $(seq 1 100); do
    if xwininfo -display "$display" -root >/dev/null 2>&1; then break; fi
    sleep 0.1
  done
  "$reaper" -cfgfile "$profile/config/reaper.ini" -newinst -nosplash \
    "$repo_root/scripts/reaper_parity/probe-item-implode-mixed-audio-midi-takes.lua" \
    >"$run_dir/$target_kind-reaper.log" 2>&1 &
  reaper_pid=$!

  for _ in $(seq 1 200); do
    if [[ -s "$run_dir/output/$prefix-complete" ]]; then break; fi
    sleep 0.1
  done
  [[ -s "$run_dir/output/$prefix-complete" ]] || {
    cat "$run_dir/$target_kind-reaper.log" >&2
    printf 'REAPER did not complete the mixed-media %s-target probe.\n' "$target_kind" >&2
    exit 1
  }

  sleep 0.8
  modal=$("$xdotool" search --onlyvisible --name 'Error opening devices' | head -1 || true)
  if [[ -n "$modal" ]]; then
    "$xdotool" mousemove 20 175 click 1
    sleep 0.2
    "$xdotool" mousemove 435 205 click 1
  fi
  sleep 8
  "$xdotool" mousemove 490 396 click 1
  sleep 0.5
  "$xdotool" mousemove 490 396 click 1
  sleep 0.5
  main_window=$("$xdotool" search --onlyvisible --name 'REAPER v7.82' | head -1 || true)
  [[ -n "$main_window" ]] || { printf 'REAPER %s-target window was not found.\n' "$target_kind" >&2; exit 1; }
  "$xdotool" windowmove "$main_window" 0 0
  "$xdotool" windowsize "$main_window" 1280 800
  sleep 0.5
  import -display "$display" -window root "$run_dir/$prefix-native-after.png"

  for suffix in native.tsv before.rpp after.rpp undo.rpp redo.rpp; do
    source="$run_dir/output/$prefix-$suffix"
    target="$fixtures/$prefix-$suffix"
    [[ -s "$source" ]] || { printf 'Probe artifact is missing: %s\n' "$source" >&2; exit 1; }
    cp -- "$source" "$target"
    sha256sum "$target"
  done
  cp -- "$run_dir/$prefix-native-after.png" "$fixtures/$prefix-native-after.png"
  identify "$fixtures/$prefix-native-after.png"
  sha256sum "$fixtures/$prefix-native-after.png"
  cleanup_display
  xvfb_pid=
  reaper_pid=
done

printf 'native_run_dir\t%s\nnative_targets\t%s\n' "$run_dir" "${target_kinds[*]}"
