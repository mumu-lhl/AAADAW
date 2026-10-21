-- Run only in an isolated Linux REAPER 7.82 / Default 7 session.
-- Set AAADAW_REAPER_PROBE_DIR and optionally AAADAW_REAPER_EMPTY_TIME=1.
-- The track and MIDI item are intentionally unselected; the item is at 0-1 sec.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Unselected track", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 1, false))
reaper.SetTrackSelected(track, false)
reaper.SetMediaItemSelected(item, false)
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 240, 0, 60, 100, false))
reaper.MIDI_Sort(take)

local has_time_selection = os.getenv("AAADAW_REAPER_EMPTY_TIME") == "1"
if has_time_selection then
  reaper.GetSet_LoopTimeRange2(0, true, false, 2, 3, false)
else
  reaper.GetSet_LoopTimeRange2(0, true, false, 0, 0, false)
end
local suffix = has_time_selection and "time-selection" or "no-time-selection"
reaper.Main_SaveProjectEx(0, root .. "midi-export-empty-scope-" .. suffix .. ".rpp", 0)
local metadata = assert(io.open(root .. "midi-export-empty-scope-" .. suffix .. ".tsv", "w"))
metadata:write("version\t", reaper.GetAppVersion(), "\n")
metadata:write("theme\tDefault_7.0.ReaperThemeZip\n")
metadata:write("track_selection\tno tracks selected\n")
metadata:write("item_selection\tno MIDI items selected\n")
metadata:write("time_selection\t", has_time_selection and "2-3 seconds" or "none", "\n")
metadata:close()
reaper.Main_OnCommand(40849, 0)
