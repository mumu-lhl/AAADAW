-- Run in an isolated, unsaved Linux REAPER 7.82 / Default 7 project.
-- Creates a looped MIDI item with two notes, opens Item: Show media item
-- source properties (40011), and records the untouched source state.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0, "Stop playback before running this probe")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "Probe requires an empty project")

local directory = os.getenv("AAADAW_MIDI_SOURCE_PROPERTIES_PROBE_DIR") or "/tmp"
local output = assert(io.open(directory .. "/midi-source-properties-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI source properties", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 2, false))
reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 1)
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 960, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(take, false, false, 1920, 2880, 0, 64, 108, false))
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
local source = reaper.GetMediaItemTake_Source(take)
local source_length, length_is_qn = reaper.GetMediaSourceLength(source)
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("item\tposition=", reaper.GetMediaItemInfo_Value(item, "D_POSITION"),
  "\tlength=", reaper.GetMediaItemInfo_Value(item, "D_LENGTH"),
  "\tloop_source=", reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC"), "\n")
output:write("source\tlength=", source_length, "\tlength_is_qn=", tostring(length_is_qn), "\n")
output:write("action\t40011 Item properties: Show media item source properties\n")
output:flush()
output:close()
reaper.Main_OnCommand(40011, 0)
