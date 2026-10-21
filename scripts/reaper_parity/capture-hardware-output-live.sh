#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
fixtures=${1:-"$repo_root/docs/verification/reaper-parity/fixtures/hardware-output-live"}
reaper=${AAADAW_REAPER_BIN:-"$repo_root/target/reaper-7.82/reaper_linux_x86_64/REAPER/reaper"}
parecord=${AAADAW_PARECORD:-parecord}
pacat=${AAADAW_PACAT:-pacat}
pactl=${AAADAW_PACTL:-pactl}
python_bin=${AAADAW_PYTHON:-python3}
runtime_root=${AAADAW_XVFB_RUNTIME:-"$repo_root/target/reaper782/root/usr/bin"}
if [[ ! -x "$runtime_root/Xvfb" ]]; then runtime_root=/tmp/aaadaw-xvfb-apt/root/usr/bin; fi
if [[ ! -x "$runtime_root/Xvfb" ]]; then runtime_root=/workspace/.aaadaw-env/sysroot/usr/bin; fi
xvfb=${AAADAW_XVFB:-"$runtime_root/Xvfb"}
probe_lua="$repo_root/scripts/reaper_parity/probe-hardware-output-live.lua"
source_wav="$repo_root/docs/verification/reaper-parity/fixtures/hardware-output-live/source.wav"

[[ -x "$reaper" && -x "$xvfb" ]] || { printf 'REAPER/Xvfb executable is missing.\n' >&2; exit 2; }
command -v "$parecord" >/dev/null || { printf 'parecord is required.\n' >&2; exit 2; }
command -v "$pacat" >/dev/null || { printf 'pacat is required.\n' >&2; exit 2; }
command -v "$pactl" >/dev/null || { printf 'pactl is required.\n' >&2; exit 2; }
command -v "$python_bin" >/dev/null || { printf 'python3 is required.\n' >&2; exit 2; }
command -v xwininfo >/dev/null || { printf 'xwininfo is required.\n' >&2; exit 2; }
[[ -n "${PULSE_SERVER:-}" ]] || { printf 'Set PULSE_SERVER to a running PulseAudio server.\n' >&2; exit 2; }
[[ -s "$source_wav" ]] || { printf 'The deterministic source fixture is missing: %s\n' "$source_wav" >&2; exit 2; }

pulse_info=$("$pactl" info)
server_version=$(awk -F ': ' '/^Server Version:/ { print $2; exit }' <<<"$pulse_info")
previous_sink=$(awk -F ': ' '/^Default Sink:/ { print $2; exit }' <<<"$pulse_info")
sink_name="aaadaw_hw_output_probe_$$"
module_id=$("$pactl" load-module module-null-sink "sink_name=$sink_name" rate=44100 channels=2)
trap 'if [[ -n "$previous_sink" ]]; then "$pactl" set-default-sink "$previous_sink" >/dev/null 2>&1 || true; fi; "$pactl" unload-module "$module_id" >/dev/null 2>&1 || true' EXIT
"$pactl" set-default-sink "$sink_name"
monitor="$sink_name.monitor"

mkdir -p "$fixtures"
run_dir=$(mktemp -d "${TMPDIR:-/tmp}/aaadaw-hardware-output-live.XXXXXX")
profile="$run_dir/profile"
mkdir -p "$profile/config" "$profile/data" "$profile/state" "$profile/cache" "$profile/runtime" "$profile/home"
chmod 700 "$profile/runtime"
cp -- "$source_wav" "$profile/source.wav"
cp -- "$probe_lua" "$profile/probe-hardware-output-live.lua"
cat >"$profile/config/reaper.ini" <<'EOF'
[reaper]
linux_audio_bits=32
linux_audio_bsize=512
linux_audio_bufs=3
linux_audio_mode=3
linux_audio_nch_in=2
linux_audio_nch_out=2
linux_audio_srate=44100
linux_audio_srateor=1
EOF

