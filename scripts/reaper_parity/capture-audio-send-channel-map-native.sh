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
[[ -x "$reaper" && -x "$xdotool" && -x "$xvfb" ]] || { printf 'REAPER/X11 capture executable is missing.\n' >&2; exit 2; }

mkdir -p "$fixtures"
run_dir=$(mktemp -d "$repo_root/target/reaper-audio-send-channel-map.XXXXXX")
profile=${AAADAW_REAPER_CAPTURE_PROFILE:-"$run_dir"}
mkdir -p "$profile/config/Effects/JSFX" "$profile/data" "$profile/state" "$profile/cache" "$profile/runtime" "$run_dir/output"
chmod 700 "$profile/runtime"
cp -- "$repo_root/scripts/reaper_parity/jsfx/aaadaw-audio-channel-map-source.jsfx" "$profile/config/Effects/JSFX/aaadaw-audio-channel-map-source.jsfx"
cp -- "$repo_root/scripts/reaper_parity/jsfx/aaadaw-audio-channel-map-destination.jsfx" "$profile/config/Effects/JSFX/aaadaw-audio-channel-map-destination.jsfx"
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
  "$repo_root/scripts/reaper_parity/probe-audio-send-channel-map.lua" >"$run_dir/reaper.log" 2>&1 &
reaper_pid=$!
for _ in $(seq 1 300); do
  if [[ -s "$run_dir/output/audio-send-channel-map-capture-complete" ]]; then break; fi
  device_dialog=$("$xdotool" search --onlyvisible --name 'Error opening devices' | head -1 || true)
  if [[ -n "$device_dialog" ]]; then
    "$xdotool" windowraise "$device_dialog" || true
    "$xdotool" mousemove 20 175 click 1 || true
    sleep 0.2
    "$xdotool" mousemove 435 205 click 1 || true
  fi
  sleep 0.1
done
[[ -s "$run_dir/output/audio-send-channel-map-capture-complete" ]] || {
  cat "$run_dir/reaper.log" >&2
  import -display "$display" -window root "$run_dir/native-timeout.png" || true
  printf 'REAPER did not complete the audio Send channel-map probe.\n' >&2
  exit 1
}
sleep 8
for _ in 1 2 3; do
  about_dialog=$("$xdotool" search --onlyvisible --name 'About REAPER v7.82' | head -1 || true)
  if [[ -z "$about_dialog" ]]; then break; fi
  "$xdotool" windowraise "$about_dialog" || true
  "$xdotool" mousemove 490 396 click 1 || true
  sleep 0.5
done
main_window=
for window in $("$xdotool" search --onlyvisible --name 'REAPER v7\.82' || true); do
  window_name=$("$xdotool" getwindowname "$window" 2>/dev/null || true)
  [[ "$window_name" == About* ]] && continue
  main_window=$window
  break
done
if [[ -z "$main_window" ]]; then
  printf 'REAPER main window was not found after probe completion; visible windows were:\n' >&2
  for window in $("$xdotool" search --onlyvisible --name '.*' || true); do
    printf '%s\t%s\n' "$window" "$("$xdotool" getwindowname "$window" 2>/dev/null || true)" >&2
  done
  import -display "$display" -window root "$run_dir/native-window-search-failure.png" || true
  exit 1
fi
"$xdotool" windowraise "$main_window"
"$xdotool" windowmove "$main_window" 0 0
"$xdotool" windowsize "$main_window" 1280 800
sleep 0.5
"$xdotool" mousemove 211 645 click 1
sleep 1
import -display "$display" -window root "$fixtures/audio-send-channel-map-native.png"
identify "$fixtures/audio-send-channel-map-native.png"
sha256sum "$fixtures/audio-send-channel-map-native.png"
"$xdotool" mousemove 331 684 click 1
sleep 0.3
import -display "$display" -window root "$fixtures/audio-send-channel-map-native-source-choices.png"
"$xdotool" key Escape
identify "$fixtures/audio-send-channel-map-native-source-choices.png"
sha256sum "$fixtures/audio-send-channel-map-native-source-choices.png"
for suffix in native.tsv before.rpp after.rpp; do
  source="$run_dir/output/audio-send-channel-map-$suffix"
  target="$fixtures/audio-send-channel-map-$suffix"
  [[ -s "$source" ]] || { printf 'Probe artifact is missing: %s\n' "$source" >&2; exit 1; }
  cp -- "$source" "$target"
  sha256sum "$target"
done
{
  printf 'reference\tLinux REAPER 7.82 / x86_64 / Default 7 factory configuration\n'
  printf 'capture\t1280x800 isolated Xvfb desktop, Audio Send channel selectors visible\n'
  printf 'reaper_binary_sha256\t%s\n' "$(sha256sum "$reaper" | awk '{print $1}')"
  printf 'probe_script_sha256\t%s\n' "$(sha256sum "$repo_root/scripts/reaper_parity/probe-audio-send-channel-map.lua" | awk '{print $1}')"
  printf 'capture_script_sha256\t%s\n' "$(sha256sum "$repo_root/scripts/reaper_parity/capture-audio-send-channel-map-native.sh" | awk '{print $1}')"
  printf 'source_jsfx_sha256\t%s\n' "$(sha256sum "$repo_root/scripts/reaper_parity/jsfx/aaadaw-audio-channel-map-source.jsfx" | awk '{print $1}')"
  printf 'destination_jsfx_sha256\t%s\n' "$(sha256sum "$repo_root/scripts/reaper_parity/jsfx/aaadaw-audio-channel-map-destination.jsfx" | awk '{print $1}')"
  for artifact in audio-send-channel-map-native.tsv audio-send-channel-map-before.rpp audio-send-channel-map-after.rpp audio-send-channel-map-native.png audio-send-channel-map-native-source-choices.png; do
    printf '%s_sha256\t%s\n' "$artifact" "$(sha256sum "$fixtures/$artifact" | awk '{print $1}')"
  done
} >"$fixtures/audio-send-channel-map-native-evidence.tsv"
cat "$fixtures/audio-send-channel-map-native-evidence.tsv"
printf 'native_run_dir\t%s\nnative_display\t%s\n' "$run_dir" "$display"
