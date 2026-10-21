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
local output = assert(io.open(output_dir .. "/audio-paste-as-takes-multi-native.tsv", "w"))
local project_base = output_dir .. "/audio-paste-as-takes-multi"
local positions = { 0, 2, 4 }
local track

local function source_path(take)
  return reaper.GetMediaSourceFileName(assert(reaper.GetMediaItemTake_Source(take)), "")
end

local function item_at(position)
  for index = 0, reaper.CountTrackMediaItems(track) - 1 do
    local item = assert(reaper.GetTrackMediaItem(track, index))
    if math.abs(reaper.GetMediaItemInfo_Value(item, "D_POSITION") - position) < 0.000001 then
      return item
    end
  end
end

local function capture(label)
  local states = {}
  for _, position in ipairs(positions) do
    local item = item_at(position)
    assert(item, label .. " lost target at " .. position)
    local state = {
      position = position,
      takes = {},
      active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE")),
    }
    for index = 0, reaper.CountTakes(item) - 1 do
      local take = assert(reaper.GetTake(item, index))
      local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
      state.takes[#state.takes + 1] = { name = name, source = source_path(take) }
    end
    state.active_source = state.takes[state.active_index + 1].source
    states[#states + 1] = state
    output:write(label, "\tposition=", position, "\ttake_count=", #state.takes,
      "\tactive_index=", state.active_index, "\tactive_source=", state.active_source,
      "\ttrack_item_count=", reaper.CountTrackMediaItems(track), "\n")
    for index, take in ipairs(state.takes) do
      output:write(label, "\tposition=", position, "\ttake_", index - 1,
        "\tname=", take.name, "\tsource=", take.source, "\n")
    end
  end
  output:flush()
  return states
end

local function assert_same(actual, expected, label)
  assert(#actual == #expected, label .. " changed the number of selected targets")
  for index, expected_item in ipairs(expected) do
    local actual_item = actual[index]
    assert(actual_item.position == expected_item.position
      and actual_item.active_index == expected_item.active_index
      and actual_item.active_source == expected_item.active_source,
      label .. " did not restore target " .. index)
    assert(#actual_item.takes == #expected_item.takes,
      label .. " changed the take count for target " .. index)
    for take_index, expected_take in ipairs(expected_item.takes) do
      local actual_take = actual_item.takes[take_index]
      assert(actual_take.name == expected_take.name and actual_take.source == expected_take.source,
        label .. " did not restore take " .. (take_index - 1) .. " on target " .. index)
    end
  end
end

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local file = assert(io.open(path, "r"), "could not save " .. suffix .. " project")
  file:close()
end

local function make_target(position, active_index, item_name)
  reaper.SelectAllMediaItems(0, false)
  assert(reaper.InsertMedia(media_a, 0) == 1, "could not insert target Take A")
  local item = assert(reaper.GetSelectedMediaItem(0, 0))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  local take_a = assert(reaper.GetActiveTake(item))
  reaper.GetSetMediaItemTakeInfo_String(take_a, "P_NAME", item_name or "Take A", true)
  if active_index ~= nil then
    reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
    local original = assert(reaper.GetTake(item, 0), "original Take A should remain at index 0")
    local alternate = assert(reaper.PCM_Source_CreateFromFile(media_b), "could not load Take B")
    assert(reaper.SetMediaItemTake_Source(original, alternate), "could not assign Take B")
    reaper.GetSetMediaItemTakeInfo_String(original, "P_NAME", "Take B", true)
    reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", active_index)
  end
  return item
end

reaper.InsertTrackAtIndex(0, true)
track = assert(reaper.GetTrack(0, 0))
reaper.SetOnlyTrackSelected(track)
local first = make_target(0, 0)
local single = make_target(2, nil, "Single take")
local last = make_target(4, 1)
reaper.SelectAllMediaItems(0, false)
assert(reaper.InsertMedia(media_b, 0) == 1, "could not insert clipboard source")
local source = assert(reaper.GetSelectedMediaItem(0, 0))
reaper.SetMediaItemInfo_Value(source, "D_POSITION", 6)
reaper.SetMediaItemInfo_Value(source, "D_LENGTH", 1)
reaper.GetSetMediaItemTakeInfo_String(assert(reaper.GetActiveTake(source)), "P_NAME", "Clipboard B", true)

reaper.SelectAllMediaItems(0, false)
reaper.SetMediaItemSelected(source, true)
reaper.Main_OnCommand(40057, 0) -- Edit: Copy items/tracks.
reaper.SelectAllMediaItems(0, false)
for _, item in ipairs({ first, single, last }) do reaper.SetMediaItemSelected(item, true) end
assert(reaper.CountSelectedMediaItems(0) == 3, "probe must select all three target items")
local before = capture("before")
save("before")

reaper.Undo_BeginBlock2(0)
reaper.Main_OnCommand(40603, 0) -- Take: Paste as takes in items.
reaper.Undo_EndBlock2(0, "Paste as takes into selected items", -1)
track = assert(reaper.GetTrack(0, 0))
local after = capture("after")
assert(reaper.CountTrackMediaItems(track) == 4, "paste as takes should retain the source item")
for index, state in ipairs(before) do
  local pasted = after[index]
  assert(#pasted.takes == #state.takes + 1,
    "action 40603 should append one take to every selected target")
  assert(pasted.active_source == media_b,
    "action 40603 should activate the copied source on every target")
end
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
  if reaper.time_precise() < redo_readback_at then reaper.defer(wait_for_redo); return end
  local redone = capture("redo")
  assert_same(redone, after, "one Redo")
  save("redo")
  output:write("action\t40603\tTake: Paste as takes in items\n")
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
  if reaper.time_precise() < undo_readback_at then reaper.defer(wait_for_undo); return end
  local undone = capture("undo")
  assert_same(undone, before, "one Undo")
  save("undo")
  output:write("redo_description\t", reaper.Undo_CanRedo2(0) or "", "\nundo_complete\ttrue\n")
  output:flush()
  write_marker("await-redo")
  reaper.defer(wait_for_redo)
end

reaper.UpdateArrange()
write_marker("await-undo")
reaper.defer(wait_for_undo)
