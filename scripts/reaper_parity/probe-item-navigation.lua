-- Run in a fresh unsaved Linux REAPER 7.82 / Default 7 project while stopped.
-- This fixture uses real item clicks: click a source item, then write
-- label<TAB>action-id[<TAB>cursor-seconds] to the request file below.
-- The script records one action result and never saves the project.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)

local tracks, labels = {}, {}
local function add_track(name, index)
  reaper.InsertTrackAtIndex(index, true)
  local tr = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(tr, "P_NAME", name, true)
  tracks[name] = tr
end
local function add_item(track, name, pos, len)
  local item = reaper.AddMediaItemToTrack(tracks[track])
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", pos)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", len)
  labels[item] = name
end
add_track("U", 0)
add_track("EMPTY", 1)
add_track("M", 2)
add_track("L", 3)
add_item("U", "UA", 0, 1)
add_item("U", "UB", 2, 1)
add_item("U", "UO", 2.5, 1)
add_item("U", "UC", 4, 1)
add_item("EMPTY", "EA", 0, 1)
add_item("EMPTY", "EA2", 2, 1)
add_item("EMPTY", "EASTART", 3, 0.25)
add_item("EMPTY", "EALATE", 3.5, 1) -- overlaps MX but not its start
add_item("M", "MD", 0.5, 1)
add_item("M", "MN", 1.5, 0.5) -- exactly touches MD and ME
add_item("M", "ME", 2, 1)
add_item("M", "MO", 2.5, 1)
add_item("M", "MX", 3, 1) -- exactly touches ME and MF
add_item("M", "MF", 4, 1)
local midi = reaper.CreateNewMIDIItemInProj(tracks.M, 5, 6, false)
assert(midi, "Could not create MIDI edge fixture")
labels[midi] = "MIDI"
add_item("L", "LA", 0, 1)
add_item("L", "LB", 2, 1)
add_item("L", "LC", 4, 1)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 6.5)
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()

local req_path = "/tmp/aaadaw-item-navigation-request"
os.remove(req_path)
os.remove("/tmp/aaadaw-item-navigation-ready")
local out = assert(io.open("/tmp/aaadaw-item-navigation-native.tsv", "w"))
local function selected_items()
  local found = {}
  for i = 0, reaper.CountSelectedMediaItems(0) - 1 do
    local item = reaper.GetSelectedMediaItem(0, i)
    found[#found + 1] = labels[item] or "?"
  end
  table.sort(found)
  return table.concat(found, ",")
end
local function selected_tracks()
  local found = {}
  for i = 0, reaper.CountTracks(0) - 1 do
    local tr = reaper.GetTrack(0, i)
    if reaper.IsTrackSelected(tr) then
      local _, name = reaper.GetSetMediaTrackInfo_String(tr, "P_NAME", "", false)
      found[#found + 1] = name
    end
  end
  return table.concat(found, ",")
end
out:write("version\t", reaper.GetAppVersion(), "\n")
out:write("theme\tDefault_7.0.ReaperThemeZip\n")
out:write("fixture\tU:UA[0,1],UB[2,3],UO[2.5,3.5],UC[4,5]; EMPTY:EA[0,1],EA2[2,3],EASTART[3,3.25],EALATE[3.5,4.5]; M:MD[0.5,1.5],MN[1.5,2],ME[2,3],MO[2.5,3.5],MX[3,4],MF[4,5],MIDI[5,6]; L:LA[0,1],LB[2,3],LC[4,5]\n")
out:write("case\taction_id\tselected_items_before\tselected_items_after\tselected_tracks_after\tcursor_after_seconds\tproject_state_delta\n")
out:flush()
local ready = assert(io.open("/tmp/aaadaw-item-navigation-ready", "w"))
ready:write("ready\n")
ready:close()
local function poll()
  local input = io.open(req_path, "r")
  if input then
    local request = input:read("*a")
    input:close()
    os.remove(req_path)
    local label, action_id, cursor_override = request:match("([^\t\r\n]+)\t(%d+)\t?([^\t\r\n]*)")
    if label == "done" then out:close(); return end
    if label and action_id then
      local before = selected_items()
      if cursor_override and cursor_override ~= "" then
        reaper.SetEditCurPos(tonumber(cursor_override), false, false)
      end
      local change_before = reaper.GetProjectStateChangeCount(0)
      reaper.Main_OnCommand(tonumber(action_id), 0)
      out:write(table.concat({
        label, action_id, before, selected_items(), selected_tracks(),
        string.format("%.6f", reaper.GetCursorPosition()),
        tostring(reaper.GetProjectStateChangeCount(0) - change_before),
      }, "\t"), "\n")
      out:flush()
    end
  end
  reaper.defer(poll)
end
reaper.defer(poll)
