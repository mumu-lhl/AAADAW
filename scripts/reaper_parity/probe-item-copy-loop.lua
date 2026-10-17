-- Run against a fresh, unsaved, stopped Linux REAPER 7.82 / Default 7 project.
-- Generate the fixture with generate-item-copy-loop-wav.py, then set
-- AAADAW_REAPER_PROBE_MEDIA and optionally AAADAW_REAPER_PROBE_DIR.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
for track_index = 0, reaper.CountTracks(0) - 1 do
  assert(reaper.CountTrackMediaItems(reaper.GetTrack(0, track_index)) == 0,
    "probe requires an empty project")
end

local media_path = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA"),
  "set AAADAW_REAPER_PROBE_MEDIA to the probe WAV")
local output_dir = os.getenv("AAADAW_REAPER_PROBE_DIR") or "/tmp"
local output_path = output_dir .. "/item-copy-loop-native.tsv"
local output = assert(io.open(output_path, "w"))
local base_track_count = reaper.CountTracks(0)
reaper.InsertTrackAtIndex(base_track_count, true)
local track = reaper.GetTrack(0, base_track_count)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Copy loop probe", true)
reaper.SetOnlyTrackSelected(track)
reaper.SetEditCurPos(0, false, false)
assert(reaper.InsertMedia(media_path, 0) == 1, "could not insert probe audio")
local item = reaper.GetTrackMediaItem(track, 0)
assert(item, "inserted audio item is missing")
reaper.SetMediaItemSelected(item, true)
reaper.GetSet_LoopTimeRange2(0, true, false, 3, 7, false)
local take = reaper.GetActiveTake(item)
assert(take, "inserted audio item has no active take")

local function state()
  return {
    position = reaper.GetMediaItemInfo_Value(item, "D_POSITION"),
    length = reaper.GetMediaItemInfo_Value(item, "D_LENGTH"),
    source_offset = reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS"),
    playrate = reaper.GetMediaItemTakeInfo_Value(take, "D_PLAYRATE"),
    loop_source = reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC"),
  }
end

local before = state()
local state_before = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40014, 0)
reaper.UpdateArrange()
local after = state()
local state_delta = reaper.GetProjectStateChangeCount(0) - state_before
local items_after_copy = reaper.CountTrackMediaItems(track)
local selected_after_copy = reaper.CountSelectedMediaItems(0)
reaper.SetEditCurPos(12, false, false)
reaper.Main_OnCommand(42398, 0) -- Item: Paste items/tracks
reaper.UpdateArrange()
local count_after_loop_copy_paste = reaper.CountTrackMediaItems(track)
local pasted_count = count_after_loop_copy_paste - 1
local loop_paste_item = reaper.GetTrackMediaItem(track, 1)
local loop_paste_take = loop_paste_item and reaper.GetActiveTake(loop_paste_item)
local loop_paste_source = loop_paste_take and reaper.GetMediaItemTake_Source(loop_paste_take)
local source_length = loop_paste_source and reaper.GetMediaSourceLength(loop_paste_source) or 0
local source_file = loop_paste_source and reaper.GetMediaSourceFileName(loop_paste_source, "") or ""
local section_result, section_start, section_length, section_reversed = false, 0, 0, false
if loop_paste_source then
  section_result, section_start, section_length, section_reversed =
    reaper.PCM_Source_GetSectionInfo(loop_paste_source, 0, 0, false)
end
reaper.Main_OnCommand(40057, 0) -- Edit: Copy items/tracks, ignoring time selection
reaper.SetEditCurPos(20, false, false)
reaper.Main_OnCommand(42398, 0)
reaper.UpdateArrange()
local control_pasted_count = reaper.CountTrackMediaItems(track) - count_after_loop_copy_paste
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("audio\tDummy Audio if available; no recording or playback\n")
output:write("transport\tstopped\n")
output:write("action\t40014\tItem: Copy loop of selected area of audio items\n")
output:write("selection\t3.000000000\t7.000000000\n")
output:write("field\tbefore\tafter\n")
for _, field in ipairs({ "position", "length", "source_offset", "playrate", "loop_source" }) do
  output:write(field, "\t", string.format("%.9f", before[field]), "\t",
    string.format("%.9f", after[field]), "\n")
end
output:write("items_after_copy\t", items_after_copy, "\n")
output:write("selected_after_copy\t", selected_after_copy, "\n")
output:write("state_change_delta\t", state_delta, "\n")
output:write("pasted_items\t", pasted_count, "\n")
output:write("pasted_source_length\t", string.format("%.9f", source_length), "\n")
output:write("pasted_source_file\t", source_file:match("([^/]+)$") or source_file, "\n")
output:write("pasted_source_section\t", tostring(section_result),
  "\t", string.format("%.9f", section_start),
  "\t", string.format("%.9f", section_length),
  "\t", tostring(section_reversed), "\n")
