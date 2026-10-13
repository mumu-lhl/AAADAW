-- Run only in a fresh, unsaved project and isolated REAPER 7.82 configuration.
-- Move the OS pointer over the region ruler band while the two-second delay runs.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use an empty disposable project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local log = assert(io.open(root .. "region-delete-under-mouse.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("action_40615\t", reaper.kbd_getTextFromCmd(40615, 0) or "<missing>", "\n")
local start_position = tonumber(os.getenv("AAADAW_REAPER_REGION_START")) or 1
local end_position = tonumber(os.getenv("AAADAW_REAPER_REGION_END")) or 2
local cursor_position = tonumber(os.getenv("AAADAW_REAPER_CURSOR")) or 1.5
local number = reaper.AddProjectMarker2(0, true, start_position, end_position, "Probe Region", -1, 0)
reaper.SetEditCurPos2(0, cursor_position, false, false)
reaper.UpdateArrange()
log:write("region_number\t", number, "\n")
log:write("edit_cursor\t", string.format("%.9f", reaper.GetCursorPosition()), "\n")
log:write("play_cursor\t", string.format("%.9f", reaper.GetPlayPosition2()), "\n")
log:flush()

local started = reaper.time_precise()
local function run_action()
  if reaper.time_precise() - started < 2 then
    reaper.defer(run_action)
    return
  end
  reaper.Main_OnCommand(40615, 0)
  local count = reaper.CountProjectMarkers(0)
  local state = {}
  for index = 0, count - 1 do
    local valid, is_region, position, region_end, name, marker_number =
      reaper.EnumProjectMarkers3(0, index)
    if valid ~= 0 then
      state[#state + 1] = string.format("%s#%d[%.6f,%.6f)",
        is_region and "region:" .. name or "marker:" .. name,
        marker_number, position, region_end)
    end
  end
  log:write("after\t", #state > 0 and table.concat(state, ";") or "-", "\n")
  log:flush()
  log:close()
end
run_action()
