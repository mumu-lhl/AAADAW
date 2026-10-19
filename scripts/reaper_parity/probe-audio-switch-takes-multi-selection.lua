-- Run in an empty, stopped Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_REAPER_PROBE_MEDIA_A/B and AAADAW_REAPER_PROBE_DIR.
-- The live harness performs the requested Ctrl+Z/Ctrl+Shift+Z steps at markers.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")

local media_a = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_A"))
local media_b = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_B"))
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local mixed_index_control = os.getenv("AAADAW_REAPER_PROBE_MIXED_INDEX_CONTROL") == "1"
local output = assert(io.open(output_dir .. "/audio-switch-takes-multi-native.tsv", "w"))
local project_base = output_dir .. "/audio-switch-takes-multi"
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
    local item = assert(item_at(position), label .. " lost item at " .. position)
    local state = {
      position = position,
      active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE")),
      selected = reaper.IsMediaItemSelected(item),
      takes = {},
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
      "\tselected=", reaper.IsMediaItemSelected(item) and "1" or "0", "\n")
    for index, take in ipairs(state.takes) do
      output:write(label, "\tposition=", position, "\ttake_", index - 1,
        "\tname=", take.name, "\tsource=", take.source, "\n")
    end
  end
  output:flush()
  return states
end

local function assert_same(actual, expected, label)
  assert(#actual == #expected, label .. " changed the number of selected items")
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

local function assert_active_indices(states, expected, label)
  assert(#states == #expected, label .. " changed the selected item count")
  for index, state in ipairs(states) do
    assert(state.active_index == expected[index],
      label .. " set item " .. index .. " to take index " .. state.active_index)
  end
end

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local file = assert(io.open(path, "r"), "could not save " .. suffix .. " project")
  file:close()
end

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

local function make_item(position, take_count, active_index)
  reaper.SelectAllMediaItems(0, false)
  assert(reaper.InsertMedia(media_a, 0) == 1, "could not insert first Take A")
  local item = assert(reaper.GetSelectedMediaItem(0, 0))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  local take_a = assert(reaper.GetActiveTake(item))
  reaper.GetSetMediaItemTakeInfo_String(take_a, "P_NAME", "Take A", true)
  if take_count >= 2 then
    reaper.Main_OnCommand(40639, 0)
    local original = assert(reaper.GetTake(item, 0))
    local take_b = assert(reaper.PCM_Source_CreateFromFile(media_b))
    assert(reaper.SetMediaItemTake_Source(original, take_b))
    reaper.GetSetMediaItemTakeInfo_String(original, "P_NAME", "Take B", true)
  end
  if take_count >= 3 then reaper.Main_OnCommand(40639, 0) end
  if take_count > 1 then reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", active_index) end
  return item
end

reaper.InsertTrackAtIndex(0, true)
track = assert(reaper.GetTrack(0, 0))
reaper.SetOnlyTrackSelected(track)
make_item(0, 2, 0)
make_item(2, 1, 0)
make_item(4, 3, mixed_index_control and 0 or 2)
if mixed_index_control then
  reaper.SetMediaItemInfo_Value(assert(item_at(0)), "I_CURTAKE", 1)
end
for index = 0, reaper.CountTrackMediaItems(track) - 1 do
  reaper.SetMediaItemSelected(assert(reaper.GetTrackMediaItem(track, index)), true)
end
assert(reaper.CountSelectedMediaItems(0) == 3, "probe must select all three items")
local before = capture("before")
save("before")

reaper.SetEditCurPos2(0, 0, false, false)
reaper.UpdateArrange()
reaper.Main_OnCommand(40125, 0) -- Take: Switch items to next take.
local after_next = capture("after_next")
save("after-next")
if mixed_index_control then
  assert_active_indices(before, { 1, 0, 0 }, "mixed-index control before next take")
  assert_active_indices(after_next, { 1, 0, 2 }, "mixed-index control next take")
  reaper.Main_OnCommand(40126, 0) -- Take: Switch items to previous take.
  local after_previous = capture("control_previous")
  assert_active_indices(after_previous, { 1, 0, 1 }, "mixed-index control previous take")
  save("control-previous")
  output:write("control_actions\t40125=>1/0/2\t40126=>1/0/1\n")
  output:close()
  reaper.UpdateArrange()
  write_marker("capture-complete")
  return
end
output:write("next_undo_description\t", reaper.Undo_CanUndo2(0) or "", "\n")
output:flush()

local undo_next_at
local redo_next_at
local previous_at
local undo_previous_at
local redo_previous_at
local after_previous
local wait_for_undo_next
local wait_for_redo_next
local wait_for_previous
local wait_for_undo_previous
local wait_for_redo_previous

wait_for_redo_previous = function()
  if not marker_exists("redo-previous-request") then
    redo_previous_at = nil
    reaper.defer(wait_for_redo_previous)
    return
  end
  redo_previous_at = redo_previous_at or (reaper.time_precise() + 0.3)
  if reaper.time_precise() < redo_previous_at then reaper.defer(wait_for_redo_previous); return end
  local redone = capture("redo_previous")
  assert_same(redone, after_previous, "one Redo of previous-take action")
  save("redo-previous")
  output:write("previous_action\t40126\tTake: Switch items to previous take\n")
  output:close()
  reaper.UpdateArrange()
  write_marker("capture-complete")
end

wait_for_undo_previous = function()
  if not marker_exists("undo-previous-request") then
    undo_previous_at = nil
    reaper.defer(wait_for_undo_previous)
    return
  end
  undo_previous_at = undo_previous_at or (reaper.time_precise() + 0.3)
  if reaper.time_precise() < undo_previous_at then reaper.defer(wait_for_undo_previous); return end
  local undone = capture("undo_previous")
  assert_same(undone, after_next, "one Undo of previous-take action")
  save("undo-previous")
  output:write("previous_redo_description\t", reaper.Undo_CanRedo2(0) or "", "\n")
  output:flush()
  write_marker("await-redo-previous")
  reaper.defer(wait_for_redo_previous)
end

wait_for_previous = function()
  if not marker_exists("previous-request") then
    previous_at = nil
    reaper.defer(wait_for_previous)
    return
  end
  previous_at = previous_at or (reaper.time_precise() + 0.3)
  if reaper.time_precise() < previous_at then reaper.defer(wait_for_previous); return end
  reaper.SetEditCurPos2(0, 4, false, false)
  reaper.UpdateArrange()
  reaper.Main_OnCommand(40126, 0) -- Take: Switch items to previous take.
  after_previous = capture("after_previous")
  save("after-previous")
  output:write("previous_undo_description\t", reaper.Undo_CanUndo2(0) or "", "\n")
  output:flush()
  write_marker("await-undo-previous")
  reaper.defer(wait_for_undo_previous)
end

wait_for_redo_next = function()
  if not marker_exists("redo-next-request") then
    redo_next_at = nil
    reaper.defer(wait_for_redo_next)
    return
  end
  redo_next_at = redo_next_at or (reaper.time_precise() + 0.3)
  if reaper.time_precise() < redo_next_at then reaper.defer(wait_for_redo_next); return end
  local redone = capture("redo_next")
  assert_same(redone, after_next, "one Redo of next-take action")
  save("redo-next")
  output:write("next_redo_description\t", reaper.Undo_CanRedo2(0) or "", "\n")
  output:flush()
  write_marker("await-previous")
  reaper.defer(wait_for_previous)
end

wait_for_undo_next = function()
  if not marker_exists("undo-next-request") then
    undo_next_at = nil
    reaper.defer(wait_for_undo_next)
    return
  end
  undo_next_at = undo_next_at or (reaper.time_precise() + 0.3)
  if reaper.time_precise() < undo_next_at then reaper.defer(wait_for_undo_next); return end
  local undone = capture("undo_next")
  assert_same(undone, before, "one Undo of next-take action")
  save("undo-next")
  output:flush()
  write_marker("await-redo-next")
  reaper.defer(wait_for_redo_next)
end

reaper.UpdateArrange()
write_marker("await-undo-next")
reaper.defer(wait_for_undo_next)