display=
for number in $(seq 700 799); do
  candidate=":$number"
  if [[ ! -e "/tmp/.X$number-lock" && ! -S "/tmp/.X11-unix/X$number" ]]; then display=$candidate; break; fi
done
[[ -n "$display" ]] || { printf 'No free X11 display was found.\n' >&2; exit 1; }

export DISPLAY="$display"
export HOME="$profile/home"
export XDG_CONFIG_HOME="$profile/config" XDG_DATA_HOME="$profile/data"
export XDG_STATE_HOME="$profile/state" XDG_CACHE_HOME="$profile/cache" XDG_RUNTIME_DIR="$profile/runtime"
export AAADAW_REAPER_PROBE_DIR="$profile"
export LD_LIBRARY_PATH="$runtime_root/../lib/x86_64-linux-gnu:/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

xvfb_pid=
reaper_pid=
recorder_pid=
keepalive_pid=
cleanup() {
  if [[ -n "$recorder_pid" ]]; then kill -INT "$recorder_pid" 2>/dev/null || true; wait "$recorder_pid" 2>/dev/null || true; fi
  if [[ -n "$reaper_pid" ]]; then kill -TERM "$reaper_pid" 2>/dev/null || true; wait "$reaper_pid" 2>/dev/null || true; fi
  if [[ -n "$keepalive_pid" ]]; then kill -INT "$keepalive_pid" 2>/dev/null || true; wait "$keepalive_pid" 2>/dev/null || true; fi
  if [[ -n "$xvfb_pid" ]]; then kill "$xvfb_pid" 2>/dev/null || true; wait "$xvfb_pid" 2>/dev/null || true; fi
  if [[ -n "$previous_sink" ]]; then "$pactl" set-default-sink "$previous_sink" >/dev/null 2>&1 || true; fi
  "$pactl" unload-module "$module_id" >/dev/null 2>&1 || true
  rm -rf -- "$run_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

"$xvfb" "$display" -screen 0 1280x800x24 -ac -nolisten tcp >"$run_dir/xvfb.log" 2>&1 &
xvfb_pid=$!
for _ in $(seq 1 100); do
  if xwininfo -display "$display" -root >/dev/null 2>&1; then break; fi
  sleep 0.1
done

run_case() {
  local scenario=$1
  local complete="$profile/$scenario.complete"
  local capture="$run_dir/$scenario.wav"
  rm -f -- "$complete" "$profile/$scenario.state" "$profile/$scenario.peaks.tsv" "$profile/$scenario.rpp"
  printf '%s\n' "$scenario" >"$profile/scenario.txt"
  "$parecord" --device="$monitor" --rate=44100 --format=s16le --channels=2 --file-format=wav "$capture" >"$run_dir/parecord-$scenario.log" 2>&1 &
  recorder_pid=$!
  sleep 0.15
  "$reaper" -cfgfile "$profile/config/reaper.ini" -nonewinst -nosplash "$probe_lua" >"$run_dir/reaper-$scenario.log" 2>&1
  for _ in $(seq 1 200); do
    [[ -s "$complete" ]] && break
    sleep 0.05
  done
  if [[ ! -s "$complete" ]]; then
    cat "$run_dir/reaper-$scenario.log" >&2 || true
    printf 'REAPER did not complete the %s playback probe.\n' "$scenario" >&2
    return 1
  fi
  kill -INT "$recorder_pid" 2>/dev/null || true
  wait "$recorder_pid" 2>/dev/null || true
  recorder_pid=
  cp -- "$capture" "$fixtures/$scenario.wav"
  for suffix in state peaks.tsv rpp; do cp -- "$profile/$scenario.$suffix" "$fixtures/$scenario.$suffix"; done
  "$python_bin" - "$fixtures/$scenario.rpp" "$profile/source.wav" <<'PY'
from pathlib import Path
import sys

project = Path(sys.argv[1])
temporary_source = sys.argv[2]
contents = project.read_text()
if temporary_source not in contents:
    raise SystemExit(f"{project}: expected source path was not found")
project.write_text(contents.replace(temporary_source, "source.wav"))
PY
}

"$reaper" -cfgfile "$profile/config/reaper.ini" -newinst -nosplash >"$run_dir/reaper.log" 2>&1 &
reaper_pid=$!
for _ in $(seq 1 100); do
  if [[ -n $("$pactl" list short sink-inputs) ]]; then break; fi
  sleep 0.1
done
sleep 0.2

# Keep the null sink clock running before the first playback so its monitor
# records every frame even when REAPER's PulseAudio auto-suspend is enabled.
"$pacat" --playback --device="$sink_name" --raw --rate=44100 --format=s16le --channels=2 </dev/zero >"$run_dir/pacat-keepalive.log" 2>&1 &
keepalive_pid=$!
sleep 0.2

for scenario in main routed muted half phase pan-right mono-1-stereo mono-1-output-2 track-half pre-fx post-fx-2 post-fx-3 track-muted source-solo other-solo master-bypass; do
  run_case "$scenario"
done
cp -- "$probe_lua" "$fixtures/probe-hardware-output-live.lua"
cp -- "$repo_root/scripts/reaper_parity/verify-hardware-output-live.py" "$fixtures/verify-hardware-output-live.py"
cp -- "$repo_root/scripts/reaper_parity/capture-hardware-output-live.sh" "$fixtures/capture-hardware-output-live.sh"
{
  printf 'reference\tLinux REAPER 7.82 / x86_64 / Default 7 factory configuration\n'
  printf 'capture\tPulseAudio null sink monitor at 44100 Hz stereo; Xvfb 1280x800; no physical device\n'
  printf 'pulseaudio_server_version\t%s\n' "${server_version:-unknown}"
  printf 'reaper_binary_sha256\t%s\n' "$(sha256sum "$reaper" | awk '{print $1}')"
  printf 'probe_script_sha256\t%s\n' "$(sha256sum "$probe_lua" | awk '{print $1}')"
  printf 'verification_script_sha256\t%s\n' "$(sha256sum "$repo_root/scripts/reaper_parity/verify-hardware-output-live.py" | awk '{print $1}')"
  printf 'capture_script_sha256\t%s\n' "$(sha256sum "${BASH_SOURCE[0]}" | awk '{print $1}')"
  for artifact in capture-hardware-output-live.sh probe-hardware-output-live.lua verify-hardware-output-live.py source.wav main.wav main.rpp main.peaks.tsv routed.wav routed.rpp routed.peaks.tsv muted.wav muted.rpp muted.peaks.tsv half.wav half.rpp half.peaks.tsv phase.wav phase.rpp phase.peaks.tsv pan-right.wav pan-right.rpp pan-right.peaks.tsv mono-1-stereo.wav mono-1-stereo.rpp mono-1-stereo.peaks.tsv mono-1-output-2.wav mono-1-output-2.rpp mono-1-output-2.peaks.tsv track-half.wav track-half.rpp track-half.peaks.tsv pre-fx.wav pre-fx.rpp pre-fx.peaks.tsv post-fx-2.wav post-fx-2.rpp post-fx-2.peaks.tsv post-fx-3.wav post-fx-3.rpp post-fx-3.peaks.tsv track-muted.wav track-muted.rpp track-muted.peaks.tsv source-solo.wav source-solo.rpp source-solo.peaks.tsv other-solo.wav other-solo.rpp other-solo.peaks.tsv master-bypass.wav master-bypass.rpp master-bypass.peaks.tsv; do
    printf '%s_sha256\t%s\n' "$artifact" "$(sha256sum "$fixtures/$artifact" | awk '{print $1}')"
  done
} >"$fixtures/live-evidence.tsv"
cat "$fixtures/live-evidence.tsv"
"$python_bin" "$repo_root/scripts/reaper_parity/verify-hardware-output-live.py" "$fixtures"
