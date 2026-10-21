#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
binary=${1:-"$repo_root/target/debug/aaadaw"}
output=${2:-"$repo_root/target/aaadaw-desktop-smoke.png"}

if [[ ! -x "$binary" ]]; then
  printf 'AAADAW executable is missing or not executable: %s\n' "$binary" >&2
  exit 2
fi
for utility in xwininfo rg import identify sha256sum; do
  if ! command -v "$utility" >/dev/null 2>&1; then
    printf 'Required screenshot utility is missing: %s\n' "$utility" >&2
    exit 2
  fi
done

xvfb=${AAADAW_XVFB:-}
if [[ -z "$xvfb" ]]; then
  xvfb=$(command -v Xvfb || true)
fi
if [[ -z "$xvfb" && -x "$repo_root/target/reaper782/root/usr/bin/Xvfb" ]]; then
  xvfb="$repo_root/target/reaper782/root/usr/bin/Xvfb"
fi
if [[ -z "$xvfb" && -x /tmp/aaadaw-reaper-782/root/usr/bin/Xvfb ]]; then
  xvfb=/tmp/aaadaw-reaper-782/root/usr/bin/Xvfb
fi
if [[ -z "$xvfb" || ! -x "$xvfb" ]]; then
  printf 'Xvfb is unavailable; install it or set AAADAW_XVFB to its executable path.\n' >&2
  exit 2
fi

mkdir -p "$repo_root/target" "$(dirname -- "$output")"
run_dir=$(mktemp -d "$repo_root/target/aaadaw-desktop-smoke.XXXXXX")
mkdir -p "$run_dir/config" "$run_dir/data" "$run_dir/state" "$run_dir/cache" "$run_dir/runtime"
chmod 700 "$run_dir/runtime"

display=
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
if [[ -z "$display" ]]; then
  printf 'No free X11 display number was found in :90..:599.\n' >&2
  exit 2
fi

xvfb_pid=
app_pid=
cleanup() {
  if [[ -n "$app_pid" ]]; then
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
  fi
  if [[ -n "$xvfb_pid" ]]; then
    kill "$xvfb_pid" 2>/dev/null || true
    wait "$xvfb_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

"$xvfb" "$display" -screen 0 1280x800x24 -ac -nolisten tcp >"$run_dir/xvfb.log" 2>&1 &
xvfb_pid=$!
ready=0
for _ in $(seq 1 100); do
  if ! kill -0 "$xvfb_pid" 2>/dev/null; then
    cat "$run_dir/xvfb.log" >&2
    exit 1
  fi
  if DISPLAY="$display" xwininfo -root >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.1
done
if [[ "$ready" != 1 ]]; then
  printf 'Xvfb did not become ready on %s.\n' "$display" >&2
  cat "$run_dir/xvfb.log" >&2
  exit 1
fi

export DISPLAY="$display"
export WGPU_BACKEND=${WGPU_BACKEND:-gl}
export XDG_CONFIG_HOME="$run_dir/config"
export XDG_DATA_HOME="$run_dir/data"
export XDG_STATE_HOME="$run_dir/state"
export XDG_CACHE_HOME="$run_dir/cache"
export XDG_RUNTIME_DIR="$run_dir/runtime"
"$binary" >"$run_dir/aaadaw.log" 2>&1 &
app_pid=$!

ready=0
for _ in $(seq 1 150); do
  if ! kill -0 "$app_pid" 2>/dev/null; then
    cat "$run_dir/aaadaw.log" >&2
    exit 1
  fi
  if xwininfo -root -tree 2>/dev/null | rg -q 'AAADAW'; then
    ready=1
    break
  fi
  sleep 0.2
done
if [[ "$ready" != 1 ]]; then
  printf 'AAADAW did not create a visible X11 window on %s.\n' "$display" >&2
  cat "$run_dir/aaadaw.log" >&2
  exit 1
fi

# The X11 window is mapped slightly before its first GPU frame is presented.
ready=0
for _ in $(seq 1 50); do
  import -display "$display" -window root "$run_dir/frame.png"
  colors=$(identify -format '%k' "$run_dir/frame.png")
  if [[ "$colors" -gt 1 ]]; then
    cp "$run_dir/frame.png" "$output"
    ready=1
    break
  fi
  sleep 0.2
done
if [[ "$ready" != 1 ]]; then
  printf 'AAADAW window appeared, but no rendered frame was captured on %s.\n' "$display" >&2
  cat "$run_dir/aaadaw.log" >&2
  exit 1
fi
printf 'display\t%s\n' "$display"
printf 'binary\t%s\n' "$binary"
printf 'binary_sha256\t%s\n' "$(sha256sum "$binary" | cut -d ' ' -f 1)"
printf 'window\tAAADAW main window visible\n'
printf 'screenshot\t%s\n' "$output"
printf 'screenshot_sha256\t%s\n' "$(sha256sum "$output" | cut -d ' ' -f 1)"
printf 'run_logs\t%s\n' "$run_dir"
