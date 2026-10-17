-- Run in a fresh, stopped Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_REAPER_PROBE_MEDIA to the deterministic probe WAV and
-- AAADAW_REAPER_PROBE_DIR to an output directory.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local media_path = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA"),
  "set AAADAW_REAPER_PROBE_MEDIA to the probe WAV")
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"),
  "set AAADAW_REAPER_PROBE_DIR to an output directory")
local output = assert(io.open(output_dir .. "/item-split-time-or-razor-native.tsv", "w"))

local function add_track(name)
  local index = reaper.CountTracks(0)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  return track
end

local function add_audio(track, position, length)
  reaper.SetOnlyTrackSelected(track)
  reaper.SetEditCurPos(position, false, false)
  assert(reaper.InsertMedia(media_path, 0) == 1, "could not insert probe audio")
  local item = reaper.GetTrackMediaItem(track, reaper.CountTrackMediaItems(track) - 1)
  assert(item, "inserted audio item is missing")
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", length)
  reaper.SetMediaItemSelected(item, false)
  return item
end

local function razor(track)
  local _, value = reaper.GetSetMediaTrackInfo_String(track, "P_RAZOREDITS", "", false)
  return value
end

local function write_track_items(prefix, tracks)
  for index, track in ipairs(tracks) do
    local count = reaper.CountTrackMediaItems(track)
    local area = razor(track)
    if area == "" then area = "(empty)" end
    output:write(prefix, "\t", index - 1, "\t", count, "\t", area, "\n")
    for item_index = 0, count - 1 do
      local item = reaper.GetTrackMediaItem(track, item_index)
      output:write(prefix, "_item\t", index - 1, "\t", item_index,
        "\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
        "\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
        "\t", reaper.IsMediaItemSelected(item) and "selected" or "unselected", "\n")
    end
  end
end

local function clear_item_selection()
  for track_index = 0, reaper.CountTracks(0) - 1 do
    local track = reaper.GetTrack(0, track_index)
    for item_index = 0, reaper.CountTrackMediaItems(track) - 1 do
      reaper.SetMediaItemSelected(reaper.GetTrackMediaItem(track, item_index), false)
    end
  end
end

local tracks = {
  add_track("Razor area, no item selection"),
  add_track("Time selection, selected item"),
  add_track("Time selection, no item selection"),
  add_track("Outside razor area"),
}
local razor_item = add_audio(tracks[1], 0, 10)
local selected_time_item = add_audio(tracks[2], 0, 10)
local unselected_time_item = add_audio(tracks[3], 0, 10)
local outside_item = add_audio(tracks[4], 0, 10)

for _, track in ipairs(tracks) do
  reaper.GetSetMediaTrackInfo_String(track, "P_RAZOREDITS", "", true)
end
reaper.GetSetMediaTrackInfo_String(tracks[1], "P_RAZOREDITS", '3 7 ""', true)
reaper.GetSet_LoopTimeRange2(0, true, false, 0, 0, false)
reaper.SetEditCurPos(5, false, false)
reaper.SetMediaItemSelected(razor_item, false)
reaper.SetMediaItemSelected(selected_time_item, false)
reaper.SetMediaItemSelected(unselected_time_item, false)
reaper.SetMediaItemSelected(outside_item, false)

output:write("razor_before\t", razor(tracks[1]), "\n")
output:flush()
local before_razor = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40061, 0) -- Item: Split items at time selection or razor edit
reaper.UpdateArrange()
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("action\t40061\tItem: Split items at time selection or razor edit\n")
output:write("razor_state_change_delta\t",
  reaper.GetProjectStateChangeCount(0) - before_razor, "\n")
write_track_items("razor", tracks)
local result_project = output_dir .. "/item-split-razor-result.rpp"
output:write("razor_result_project\titem-split-razor-result.rpp\n")
reaper.Main_SaveProjectEx(0, result_project, 0)
output:flush()

for _, item in ipairs({ selected_time_item, unselected_time_item, outside_item }) do
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 10)
end
clear_item_selection()
reaper.SetMediaItemSelected(selected_time_item, true)
reaper.SetMediaItemSelected(unselected_time_item, false)
reaper.SetMediaItemSelected(outside_item, false)
for _, track in ipairs(tracks) do
  reaper.GetSetMediaTrackInfo_String(track, "P_RAZOREDITS", "", true)
end
reaper.GetSet_LoopTimeRange2(0, true, false, 4, 6, false)
local before_selected_time = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40061, 0)
reaper.UpdateArrange()
output:write("selected_time_state_change_delta\t",
  reaper.GetProjectStateChangeCount(0) - before_selected_time, "\n")
write_track_items("selected_time", tracks)
output:flush()

for _, item in ipairs({ unselected_time_item, outside_item }) do
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 10)
end
clear_item_selection()
reaper.GetSet_LoopTimeRange2(0, true, false, 4, 6, false)
local before_unselected_time = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40061, 0)
reaper.UpdateArrange()
output:write("unselected_time_state_change_delta\t",
  reaper.GetProjectStateChangeCount(0) - before_unselected_time, "\n")
write_track_items("unselected_time", tracks)

output:flush()
output:close()
