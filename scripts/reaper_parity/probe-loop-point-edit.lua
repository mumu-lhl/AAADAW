-- Run only in a fresh, unsaved project and isolated REAPER 7.82 configuration.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use an empty disposable project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local log = assert(io.open(root .. "loop-point-edit.tsv", "w"))
local initial_linked = reaper.GetToggleCommandState(40621) == 1

local function linked()
  return reaper.GetToggleCommandState(40621) == 1
end

local function set_link(enabled)
  if linked() ~= enabled then
    reaper.Main_OnCommand(enabled and 40749 or 40750, 0)
  end
end

local function set_range(is_loop, start_time, end_time)
  reaper.GetSet_LoopTimeRange2(0, true, is_loop, start_time, end_time, false)
end

local function range(is_loop)
  local start_time, end_time = reaper.GetSet_LoopTimeRange2(0, false, is_loop, 0, 0, false)
  if math.abs(start_time) < 0.000000001 and math.abs(end_time) < 0.000000001 then
    return "-"
  end
  return string.format("%.3f,%.3f", start_time, end_time)
end

local function state()
  return linked() and "linked" or "unlinked", range(false), range(true)
end

log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\tDefault_7.0.ReaperThemeZip\n")
log:write("action_40222\t", reaper.kbd_getTextFromCmd(40222, 0) or "<missing>", "\n")
log:write("action_40223\t", reaper.kbd_getTextFromCmd(40223, 0) or "<missing>", "\n")
log:write("case\tlink_before\ttime_selection_before\tloop_points_before\tcursor\taction_id\tlink_after\ttime_selection_after\tloop_points_after\n")

local cases = {
  { name = "set_start_without_existing_range", link = false, cursor = 5, action = 40222 },
  { name = "set_end_without_existing_range", link = false, cursor = 5, action = 40223 },
  { name = "set_start_inside_range", link = false, time = {10, 20}, loop = {10, 20}, cursor = 15, action = 40222 },
  { name = "set_end_inside_range", link = false, time = {10, 20}, loop = {10, 20}, cursor = 15, action = 40223 },
  { name = "set_start_after_end", link = false, time = {10, 20}, loop = {10, 20}, cursor = 25, action = 40222 },
  { name = "set_end_before_start", link = false, time = {10, 20}, loop = {10, 20}, cursor = 5, action = 40223 },
  { name = "set_start_linked", link = true, time = {10, 20}, cursor = 15, action = 40222 },
  { name = "set_end_linked", link = true, time = {10, 20}, cursor = 15, action = 40223 },
  { name = "set_start_unlinked_with_different_ranges", link = false, time = {30, 40}, loop = {10, 20}, cursor = 15, action = 40222 },
  { name = "set_end_unlinked_with_different_ranges", link = false, time = {30, 40}, loop = {10, 20}, cursor = 15, action = 40223 },
}

local case_index = 0
local function next_case()
  case_index = case_index + 1
  local case = cases[case_index]
  if not case then
    set_range(false, 0, 0)
    set_range(true, 0, 0)
    set_link(initial_linked)
    reaper.SetEditCurPos2(0, 0, false, false)
    reaper.UpdateArrange()
    log:close()
    return
  end

  set_link(false)
  set_range(false, 0, 0)
  set_range(true, 0, 0)
  if case.time then set_range(false, case.time[1], case.time[2]) end
  if case.loop then set_range(true, case.loop[1], case.loop[2]) end
  set_link(case.link)
  reaper.SetEditCurPos2(0, case.cursor, false, false)
  reaper.UpdateArrange()

  reaper.defer(function()
    local link_before, time_before, loop_before = state()
    reaper.Main_OnCommand(case.action, 0)
    reaper.UpdateArrange()
    local link_after, time_after, loop_after = state()
    log:write(case.name, "\t", link_before, "\t", time_before, "\t", loop_before,
      "\t", string.format("%.3f", reaper.GetCursorPosition()), "\t", case.action,
      "\t", link_after, "\t", time_after, "\t", loop_after, "\n")
    log:flush()
    reaper.defer(next_case)
  end)
end

next_case()
