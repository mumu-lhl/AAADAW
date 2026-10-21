-- Run only in an isolated Linux REAPER 7.82 / Default 7 session.
-- Start with an All / Entire project MIDI export config, then select
-- Selected tracks only + Time selection only in the export dialog.
-- Set AAADAW_REAPER_PROBE_DIR before launching this script through REAPER.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Outside time selection", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 0.5, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 240, 0, 60, 100, false))
reaper.MIDI_Sort(take)
reaper.SetTrackSelected(track, true)
reaper.SetMediaItemSelected(item, false)
reaper.GetSet_LoopTimeRange2(0, true, false, 1, 2, false)

reaper.Main_SaveProjectEx(0, root .. "menu-export-midi-native-selected-track-time-empty.rpp", 0)
local metadata = assert(io.open(root .. "selected-track-time-empty.tsv", "w"))
metadata:write("version\t", reaper.GetAppVersion(), "\n")
metadata:write("theme\tDefault_7.0.ReaperThemeZip\n")
metadata:write("track_selected\ttrue\n")
metadata:write("item_selected\tfalse\n")
metadata:write("item_range_seconds\t0-0.5\n")
metadata:write("time_selection_seconds\t1-2\n")
metadata:write("export_scope\tSelected tracks only / Time selection only\n")
metadata:close()
reaper.Main_OnCommand(40849, 0)