output:write("control_copy_pasted_items\t", control_pasted_count, "\n")
for index = 1, reaper.CountTrackMediaItems(track) - 1 do
  local pasted_item = reaper.GetTrackMediaItem(track, index)
  local pasted_take = reaper.GetActiveTake(pasted_item)
  output:write("pasted\t", index,
    "\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(pasted_item, "D_POSITION")),
    "\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(pasted_item, "D_LENGTH")),
    "\t", pasted_take and string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(pasted_take, "D_STARTOFFS")) or "none",
    "\t", pasted_take and string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(pasted_take, "D_PLAYRATE")) or "none",
    "\t", reaper.GetMediaItemInfo_Value(pasted_item, "B_LOOPSRC"),
    "\n")
end

local second_track_index = reaper.CountTracks(0)
reaper.InsertTrackAtIndex(second_track_index, true)
local second_track = reaper.GetTrack(0, second_track_index)
reaper.GetSetMediaTrackInfo_String(second_track, "P_NAME", "Copy loop probe 2", true)
reaper.SetOnlyTrackSelected(second_track)
reaper.SetEditCurPos(0, false, false)
assert(reaper.InsertMedia(media_path, 0) == 1, "could not insert second probe audio")
local second_item = reaper.GetTrackMediaItem(second_track, 0)
assert(second_item, "second inserted audio item is missing")
reaper.SetMediaItemInfo_Value(second_item, "D_POSITION", 4)
for index = 0, reaper.CountMediaItems(0) - 1 do
  reaper.SetMediaItemSelected(reaper.GetMediaItem(0, index), false)
end
reaper.SetMediaItemSelected(item, true)
reaper.SetMediaItemSelected(second_item, true)
local partial_before = reaper.CountMediaItems(0)
local partial_state_before = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40014, 0)
local partial_after_copy = reaper.CountMediaItems(0)
local partial_state_delta = reaper.GetProjectStateChangeCount(0) - partial_state_before
reaper.SetEditCurPos(25, false, false)
reaper.Main_OnCommand(42398, 0)
reaper.UpdateArrange()
local partial_after_paste = reaper.CountMediaItems(0)
output:write("multi_partial_selected_items\t2\n")
output:write("multi_partial_item_spans\t0.000000000-10.000000000\t4.000000000-14.000000000\n")
output:write("multi_partial_selection\t3.000000000\t7.000000000\n")
output:write("multi_partial_items_before_copy\t", partial_before, "\n")
output:write("multi_partial_items_after_copy\t", partial_after_copy, "\n")
output:write("multi_partial_state_change_delta\t", partial_state_delta, "\n")
output:write("multi_partial_pasted_items\t", partial_after_paste - partial_before, "\n")

-- Both selected items fully contain the 3-7s selection. Paste retains their
-- relative track positions when more than one item qualifies.
reaper.SetMediaItemInfo_Value(second_item, "D_POSITION", 0)
for index = 0, reaper.CountMediaItems(0) - 1 do
  reaper.SetMediaItemSelected(reaper.GetMediaItem(0, index), false)
end
reaper.SetMediaItemSelected(item, true)
reaper.SetMediaItemSelected(second_item, true)
local full_before = reaper.CountMediaItems(0)
local full_state_before = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40014, 0)
local full_after_copy = reaper.CountMediaItems(0)
local full_state_delta = reaper.GetProjectStateChangeCount(0) - full_state_before
reaper.SetEditCurPos(30, false, false)
reaper.Main_OnCommand(42398, 0)
reaper.UpdateArrange()
local full_after_paste = reaper.CountMediaItems(0)
output:write("multi_full_selected_items\t2\n")
output:write("multi_full_selection\t3.000000000\t7.000000000\n")
output:write("multi_full_items_before_copy\t", full_before, "\n")
output:write("multi_full_items_after_copy\t", full_after_copy, "\n")
output:write("multi_full_state_change_delta\t", full_state_delta, "\n")
output:write("multi_full_pasted_items\t", full_after_paste - full_before, "\n")
for track_index = 0, reaper.CountTracks(0) - 1 do
  local probe_track = reaper.GetTrack(0, track_index)
  for index = 0, reaper.CountTrackMediaItems(probe_track) - 1 do
    local probe_item = reaper.GetTrackMediaItem(probe_track, index)
    local position = reaper.GetMediaItemInfo_Value(probe_item, "D_POSITION")
    if position >= 30 then
      output:write("multi_full_pasted\t", track_index,
        "\t", string.format("%.9f", position),
        "\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(probe_item, "D_LENGTH")),
        "\t", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(
          reaper.GetActiveTake(probe_item), "D_STARTOFFS")),
        "\n")
    end
  end
end
output:flush()

reaper.GetSet_LoopTimeRange2(0, true, false, 0, 0, false)
reaper.DeleteTrack(track)
reaper.UpdateArrange()
output:close()
