-- Run only in a disposable, unsaved Linux REAPER 7.82 / Default 7 project.
-- It creates a three-track Audio/MIDI fixture, runs Main actions 40182,
-- 40421, 40717, 40718 and 40289, records state, and never saves the project.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

local tracks, items = {}, {}
local function add_track(name, index)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  tracks[name] = track
end

local function add_audio(track_name, name, position, length)
  local item = reaper.AddMediaItemToTrack(tracks[track_name])
  local take = reaper.AddTakeToMediaItem(item)
  local source = reaper.PCM_Source_CreateFromFile(root .. "source.wav")
  assert(item and take and source, "Could not create Audio fixture item " .. name)
  assert(reaper.SetMediaItemTake_Source(take, source))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", length)
  reaper.GetSetMediaItemInfo_String(item, "P_NAME", name, true)
  items[name] = item
end

local function add_midi(track_name, name, position, length)
  local item = reaper.CreateNewMIDIItemInProj(
    tracks[track_name], position, position + length, false
  )
  assert(item, "Could not create MIDI fixture item " .. name)
  reaper.GetSetMediaItemInfo_String(item, "P_NAME", name, true)
  items[name] = item
end

add_track("A", 0)
add_track("B", 1)
add_track("C", 2)
add_audio("A", "A_before", 0.0, 1.0)      -- ends on time-selection start
add_audio("A", "A_start", 1.0, 0.5)       -- starts on time-selection start
add_audio("A", "A_inside", 2.0, 0.5)
add_audio("A", "A_spans_end", 2.5, 1.0)  -- spans time-selection end
add_audio("A", "A_end", 3.0, 0.5)        -- starts on time-selection end
add_audio("A", "A_after", 4.0, 1.0)
add_audio("B", "B_start", 1.0, 0.5)
add_midi("B", "B_midi_start", 1.0, 0.5)
add_audio("B", "B_inside", 2.0, 0.5)
add_midi("B", "B_midi_spans_end", 2.5, 1.0)
add_audio("B", "B_end", 3.0, 0.5)
add_audio("C", "C_start", 1.0, 0.5)
add_audio("C", "C_end", 3.0, 0.5)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 5.5)
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()

local output_path = root .. "item-selection-actions-native.tsv"
local out = assert(io.open(output_path, "w"))
local function selected_items()
  local found = {}
  for index = 0, reaper.CountSelectedMediaItems(0) - 1 do
    local item = reaper.GetSelectedMediaItem(0, index)
    for name, candidate in pairs(items) do
      if candidate == item then found[#found + 1] = name end
    end
  end
  table.sort(found)
  return table.concat(found, ",")
end

local function selected_tracks()
  local found = {}
  for index = 0, reaper.CountTracks(0) - 1 do
    local track = reaper.GetTrack(0, index)
    if reaper.IsTrackSelected(track) then
      local _, name = reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "", false)
      found[#found + 1] = name
    end
  end
  return table.concat(found, ",")
end

local function clear_items()
  for _, item in pairs(items) do reaper.SetMediaItemSelected(item, false) end
end

local function select_tracks(names)
  for _, track in pairs(tracks) do reaper.SetTrackSelected(track, false) end
  for _, name in ipairs(names) do reaper.SetTrackSelected(tracks[name], true) end
end

local function set_time_selection(start_time, end_time)
  reaper.GetSet_LoopTimeRange2(0, true, false, start_time, end_time, false)
end

local function run_case(label, action_id, setup)
  setup()
  local before_items = selected_items()
  local before_tracks = selected_tracks()
  local cursor_before = reaper.GetCursorPosition()
  local time_start, time_end = reaper.GetSet_LoopTimeRange2(0, false, false, 0, 0, false)
  local state_before = reaper.GetProjectStateChangeCount(0)
  local undo_before = reaper.Undo_CanUndo2(0) or ""
  local redo_before = reaper.Undo_CanRedo2(0) or ""
  reaper.Main_OnCommand(action_id, 0)
  out:write(table.concat({
    label,
    tostring(action_id),
    before_items,
    selected_items(),
    before_tracks,
    selected_tracks(),
    string.format("%.6f", cursor_before),
    string.format("%.6f", reaper.GetCursorPosition()),
    string.format("%.6f..%.6f", time_start, time_end),
    tostring(reaper.GetProjectStateChangeCount(0) - state_before),
    undo_before,
    reaper.Undo_CanUndo2(0) or "",
    redo_before,
    reaper.Undo_CanRedo2(0) or "",
  }, "\t"), "\n")
  out:flush()
end

out:write("version\t", reaper.GetAppVersion(), "\n")
out:write("theme\tDefault_7.0.ReaperThemeZip\n")
out:write("audio_device\tDummy Audio; no hardware I/O\n")
out:write("fixture\t3 tracks; 13 media items; mixed Audio/MIDI; time selection [1,3]\n")
out:write("case\taction_id\titems_before\titems_after\ttracks_before\ttracks_after\tcursor_before\tcursor_after\ttime_selection\tproject_state_change_delta\tundo_before\tundo_after\tredo_before\tredo_after\n")

run_case("select_all_items", 40182, function()
  clear_items()
  select_tracks({})
  set_time_selection(0, 0)
end)

run_case("select_all_items_in_one_track_A", 40421, function()
  clear_items()
  select_tracks({ "A" })
  set_time_selection(0, 0)
end)

run_case("select_all_items_in_tracks_A_then_B", 40421, function()
  clear_items()
  select_tracks({ "A", "B" })
end)

run_case("select_all_items_in_tracks_B_then_A", 40421, function()
  clear_items()
  select_tracks({ "B", "A" })
end)

run_case("select_items_overlapping_time_selection", 40717, function()
  clear_items()
  select_tracks({})
  set_time_selection(1, 3)
end)

run_case("select_items_on_selected_tracks_overlapping_time_selection", 40718, function()
  clear_items()
  select_tracks({ "A", "C" })
  set_time_selection(1, 3)
end)

run_case("unselect_all_items_preserves_other_view_state", 40289, function()
  reaper.Main_OnCommand(40182, 0)
  select_tracks({ "C" })
  set_time_selection(1, 3)
  reaper.SetEditCurPos(4, false, false)
end)

local before_undo_items = selected_items()
local before_undo_label = reaper.Undo_CanUndo2(0) or ""
local before_undo_state = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40029, 0) -- Edit: Undo
out:write(table.concat({
  "undo_after_unselect",
  "40029",
  before_undo_items,
  selected_items(),
  "",
  selected_tracks(),
  "",
  string.format("%.6f", reaper.GetCursorPosition()),
  "",
  tostring(reaper.GetProjectStateChangeCount(0) - before_undo_state),
  before_undo_label,
  reaper.Undo_CanUndo2(0) or "",
  "",
  reaper.Undo_CanRedo2(0) or "",
}, "\t"), "\n")
out:flush()

run_case("unselect_all_after_undo_branches_history", 40289, function() end)
run_case("unselect_all_when_already_empty", 40289, function() end)

-- Leave all items selected for a native Arrange screenshot after execution.
clear_items()
reaper.Main_OnCommand(40182, 0)
select_tracks({})
reaper.UpdateArrange()
out:close()
reaper.ShowConsoleMsg("Wrote " .. output_path .. "\n")
