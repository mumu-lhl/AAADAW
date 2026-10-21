-- Run in an empty, stopped Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_REAPER_PROBE_MEDIA_A/B and AAADAW_REAPER_PROBE_DIR.
-- After await-undo, send Ctrl+Z and create undo-request in the output dir.
-- After await-redo, send Ctrl+Shift+Z and create redo-request in the output dir.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")

local media_a = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_A"))
local media_b = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_B"))
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/audio-delete-active-take-multi-native.tsv", "w"))
local project_base = output_dir .. "/audio-delete-active-take-multi"
local track

local function source_path(take)
  local source = assert(reaper.GetMediaItemTake_Source(take))
  return reaper.GetMediaSourceFileName(source, "")
end

local function state(label, position)
  local item
  for index = 0, reaper.CountTrackMediaItems(track) - 1 do
    local candidate = assert(reaper.GetTrackMediaItem(track, index))
    if math.abs(reaper.GetMediaItemInfo_Value(candidate, "D_POSITION") - position) < 0.000001 then
      item = candidate
      break
    end
  end
  output:write(label, "\tposition=", position, "\texists=", item and "1" or "0",
    "\ttrack_item_count=", reaper.CountTrackMediaItems(track), "\n")
  if item then
    local count = reaper.CountTakes(item)
    local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
    output:write(label, "\ttake_count=", count, "\tactive_index=", active_index, "\n")
    for index = 0, count - 1 do
      local take = assert(reaper.GetTake(item, index))
      local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
      output:write(label, "\ttake_", index, "\tname=", name,
        "\tsource=", source_path(take), "\n")
    end
  end
  output:flush()
  return item
end

local function assert_item(position, count, active_index, active_source, label)
  local item = state("check_" .. label, position)
  assert(item, label .. " item should exist")
  assert(reaper.CountTakes(item) == count, label .. " has unexpected take count")
  local actual_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
  assert(actual_index == active_index,
    label .. " has active index " .. actual_index .. ", expected " .. active_index)
  assert(source_path(assert(reaper.GetActiveTake(item))) == active_source,
    label .. " has an unexpected active source")
  return item
end

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local saved = assert(io.open(path, "r"), "could not save " .. suffix .. " project")
  saved:close()
end

local function find_item(position)
  for index = 0, reaper.CountTrackMediaItems(track) - 1 do
    local candidate = assert(reaper.GetTrackMediaItem(track, index))
    if math.abs(reaper.GetMediaItemInfo_Value(candidate, "D_POSITION") - position) < 0.000001 then
      return candidate
    end
  end
end

local function make_item(position, active_source, active_name, alternate_source, alternate_name)
  reaper.SelectAllMediaItems(0, false)
  assert(reaper.InsertMedia(active_source, 0) == 1, "could not insert source item")
  local item = assert(reaper.GetSelectedMediaItem(0, 0), "inserted source item should be selected")
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  reaper.GetSetMediaItemTakeInfo_String(assert(reaper.GetActiveTake(item)), "P_NAME", active_name, true)
  if alternate_source then
    reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
    local alternate = assert(reaper.GetTake(item, 0), "original take should remain at index 0")
    local source = assert(reaper.PCM_Source_CreateFromFile(alternate_source),
      "could not load alternate source")
    assert(reaper.SetMediaItemTake_Source(alternate, source), "could not assign alternate source")
    reaper.GetSetMediaItemTakeInfo_String(alternate, "P_NAME", alternate_name, true)
  end
  return item
end

