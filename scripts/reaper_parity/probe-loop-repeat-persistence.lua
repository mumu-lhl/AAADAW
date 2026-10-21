-- Run write, read, toggle-unsaved, and read-after-unsaved in one isolated
-- REAPER 7.82 config. The script saves projects with opposite Repeat values,
-- restarts REAPER between phases, and checks saved and unsaved toggles.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
local phase = assert(os.getenv("AAADAW_REAPER_REPEAT_PROBE_PHASE"), "Set probe phase to write or read")
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))

local enabled_project = root .. "repeat-enabled.rpp"
local disabled_project = root .. "repeat-disabled.rpp"
local output = assert(io.open(root .. "loop-repeat-persistence-native.tsv", phase == "write" and "w" or "a"))

local function repeat_enabled()
  return reaper.GetToggleCommandState(1068) == 1
end

local function set_repeat(enabled)
  if repeat_enabled() ~= enabled then
    reaper.Main_OnCommand(1068, 0)
  end
end

local function set_linked(enabled)
  if (reaper.GetToggleCommandState(40621) == 1) ~= enabled then
    reaper.Main_OnCommand(40621, 0)
  end
end

local function set_ranges(time_start, time_end, loop_start, loop_end)
  reaper.GetSet_LoopTimeRange2(0, true, false, time_start, time_end, false)
  reaper.GetSet_LoopTimeRange2(0, true, true, loop_start, loop_end, false)
end

local function record(label, path_override)
  local time_start, time_end = reaper.GetSet_LoopTimeRange2(0, false, false, 0, 0, false)
  local loop_start, loop_end = reaper.GetSet_LoopTimeRange2(0, false, true, 0, 0, false)
  local _, project_path = reaper.EnumProjects(-1, "")
  project_path = path_override or project_path
  project_path = project_path:match("[^/]+$") or "unsaved"
  output:write(label, "\t", repeat_enabled() and "on" or "off", "\t",
    string.format("%.9f\t%.9f\t%.9f\t%.9f\t%s", time_start, time_end,
      loop_start, loop_end, project_path), "\t", reaper.GetProjectStateChangeCount(0), "\n")
  output:flush()
end

if phase == "write" then
  output:write("case\trepeat\ttime_start\ttime_end\tloop_start\tloop_end\tproject_path\tstate_change_count\n")
  set_linked(false)
  record("default-before-repeat")
  set_repeat(true)
  record("repeat-on-before-save")
  set_repeat(false)
  record("repeat-off-before-save")
  set_ranges(1.25, 2.5, 3.5, 5.75)
  set_repeat(true)
  reaper.Main_SaveProjectEx(0, enabled_project, 0)
  local enabled_file = assert(io.open(enabled_project, "r"), "enabled project should save")
  enabled_file:close()
  record("before-restart-enabled", enabled_project)
  set_repeat(false)
  record("after-repeat-toggle-off")

  set_ranges(6.25, 7.5, 8.5, 10.75)
  reaper.Main_SaveProjectEx(0, disabled_project, 0)
  local disabled_file = assert(io.open(disabled_project, "r"), "disabled project should save")
  disabled_file:close()
  record("before-restart-disabled", disabled_project)
elseif phase == "read" then
  reaper.Main_openProject(enabled_project)
  record("after-restart-enabled-project")
  reaper.Main_openProject(disabled_project)
  record("after-restart-disabled-project")
elseif phase == "toggle-unsaved" then
  record("startup-before-project-open")
  reaper.Main_openProject(disabled_project)
  record("before-unsaved-repeat-toggle")
  set_repeat(true)
  record("after-unsaved-repeat-toggle")
elseif phase == "read-after-unsaved" then
  record("startup-before-project-open")
  reaper.Main_openProject(disabled_project)
  record("after-restart-disabled-project")
  reaper.Main_OnCommand(40859, 0)
  record("after-new-project-tab")
else
  error("AAADAW_REAPER_REPEAT_PROBE_PHASE must be write, read, toggle-unsaved, or read-after-unsaved")
end

output:close()
reaper.defer(function() reaper.Main_OnCommand(40004, 0) end)
