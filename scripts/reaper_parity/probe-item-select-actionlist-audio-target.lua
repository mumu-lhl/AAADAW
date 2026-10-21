-- Run in a fresh, unsaved REAPER 7.82 / Default 7 project with no tracks.
-- Opens the native Action List and records manual keyboard execution of
-- 40528 while the real X11 pointer remains over the Audio item B1.
-- Set AAADAW_ACTIONLIST_TARGET_PROBE_DIR and
-- AAADAW_ACTIONLIST_TARGET_AUDIO_WAV before launching.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in an unsaved project")

local directory = assert(os.getenv("AAADAW_ACTIONLIST_TARGET_PROBE_DIR"))
local audio_path = assert(os.getenv("AAADAW_ACTIONLIST_TARGET_AUDIO_WAV"))
local output_path = directory .. "/item-select-actionlist-target-audio-native.tsv"
local stop_path = directory .. "/stop"
local tracks, items = {}, {}

local function add_track(name, index)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  tracks[name] = track
end

local function add_midi_item(track_name, label, start_time, end_time)
  local item = reaper.CreateNewMIDIItemInProj(
    tracks[track_name], start_time, end_time, false
  )
  assert(item, "Could not create MIDI item " .. label)
  local take = reaper.GetActiveTake(item)
  assert(take, "Could not create MIDI take " .. label)
  reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", label, true)
  items[label] = item
end

local function add_audio_item(track_name, label, start_time, length)
  local source = reaper.PCM_Source_CreateFromFile(audio_path)
  assert(source, "Could not open audio source " .. audio_path)
  local item = reaper.AddMediaItemToTrack(tracks[track_name])
  assert(item, "Could not create audio item " .. label)
  local take = reaper.AddTakeToMediaItem(item)
  assert(take, "Could not create audio take " .. label)
  reaper.SetMediaItemTake_Source(take, source)
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", start_time)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", length)
  reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", label, true)
  items[label] = item
end

local function item_name(item)
  for label, candidate in pairs(items) do
    if item == candidate then return label end
  end
  return item and "other" or "none"
end

local function selected_items(api_order)
  local labels = {}
  for index = 0, reaper.CountSelectedMediaItems(0) - 1 do
    labels[#labels + 1] = item_name(reaper.GetSelectedMediaItem(0, index))
  end
  if not api_order then table.sort(labels) end
  return table.concat(labels, ",")
end

local function selected_tracks()
  local names = {}
  for index = 0, reaper.CountTracks(0) - 1 do
    local track = reaper.GetTrack(0, index)
    if reaper.IsTrackSelected(track) then
      local _, name = reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "", false)
      names[#names + 1] = name
    end
  end
  return table.concat(names, ",")
end

local function last_touched_track()
  local track = reaper.GetLastTouchedTrack()
  for name, candidate in pairs(tracks) do
    if track == candidate then return name end
  end
  return track and "other" or "none"
end

local function hovered_item()
  local x, y = reaper.GetMousePosition()
  return x, y, item_name(reaper.GetItemFromPoint(x, y, true))
end

add_track("A", 0)
add_track("B", 1)
add_midi_item("A", "A1", 1.0, 2.0)
add_midi_item("A", "A2", 4.0, 5.0)
add_audio_item("B", "B1", 1.0, 1.0)
add_midi_item("B", "B2", 3.0, 4.0)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 6.0)
reaper.SetMediaItemSelected(items.A1, true)
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()

local output = assert(io.open(output_path, "w"))
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("fixture\tTrack A MIDI items A1/A2; Track B Audio item B1 and MIDI item B2; initial selection A1\n")
output:write("action\t40528 Item: Select item under mouse cursor; no default shortcut\n")
output:write("case\taction_id\tpointer_xy\thovered_item\tselection_before\tselection_api_order_before\tselection_after\tselection_api_order_after\tselected_tracks_before\tselected_tracks_after\tlast_touched_track_after\tproject_state_change_count\tundo_label_after\n")
output:flush()
local ready = assert(io.open(directory .. "/ready", "w"))
ready:write("ready\n")
ready:close()
reaper.Main_OnCommand(40605, 0) -- Main: Show action list
local previous_selection = "A1"
local previous_api_order = selected_items(true)
local recorded = false
local function poll()
  local x, y, hovered = hovered_item()
  local current = selected_items(false)
  if not recorded and current ~= previous_selection then
    local case = current == "B1" and "actionlist_select_hovered_audio_item" or "unexpected_selection_change"
    output:write(table.concat({
      case,
      "40528",
      tostring(x) .. "," .. tostring(y),
      hovered,
      previous_selection,
      previous_api_order,
      current,
      selected_items(true),
      "",
      selected_tracks(),
      last_touched_track(),
      tostring(reaper.GetProjectStateChangeCount(0)),
      reaper.Undo_CanUndo2(0) or "",
    }, "\t"), "\n")
    output:flush()
    previous_selection = current
    previous_api_order = selected_items(true)
    recorded = true
  end
  local stop = io.open(stop_path, "r")
  if stop then
    stop:close()
    output:close()
    return
  end
  reaper.defer(poll)
end
reaper.defer(poll)
