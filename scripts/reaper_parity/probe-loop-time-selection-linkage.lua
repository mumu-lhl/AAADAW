-- Run only in a fresh, unsaved project and isolated REAPER 7.82 configuration.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use an empty disposable project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local log = assert(io.open(root .. "loop-time-selection-linkage.tsv", "w"))
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
for _, action_id in ipairs({40621, 40622, 40623, 40624, 40634, 40635, 40020}) do
  log:write("action_", action_id, "\t",
    reaper.kbd_getTextFromCmd(action_id, 0) or "<missing>", "\n")
end
log:write("default_link_state\t", linked() and "linked" or "unlinked", "\n")
log:write("case\tlink_before\ttime_selection_before\tloop_points_before\taction_id\tlink_after\ttime_selection_after\tloop_points_after\n")

local function prepare(link_enabled)
  set_link(false)
  set_range(false, 0, 0)
  set_range(true, 0, 0)
  set_range(false, 10, 20)
  set_range(true, 30, 40)
  set_link(link_enabled)
end

local cases = {
  { name = "toggle_link_on", link = false, action = 40621 },
  { name = "toggle_link_off", link = true, action = 40621 },
  { name = "copy_time_selection_to_loop_points", link = false, action = 40622 },
  { name = "copy_loop_points_to_time_selection", link = false, action = 40623 },
  { name = "remove_loop_unlinked_only_if_unlinked", link = false, action = 40624 },
  { name = "remove_loop_conditional_while_linked", link = true, action = 40624 },
  { name = "remove_loop_unlinked", link = false, action = 40634 },
  { name = "remove_time_selection_unlinked", link = false, action = 40635 },
  { name = "remove_loop_linked", link = true, action = 40634 },
  { name = "remove_time_selection_linked", link = true, action = 40635 },
  { name = "remove_both_unlinked", link = false, action = 40020 },
}

local case_index = 0
local function next_case()
  case_index = case_index + 1
  local case = cases[case_index]
  if not case then
    set_link(initial_linked)
    set_range(false, 0, 0)
    set_range(true, 0, 0)
    reaper.UpdateArrange()
    log:close()
    return
  end

  prepare(case.link)
  reaper.UpdateArrange()
  local link_before, time_before, loop_before = state()
  reaper.defer(function()
    reaper.Main_OnCommand(case.action, 0)
    reaper.UpdateArrange()
    local link_after, time_after, loop_after = state()
    log:write(case.name, "\t", link_before, "\t", time_before, "\t", loop_before,
      "\t", case.action, "\t", link_after, "\t", time_after, "\t", loop_after, "\n")
    log:flush()
    reaper.defer(next_case)
  end)
end

next_case()
