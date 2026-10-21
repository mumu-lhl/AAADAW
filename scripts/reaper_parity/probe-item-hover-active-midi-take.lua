-- Run in a fresh, unsaved REAPER 7.82 / Default 7 project with no tracks.
-- Selects two MIDI items, adds the item under the real X11 pointer to the
-- selection, then opens the built-in MIDI editor to inspect its active take.
-- Set AAADAW_ACTIVE_TAKE_PROBE_DIR to an existing writable output directory.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in an unsaved project")

local directory = assert(os.getenv("AAADAW_ACTIVE_TAKE_PROBE_DIR"))
local request_path = directory .. "/request"
local ready_path = directory .. "/ready"
local output_path = directory .. "/item-hover-active-midi-take-native.tsv"
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
  local pitch = ({ A1 = 60, A2 = 62, B1 = 64, B2 = 65 })[label]
  assert(reaper.MIDI_InsertNote(take, false, false, 0, 240, 0, pitch, 100, false))
  reaper.MIDI_Sort(take)
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

local function item_name(item)
  for label, candidate in pairs(items) do
    if item == candidate then return label end
  end
  return item and "other" or "none"
end

local function take_name(take)
  if not take then return "none" end
  local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
  return name
end

local function selected_items(api_order)
  local names = {}
  for index = 0, reaper.CountSelectedMediaItems(0) - 1 do
    names[#names + 1] = item_name(reaper.GetSelectedMediaItem(0, index))
  end
  if not api_order then table.sort(names) end
  return table.concat(names, ",")
end

reaper.SetMediaItemSelected(items.A1, true)
reaper.SetMediaItemSelected(items.B1, true)
reaper.UpdateArrange()
local output = assert(io.open(output_path, "w"))
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("fixture\tTrack A: A1/A2; Track B: B1/B2; named MIDI takes\n")
output:write("step\taction_id\tpointer_xy\thovered_item\tselection_before\tselection_api_order_before\tselection_after\tselection_api_order_after\tactive_take\tactive_item\tactive_note_pitch\n")
output:flush()

local ready = assert(io.open(ready_path, "w"))
ready:write("ready\n")
ready:close()

local function poll_request()
  local request = io.open(request_path, "r")
  if request then
    local action_text = request:read("*l")
    request:close()
    os.remove(request_path)
    if action_text == "40529" or action_text == "40530" or action_text == "0" then
      local x, y = reaper.GetMousePosition()
      local hovered = reaper.GetItemFromPoint(x, y, true)
      local before = selected_items(false)
      local before_api = selected_items(true)
      if action_text ~= "0" then
        reaper.Main_OnCommand(tonumber(action_text), 0)
        reaper.UpdateArrange()
      end
      local after = selected_items(false)
      local after_api = selected_items(true)
      local result = {
        step = action_text == "40529"
          and "add_hovered_then_open_midi_editor"
          or action_text == "40530"
            and "toggle_hovered_then_open_midi_editor"
            or "open_midi_editor_without_action",
        action_id = action_text,
        pointer_xy = tostring(x) .. "," .. tostring(y),
        hovered_item = item_name(hovered),
        selection_before = before,
        selection_api_order_before = before_api,
        selection_after = after,
        selection_api_order_after = after_api,
      }
      reaper.Main_OnCommand(40153, 0) -- Item: Open in built-in MIDI editor
      local started = reaper.time_precise()
      local function poll_editor()
        local editor = reaper.MIDIEditor_GetActive()
        local take = editor and reaper.MIDIEditor_GetTake(editor)
        if take or reaper.time_precise() - started >= 10 then
          local note_pitch = "none"
          if take then
            local has_note, _, _, _, _, _, pitch = reaper.MIDI_GetNote(take, 0)
            if has_note then note_pitch = tostring(pitch) end
          end
          result.active_take = take_name(take)
          result.active_item = item_name(take and reaper.GetMediaItemTake_Item(take))
          result.active_note_pitch = note_pitch
          output:write(table.concat({
            result.step,
            result.action_id,
            result.pointer_xy,
            result.hovered_item,
            result.selection_before,
            result.selection_api_order_before,
            result.selection_after,
            result.selection_api_order_after,
            result.active_take or "unavailable",
            result.active_item or "unavailable",
            result.active_note_pitch,
          }, "\t"), "\n")
          output:flush()
          return
        end
        reaper.defer(poll_editor)
      end
      reaper.defer(poll_editor)
      return
    end
  end
  reaper.defer(poll_request)
end

reaper.defer(poll_request)
