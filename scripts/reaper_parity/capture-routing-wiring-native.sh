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
  command -v "$utility" >/dev/null || { printf 'Required capture utility is missing: %s\n' "$utility" >&2; exit 2; }
done
[[ -x "$reaper" && -x "$xdotool" && -x "$xvfb" ]] || { printf 'REAPER/X11 capture executable is missing.\n' >&2; exit 2; }

mkdir -p "$fixtures"
run_dir=$(mktemp -d "$repo_root/target/reaper-routing-wiring.XXXXXX")
profile="$run_dir/profile"
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
  "$repo_root/scripts/reaper_parity/probe-routing-wiring.lua" >"$run_dir/reaper.log" 2>&1 &
reaper_pid=$!
for _ in $(seq 1 200); do
  if [[ -s "$run_dir/output/routing-wiring-native-ready" ]]; then break; fi
  sleep 0.1
done
[[ -s "$run_dir/output/routing-wiring-native-ready" ]] || {
  cat "$run_dir/reaper.log" >&2
  import -display "$display" -window root "$run_dir/native-timeout.png" || true
  printf 'REAPER did not open the Routing Matrix.\n' >&2
  exit 1
}

docker_window=$("$xdotool" search --onlyvisible --name '^Docker$' | head -1 || true)
[[ -n "$docker_window" ]] || { printf 'REAPER Docker window was not found.\n' >&2; exit 1; }
"$xdotool" windowraise "$docker_window"
"$xdotool" windowmove "$docker_window" 0 0
"$xdotool" windowsize "$docker_window" 1280 800
sleep 0.5
import -display "$display" -window root "$fixtures/routing-wiring-matrix-native.png"

"$xdotool" mousemove 90 18 click 1
sleep 0.2
"$xdotool" mousemove 52 62 click 1
sleep 0.5
import -display "$display" -window root "$fixtures/routing-wiring-native.png"
for artifact in routing-wiring-matrix-native.png routing-wiring-native.png; do
  identify "$fixtures/$artifact"
  sha256sum "$fixtures/$artifact"
done

for suffix in routing-wiring-native.tsv routing-wiring-native.rpp; do
  source="$run_dir/output/$suffix"
  target="$fixtures/$suffix"
  [[ -s "$source" ]] || { printf 'Probe artifact is missing: %s\n' "$source" >&2; exit 1; }
  cp -- "$source" "$target"
  sha256sum "$target"
done

manifest="$fixtures/routing-wiring-native-evidence.tsv"
reaper_reference=${reaper#"$repo_root"/}
{
  printf 'reference\tLinux REAPER 7.82 / x86_64 / Default 7 factory configuration\n'
  printf 'reaper_binary\t%s\n' "$reaper_reference"
  printf 'reaper_binary_sha256\t%s\n' "$(sha256sum "$reaper" | awk '{print $1}')"
  printf 'capture\t1280x800 isolated Xvfb; native Audio device closed\n'
  printf 'probe_script_sha256\t%s\n' "$(sha256sum "$repo_root/scripts/reaper_parity/probe-routing-wiring.lua" | awk '{print $1}')"
  printf 'capture_script_sha256\t%s\n' "$(sha256sum "$repo_root/scripts/reaper_parity/capture-routing-wiring-native.sh" | awk '{print $1}')"
  for artifact in routing-wiring-native.tsv routing-wiring-native.rpp routing-wiring-matrix-native.png routing-wiring-native.png; do
    printf '%s_sha256\t%s\n' "$artifact" "$(sha256sum "$fixtures/$artifact" | awk '{print $1}')"
  done
} >"$manifest"
cat "$manifest"
printf 'native_run_dir\t%s\nnative_display\t%s\n' "$run_dir" "$display"
