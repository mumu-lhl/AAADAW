-- Run in an unsaved, stopped Linux REAPER 7.82 / Default 7 project with Dummy Audio.
-- Creates only isolated MIDI test items, runs Item: Trim items to selected area
-- (40508), and writes a TSV under AAADAW_REAPER_PROBE_DIR or /tmp.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
local base_track_count = reaper.CountTracks(0)
for track_index = 0, reaper.CountTracks(0) - 1 do
  assert(reaper.CountTrackMediaItems(reaper.GetTrack(0, track_index)) == 0,
    "probe requires an empty project")
end

local output_dir = os.getenv("AAADAW_REAPER_PROBE_DIR") or "/tmp"
local output_path = output_dir .. "/item-trim-native.tsv"
local output = assert(io.open(output_path, "w"))
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("audio\tDummy Audio; no hardware I/O\n")
output:write("transport\tstopped\n")
output:write("case\tselection_start\tselection_end\titem_start_before\titem_end_before\titems_after\titem_start_after\titem_end_after\tselected_after\tstate_change_delta\n")

local cases = {
  {"interior", 3, 7, 0, 10},
  {"left_overlap", 3, 7, 0, 4},
  {"right_overlap", 3, 7, 6, 10},
  {"outside_before", 3, 7, 0, 2},
  {"outside_after", 3, 7, 8, 10},
  {"touch_start", 3, 7, 0, 3},
  {"touch_end", 3, 7, 7, 10},
  {"no_selection", 0, 0, 0, 10},
}

for index, case in ipairs(cases) do
  local name, selection_start, selection_end, item_start, item_end = table.unpack(case)
  reaper.SelectAllMediaItems(0, false)
  reaper.InsertTrackAtIndex(reaper.CountTracks(0), true)
  local track = reaper.GetTrack(0, reaper.CountTracks(0) - 1)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Trim " .. name, true)
  local item = reaper.CreateNewMIDIItemInProj(track, item_start, item_end, false)
  assert(item, "could not create test MIDI item: " .. name)
  reaper.SetMediaItemSelected(item, true)
  reaper.GetSet_LoopTimeRange2(0, true, false, selection_start, selection_end, false)
  local state_before = reaper.GetProjectStateChangeCount(0)
  reaper.Main_OnCommand(40508, 0)
  reaper.UpdateArrange()

  local count = reaper.CountTrackMediaItems(track)
  local first = count > 0 and reaper.GetTrackMediaItem(track, 0) or nil
  local start_after = first and reaper.GetMediaItemInfo_Value(first, "D_POSITION") or -1
  local end_after = first and start_after + reaper.GetMediaItemInfo_Value(first, "D_LENGTH") or -1
  output:write(table.concat({
    name,
    string.format("%.9f", selection_start),
    string.format("%.9f", selection_end),
    string.format("%.9f", item_start),
    string.format("%.9f", item_end),
    tostring(count),
    string.format("%.9f", start_after),
    string.format("%.9f", end_after),
    tostring(reaper.CountSelectedMediaItems(0)),
    tostring(reaper.GetProjectStateChangeCount(0) - state_before),
  }, "\t"), "\n")
  output:flush()
end

reaper.GetSet_LoopTimeRange2(0, true, false, 0, 0, false)

for index = reaper.CountTracks(0) - 1, base_track_count, -1 do
  reaper.DeleteTrack(reaper.GetTrack(0, index))
end
reaper.InsertTrackAtIndex(base_track_count, true)
local visual_track = reaper.GetTrack(0, base_track_count)
reaper.GetSetMediaTrackInfo_String(visual_track, "P_NAME", "Trim visual", true)
local visual_item = reaper.CreateNewMIDIItemInProj(visual_track, 0, 10, false)
assert(visual_item)
reaper.SetMediaItemSelected(visual_item, true)
reaper.GetSet_LoopTimeRange2(0, true, false, 3, 7, false)
reaper.Main_OnCommand(40508, 0)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 10)
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()
output:write("visual_state\tselected MIDI item clipped from 0-10 to 3-7 seconds; time selection 3-7 seconds\n")
output:close()
