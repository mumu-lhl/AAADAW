-- Run only in an isolated, unsaved Linux REAPER 7.82 / Default 7 project.
-- Records Main action 41115 for empty, partial, full, and zero-item selection.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

local tracks = {}
local items = {}
local function add_track(name)
  reaper.InsertTrackAtIndex(reaper.CountTracks(0), true)
  local track = reaper.GetTrack(0, reaper.CountTracks(0) - 1)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  tracks[name] = track
end

local function add_audio(name, position, length)
  local item = reaper.AddMediaItemToTrack(tracks.Audio)
  local take = reaper.AddTakeToMediaItem(item)
  local source = reaper.PCM_Source_CreateFromFile(root .. "source.wav")
  assert(item and take and source, "Could not create audio item " .. name)
  assert(reaper.SetMediaItemTake_Source(take, source))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", length)
  reaper.GetSetMediaItemInfo_String(item, "P_NAME", name, true)
  items[name] = item
end

local function add_midi(name, position, length)
  local item = reaper.CreateNewMIDIItemInProj(
    tracks.MIDI, position, position + length, false
  )
  assert(item, "Could not create MIDI item " .. name)
  reaper.GetSetMediaItemInfo_String(item, "P_NAME", name, true)
  items[name] = item
end

add_track("Audio")
add_track("MIDI")
add_audio("audio-1", 0.0, 1.0)
add_audio("audio-2", 2.0, 1.0)
add_midi("midi-1", 0.5, 1.0)
add_midi("midi-2", 2.5, 1.0)
reaper.SetEditCurPos(4.0, false, false)
reaper.UpdateArrange()

local function item_names_in_selection(order_sensitive)
  local found = {}
  for index = 0, reaper.CountSelectedMediaItems(0) - 1 do
    local item = reaper.GetSelectedMediaItem(0, index)
    for name, candidate in pairs(items) do
      if candidate == item then
        found[#found + 1] = name
        break
      end
    end
  end
  if not order_sensitive then table.sort(found) end
  return table.concat(found, ",")
end

local function track_names_in_selection()
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

local function select_names(names)
  clear_items()
  for _, name in ipairs(names) do reaper.SetMediaItemSelected(items[name], true) end
end

local output = assert(io.open(root .. "item-invert-selection-native.tsv", "w"))
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("audio_device\tDummy Audio; no hardware I/O\n")
output:write("fixture\t2 tracks; 2 Audio and 2 MIDI items; unsaved project\n")
output:write("case\titems_before\titems_after\tselection_order_after\ttracks_before\ttracks_after\tcursor_before\tcursor_after\tstate_change_delta\tundo_before\tundo_after\tredo_before\tredo_after\tselected_count_after\n")

local function run_case(label, setup, remove_all_items)
  setup()
  if remove_all_items then
    for track_index = 0, reaper.CountTracks(0) - 1 do
      local track = reaper.GetTrack(0, track_index)
      for item_index = reaper.CountTrackMediaItems(track) - 1, 0, -1 do
        reaper.DeleteTrackMediaItem(track, reaper.GetTrackMediaItem(track, item_index))
      end
    end
  end
  local before_items = item_names_in_selection(false)
  local before_tracks = track_names_in_selection()
  local cursor_before = reaper.GetCursorPosition()
  local state_before = reaper.GetProjectStateChangeCount(0)
  local undo_before = reaper.Undo_CanUndo2(0) or ""
  local redo_before = reaper.Undo_CanRedo2(0) or ""
  reaper.Main_OnCommand(41115, 0)
  local items_after = item_names_in_selection(false)
  local order_after = item_names_in_selection(true)
  local state_change_delta = reaper.GetProjectStateChangeCount(0) - state_before
  local undo_after = reaper.Undo_CanUndo2(0) or ""
  local redo_after = reaper.Undo_CanRedo2(0) or ""
  output:write(table.concat({
    label,
    before_items,
    items_after,
    order_after,
    before_tracks,
    track_names_in_selection(),
    string.format("%.6f", cursor_before),
    string.format("%.6f", reaper.GetCursorPosition()),
    tostring(state_change_delta),
    undo_before,
    undo_after,
    redo_before,
    redo_after,
    tostring(reaper.CountSelectedMediaItems(0)),
  }, "\t"), "\n")
  output:flush()
end

run_case("empty_selection", clear_items)
run_case("one_midi_selected", function() select_names({ "midi-2" }) end)
run_case("all_items_selected", function() reaper.Main_OnCommand(40182, 0) end)
run_case("one_audio_selected", function() select_names({ "audio-1" }) end)
run_case("no_items_in_project", function() end, true)

output:close()
reaper.ShowConsoleMsg("Wrote " .. root .. "item-invert-selection-native.tsv\n")
