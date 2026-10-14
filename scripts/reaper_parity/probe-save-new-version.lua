-- Run only in a fresh REAPER 7.82 Default 7 profile and disposable project dir.
-- The probe saves one empty project, invokes Main action 41895, records the
-- active project path and generated files, then exits without touching user data.
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

local log = assert(io.open(root .. "save-new-version-probe.tsv", "w"))
local function basename(path)
  return path:match("([^/]+)$") or path
end
local function write_files()
  local index = 0
  while true do
    local name = reaper.EnumerateFiles(root, index)
    if not name then break end
    log:write("file\t", name, "\n")
    index = index + 1
  end
end

local baseline = root .. "save-version-probe.rpp"
reaper.Main_SaveProjectEx(0, baseline, 0)
local baseline_file = assert(io.open(baseline, "rb"), "Could not save baseline project")
local baseline_size = baseline_file:seek("end")
baseline_file:close()
assert(baseline_size and baseline_size > 0, "Baseline project file is empty")
reaper.Main_openProject(baseline)
local _, baseline_path = reaper.EnumProjects(-1, "")
assert(baseline_path == baseline, "Could not open the saved baseline project")
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", basename(reaper.GetLastColorThemeFile()), "\n")
log:write("action\t41895\tFile: Save new version of project (automatically increment project name)\n")
log:write("shortcut\tCtrl+Alt+Shift+S\n")
log:write("baseline_path\t", basename(baseline_path), "\n")
log:flush()

reaper.Main_OnCommand(41895, 0)
local deadline = reaper.time_precise() + 1.0
local function finish()
  if reaper.time_precise() < deadline then
    reaper.defer(finish)
    return
  end
  local _, first_version_path = reaper.EnumProjects(-1, "")
  log:write("first_version_path\t", basename(first_version_path), "\n")
  log:flush()
  reaper.Main_OnCommand(41895, 0)
  deadline = reaper.time_precise() + 1.0
  local function finish_second()
    if reaper.time_precise() < deadline then
      reaper.defer(finish_second)
      return
    end
    local _, second_version_path = reaper.EnumProjects(-1, "")
    log:write("second_version_path\t", basename(second_version_path), "\n")
    write_files()
    log:close()
    reaper.Main_OnCommand(40004, 0)
  end
  reaper.defer(finish_second)
end
reaper.defer(finish)
