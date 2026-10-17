-- Run in an isolated Linux REAPER 7.82 / Default 7 instance.
-- Use the Marker Manager to select one marker, then create the trigger file to run 43484.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "Run in a fresh project")

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
local item = reaper.CreateNewMIDIItemInProj(track, 0, 4, false)
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 960, 0, 60, 100, false))
reaper.MIDI_Sort(take)
for index, position in ipairs({ 1, 3, 5 }) do
  assert(reaper.AddProjectMarker2(0, false, position, 0, "Visibility " .. index, index, 0))
end
assert(reaper.AddProjectMarker2(0, true, 7, 9, "Visible region", 1, 0))
reaper.Main_SaveProjectEx(0, root .. "marker-visibility-selected-before.rpp", 0)
reaper.Main_OnCommand(40326, 0)

local function marker_count()
  local _, markers = reaper.CountProjectMarkers(0)
  return markers
end

local function wait_for_selection()
  local trigger = io.open(root .. "toggle-selected-marker", "r")
  if not trigger then
    reaper.defer(wait_for_selection)
    return
  end
  trigger:close()
  local before = reaper.GetProjectStateChangeCount(0)
  local undo_before = reaper.Undo_CanUndo2(0) or ""
  reaper.Main_OnCommand(43484, 0)
  local log = assert(io.open(root .. "marker-visibility-selected-native.tsv", "w"))
  log:write("version\t", reaper.GetAppVersion(), "\n")
  log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
  log:write("action\t43484\n")
  log:write("state_delta\t", reaper.GetProjectStateChangeCount(0) - before, "\n")
  log:write("undo_label_before\t", undo_before, "\n")
  log:write("undo_label_after\t", reaper.Undo_CanUndo2(0) or "", "\n")
  log:write("marker_count\t", marker_count(), "\n")
  log:close()
  reaper.Main_SaveProjectEx(0, root .. "marker-visibility-selected-after.rpp", 0)
  local ready = assert(io.open(root .. "marker-visibility-selected-ready", "w"))
  ready:write("Selected marker action completed\n")
  ready:close()
end

reaper.defer(wait_for_selection)
