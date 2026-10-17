-- Run only in an isolated Linux REAPER 7.82 / Default 7 session.
-- Set AAADAW_REAPER_PROBE_DIR and exactly one of AAADAW_MUTE_TRACK or
-- AAADAW_MUTE_ITEM to "1" before launching this script through REAPER.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

local track_muted = os.getenv("AAADAW_MUTE_TRACK") == "1"
local item_muted = os.getenv("AAADAW_MUTE_ITEM") == "1"
assert(track_muted ~= item_muted, "Mute either the track or the item, not both")

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(
  track,
  "P_NAME",
  track_muted and "Muted Track" or "Unmuted Track",
  true
)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 1, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 240, 0, 60, 100, false))
reaper.MIDI_Sort(take)
if track_muted then reaper.SetMediaTrackInfo_Value(track, "B_MUTE", 1) end
if item_muted then reaper.SetMediaItemInfo_Value(item, "B_MUTE", 1) end

local name = track_muted and "track-muted" or "item-muted"
reaper.Main_SaveProjectEx(0, root .. "menu-export-midi-native-" .. name .. ".rpp", 0)
local metadata = assert(io.open(root .. name .. ".tsv", "w"))
metadata:write("version\t", reaper.GetAppVersion(), "\n")
metadata:write("theme\tDefault_7.0.ReaperThemeZip\n")
metadata:write("track_muted\t", tostring(track_muted), "\n")
metadata:write("item_muted\t", tostring(item_muted), "\n")
metadata:close()
reaper.Main_OnCommand(40849, 0)
