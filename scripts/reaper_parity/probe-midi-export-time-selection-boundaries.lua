-- Run only in an isolated Linux REAPER 7.82 / Default 7 project.
-- Records MIDI events that straddle a time selection, then opens Main action 40849.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Wide", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0.25, 2.75, false))
local take = assert(reaper.GetActiveTake(item))
for _, note in ipairs({
  {60, 0, 960},
  {61, 960, 2880},
  {62, 3000, 4080},
  {63, 3360, 3840},
}) do
  assert(reaper.MIDI_InsertNote(take, false, false, note[2], note[3], 0, note[1], 100, false))
end
reaper.MIDI_Sort(take)
reaper.GetSet_LoopTimeRange2(0, true, false, 1.0, 2.0, false)
reaper.UpdateArrange()
local save_project = os.getenv("AAADAW_REAPER_SAVE_PROJECT")
if save_project then reaper.Main_SaveProjectEx(0, save_project, 0) end
local meta = assert(io.open(root .. "midi-export-time-selection-boundaries-native-meta.tsv", "w"))
meta:write("version\t", reaper.GetAppVersion(), "\n")
meta:write("theme\tDefault_7.0.ReaperThemeZip\n")
meta:write("time_selection_seconds\t1.000000-2.000000\n")
meta:write("time_selection_ticks\t1920-3840\n")
meta:write("item_project_range_ticks\t480-5280\n")
meta:write("source_notes\tC4 480-1440; C#4 1440-3360; D4 3480-4560; D#4 3840-4320\n")
meta:write("export_action\t40849\n")
meta:write("export_time_mode\tTime selection only\n")
meta:close()
reaper.Main_OnCommand(40849, 0)
local ready = assert(io.open(root .. "midi-export-time-selection-boundaries-ready", "w"))
ready:write("Choose Time selection only, all MIDI items, and an unused .mid path\n")
ready:close()