reaper.InsertTrackAtIndex(0, true)
track = assert(reaper.GetTrack(0, 0))
reaper.SetOnlyTrackSelected(track)
local delete_first = make_item(0, media_a, "Take A", media_b, "Take B")
reaper.SetMediaItemInfo_Value(delete_first, "I_CURTAKE", 0)
local delete_last = make_item(2, media_b, "Take B", media_a, "Take A")
local delete_single = make_item(4, media_a, "Single take", nil, nil)
local items = { delete_first, delete_last, delete_single }
for _, item in ipairs(items) do reaper.SetMediaItemSelected(item, true) end
assert(reaper.CountSelectedMediaItems(0) == 3, "probe must select all three items")
assert_item(0, 2, 0, media_b, "first-take target before deletion")
assert_item(2, 2, 1, media_b, "last-take target before deletion")
assert_item(4, 1, 0, media_a, "single-take target before deletion")
state("before_1", 0)
state("before_2", 2)
state("before_3", 4)
save("before")
reaper.Undo_OnStateChange2(0, "Prepare multi-selection active-take deletion probe")

reaper.Main_OnCommand(40129, 0) -- Take: Delete active take from items.
track = assert(reaper.GetTrack(0, 0))
assert(reaper.CountTrackMediaItems(track) == 2,
  "deleting selected single-take item should leave only the two multi-take items")
assert_item(0, 1, 0, media_a, "first-take target after deletion")
assert_item(2, 1, 0, media_a, "last-take target after deletion")
assert(state("after_3", 4) == nil, "single-take target should have been deleted")
save("after")
output:write("undo_description\t", reaper.Undo_CanUndo2(0) or "", "\n")
output:flush()

local function marker_exists(name)
  local file = io.open(output_dir .. "/" .. name, "r")
  if not file then return false end
  file:close()
  return true
end

local function write_marker(name)
  local file = assert(io.open(output_dir .. "/" .. name, "w"))
  file:write("ready\n")
  file:close()
end

local undo_readback_at
local redo_readback_at
local wait_for_undo
local wait_for_redo

wait_for_redo = function()
  if not marker_exists("redo-request") then
    redo_readback_at = nil
    reaper.defer(wait_for_redo)
    return
  end
  redo_readback_at = redo_readback_at or (reaper.time_precise() + 0.3)
  if reaper.time_precise() < redo_readback_at then
    reaper.defer(wait_for_redo)
    return
  end
  track = assert(reaper.GetTrack(0, 0), "track should remain after Redo")
  assert(reaper.CountTrackMediaItems(track) == 2, "one Redo should delete the selected targets")
  assert_item(0, 1, 0, media_a, "first-take target after one Redo")
  assert_item(2, 1, 0, media_a, "last-take target after one Redo")
  assert(state("redo_3", 4) == nil, "single-take target should be deleted after Redo")
  for _, position in ipairs({ 0, 2, 4 }) do state("redo", position) end
  save("redo")
  output:write("action\t40129\tTake: Delete active take from items\n")
  output:close()
  reaper.UpdateArrange()
  write_marker("capture-complete")
end

wait_for_undo = function()
  if not marker_exists("undo-request") then
    undo_readback_at = nil
    reaper.defer(wait_for_undo)
    return
  end
  undo_readback_at = undo_readback_at or (reaper.time_precise() + 0.3)
  if reaper.time_precise() < undo_readback_at then
    reaper.defer(wait_for_undo)
    return
  end
  track = assert(reaper.GetTrack(0, 0), "track should remain after Undo")
  assert(reaper.CountTrackMediaItems(track) == 3, "one Undo should restore all selected items")
  delete_first = assert_item(0, 2, 0, media_b, "first-take target after one Undo")
  delete_last = assert_item(2, 2, 1, media_b, "last-take target after one Undo")
  delete_single = assert_item(4, 1, 0, media_a, "single-take target after one Undo")
  for _, position in ipairs({ 0, 2, 4 }) do state("undo", position) end
  save("undo")
  output:write("redo_description\t", reaper.Undo_CanRedo2(0) or "", "\n")
  output:write("undo_complete\ttrue\n")
  output:flush()
  write_marker("await-redo")
  reaper.defer(wait_for_redo)
end

write_marker("await-undo")
reaper.defer(wait_for_undo)
