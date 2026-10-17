-- Run only in a fresh, unsaved project and isolated REAPER 7.82 configuration.
-- The script leaves the dialog open for inspection, then exits with the host.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use an empty disposable project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local log = assert(io.open(root .. "region-edit-near-cursor.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("action_40616\t", reaper.kbd_getTextFromCmd(40616, 0) or "<missing>", "\n")
local number = reaper.AddProjectMarker2(0, true, 1, 2, "Probe Region", -1, 0)
reaper.SetEditCurPos2(0, 1.5, false, false)
log:write("region_number\t", number, "\n")
log:write("cursor_before\t", string.format("%.9f", reaper.GetCursorPosition()), "\n")
local started = reaper.time_precise()
local function run_action()
  if reaper.time_precise() - started < 2 then
    reaper.defer(run_action)
    return
  end
  reaper.Main_OnCommand(40616, 0)
  log:write("cursor_after\t", string.format("%.9f", reaper.GetCursorPosition()), "\n")
  log:flush()
  log:close()
  local hold_started = reaper.time_precise()
  local function keep_open()
    if reaper.time_precise() - hold_started < 20 then reaper.defer(keep_open) end
  end
  keep_open()
end
run_action()
