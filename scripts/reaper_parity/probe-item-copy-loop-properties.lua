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
local output = assert(io.open(output_dir .. "/item-copy-loop-properties-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.SetOnlyTrackSelected(track)
assert(reaper.InsertMedia(media_path, 0) == 1, "could not insert source WAV")
local item = assert(reaper.GetTrackMediaItem(track, 0))
local take = assert(reaper.GetActiveTake(item))
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 10)
reaper.SetMediaItemInfo_Value(item, "D_VOL", 0.5)
reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0.5)
reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0.75)
reaper.SetMediaItemTakeInfo_Value(take, "D_VOL", 0.25)
reaper.SetMediaItemTakeInfo_Value(take, "D_PAN", 0.5)
reaper.SetMediaItemTakeInfo_Value(take, "D_PLAYRATE", 1.25)
reaper.SetMediaItemSelected(item, true)
reaper.GetSet_LoopTimeRange2(0, true, false, 3, 7, false)
reaper.UpdateArrange()

local function read_fields(target_item, target_take)
  return {
    position = reaper.GetMediaItemInfo_Value(target_item, "D_POSITION"),
    length = reaper.GetMediaItemInfo_Value(target_item, "D_LENGTH"),
    item_volume = reaper.GetMediaItemInfo_Value(target_item, "D_VOL"),
    fade_in = reaper.GetMediaItemInfo_Value(target_item, "D_FADEINLEN"),
    fade_out = reaper.GetMediaItemInfo_Value(target_item, "D_FADEOUTLEN"),
    take_volume = reaper.GetMediaItemTakeInfo_Value(target_take, "D_VOL"),
    take_pan = reaper.GetMediaItemTakeInfo_Value(target_take, "D_PAN"),
    take_playrate = reaper.GetMediaItemTakeInfo_Value(target_take, "D_PLAYRATE"),
    take_start_offset = reaper.GetMediaItemTakeInfo_Value(target_take, "D_STARTOFFS"),
    loop_source = reaper.GetMediaItemInfo_Value(target_item, "B_LOOPSRC"),
  }
end

local original = read_fields(item, take)
reaper.Main_OnCommand(40014, 0)
reaper.SetEditCurPos(12, false, false)
reaper.Main_OnCommand(42398, 0)
reaper.UpdateArrange()
local pasted_item = assert(reaper.GetTrackMediaItem(track, 1), "pasted item is missing")
local pasted_take = assert(reaper.GetActiveTake(pasted_item))
local pasted = read_fields(pasted_item, pasted_take)
local _, section_source_start, section_length = reaper.PCM_Source_GetSectionInfo(
  reaper.GetMediaItemTake_Source(pasted_take), 0, 0, false)

local function copy_range(start_time, end_time, paste_time, index)
  reaper.SetMediaItemSelected(item, true)
  for item_index = 0, reaper.CountTrackMediaItems(track) - 1 do
    local current = reaper.GetTrackMediaItem(track, item_index)
    if current ~= item then reaper.SetMediaItemSelected(current, false) end
  end
  reaper.GetSet_LoopTimeRange2(0, true, false, start_time, end_time, false)
  reaper.Main_OnCommand(40014, 0)
  reaper.SetEditCurPos(paste_time, false, false)
  reaper.Main_OnCommand(42398, 0)
  reaper.UpdateArrange()
  local pasted_range_item = assert(reaper.GetTrackMediaItem(track, index))
  return read_fields(pasted_range_item, assert(reaper.GetActiveTake(pasted_range_item)))
end

local start_edge = copy_range(0, 4, 20, 2)
local complete_item = copy_range(0, 10, 30, 3)

output:write("key\tvalue\n")
output:write("reference\t", reaper.GetAppVersion(), "; Default 7\n")
output:write("action\t40014 Copy loop; 42398 Paste items/tracks\n")
output:write("time_selection\t3.0\t7.0 seconds\n")
output:write("source\t10 second audio item; take rate=1.25; copied selection maps to source section\n")
for _, key in ipairs({
  "position", "length", "item_volume", "fade_in", "fade_out",
  "take_volume", "take_pan", "take_playrate", "take_start_offset", "loop_source",
}) do
  output:write(key, "\toriginal=", string.format("%.9f", original[key]),
    "\tpasted=", string.format("%.9f", pasted[key]), "\n")
end
output:write("pasted_source_section\t", string.format("%.9f", section_source_start),
  "\t", string.format("%.9f", section_length), " seconds\n")
for name, state in pairs({ start_edge = start_edge, complete_item = complete_item }) do
  for _, key in ipairs({ "item_volume", "fade_in", "fade_out", "take_volume", "take_pan", "take_playrate" }) do
    output:write(name, "_", key, "\t", string.format("%.9f", state[key]), "\n")
  end
end
output:write("selected_after_paste\t", reaper.CountSelectedMediaItems(0), "\n")
output:close()
