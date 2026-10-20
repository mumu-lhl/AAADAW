#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
reaper=${AAADAW_REAPER_BIN:-"$repo_root/target/reaper-7.82/reaper_linux_x86_64/REAPER/reaper"}
fixtures=${1:-"$repo_root/docs/verification/reaper-parity/fixtures"}
runtime_root=${AAADAW_XVFB_RUNTIME:-"$repo_root/target/reaper782/root/usr/bin"}
if [[ ! -x "$runtime_root/Xvfb" ]]; then runtime_root=/tmp/aaadaw-xvfb-apt/root/usr/bin; fi
xdotool=${AAADAW_XDOTOOL:-"$runtime_root/xdotool"}
xvfb=${AAADAW_XVFB:-"$runtime_root/Xvfb"}

for utility in import identify sha256sum xwininfo awk; do
  command -v "$utility" >/dev/null || { printf 'Required screenshot utility is missing: %s\n' "$utility" >&2; exit 2; }
done
[[ -x "$reaper" && -x "$xdotool" && -x "$xvfb" ]] || { printf 'REAPER/X11 capture executable is missing.\n' >&2; exit 2; }

mkdir -p "$fixtures"
run_dir=$(mktemp -d "$repo_root/target/reaper-routing-matrix.XXXXXX")
profile=${AAADAW_REAPER_CAPTURE_PROFILE:-"$run_dir"}
mkdir -p "$profile/config" "$profile/data" "$profile/state" "$profile/cache" "$profile/runtime" "$run_dir/output"
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
  "$repo_root/scripts/reaper_parity/probe-routing-matrix.lua" >"$run_dir/reaper.log" 2>&1 &
reaper_pid=$!
for _ in $(seq 1 200); do
  if [[ -s "$run_dir/output/routing-matrix-native-capture-complete" ]]; then break; fi
  sleep 0.1
done
[[ -s "$run_dir/output/routing-matrix-native-capture-complete" ]] || {
  cat "$run_dir/reaper.log" >&2
  import -display "$display" -window root "$run_dir/native-timeout.png" || true
  printf 'REAPER did not open the Routing Matrix.\n' >&2
  exit 1
}
sleep 5
device_dialog=$("$xdotool" search --onlyvisible --name 'Error opening devices' | head -1 || true)
if [[ -n "$device_dialog" ]]; then
  "$xdotool" windowraise "$device_dialog" || true
  "$xdotool" mousemove 20 175 click 1 || true
  sleep 0.2
  "$xdotool" mousemove 435 205 click 1 || true
fi
about_dialog=$("$xdotool" search --onlyvisible --name 'About REAPER v7.82' | head -1 || true)
if [[ -n "$about_dialog" ]]; then
  "$xdotool" windowraise "$about_dialog" || true
  sleep 0.2
  "$xdotool" mousemove 490 396 click 1 || true
fi
sleep 1
docker_window=$("$xdotool" search --onlyvisible --name '^Docker$' | head -1 || true)
[[ -n "$docker_window" ]] || { printf 'REAPER Docker window was not found.\n' >&2; exit 1; }
"$xdotool" windowraise "$docker_window"
"$xdotool" windowmove "$docker_window" 0 0
"$xdotool" windowsize "$docker_window" 1280 800
sleep 1
import -display "$display" -window root "$fixtures/routing-matrix-native.png"
identify "$fixtures/routing-matrix-native.png"
sha256sum "$fixtures/routing-matrix-native.png"
for case in add open; do
  case "$case" in
    add) cell_x=207; cell_y=188 ;;
    open) cell_x=207; cell_y=188 ;;
  esac
  "$xdotool" mousemove "$cell_x" "$cell_y" click 1
  sleep 0.4
  : >"$run_dir/output/routing-matrix-native-click-$case"
  for _ in $(seq 1 100); do
    [[ -s "$run_dir/output/routing-matrix-native-observed-$case" ]] && break
    sleep 0.1
  done
  [[ -s "$run_dir/output/routing-matrix-native-observed-$case" ]] || {
    printf 'REAPER did not observe Routing Matrix %s cell click.\n' "$case" >&2
    exit 1
  }
  import -display "$display" -window root "$fixtures/routing-matrix-native-after-$case.png"
  identify "$fixtures/routing-matrix-native-after-$case.png"
  sha256sum "$fixtures/routing-matrix-native-after-$case.png"
