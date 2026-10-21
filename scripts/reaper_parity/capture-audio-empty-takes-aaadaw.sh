#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
binary=${1:-"$repo_root/target/cranelift/debug/aaadaw"}
fixtures=${2:-"$repo_root/docs/verification/reaper-parity/fixtures"}
project=${AAADAW_PROJECT_PATH:-"$repo_root/docs/verification/reaper-parity/fixtures/audio-multi-take-aaadaw.aaadaw"}
runtime_root=${AAADAW_REAPER_RUNTIME:-"$repo_root/target/reaper782/root/usr/bin"}
if [[ ! -x "$runtime_root/Xvfb" ]]; then runtime_root=/tmp/aaadaw-xvfb-apt/root/usr/bin; fi
xdotool=${AAADAW_XDOTOOL:-"$runtime_root/xdotool"}
xvfb=${AAADAW_XVFB:-"$runtime_root/Xvfb"}

for utility in import identify sha256sum xwininfo; do
  command -v "$utility" >/dev/null || { printf 'Required screenshot utility is missing: %s\n' "$utility" >&2; exit 2; }
done
[[ -x "$binary" ]] || { printf 'AAADAW executable is missing: %s\n' "$binary" >&2; exit 2; }
[[ -x "$xdotool" ]] || { printf 'xdotool executable is missing: %s\n' "$xdotool" >&2; exit 2; }
[[ -x "$xvfb" ]] || { printf 'Xvfb executable is missing: %s\n' "$xvfb" >&2; exit 2; }
[[ -f "$project" ]] || { printf 'Audio project fixture is missing: %s\n' "$project" >&2; exit 2; }

mkdir -p "$fixtures"
run_dir=$(mktemp -d "$repo_root/target/aaadaw-audio-empty-takes-capture.XXXXXX")
mkdir -p "$run_dir/config/aaadaw" "$run_dir/data" "$run_dir/state" "$run_dir/cache" "$run_dir/runtime"
chmod 700 "$run_dir/runtime"
capture_project="$run_dir/$(basename -- "$project")"
cp -- "$project" "$capture_project"
printf 'item.select-all\tCtrl+Alt+Shift+A\ntake.add-empty-after-active\tCtrl+Shift+O\ntake.switch-next\tCtrl+Shift+N\n' \
  >"$run_dir/config/aaadaw/shortcuts.conf"
display=
for number in $(seq 590 699); do
  candidate=":$number"
  if [[ ! -e "/tmp/.X$number-lock" && ! -S "/tmp/.X11-unix/X$number" ]]; then display=$candidate; break; fi
done
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

"$xvfb" "$display" -screen 0 1280x800x24 -ac -nolisten tcp >"$run_dir/xvfb.log" 2>&1 &
xvfb_pid=$!
export DISPLAY="$display" WGPU_BACKEND=${WGPU_BACKEND:-gl}
export XDG_CONFIG_HOME="$run_dir/config" XDG_DATA_HOME="$run_dir/data"
export XDG_STATE_HOME="$run_dir/state" XDG_CACHE_HOME="$run_dir/cache" XDG_RUNTIME_DIR="$run_dir/runtime"
export LD_LIBRARY_PATH="$repo_root/.rtk-lib:$runtime_root/../lib/x86_64-linux-gnu:/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
for _ in $(seq 1 100); do
  if xwininfo -display "$display" -root >/dev/null 2>&1; then break; fi
  sleep 0.1
done
"$binary" "$capture_project" >"$run_dir/aaadaw.log" 2>&1 &
app_pid=$!

main_window=
for _ in $(seq 1 150); do
  main_window=$("$xdotool" search --name AAADAW 2>/dev/null | head -1 || true)
  [[ -n "$main_window" ]] && break
  sleep 0.2
done
[[ -n "$main_window" ]] || { cat "$run_dir/aaadaw.log" >&2; exit 1; }
project_title=
for _ in $(seq 1 100); do
  project_title=$("$xdotool" getwindowname "$main_window" 2>/dev/null || true)
  [[ "$project_title" == *"$(basename -- "$project")"* ]] && break
  sleep 0.2
done
[[ "$project_title" == *"$(basename -- "$project")"* ]] || {
  printf 'AAADAW did not finish opening the project fixture.\n' >&2
  cat "$run_dir/aaadaw.log" >&2
  exit 1
}

"$xdotool" windowfocus --sync "$main_window" || true
sleep 1.0
"$xdotool" key --clearmodifiers --window "$main_window" ctrl+alt+shift+a
sleep 0.6
for state in before after undo redo active-empty; do
  case "$state" in
    after) "$xdotool" key --clearmodifiers --window "$main_window" ctrl+shift+o; sleep 0.8 ;;
    undo) "$xdotool" key --clearmodifiers --window "$main_window" ctrl+z; sleep 0.8 ;;
    redo) "$xdotool" key --clearmodifiers --window "$main_window" ctrl+y; sleep 0.8 ;;
    active-empty) "$xdotool" key --clearmodifiers --window "$main_window" ctrl+shift+n; sleep 0.8 ;;
  esac
  output="$fixtures/audio-empty-takes-aaadaw-$state.png"
  import -display "$display" -window root "$output"
  identify "$output"
  sha256sum "$output"
done
printf 'aaadaw_binary\t%s\n' "$binary"
printf 'aaadaw_run_dir\t%s\n' "$run_dir"
printf 'aaadaw_display\t%s\n' "$display"
