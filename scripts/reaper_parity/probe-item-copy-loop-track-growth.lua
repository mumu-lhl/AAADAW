-- Run in an empty, stopped Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_REAPER_PROBE_MEDIA and AAADAW_REAPER_PROBE_DIR.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")

local media_path = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA"))
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/item-copy-loop-track-growth-native.tsv", "w"))
local function create_item(track)
  local item = reaper.AddMediaItemToTrack(track)
  local take = reaper.AddTakeToMediaItem(item)
  assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(media_path)))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 10)
  return item
end

reaper.InsertTrackAtIndex(0, true)
reaper.InsertTrackAtIndex(1, true)
local first_track = reaper.GetTrack(0, 0)
local second_track = reaper.GetTrack(0, 1)
reaper.GetSetMediaTrackInfo_String(first_track, "P_NAME", "Copy source A", true)
reaper.GetSetMediaTrackInfo_String(second_track, "P_NAME", "Copy source B", true)
local first_item = create_item(first_track)
local second_item = create_item(second_track)
reaper.SetMediaItemSelected(first_item, true)
reaper.SetMediaItemSelected(second_item, true)
reaper.GetSet_LoopTimeRange2(0, true, false, 3, 7, false)
reaper.Main_OnCommand(40014, 0)

local function paste_at(position)
  reaper.SetEditCurPos(position, false, false)
  reaper.Main_OnCommand(42398, 0)
  reaper.UpdateArrange()
end

local function snapshot(label)
  output:write(label, "_track_count\t", reaper.CountTracks(0), "\n")
  for track_index = 0, reaper.CountTracks(0) - 1 do
    local track = reaper.GetTrack(0, track_index)
    local _, name = reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "", false)
    output:write(label, "_track\t", track_index + 1, "\t", name,
      "\t", reaper.CountTrackMediaItems(track), "\n")
    for item_index = 0, reaper.CountTrackMediaItems(track) - 1 do
      local item = reaper.GetTrackMediaItem(track, item_index)
      local take = reaper.GetActiveTake(item)
      output:write(label, "_item\t", track_index + 1,
        "\t", string.format("%.3f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
        "\t", string.format("%.3f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
        "\t", take and string.format("%.3f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")) or "none",
        "\n")
    end
  end
end

output:write("reference\t", reaper.GetAppVersion(), "; Default 7\n")
output:write("actions\t40014 Copy loop; 42398 Paste items/tracks\n")
output:write("selection\t3.0–7.0 seconds; source items span 0–10 seconds on two tracks\n")
output:write("track_deletion\tcopy first, then delete source track B and its item\n")
reaper.DeleteTrack(second_track)
paste_at(12)
snapshot("one_track_remains")

output:write("track_deletion_empty\tdelete all tracks after the first paste, then paste again\n")
while reaper.CountTracks(0) > 0 do
  reaper.DeleteTrack(reaper.GetTrack(0, reaper.CountTracks(0) - 1))
end
paste_at(30)
snapshot("no_tracks_remain")
output:write("selected_after_empty_project_paste\t", reaper.CountSelectedMediaItems(0), "\n")
output:close()
