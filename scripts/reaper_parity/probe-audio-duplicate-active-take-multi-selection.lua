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
local output = assert(io.open(output_dir .. "/audio-duplicate-active-take-multi-native.tsv", "w"))
local project_base = output_dir .. "/audio-duplicate-active-take-multi"
local track
local positions = { 0, 2, 4 }

local function source_path(take)
  local source = assert(reaper.GetMediaItemTake_Source(take))
  return reaper.GetMediaSourceFileName(source, "")
end

local function find_item(position)
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
    local item = find_item(position)
    local state = { position = position, exists = item ~= nil, takes = {} }
    if item then
      state.active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
      for index = 0, reaper.CountTakes(item) - 1 do
        local take = assert(reaper.GetTake(item, index))
        local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
        state.takes[#state.takes + 1] = { name = name, source = source_path(take) }
      end
      state.active_source = state.takes[state.active_index + 1].source
    end
    states[#states + 1] = state
    output:write(label, "\tposition=", position, "\texists=", state.exists and "1" or "0",
      "\ttrack_item_count=", reaper.CountTrackMediaItems(track), "\n")
    if item then
      output:write(label, "\ttake_count=", #state.takes, "\tactive_index=", state.active_index,
        "\tactive_source=", state.active_source, "\n")
      for index, take in ipairs(state.takes) do
        output:write(label, "\ttake_", index - 1, "\tname=", take.name,
          "\tsource=", take.source, "\n")
      end
    end
  end
  output:flush()
  return states
end

local function assert_same(actual, expected, label)
  assert(#actual == #expected, label .. " has an unexpected item count")
  for index, state in ipairs(expected) do
    local value = actual[index]
    assert(value.position == state.position and value.exists == state.exists,
      label .. " changed item positions or existence")
    assert(value.active_index == state.active_index and value.active_source == state.active_source,
      label .. " did not restore the prior active take")
    assert(#value.takes == #state.takes, label .. " has an unexpected take count")
    for take_index, take in ipairs(state.takes) do
      assert(value.takes[take_index].name == take.name
        and value.takes[take_index].source == take.source,
        label .. " did not restore take " .. (take_index - 1))
    end
  end
end

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local saved = assert(io.open(path, "r"), "could not save " .. suffix .. " project")
  saved:close()
end

local function make_item(position, active_source, active_name, alternate_source, alternate_name, active_index)
  reaper.SelectAllMediaItems(0, false)
  assert(reaper.InsertMedia(active_source, 0) == 1, "could not insert source item")
  local item = assert(reaper.GetSelectedMediaItem(0, 0), "inserted source item should be selected")
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  reaper.GetSetMediaItemTakeInfo_String(assert(reaper.GetActiveTake(item)), "P_NAME", active_name, true)
  if alternate_source then
    reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
    -- REAPER appends and activates the copy at index 1; use the original at
    -- index 0 as the alternate so this fixture can start on either take.
    local alternate = assert(reaper.GetTake(item, 0), "original take should remain at index 0")
    local source = assert(reaper.PCM_Source_CreateFromFile(alternate_source), "could not load alternate source")
    assert(reaper.SetMediaItemTake_Source(alternate, source), "could not assign alternate source")
    reaper.GetSetMediaItemTakeInfo_String(alternate, "P_NAME", alternate_name, true)
    reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", active_index)
  end
  return item
end

reaper.InsertTrackAtIndex(0, true)
track = assert(reaper.GetTrack(0, 0))
reaper.SetOnlyTrackSelected(track)
make_item(0, media_a, "Take A", media_b, "Take B", 0)
make_item(2, media_a, "Single take", nil, nil, 0)
make_item(4, media_a, "Take A", media_b, "Take B", 1)
for index = 0, reaper.CountTrackMediaItems(track) - 1 do
  reaper.SetMediaItemSelected(assert(reaper.GetTrackMediaItem(track, index)), true)
end
assert(reaper.CountSelectedMediaItems(0) == 3, "probe must select all three items")
local before = capture("before")
save("before")
reaper.Undo_OnStateChange2(0, "Prepare multi-selection active-take duplication probe")

reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
track = assert(reaper.GetTrack(0, 0))
assert(reaper.CountTrackMediaItems(track) == 3, "duplicate should keep all three items")
local after = capture("after")
for index, state in ipairs(before) do
  local duplicated = after[index]
  assert(duplicated.exists and #duplicated.takes == #state.takes + 1,
    "action 40639 should add one take to every selected Audio Item")
  assert(duplicated.active_source == state.active_source,
    "action 40639 should keep each selected item's active source")
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
  track = assert(reaper.GetTrack(0, 0), "track should remain after Redo")
  local redone = capture("redo")
  assert_same(redone, after, "one Redo")
  save("redo")
  output:write("action\t40639\tTake: Duplicate active take\n")
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
  track = assert(reaper.GetTrack(0, 0), "track should remain after Undo")
  local undone = capture("undo")
  assert_same(undone, before, "one Undo")
  save("undo")
  output:write("redo_description\t", reaper.Undo_CanRedo2(0) or "", "\nundo_complete\ttrue\n")
  output:flush()
  write_marker("await-redo")
  reaper.defer(wait_for_redo)
end

write_marker("await-undo")
reaper.defer(wait_for_undo)
