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
local output = assert(io.open(output_dir .. "/item-copy-loop-name-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.SetOnlyTrackSelected(track)
assert(reaper.InsertMedia(media_path, 0) == 1, "could not insert source WAV")
local item = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 10)
local take = assert(reaper.GetActiveTake(item))

local function take_name(target_take)
  local ok, name = reaper.GetSetMediaItemTakeInfo_String(target_take, "P_NAME", "", false)
  assert(ok, "could not read take name")
  return name
end

local function paste_copy_from(source_item, source_take, name, selection_start, selection_end, paste_position, item_index)
  reaper.GetSetMediaItemTakeInfo_String(source_take, "P_NAME", name, true)
  assert(take_name(source_take) == name, "take name did not round-trip before copy")
  for index = 0, reaper.CountTrackMediaItems(track) - 1 do
    local current = reaper.GetTrackMediaItem(track, index)
    reaper.SetMediaItemSelected(current, current == source_item)
  end
  reaper.GetSet_LoopTimeRange2(0, true, false, selection_start, selection_end, false)
  reaper.Main_OnCommand(40014, 0)
  reaper.SetEditCurPos(paste_position, false, false)
  reaper.Main_OnCommand(42398, 0)
  reaper.UpdateArrange()
  local pasted_item = assert(reaper.GetTrackMediaItem(track, item_index), "pasted item is missing")
  local pasted_take = assert(reaper.GetActiveTake(pasted_item))
  return pasted_item, pasted_take, take_name(pasted_take)
end

local named_pasted_item, named_pasted_take, named_pasted_name = paste_copy_from(item, take, "Lead phrase", 3, 7, 12, 1)
local _, _, unnamed_pasted_name = paste_copy_from(item, take, "", 3, 7, 20, 2)
local _, _, repeated_pasted_name = paste_copy_from(
  named_pasted_item, named_pasted_take, named_pasted_name, 12, 16, 25, 3)
output:write("key\tvalue\n")
output:write("reference\t", reaper.GetAppVersion(), "; Default 7\n")
output:write("action\t40014 Copy loop; 42398 Paste items/tracks\n")
output:write("selection\t3.0–7.0 seconds\n")
output:write("named_source_take\tLead phrase\n")
output:write("named_pasted_take\t", named_pasted_name, "\n")
output:write("unnamed_source_take\t(empty; source label is displayed)\n")
output:write("unnamed_pasted_take\t", unnamed_pasted_name, "\n")
output:write("repeated_loop_source_take\t", named_pasted_name, "\n")
output:write("repeated_loop_pasted_take\t", repeated_pasted_name, "\n")
output:close()
