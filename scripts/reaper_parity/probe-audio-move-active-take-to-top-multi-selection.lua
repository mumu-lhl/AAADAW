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
local output = assert(io.open(output_dir .. "/audio-move-active-take-to-top-multi-selection-native.tsv", "w"))
local project_base = output_dir .. "/audio-move-active-take-to-top-multi-selection"

local function source_path(take)
  local source = assert(reaper.GetMediaItemTake_Source(take))
  return reaper.GetMediaSourceFileName(source, "")
end

local function state(label, item)
  local count = reaper.CountTakes(item)
  local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
  output:write(label, "\ttake_count=", count, "\tactive_index=", active_index, "\n")
  for index = 0, count - 1 do
    local take = assert(reaper.GetTake(item, index))
    local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
    output:write(label, "\ttake_", index, "\tname=", name,
      "\tsource=", source_path(take), "\n")
  end
  output:flush()
end

local function assert_item(item, count, active_index, active_source, label)
  assert(reaper.CountTakes(item) == count, label .. " has an unexpected take count")
  local actual_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
  assert(actual_index == active_index,
    label .. " has active index " .. actual_index .. ", expected " .. active_index)
  assert(source_path(assert(reaper.GetActiveTake(item))) == active_source,
    label .. " has an unexpected active source")
end

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local saved = assert(io.open(path, "r"), "could not save " .. suffix .. " project")
  saved:close()
end

local function reacquire_items(track)
  local items = {}
  for index = 0, reaper.CountTrackMediaItems(track) - 1 do
    items[#items + 1] = assert(reaper.GetTrackMediaItem(track, index))
  end
  return items
end

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.SetOnlyTrackSelected(track)

local function make_item(position, active_source, active_name, alternate_source, alternate_name)
  reaper.SelectAllMediaItems(0, false)
  assert(reaper.InsertMedia(active_source, 0) == 1, "could not insert active source")
  local item = assert(reaper.GetSelectedMediaItem(0, 0), "new source item should be selected")
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  local active_take = assert(reaper.GetActiveTake(item))
  reaper.GetSetMediaItemTakeInfo_String(active_take, "P_NAME", active_name, true)
  if alternate_source then
    reaper.SetMediaItemSelected(item, true)
    reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
    local alternate = assert(reaper.GetTake(item, 0))
    local source = assert(reaper.PCM_Source_CreateFromFile(alternate_source), "could not load alternate source")
    assert(reaper.SetMediaItemTake_Source(alternate, source), "could not assign alternate source")
    reaper.GetSetMediaItemTakeInfo_String(alternate, "P_NAME", alternate_name, true)
  end
  return item
end

local active_a = make_item(0, media_a, "Take A", media_b, "Take B")
local single = make_item(1, media_a, "Single take", nil, nil)
local active_b = make_item(2, media_b, "Take B", media_a, "Take A")
local already_top = make_item(3, media_a, "Take A", media_b, "Take B")
reaper.SetMediaItemInfo_Value(already_top, "I_CURTAKE", 0)
local items = { active_a, single, active_b, already_top }
for _, item in ipairs(items) do reaper.SetMediaItemSelected(item, true) end
assert(reaper.CountSelectedMediaItems(0) == 4, "probe must select all four items")
assert_item(active_a, 2, 1, media_a, "Take A item before move")
assert_item(single, 1, 0, media_a, "single-take item before move")
assert_item(active_b, 2, 1, media_b, "Take B item before move")
assert_item(already_top, 2, 0, media_b, "already-top item before move")
for index, item in ipairs(items) do state("before_" .. index, item) end
save("before")
reaper.Undo_OnStateChange2(0, "Prepare multi-selection active-take ordering probe")

reaper.Main_OnCommand(41380, 0) -- Item: Move active takes to top.
assert_item(active_a, 2, 0, media_a, "Take A item after move")
assert_item(single, 1, 0, media_a, "single-take item after move")
assert_item(active_b, 2, 0, media_b, "Take B item after move")
assert_item(already_top, 2, 0, media_b, "already-top item after move")
for index, item in ipairs(items) do state("after_" .. index, item) end
save("after")
local undo_description = reaper.Undo_CanUndo2(0) or ""
output:write("undo_description\t", undo_description, "\n")
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

local wait_for_undo
local wait_for_redo
local undo_readback_at
local redo_readback_at

local function no_op_step()
  local no_op_undo = reaper.Undo_CanUndo2(0) or ""
  reaper.Main_OnCommand(41380, 0) -- All selected multi-take items are already at index 0.
  assert((reaper.Undo_CanUndo2(0) or "") == no_op_undo,
    "already-top multi-selection should not add an undo entry")
  track = assert(reaper.GetTrack(0, 0))
  items = reacquire_items(track)
  for index, item in ipairs(items) do state("already_top_" .. index, item) end
  save("already-top")
  output:write("already_top_undo\t", no_op_undo, "\n")
  output:write("action\t41380\tItem: Move active takes to top\n")
  output:close()
  reaper.UpdateArrange()
  write_marker("capture-complete")
end

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
  items = reacquire_items(track)
  active_a, single, active_b, already_top = table.unpack(items)
  assert_item(active_a, 2, 0, media_a, "Take A item after one Redo")
  assert_item(single, 1, 0, media_a, "single-take item after one Redo")
  assert_item(active_b, 2, 0, media_b, "Take B item after one Redo")
  assert_item(already_top, 2, 0, media_b, "already-top item after one Redo")
  for index, item in ipairs(items) do state("redo_" .. index, item) end
  save("redo")
  reaper.defer(no_op_step)
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
  items = reacquire_items(track)
  active_a, single, active_b, already_top = table.unpack(items)
  for index, item in ipairs(items) do state("undo_observed_" .. index, item) end
  assert_item(active_a, 2, 1, media_a, "Take A item after one Undo")
  assert_item(single, 1, 0, media_a, "single-take item after one Undo")
  assert_item(active_b, 2, 1, media_b, "Take B item after one Undo")
  assert_item(already_top, 2, 0, media_b, "already-top item after one Undo")
  for index, item in ipairs(items) do state("undo_" .. index, item) end
  save("undo")
  local redo_description = reaper.Undo_CanRedo2(0) or ""
  output:write("redo_description\t", redo_description, "\n")
  output:write("undo_complete\ttrue\n")
  output:flush()
  write_marker("await-redo")
  reaper.defer(wait_for_redo)
end

write_marker("await-undo")
reaper.defer(wait_for_undo)
