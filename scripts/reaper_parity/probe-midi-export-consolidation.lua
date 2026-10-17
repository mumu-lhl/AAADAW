-- Run only in an isolated Linux REAPER 7.82 / Default 7 project.
-- Creates a selected-track/item fixture and opens Main action 40849.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

local function create_track(index, name, selected)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  reaper.SetTrackSelected(track, selected)
  return track
end

local function create_item(track, name, start_time, end_time, pitch, selected)
  local item = assert(reaper.CreateNewMIDIItemInProj(track, start_time, end_time, false))
  reaper.GetSetMediaItemInfo_String(item, "P_NAME", name, true)
  reaper.SetMediaItemSelected(item, selected)
  local take = assert(reaper.GetActiveTake(item))
  assert(reaper.MIDI_InsertNote(take, false, false, 0, 240, 0, pitch, 100, false))
  reaper.MIDI_Sort(take)
  return item
end

local track_a = create_track(0, "Track A", false)
local track_b = create_track(1, "Track B", true)
local item_a_selected = create_item(track_a, "A selected", 0.25, 0.75, 60, true)
local item_a_unselected = create_item(track_a, "A unselected", 1.25, 1.75, 61, false)
local item_b_unselected = create_item(track_b, "B unselected", 2.25, 2.75, 62, false)
reaper.GetSet_LoopTimeRange2(0, true, false, 1.0, 2.0, false)
reaper.UpdateArrange()

local save_project = os.getenv("AAADAW_REAPER_SAVE_PROJECT")
if save_project then
  reaper.Main_SaveProjectEx(0, save_project, 0)
end

local meta = assert(io.open(root .. "midi-export-consolidation-native-meta.tsv", "w"))
meta:write("version\t", reaper.GetAppVersion(), "\n")
meta:write("theme\tDefault_7.0.ReaperThemeZip\n")
meta:write("track_a_selected\t", tostring(reaper.IsTrackSelected(track_a)), "\n")
meta:write("track_b_selected\t", tostring(reaper.IsTrackSelected(track_b)), "\n")
meta:write("item_a_selected\t", tostring(reaper.IsMediaItemSelected(item_a_selected)), "\n")
meta:write("item_a_unselected\t", tostring(reaper.IsMediaItemSelected(item_a_unselected)), "\n")
meta:write("item_b_unselected\t", tostring(reaper.IsMediaItemSelected(item_b_unselected)), "\n")
meta:write("time_selection_seconds\t1.000000-2.000000\n")
meta:write("time_selection_ticks\t1920-3840\n")
meta:write("item_a_selected_seconds\t0.25-0.75; pitch C4; note source ticks 0-240\n")
meta:write("item_a_unselected_seconds\t1.25-1.75; pitch C#4; note source ticks 0-240\n")
meta:write("item_b_unselected_seconds\t2.25-2.75; pitch D4; note source ticks 0-240\n")
meta:write("export_action\t40849\n")
meta:write("export_defaults\tEntire project; all MIDI items; Type 1; PPQN 960\n")
meta:close()

reaper.Main_OnCommand(40849, 0)
local ready = assert(io.open(root .. "midi-export-consolidation-ready", "w"))
ready:write("Export the selected consolidation modes to unused .mid paths\n")
ready:close()
