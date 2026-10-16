-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- It builds a disposable two-track MIDI arrange and waits for requests:
-- label<TAB>action-id<TAB>selection-list (comma-separated labels or "none").
-- Move the real X11 pointer to an item or empty Arrange area before each request.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local request_path = output_dir .. "/item-hover-actions-request"
local ready_path = output_dir .. "/item-hover-actions-ready"
local output_path = output_dir .. "/item-hover-actions-native.tsv"
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

local function set_selected(spec)
  for _, item in pairs(items) do reaper.SetMediaItemSelected(item, false) end
  if spec ~= "none" and spec ~= "" then
    for label in spec:gmatch("[^,]+") do
      assert(items[label], "Unknown selection label " .. label)
      reaper.SetMediaItemSelected(items[label], true)
    end
  end
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

out:write("version\t", reaper.GetAppVersion(), "\n")
out:write("theme\tDefault_7.0.ReaperThemeZip\n")
out:write("fixture\tTwo tracks; MIDI items A1/A2 and B1/B2 at 1-2, 4-5, 1-2, and 3-4 seconds\n")
out:write("case\taction_id\tpointer_xy\thovered_item\tselection_before\tselection_after\tselected_tracks_after\tproject_state_delta\tundo_before\tundo_after\tcursor_seconds\n")
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
    local label, action_text, selection_spec = request:match("([^\t\r\n]+)\t(%d+)\t([^\t\r\n]*)")
    if label == "done" then out:close(); return end
    if label and action_text and selection_spec then
      set_selected(selection_spec)
      reaper.UpdateArrange()
      local before = selected_items()
      local x, y = reaper.GetMousePosition()
      local hovered_before = hovered_item()
      local project_state_before = reaper.GetProjectStateChangeCount(0)
      local undo_before = reaper.Undo_CanUndo2(0) or ""
      reaper.Main_OnCommand(tonumber(action_text), 0)
      out:write(table.concat({
        label,
        action_text,
        tostring(x) .. "," .. tostring(y),
        hovered_before,
        before,
        selected_items(),
        selected_tracks(),
        tostring(reaper.GetProjectStateChangeCount(0) - project_state_before),
        undo_before,
        reaper.Undo_CanUndo2(0) or "",
        string.format("%.6f", reaper.GetCursorPosition()),
      }, "\t"), "\n")
      out:flush()
    end
  end
  reaper.defer(poll)
end
reaper.defer(poll)
