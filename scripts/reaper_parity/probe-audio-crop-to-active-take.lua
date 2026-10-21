-- Run in an empty Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_REAPER_PROBE_MEDIA_A, AAADAW_REAPER_PROBE_MEDIA_B, and
-- AAADAW_REAPER_PROBE_DIR before launching REAPER with this script path.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")

local media_a = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_A"))
local media_b = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_B"))
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/audio-crop-to-active-take-native.tsv", "w"))
local project_base = output_dir .. "/audio-crop-to-active-take"
if reaper.GetToggleCommandStateEx(0, 1157) == 1 then
  reaper.Main_OnCommand(1157, 0) -- Options: Toggle snapping.
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

local function state(label, item)
  local count = reaper.CountTakes(item)
  local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
  output:write(label, "\ttake_count=", count,
    "\tactive_index=", active_index,
    "\tposition=", reaper.GetMediaItemInfo_Value(item, "D_POSITION"),
    "\tlength=", reaper.GetMediaItemInfo_Value(item, "D_LENGTH"),
    "\titem_volume=", reaper.GetMediaItemInfo_Value(item, "D_VOL"), "\n")
  for index = 0, count - 1 do
    local take = assert(reaper.GetTake(item, index))
    output:write(label, "\ttake_", index,
      "\tname=", take_name(take),
      "\tsource=", take_source(take),
      "\tsource_offset=", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS"),
      "\tvolume=", reaper.GetMediaItemTakeInfo_Value(take, "D_VOL"),
      "\tpan=", reaper.GetMediaItemTakeInfo_Value(take, "D_PAN"),
      "\tplay_rate=", reaper.GetMediaItemTakeInfo_Value(take, "D_PLAYRATE"), "\n")
  end
  output:flush()
end

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local saved = io.open(path, "r")
  assert(saved, "could not save project " .. suffix)
  saved:close()
end

local track = nil
local function new_item(label, take_a_active, single_take)
  if not track then
    reaper.InsertTrackAtIndex(0, true)
    track = assert(reaper.GetTrack(0, 0))
  else
    for index = reaper.CountTrackMediaItems(track) - 1, 0, -1 do
      reaper.DeleteTrackMediaItem(track, reaper.GetTrackMediaItem(track, index))
    end
  end
  reaper.SetOnlyTrackSelected(track)
  assert(reaper.InsertMedia(media_a, 0) == 1, "could not insert Take A source")
  local item = assert(reaper.GetTrackMediaItem(track, reaper.CountTrackMediaItems(track) - 1))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", 1.0)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 2.0)
  reaper.SetMediaItemInfo_Value(item, "D_VOL", 0.8)
  local take_a = assert(reaper.GetActiveTake(item))
  reaper.GetSetMediaItemTakeInfo_String(take_a, "P_NAME", "Take A", true)
  reaper.SetMediaItemTakeInfo_Value(take_a, "D_STARTOFFS", 0.125)
  reaper.SetMediaItemTakeInfo_Value(take_a, "D_VOL", 0.7)
  reaper.SetMediaItemTakeInfo_Value(take_a, "D_PAN", -0.25)
  reaper.SetMediaItemTakeInfo_Value(take_a, "D_PLAYRATE", 1.25)
  if not single_take then
    reaper.SetMediaItemSelected(item, true)
    reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
    assert(reaper.CountTakes(item) == 2, "duplicate did not add a second take")
    local take_b = assert(reaper.GetTake(item, 0))
    local source_b = assert(reaper.PCM_Source_CreateFromFile(media_b), "could not load Take B source")
    assert(reaper.SetMediaItemTake_Source(take_b, source_b), "could not assign Take B source")
    reaper.GetSetMediaItemTakeInfo_String(take_b, "P_NAME", "Take B", true)
    reaper.SetMediaItemTakeInfo_Value(take_b, "D_STARTOFFS", 0.375)
    reaper.SetMediaItemTakeInfo_Value(take_b, "D_VOL", 0.9)
    reaper.SetMediaItemTakeInfo_Value(take_b, "D_PAN", 0.4)
    reaper.SetMediaItemTakeInfo_Value(take_b, "D_PLAYRATE", 0.8)
    reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", take_a_active and 1 or 0)
  end
  reaper.GetSetMediaItemInfo_String(item, "P_NOTES", label, true)
  return item
end

local function crop_case(label, item, make_visible)
  reaper.SelectAllMediaItems(0, false)
  reaper.SetMediaItemSelected(item, true)
  reaper.Undo_OnStateChange2(0, "Prepare " .. label)
  state(label .. "_before", item)
  save(label .. "-before")

  reaper.Main_OnCommand(40131, 0) -- Take: Crop to active take in items.
  state(label .. "_after", item)
  save(label .. "-after")
  local undo_description = reaper.Undo_CanUndo2(0) or ""
  reaper.Undo_DoUndo2(0)
  state(label .. "_undo", item)
  save(label .. "-undo")
  reaper.Undo_DoRedo2(0)
  state(label .. "_redo", item)
  save(label .. "-redo")
  output:write(label, "_undo_description\t", undo_description, "\n")
  output:flush()
  if make_visible then
    reaper.SetEditCurPos(1.0, false, false)
    reaper.UpdateArrange()
  end
end

local single = new_item("Single-take no-op", false, true)
reaper.SetMediaItemSelected(single, true)
reaper.Undo_OnStateChange2(0, "Prepare single-take no-op probe")
save("single-before")
local single_undo_before = reaper.Undo_CanUndo2(0) or ""
reaper.Main_OnCommand(40131, 0)
state("single_after", single)
save("single-after")
output:write("single_undo_before\t", single_undo_before, "\n")
output:write("single_undo_after\t", reaper.Undo_CanUndo2(0) or "", "\n")

local active_a = new_item("Take A active at index one", true, false)
crop_case("active-a", active_a, false)

local active_b = new_item("Take B active at index zero", false, false)
crop_case("active-b", active_b, true)
output:write("action\t40131\tTake: Crop to active take in items\n")
output:close()

local marker = assert(io.open(output_dir .. "/capture-complete", "w"))
marker:write("ready for live-window capture\n")
marker:close()
