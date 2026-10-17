-- Run only in an isolated, unsaved REAPER 7.82 / Default 7 project while stopped.
-- Creates a disposable track, opens the native Action List, and samples project
-- state while search-field keyboard input is exercised manually with xdotool.
-- Set AAADAW_ACTION_FOCUS_PROBE_DIR to an existing writable output directory.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0, "Stop playback before running this probe")
assert(reaper.CountTracks(0) == 0, "Run in a blank disposable project")

local directory = os.getenv("AAADAW_ACTION_FOCUS_PROBE_DIR") or "/tmp/aaadaw-actionlist-focus-native"
local output = assert(io.open(directory .. "/action-list-focus.tsv", "w"))
local request_path = directory .. "/request.tsv"

local function snapshot(label, elapsed)
  local time_start, time_end = reaper.GetSet_LoopTimeRange2(0, false, false, 0, 0, false)
  local loop_start, loop_end = reaper.GetSet_LoopTimeRange2(0, false, true, 0, 0, false)
  output:write(table.concat({
    label,
    string.format("%.3f", elapsed),
    tostring(reaper.CountTracks(0)),
    tostring(reaper.GetPlayStateEx(0)),
    string.format("%.6f", time_start),
    string.format("%.6f", time_end),
    string.format("%.6f", loop_start),
    string.format("%.6f", loop_end),
    tostring(reaper.GetProjectStateChangeCount(0)),
  }, "\t"), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("step\telapsed_seconds\ttrack_count\tplay_state\ttime_start_seconds\ttime_end_seconds\tloop_start_seconds\tloop_end_seconds\tproject_state_change_count\n")
local initial_state = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40001, 0) -- Track: Insert new track
assert(reaper.CountTracks(0) == 1, "Could not insert the temporary test track")
reaper.GetSet_LoopTimeRange2(0, true, false, 1.0, 3.0, false)
reaper.GetSet_LoopTimeRange2(0, true, true, 4.0, 6.0, false)
reaper.Main_OnCommand(40605, 0) -- Show action list
snapshot("ready", 0)

local started = reaper.time_precise()
local last_sample = 0
local function sample()
  local elapsed = reaper.time_precise() - started
  local request = io.open(request_path, "r")
  if request then
    local label = request:read("*l")
    request:close()
    os.remove(request_path)
    if label and label ~= "" then snapshot(label, elapsed) end
  end
  if elapsed - last_sample >= 0.25 then
    snapshot("sample", elapsed)
    last_sample = elapsed
  end
  if elapsed < 90 then
    reaper.defer(sample)
  else
    output:write("baseline_state_count\t", initial_state, "\n")
    output:close()
  end
end
reaper.defer(sample)
