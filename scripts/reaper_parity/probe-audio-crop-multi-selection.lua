-- Run in an empty Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_REAPER_PROBE_MEDIA_A/B and AAADAW_REAPER_PROBE_DIR.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")

if reaper.GetToggleCommandStateEx(0, 1157) == 1 then
  reaper.Main_OnCommand(1157, 0) -- Options: Toggle snapping.
end
local media_a = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_A"))
local media_b = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_B"))
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/audio-crop-multi-selection-native.tsv", "w"))
local project_base = output_dir .. "/audio-crop-multi-selection"

local function source_path(take)
  local source = assert(reaper.GetMediaItemTake_Source(take))
  return reaper.GetMediaSourceFileName(source, "")
end

local function item_state(label, item)
  output:write(label,
    "\tposition=", reaper.GetMediaItemInfo_Value(item, "D_POSITION"),
    "\tlength=", reaper.GetMediaItemInfo_Value(item, "D_LENGTH"),
    "\ttake_count=", reaper.CountTakes(item),
    "\tactive_index=", math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE")), "\n")
  for index = 0, reaper.CountTakes(item) - 1 do
    local take = assert(reaper.GetTake(item, index))
    local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
    output:write(label, "\ttake_", index,
      "\tname=", name,
      "\tsource=", source_path(take),
      "\tsource_offset=", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS"),
      "\tvolume=", reaper.GetMediaItemTakeInfo_Value(take, "D_VOL"),
      "\tpan=", reaper.GetMediaItemTakeInfo_Value(take, "D_PAN"),
      "\tplay_rate=", reaper.GetMediaItemTakeInfo_Value(take, "D_PLAYRATE"), "\n")
  end
end

local function assert_item(item, take_count, active_index, active_source, label)
  assert(reaper.CountTakes(item) == take_count, label .. " has an unexpected take count")
  assert(math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE")) == active_index,
    label .. " has an unexpected active index")
  assert(source_path(assert(reaper.GetActiveTake(item))) == active_source,
    label .. " has an unexpected active source")
end

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local saved = io.open(path, "r")
  assert(saved, "could not save project " .. suffix)
  saved:close()
end

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.SetOnlyTrackSelected(track)

local function make_item(label, position, active_take_index, with_second_take)
  reaper.SelectAllMediaItems(0, false)
  assert(reaper.InsertMedia(media_a, 0) == 1, "could not insert Take A source")
  local item = assert(reaper.GetSelectedMediaItem(0, 0), "new source item should be selected")
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 2)
  reaper.SetMediaItemInfo_Value(item, "D_VOL", 0.8)
  reaper.GetSetMediaItemInfo_String(item, "P_NOTES", label, true)
  local take_a = assert(reaper.GetActiveTake(item))
  reaper.GetSetMediaItemTakeInfo_String(take_a, "P_NAME", "Take A", true)
  reaper.SetMediaItemTakeInfo_Value(take_a, "D_STARTOFFS", 0.125)
  reaper.SetMediaItemTakeInfo_Value(take_a, "D_VOL", 0.7)
  reaper.SetMediaItemTakeInfo_Value(take_a, "D_PAN", -0.25)
  reaper.SetMediaItemTakeInfo_Value(take_a, "D_PLAYRATE", 1.25)
  if with_second_take then
    reaper.SetMediaItemSelected(item, true)
    reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
    local take_b = assert(reaper.GetTake(item, 0))
    local source_b = assert(reaper.PCM_Source_CreateFromFile(media_b), "could not load Take B source")
    assert(reaper.SetMediaItemTake_Source(take_b, source_b), "could not assign Take B source")
    reaper.GetSetMediaItemTakeInfo_String(take_b, "P_NAME", "Take B", true)
    reaper.SetMediaItemTakeInfo_Value(take_b, "D_STARTOFFS", 0.375)
    reaper.SetMediaItemTakeInfo_Value(take_b, "D_VOL", 0.9)
    reaper.SetMediaItemTakeInfo_Value(take_b, "D_PAN", 0.4)
    reaper.SetMediaItemTakeInfo_Value(take_b, "D_PLAYRATE", 0.8)
    reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", active_take_index)
  end
  return item
end

local take_a_active = make_item("Take A active at index one", 1, 1, true)
local take_b_active = make_item("Take B active at index zero", 4, 0, true)
local one_take = make_item("Single-take item", 7, 0, false)
local items = { take_a_active, take_b_active, one_take }
for _, item in ipairs(items) do reaper.SetMediaItemSelected(item, true) end
assert(reaper.CountSelectedMediaItems(0) == 3, "probe must select all three items")
assert_item(take_a_active, 2, 1, media_a, "Take A item before crop")
assert_item(take_b_active, 2, 0, media_b, "Take B item before crop")
assert_item(one_take, 1, 0, media_a, "single-take item before crop")
reaper.Undo_OnStateChange2(0, "Prepare multi-selection crop probe")
for index, item in ipairs(items) do item_state("before_" .. index, item) end
save("before")

reaper.Main_OnCommand(40131, 0) -- Take: Crop to active take in items.
assert_item(take_a_active, 1, 0, media_a, "Take A item after crop")
assert_item(take_b_active, 1, 0, media_b, "Take B item after crop")
assert_item(one_take, 1, 0, media_a, "single-take item after crop")
for index, item in ipairs(items) do item_state("after_" .. index, item) end
save("after")
local undo_description = reaper.Undo_CanUndo2(0) or ""
assert(undo_description == "Crop to active take", "native action should create one crop undo entry")
reaper.Undo_DoUndo2(0)
assert_item(take_a_active, 2, 1, media_a, "Take A item after one Undo")
assert_item(take_b_active, 2, 0, media_b, "Take B item after one Undo")
assert_item(one_take, 1, 0, media_a, "single-take item after one Undo")
for index, item in ipairs(items) do item_state("undo_" .. index, item) end
save("undo")
reaper.Undo_DoRedo2(0)
assert_item(take_a_active, 1, 0, media_a, "Take A item after one Redo")
assert_item(take_b_active, 1, 0, media_b, "Take B item after one Redo")
assert_item(one_take, 1, 0, media_a, "single-take item after one Redo")
for index, item in ipairs(items) do item_state("redo_" .. index, item) end
save("redo")
output:write("undo_description\t", undo_description, "\n")
output:write("action\t40131\tTake: Crop to active take in items\n")
output:close()

reaper.SetEditCurPos(4, false, false)
reaper.UpdateArrange()
local marker = assert(io.open(output_dir .. "/capture-complete", "w"))
marker:write("ready for live-window capture\n")
marker:close()
