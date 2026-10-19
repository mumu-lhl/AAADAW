-- Run in an empty, stopped Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_REAPER_PROBE_MEDIA_A/B and AAADAW_REAPER_PROBE_DIR.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")

local media_a = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_A"))
local media_b = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_B"))
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/audio-delete-active-take-native.tsv", "w"))
local project_base = output_dir .. "/audio-delete-active-take"
local track

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local saved = io.open(path, "r")
  assert(saved, "could not save project " .. suffix)
  saved:close()
  return path
end

local function take_name(take)
  local ok, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
  assert(ok, "could not read take name")
  return name
end

local function take_source(take)
  local source = assert(reaper.GetMediaItemTake_Source(take))
  return reaper.GetMediaSourceFileName(source, "")
end

local function item_state(label, item)
  local exists = reaper.ValidatePtr2(0, item, "MediaItem*")
  output:write(label, "\texists=", exists and "1" or "0",
    "\ttrack_item_count=", reaper.CountTrackMediaItems(track), "\n")
  if exists then
    local count = reaper.CountTakes(item)
    local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
    output:write(label, "\ttake_count=", count, "\tactive_index=", active_index, "\n")
    for index = 0, count - 1 do
      local take = assert(reaper.GetTake(item, index))
      output:write(label, "\ttake_", index, "\tname=", take_name(take),
        "\tsource=", take_source(take), "\n")
    end
  end
  output:flush()
end

local function insert_item(name, position)
  assert(reaper.InsertMedia(media_a, 0) == 1, "could not insert source WAV")
  local item = assert(reaper.GetTrackMediaItem(track, reaper.CountTrackMediaItems(track) - 1))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 2)
  local take = assert(reaper.GetActiveTake(item))
  reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", name, true)
  return item
end

local function find_item_at(position)
  for index = 0, reaper.CountTrackMediaItems(track) - 1 do
    local item = reaper.GetTrackMediaItem(track, index)
    if math.abs(reaper.GetMediaItemInfo_Value(item, "D_POSITION") - position) < 0.000001 then
      return item
    end
  end
end

local function dump_track_items(label)
  for index = 0, reaper.CountTrackMediaItems(track) - 1 do
    local item = reaper.GetTrackMediaItem(track, index)
    local position = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
    local active = reaper.GetActiveTake(item)
    output:write(label, "\tindex=", index, "\tposition=", position,
      "\tactive_name=", active and take_name(active) or "<none>", "\n")
  end
  output:flush()
end

reaper.InsertTrackAtIndex(0, true)
track = assert(reaper.GetTrack(0, 0))
reaper.SetOnlyTrackSelected(track)
local multi = insert_item("Take A", 0)
reaper.SetMediaItemSelected(multi, true)
reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
assert(reaper.CountTakes(multi) == 2, "duplicate did not add the second take")
local second = assert(reaper.GetTake(multi, 1))
local source_b = assert(reaper.PCM_Source_CreateFromFile(media_b), "could not load second source")
assert(reaper.SetMediaItemTake_Source(second, source_b), "could not assign second source")
reaper.GetSetMediaItemTakeInfo_String(second, "P_NAME", "Take B", true)
reaper.SetMediaItemInfo_Value(multi, "I_CURTAKE", 1)
local single = insert_item("Single", 4)
reaper.Undo_OnStateChange2(0, "Prepare active-take deletion probe")
save("before")
item_state("before_multi", multi)
item_state("before_single", single)

reaper.SetMediaItemSelected(multi, false)
reaper.SetMediaItemSelected(single, true)
reaper.Undo_BeginBlock2(0)
reaper.Main_OnCommand(40129, 0) -- Take: Delete active take from items.
reaper.Undo_EndBlock2(0, "Delete active single take", -1)
item_state("after_single_delete", single)
save("after-single-delete")
output:write("undo_before_single\t", reaper.Undo_CanUndo2(0) or "", "\n")
output:flush()
reaper.Undo_DoUndo2(0)
track = assert(reaper.GetTrack(0, 0), "track was not restored")
output:write("undo_single_track_items\t", reaper.CountTrackMediaItems(track), "\n")
dump_track_items("undo_single_item")
single = find_item_at(4)
output:write("undo_single_restored\t", single and "1" or "0", "\n")
output:flush()
if single then item_state("undo_single_delete", single) end
save("undo-single-delete")

multi = assert(find_item_at(0), "multi-take item was not found after single-take undo")
if single then reaper.SetMediaItemSelected(single, false) end
reaper.SetMediaItemSelected(multi, true)
reaper.Undo_BeginBlock2(0)
reaper.Main_OnCommand(40129, 0) -- Take: Delete active take from items.
reaper.Undo_EndBlock2(0, "Delete active take from multiple-take item", -1)
item_state("after_multi_delete", multi)
save("after-multi-delete")
output:write("undo_before_multi\t", reaper.Undo_CanUndo2(0) or "", "\n")
output:flush()
reaper.Undo_DoUndo2(0)
track = assert(reaper.GetTrack(0, 0), "track was not restored after multi-take undo")
multi = assert(find_item_at(0), "multi-take item was not restored by undo")
item_state("undo_multi_delete", multi)
save("undo-multi-delete")
reaper.Undo_DoRedo2(0)
track = assert(reaper.GetTrack(0, 0), "track was not restored after multi-take redo")
multi = assert(find_item_at(0), "multi-take item was not restored by redo")
item_state("redo_multi_delete", multi)
save("redo-multi-delete")

reaper.Undo_DoUndo2(0)
track = assert(reaper.GetTrack(0, 0), "track was not restored for first-take deletion")
multi = assert(find_item_at(0), "multi-take item was not restored before first-take deletion")
reaper.SetMediaItemInfo_Value(multi, "I_CURTAKE", 0)
reaper.Undo_OnStateChange2(0, "Select first take for deletion probe")
item_state("before_delete_first_take", multi)
save("before-delete-first-take")
reaper.Undo_BeginBlock2(0)
reaper.Main_OnCommand(40129, 0) -- Take: Delete active take from items.
reaper.Undo_EndBlock2(0, "Delete first active take", -1)
item_state("after_delete_first_take", multi)
save("after-delete-first-take")
reaper.Undo_DoUndo2(0)
track = assert(reaper.GetTrack(0, 0), "track was not restored after first-take undo")
multi = assert(find_item_at(0), "multi-take item was not restored after first-take undo")
item_state("undo_delete_first_take", multi)
save("undo-delete-first-take")
reaper.Undo_DoRedo2(0)
track = assert(reaper.GetTrack(0, 0), "track was not restored after first-take redo")
multi = assert(find_item_at(0), "multi-take item was not restored after first-take redo")
item_state("redo_delete_first_take", multi)
save("redo-delete-first-take")

local _, saved_path = reaper.EnumProjects(-1, "")
output:write("project\t", saved_path or "", "\n")
output:write("action\t40129\tTake: Delete active take from items\n")
output:close()
local marker = assert(io.open(output_dir .. "/capture-complete", "w"))
marker:write("ready for live-window capture\n")
marker:close()
reaper.UpdateArrange()
