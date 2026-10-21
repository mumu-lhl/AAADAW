#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
reaper=${AAADAW_REAPER_BIN:-"$repo_root/target/reaper-7.82/reaper_linux_x86_64/REAPER/reaper"}
fixtures=${1:-"$repo_root/docs/verification/reaper-parity/fixtures"}
runtime_root=${AAADAW_XVFB_RUNTIME:-"$repo_root/target/reaper782/root/usr/bin"}
if [[ ! -x "$runtime_root/Xvfb" ]]; then runtime_root=/tmp/aaadaw-xvfb-apt/root/usr/bin; fi
if [[ ! -x "$runtime_root/Xvfb" ]]; then runtime_root=/workspace/.aaadaw-env/sysroot/usr/bin; fi
xvfb=${AAADAW_XVFB:-"$runtime_root/Xvfb"}

[[ -x "$reaper" && -x "$xvfb" ]] || { printf 'REAPER/Xvfb executable is missing.\n' >&2; exit 2; }
mkdir -p "$fixtures"
run_dir=$(mktemp -d "$repo_root/target/reaper-hardware-output-native.XXXXXX")
profile="$run_dir/profile"
mkdir -p "$profile/config" "$profile/data" "$profile/state" "$profile/cache" "$profile/runtime" "$run_dir/output"
chmod 700 "$profile/runtime"

display=
for number in $(seq 590 699); do
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
export LD_LIBRARY_PATH="$runtime_root/../lib/x86_64-linux-gnu:/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
for _ in $(seq 1 100); do
  if xwininfo -display "$display" -root >/dev/null 2>&1; then break; fi
  sleep 0.1
done
"$reaper" -cfgfile "$profile/config/reaper.ini" -newinst -nosplash \
  "$repo_root/scripts/reaper_parity/probe-hardware-output.lua" >"$run_dir/reaper.log" 2>&1 &
reaper_pid=$!
for _ in $(seq 1 200); do
  [[ -s "$run_dir/output/hardware-output-capture-complete" ]] && break
  sleep 0.1
done
[[ -s "$run_dir/output/hardware-output-capture-complete" ]] || {
  cat "$run_dir/reaper.log" >&2
  printf 'REAPER did not complete the Hardware Output probe.\n' >&2
  exit 1
}

for suffix in native.tsv after.rpp mono_2_to_mono_1.rpp; do
  source="$run_dir/output/hardware-output-$suffix"
  case "$suffix" in
    native.tsv) target="$fixtures/hardware-output-native.tsv" ;;
    after.rpp) target="$fixtures/hardware-output-native-after.rpp" ;;
    mono_2_to_mono_1.rpp) target="$fixtures/hardware-output-native-mono_2_to_mono_1.rpp" ;;
  esac
  [[ -s "$source" ]] || { printf 'Probe artifact is missing: %s\n' "$source" >&2; exit 1; }
  cp -- "$source" "$target"
done
{
  printf 'reference\tLinux REAPER 7.82 / x86_64 / Default 7 factory configuration\n'
  printf 'capture\tIsolated unsaved project; ReaScript API property readback and saved RPP state\n'
  printf 'reaper_binary_sha256\t%s\n' "$(sha256sum "$reaper" | awk '{print $1}')"
  printf 'probe_script_sha256\t%s\n' "$(sha256sum "$repo_root/scripts/reaper_parity/probe-hardware-output.lua" | awk '{print $1}')"
  printf 'capture_script_sha256\t%s\n' "$(sha256sum "$repo_root/scripts/reaper_parity/capture-hardware-output-native.sh" | awk '{print $1}')"
  for artifact in hardware-output-native.tsv hardware-output-native-after.rpp hardware-output-native-mono_2_to_mono_1.rpp; do
    printf '%s_sha256\t%s\n' "$artifact" "$(sha256sum "$fixtures/$artifact" | awk '{print $1}')"
  done
} >"$fixtures/hardware-output-native-evidence.tsv"
cat "$fixtures/hardware-output-native.tsv"
cat "$fixtures/hardware-output-native-evidence.tsv"
