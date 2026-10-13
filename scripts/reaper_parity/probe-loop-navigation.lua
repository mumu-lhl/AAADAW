-- Run on a disposable unsaved REAPER 7.82 / Default 7 project while stopped.
local expected_version = "7.82/linux-x86_64"
assert(reaper.GetAppVersion() == expected_version)
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0, "Stop playback before running this probe")

local output = assert(io.open("/tmp/aaadaw-loop-navigation-native.tsv", "w"))
local old_cursor = reaper.GetCursorPosition()
local old_time_start, old_time_end = reaper.GetSet_LoopTimeRange2(0, false, false, 0, 0, false)
local old_loop_start, old_loop_end = reaper.GetSet_LoopTimeRange2(0, false, true, 0, 0, false)
local linked = reaper.GetToggleCommandState(40621) == 1

local function record(label, action_id)
  reaper.SetEditCurPos(4.0, false, false)
  reaper.Main_OnCommand(action_id, 0)
  output:write(label, "\t", string.format("%.6f", reaper.GetCursorPosition()), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("action_40632\t", reaper.kbd_getTextFromCmd(40632, 0), "\n")
output:write("action_40633\t", reaper.kbd_getTextFromCmd(40633, 0), "\n")
output:write("case\tcursor_after_seconds\n")

reaper.GetSet_LoopTimeRange2(0, true, false, 0, 0, false)
reaper.GetSet_LoopTimeRange2(0, true, true, 0, 0, false)
record("no_loop_go_to_start", 40632)
record("no_loop_go_to_end", 40633)

reaper.GetSet_LoopTimeRange2(0, true, true, 1.0, 2.0, false)
record("loop_go_to_start", 40632)
record("loop_go_to_end", 40633)

reaper.GetSet_LoopTimeRange2(0, true, false, old_time_start, old_time_end, false)
reaper.GetSet_LoopTimeRange2(0, true, true, old_loop_start, old_loop_end, false)
reaper.SetEditCurPos(old_cursor, false, false)
if (reaper.GetToggleCommandState(40621) == 1) ~= linked then
  reaper.Main_OnCommand(40621, 0)
end
reaper.UpdateArrange()
output:close()