done
"$xdotool" mousemove 568 201 click 1
sleep 0.4
: >"$run_dir/output/routing-matrix-native-click-delete"
for _ in $(seq 1 100); do
  [[ -s "$run_dir/output/routing-matrix-native-observed-delete" ]] && break
  sleep 0.1
done
[[ -s "$run_dir/output/routing-matrix-native-observed-delete" ]] || {
  printf 'REAPER did not observe the Send-window Delete button.\n' >&2
  exit 1
}
docker_window=$("$xdotool" search --onlyvisible --name '^Docker$' | head -1 || true)
[[ -n "$docker_window" ]] || { printf 'REAPER Docker window was not found after Send removal.\n' >&2; exit 1; }
"$xdotool" windowraise "$docker_window"
"$xdotool" windowmove "$docker_window" 0 0
"$xdotool" windowsize "$docker_window" 1280 800
sleep 0.3
import -display "$display" -window root "$fixtures/routing-matrix-native-after-delete.png"
identify "$fixtures/routing-matrix-native-after-delete.png"
sha256sum "$fixtures/routing-matrix-native-after-delete.png"
"$xdotool" mousemove 223 188 click 1
sleep 0.4
: >"$run_dir/output/routing-matrix-native-click-duplicate"
for _ in $(seq 1 100); do
  [[ -s "$run_dir/output/routing-matrix-native-observed-duplicate" ]] && break
  sleep 0.1
done
[[ -s "$run_dir/output/routing-matrix-native-observed-duplicate" ]] || {
  printf 'REAPER did not observe the duplicate-send matrix cell click.\n' >&2
  exit 1
}
import -display "$display" -window root "$fixtures/routing-matrix-native-after-duplicate.png"
identify "$fixtures/routing-matrix-native-after-duplicate.png"
sha256sum "$fixtures/routing-matrix-native-after-duplicate.png"
for suffix in native.tsv native-before.rpp native-after-add.rpp native-after-open.rpp native-after-delete.rpp native-after-duplicate.rpp; do
  source="$run_dir/output/routing-matrix-$suffix"
  target="$fixtures/routing-matrix-$suffix"
  [[ -s "$source" ]] || { printf 'Probe artifact is missing: %s\n' "$source" >&2; exit 1; }
  cp -- "$source" "$target"
  sha256sum "$target"
done
manifest="$fixtures/routing-matrix-native-evidence.tsv"
reaper_reference=${reaper#"$repo_root"/}
{
  printf 'reference\tLinux REAPER 7.82 / x86_64 / Default 7 factory configuration\n'
  printf 'reaper_binary\t%s\n' "$reaper_reference"
  printf 'reaper_binary_sha256\t%s\n' "$(sha256sum "$reaper" | awk '{print $1}')"
  printf 'capture\t1280x800 isolated Xvfb desktop\n'
  for artifact in \
    routing-matrix-native.tsv \
    routing-matrix-native-before.rpp \
    routing-matrix-native-after-add.rpp \
    routing-matrix-native-after-open.rpp \
    routing-matrix-native-after-delete.rpp \
    routing-matrix-native-after-duplicate.rpp \
    routing-matrix-native.png \
    routing-matrix-native-after-add.png \
    routing-matrix-native-after-open.png \
    routing-matrix-native-after-delete.png \
    routing-matrix-native-after-duplicate.png; do
    printf '%s_sha256\t%s\n' "$artifact" "$(sha256sum "$fixtures/$artifact" | awk '{print $1}')"
  done
} >"$manifest"
cat "$manifest"
printf 'native_run_dir\t%s\nnative_display\t%s\n' "$run_dir" "$display"
