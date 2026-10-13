-- Run only in an empty, unsaved Linux REAPER 7.82 / Default 7 project with Dummy Audio.
-- The fixture has one sourced Audio item and one MIDI item. Select them by mouse,
-- use Ctrl-click for multiselection, then write label<TAB>action-id<TAB>reset-list
-- to /tmp/aaadaw-item-snap-request. reset-list is optional: AUDIO=1.27;MIDI=2.73.
-- The probe never saves the project.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")

local tracks, items = {}, {}
local function add_track(name, index)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  tracks[name] = track
end
add_track("Audio", 0)
add_track("MIDI", 1)

local audio_item = reaper.AddMediaItemToTrack(tracks.Audio)
local take = reaper.AddTakeToMediaItem(audio_item)
assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
reaper.SetMediaItemInfo_Value(audio_item, "D_POSITION", 1.27)
reaper.SetMediaItemInfo_Value(audio_item, "D_LENGTH", 0.5)
items.AUDIO = audio_item

local midi_item = reaper.CreateNewMIDIItemInProj(tracks.MIDI, 2.73, 3.23, false)
assert(midi_item)
items.MIDI = midi_item

reaper.GetSetProjectGrid(0, true, 0.25, 0, 0.0) -- quarter-note grid at 120 BPM
local grid_flags, grid_division, swing_mode, swing_amount = reaper.GetSetProjectGrid(0, false, 0, 0, 0)
local snap_enabled = reaper.GetToggleCommandStateEx(0, 1157) == 1
if not snap_enabled then reaper.Main_OnCommand(1157, 0); snap_enabled = true end
reaper.GetSet_ArrangeView2(0, true, 0, 0, 6.5)
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()

local request_path = "/tmp/aaadaw-item-snap-request"
os.remove(request_path)
os.remove("/tmp/aaadaw-item-snap-ready")
local out = assert(io.open(root .. "item-snap-native.tsv", "w"))
local function selected_items()
  local found = {}
  for index = 0, reaper.CountSelectedMediaItems(0) - 1 do
    local item = reaper.GetSelectedMediaItem(0, index)
    for name, candidate in pairs(items) do
      if item == candidate then found[#found + 1] = name end
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
local function item_positions()
  local found = {}
  for name, item in pairs(items) do
    found[#found + 1] = string.format("%s:%.6f+%.6f", name,
      reaper.GetMediaItemInfo_Value(item, "D_POSITION"),
      reaper.GetMediaItemInfo_Value(item, "D_LENGTH"))
  end
  table.sort(found)
  return table.concat(found, ";")
end
out:write("version\t", reaper.GetAppVersion(), "\n")
out:write("theme\tDefault_7.0.ReaperThemeZip\n")
out:write("audio_device\tDummy Audio; no hardware I/O\n")
out:write("grid_flags\t", tostring(grid_flags), "\n")
out:write("snap_enabled\t", tostring(snap_enabled), "\n")
out:write("grid_division_beats\t", tostring(grid_division), "\n")
out:write("grid_swing_mode\t", tostring(swing_mode), "\n")
out:write("grid_swing_amount\t", tostring(swing_amount), "\n")
out:write("fixture\tAudio item AUDIO starts at 1.27s; MIDI item MIDI starts at 2.73s; quarter-note grid\n")
out:write("case\taction_id\tselected_items_before\tselected_items_after\tselected_tracks_after\tcursor_after_seconds\tproject_state_change_delta\titem_positions_before\titem_positions_after\tsnap_enabled_before\tsnap_enabled_after\tundo_name_before\tundo_name_after\tfixture_note\n")
out:flush()
local ready = assert(io.open("/tmp/aaadaw-item-snap-ready", "w"))
ready:write("ready\n")
ready:close()

local function reset_positions(reset_list)
  for record in reset_list:gmatch("[^;]+") do
    local name, seconds = record:match("([^=]+)=([%d%.]+)")
    if name and items[name] then
      reaper.SetMediaItemInfo_Value(items[name], "D_POSITION", tonumber(seconds))
      reaper.UpdateItemInProject(items[name])
    end
  end
end

local pending_result
local function poll()
  if pending_result then
    out:write(table.concat({
      pending_result.label,
      pending_result.action_id,
      pending_result.selected_before,
      selected_items(),
      selected_tracks(),
      string.format("%.6f", reaper.GetCursorPosition()),
      tostring(reaper.GetProjectStateChangeCount(0) - pending_result.change_before),
      pending_result.positions_before,
      item_positions(),
      tostring(pending_result.snap_before),
      tostring(reaper.GetToggleCommandStateEx(0, 1157) == 1),
      pending_result.undo_before,
      reaper.Undo_CanUndo2(0) or "",
      "",
    }, "\t"), "\n")
    out:flush()
    pending_result = nil
  end

  local input = io.open(request_path, "r")
  if input then
    local request = input:read("*a")
    input:close()
    os.remove(request_path)
    local label, action_id, reset_list = request:match("([^\t\r\n]+)\t(%d+)\t?([^\r\n]*)")
    if label == "done" then out:close(); return end
    if label and action_id then
      reset_positions(reset_list or "")
      local before = item_positions()
      local selected_before = selected_items()
      local snap_before = reaper.GetToggleCommandStateEx(0, 1157) == 1
      local undo_before = reaper.Undo_CanUndo2(0) or ""
      local change_before = reaper.GetProjectStateChangeCount(0)
      reaper.Main_OnCommand(tonumber(action_id), 0)
      pending_result = {
        label = label,
        action_id = action_id,
        selected_before = selected_before,
        snap_before = snap_before,
        undo_before = undo_before,
        change_before = change_before,
        positions_before = before,
      }
    end
  end
  reaper.defer(poll)
end
reaper.defer(poll)
