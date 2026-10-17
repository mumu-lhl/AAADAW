-- Probe Main actions 40897 and 40886 in a fresh REAPER 7.82 profile.
-- Only writes to AAADAW_REAPER_PROBE_DIR and refuses saved/non-empty input.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"),
  "Set AAADAW_REAPER_PROBE_DIR to an existing disposable directory")
root = root:gsub("/$", "") .. "/"

assert(reaper.GetAppVersion() == "7.82/linux-x86_64",
  "Expected Linux REAPER 7.82, got " .. reaper.GetAppVersion())
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"),
  "Expected Default 7 theme, got " .. reaper.GetLastColorThemeFile())
assert(reaper.CountTracks(0) == 0, "Refusing to modify a non-empty project")
local _, initial_path = reaper.EnumProjects(-1, "")
assert(initial_path == "", "Refusing to run in an already-saved project")

local log = assert(io.open(root .. "save-close-all-native.tsv", "w"))
local baseline = root .. "save-all-probe.rpp"
reaper.Main_SaveProjectEx(0, baseline, 0)
reaper.Main_openProject(baseline)
local _, opened_path = reaper.EnumProjects(-1, "")
assert(opened_path == baseline, "Could not open the probe project")

log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\tDefault_7.0.ReaperThemeZip\n")
log:write("save_all_action\t40897\tFile: Save all projects\n")
log:write("close_all_action\t40886\tFile: Close all projects\n")
log:write("save_all_default_shortcut\tNone\n")
log:flush()

reaper.Main_OnCommand(40001, 0) -- Track: Insert new track
local tracks_before_save = reaper.CountTracks(0)
reaper.Main_OnCommand(40897, 0)
local project_file = assert(io.open(baseline, "rb"), "Save all did not preserve the project file")
local serialized = project_file:read("*a")
project_file:close()
log:write("tracks_before_save_all\t", tracks_before_save, "\n")
log:write("save_all_wrote_track\t", tostring(serialized:find("<TRACK", 1, true) ~= nil), "\n")
log:flush()

local deadline = reaper.time_precise() + 0.5
local function close_all()
  if reaper.time_precise() < deadline then
    reaper.defer(close_all)
    return
  end
  reaper.Main_OnCommand(40886, 0)
  deadline = reaper.time_precise() + 0.5
  local function finish()
    if reaper.time_precise() < deadline then
      reaper.defer(finish)
      return
    end
    local _, active_path = reaper.EnumProjects(-1, "")
    log:write("active_path_after_close_all\t", active_path, "\n")
    log:write("tracks_after_close_all\t", reaper.CountTracks(0), "\n")
    log:write("blank_project_workspace_after_close_all\t",
      tostring(active_path == "" and reaper.CountTracks(0) == 0), "\n")
    log:close()
    reaper.Main_OnCommand(40004, 0) -- File: Quit REAPER
  end
  reaper.defer(finish)
end
reaper.defer(close_all)
