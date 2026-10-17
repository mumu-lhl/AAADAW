-- Run only in an isolated Linux REAPER 7.82 / Default 7 session.
-- Set AAADAW_REAPER_PROBE_DIR before launching this script through REAPER.
-- Select Merge to single MIDI track in the export dialog before confirming.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

reaper.Main_SaveProjectEx(0, root .. "menu-export-midi-native-empty-type0.rpp", 0)
local metadata = assert(io.open(root .. "empty-type0.tsv", "w"))
metadata:write("version\t", reaper.GetAppVersion(), "\n")
metadata:write("theme\tDefault_7.0.ReaperThemeZip\n")
metadata:write("track_count\t0\n")
metadata:write("export_scope\tAll / Entire project\n")
metadata:write("file_type\tType 0 / Merge to single MIDI track\n")
metadata:close()
reaper.Main_OnCommand(40849, 0)
