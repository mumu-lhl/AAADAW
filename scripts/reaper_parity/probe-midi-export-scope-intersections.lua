-- Run only in an isolated Linux REAPER 7.82 / Default 7 project.
-- Probes Selected tracks / Selected items combined with Time selection only.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

local function create_track(index, name, selected)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  reaper.SetTrackSelected(track, selected)
  return track
end

local function create_item(track, name, start_time, end_time, pitch, selected)
  local item = assert(reaper.CreateNewMIDIItemInProj(track, start_time, end_time, false))
  reaper.GetSetMediaItemInfo_String(item, "P_NAME", name, true)
  reaper.SetMediaItemSelected(item, selected)
  local take = assert(reaper.GetActiveTake(item))
  assert(reaper.MIDI_InsertNote(take, false, false, 0, 240, 0, pitch, 100, false))
  reaper.MIDI_Sort(take)
  return item
end

local track_a = create_track(0, "Track A", true)
create_track(1, "Track B", false)
local item_c4 = create_item(track_a, "C4 outside", 0.25, 0.75, 60, true)
local item_c_sharp = create_item(track_a, "C#4 inside", 1.25, 1.75, 61, false)
create_item(reaper.GetTrack(0, 1), "D4 outside", 2.25, 2.75, 62, false)
reaper.GetSet_LoopTimeRange2(0, true, false, 1.0, 2.0, false)
reaper.UpdateArrange()

local save_project = os.getenv("AAADAW_REAPER_SAVE_PROJECT")
if save_project then reaper.Main_SaveProjectEx(0, save_project, 0) end

local meta = assert(io.open(root .. "midi-export-scope-intersections-native-meta.tsv", "w"))
meta:write("version\t", reaper.GetAppVersion(), "\n")
meta:write("theme\tDefault_7.0.ReaperThemeZip\n")
meta:write("track_selection\tTrack A selected; Track B unselected\n")
meta:write("initial_item_selection\tC4 at 0.25-0.75 seconds; outside time selection\n")
meta:write("in_range_item\tC#4 at 1.25-1.75 seconds\n")
meta:write("time_selection\t1.000000-2.000000 seconds; project ticks 1920-3840\n")
meta:write("export_action\t40849\n")
meta:close()

local function write_ready(name)
  local ready = assert(io.open(root .. name .. "-ready", "w"))
  ready:write("Export this scope state, then create the next request file to continue\n")
  ready:close()
end

local function open_export_for_item_selection(name, selected_item)
  reaper.SetMediaItemSelected(item_c4, selected_item == item_c4)
  reaper.SetMediaItemSelected(item_c_sharp, selected_item == item_c_sharp)
  reaper.UpdateArrange()
  reaper.Main_OnCommand(40849, 0)
  write_ready(name)
end

reaper.Main_OnCommand(40849, 0)
write_ready("midi-export-scope-intersections-track-time")

local function poll_request()
  local request_path = root .. "midi-export-scope-intersections-next"
  local request = io.open(request_path, "r")
  if request then
    local mode = request:read("*l")
    request:close()
    os.remove(request_path)
    if mode == "items-outside" then
      open_export_for_item_selection("midi-export-scope-intersections-items-outside-time", item_c4)
    elseif mode == "items-inside" then
      open_export_for_item_selection("midi-export-scope-intersections-items-inside-time", item_c_sharp)
    else
      error("Unknown scope-intersection probe request: " .. tostring(mode))
    end
  end
  reaper.defer(poll_request)
end

reaper.defer(poll_request)
