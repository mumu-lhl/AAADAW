-- Run in REAPER 7.82 / Default 7 with an isolated, disposable project and
-- AAADAW_REAPER_PROBE_DIR set to an existing output directory. Select Dummy
-- Audio under Preferences > Audio > Device before measuring playback position.
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"),
  "Set AAADAW_REAPER_PROBE_DIR to an output directory")
local output = assert(io.open(output_dir .. "/loop-repeat-start-native.tsv", "w"))
local expected_version = "7.82/linux-x86_64"
assert(reaper.GetAppVersion() == expected_version)
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0, "Stop playback before running this probe")

local old_time_start, old_time_end =
  reaper.GetSet_LoopTimeRange2(0, false, false, 0, 0, false)
local old_loop_start, old_loop_end =
  reaper.GetSet_LoopTimeRange2(0, false, true, 0, 0, false)
local old_cursor = reaper.GetCursorPosition()
local old_repeat = reaper.GetToggleCommandState(1068) == 1

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("action_repeat\t", reaper.kbd_getTextFromCmd(1068, 0), "\n")
output:flush()

reaper.GetSet_LoopTimeRange2(0, true, true, 1.0, 2.0, false)
if not old_repeat then reaper.Main_OnCommand(1068, 0) end
output:write("audio_test_setup\tDummy Audio\n")
output:write("repeat_enabled\t", tostring(reaper.GetToggleCommandState(1068) == 1), "\n")
output:write("case\tplay_state\tstart_cursor_seconds\tplay_position_seconds\n")
output:flush()

local cases = {
  { "start_before_loop", 0.0 },
  { "start_inside_loop", 1.5 },
  { "start_after_loop", 3.0 },
}
local case_index = 1
local started_at = 0

local function restore_and_finish()
  reaper.Main_OnCommand(1016, 0)
  reaper.GetSet_LoopTimeRange2(0, true, false, old_time_start, old_time_end, false)
  reaper.GetSet_LoopTimeRange2(0, true, true, old_loop_start, old_loop_end, false)
  if reaper.GetToggleCommandState(1068) == 1 ~= old_repeat then
    reaper.Main_OnCommand(1068, 0)
  end
  reaper.SetEditCurPos(old_cursor, false, false)
  reaper.UpdateArrange()
  output:close()
end

local function run_case()
  if case_index > #cases then
    restore_and_finish()
    return
  end
  local case = cases[case_index]
  reaper.Main_OnCommand(1016, 0)
  reaper.SetEditCurPos(case[2], false, false)
  reaper.Main_OnCommand(1007, 0)
  started_at = reaper.time_precise()

  local function sample_after_playing()
    if reaper.time_precise() - started_at < 0.35 then
      reaper.defer(sample_after_playing)
      return
    end
    output:write(case[1], "\t", tostring(reaper.GetPlayStateEx(0)), "\t",
      string.format("%.6f", case[2]), "\t",
      string.format("%.6f", reaper.GetPlayPosition()), "\n")
    output:flush()
    reaper.Main_OnCommand(1016, 0)
    case_index = case_index + 1
    reaper.defer(run_case)
  end

  reaper.defer(sample_after_playing)
end

reaper.defer(run_case)
