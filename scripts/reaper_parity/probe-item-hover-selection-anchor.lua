-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project while stopped.
-- The probe creates the same disposable two-track MIDI arrangement as
-- probe-item-hover-actions.lua, then waits for mouse-driven requests so the
-- item-selection anchor can be tested through native clicks after 40529/40530.
-- Set AAADAW_REAPER_PROBE_DIR to an existing writable output directory.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0, "Stop playback before running this probe")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in a blank project")

local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local request_path = output_dir .. "/item-hover-anchor-request"
local ready_path = output_dir .. "/item-hover-anchor-ready"
local output_path = output_dir .. "/item-hover-anchor-native.tsv"
os.remove(request_path)
os.remove(ready_path)

local tracks, items = {}, {}
local function add_track(name, index)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  tracks[name] = track
end

local function add_item(track_name, label, start_time, end_time)
  local item = reaper.CreateNewMIDIItemInProj(
    tracks[track_name], start_time, end_time, false
  )
  assert(item, "Could not create MIDI item " .. label)
  local take = reaper.GetActiveTake(item)
  assert(take, "Could not create MIDI take " .. label)
  reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", label, true)
  reaper.SetMediaItemSelected(item, false)
  items[label] = item
end

add_track("A", 0)
add_track("B", 1)
add_item("A", "A1", 1.0, 2.0)
add_item("A", "A2", 4.0, 5.0)
add_item("B", "B1", 1.0, 2.0)
add_item("B", "B2", 3.0, 4.0)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 6.0)
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()

local out = assert(io.open(output_path, "w"))
local function selected_items()
  local found = {}
  for index = 0, reaper.CountSelectedMediaItems(0) - 1 do
    local item = reaper.GetSelectedMediaItem(0, index)
    for label, candidate in pairs(items) do
      if candidate == item then found[#found + 1] = label end
    end
  end
  table.sort(found)
  return table.concat(found, ",")
end

local function selected_flags()
  local found = {}
  for label, item in pairs(items) do
    if reaper.GetMediaItemInfo_Value(item, "B_UISEL") > 0 then
      found[#found + 1] = label
    end
  end
  table.sort(found)
  return table.concat(found, ",")
end

local function hovered_item()
  if type(reaper.GetItemFromPoint) ~= "function" then return "unavailable" end
  local x, y = reaper.GetMousePosition()
  local item = reaper.GetItemFromPoint(x, y, true)
  for label, candidate in pairs(items) do
    if item == candidate then return label end
  end
  return item and "other" or "none"
end

local function snapshot(label, action_id)
  local x, y = reaper.GetMousePosition()
  local undo = reaper.Undo_CanUndo2(0) or ""
  local project_state = reaper.GetProjectStateChangeCount(0)
  out:write(table.concat({
    label,
    action_id or "",
    tostring(x) .. "," .. tostring(y),
    hovered_item(),
    selected_items(),
    selected_flags(),
    tostring(project_state),
    undo,
  }, "\t"), "\n")
  out:flush()
end

out:write("version\t", reaper.GetAppVersion(), "\n")
out:write("theme\tDefault_7.0.ReaperThemeZip\n")
out:write("fixture\tTwo tracks; MIDI items A1/A2 and B1/B2 at 1-2, 4-5, 1-2, and 3-4 seconds\n")
out:write("step\taction_id\tpointer_xy\thovered_item\tselected_items_api\tselected_flags\tproject_state_count\tundo_label\n")
out:flush()

local ready = assert(io.open(ready_path, "w"))
ready:write("ready\n")
ready:close()

local function poll()
  local input = io.open(request_path, "r")
  if input then
    local request = input:read("*a")
    input:close()
    os.remove(request_path)
    local operation, label, action_text = request:match("([^\t\r\n]+)\t([^\t\r\n]+)\t?([^\t\r\n]*)")
    if operation == "done" then out:close(); return end
    if operation == "run" and label and action_text then
      local x, y = reaper.GetMousePosition()
      local target = reaper.GetItemFromPoint(x, y, true)
      local target_id
      for item_label, item in pairs(items) do
        if item == target then target_id = item_label end
      end
      snapshot(label .. "_before", action_text)
      reaper.Main_OnCommand(tonumber(action_text), 0)
      snapshot(label .. "_after", action_text)
      if target and not target_id then
        out:write("unmapped_target\t", label, "\t", tostring(target), "\n")
        out:flush()
      end
    elseif operation == "snapshot" and label then
      snapshot(label, "")
    end
  end
  reaper.defer(poll)
end
reaper.defer(poll)
