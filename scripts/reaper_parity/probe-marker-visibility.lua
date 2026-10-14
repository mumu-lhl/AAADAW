-- Run only in a fresh Linux REAPER 7.82 / Default 7 instance.
-- Probe the Main action that toggles all project marker visibility and saves owned evidence.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run in an unsaved project")

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
local log = assert(io.open(root .. "marker-visibility-native.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("case\taction\tstate_delta\tundo_label\tmarker_count\tregion_count\n")
local function counts()
  local _, markers, regions = reaper.CountProjectMarkers(0)
  return markers, regions
end
local function run_case(name)
  local state = reaper.GetProjectStateChangeCount(0)
  reaper.Main_OnCommand(43485, 0)
  local markers, regions = counts()
  log:write(name, "\t43485\t", reaper.GetProjectStateChangeCount(0) - state,
    "\t", reaper.Undo_CanUndo2(0) or "", "\t", markers, "\t", regions, "\n")
  log:flush()
end
run_case("hide_all")
reaper.Main_SaveProjectEx(0, root .. "marker-visibility-hidden.rpp", 0)
log:close()
reaper.Main_OnCommand(40849, 0)
local ready = assert(io.open(root .. "marker-visibility-ready", "w"))
ready:write("Hidden-marker MIDI export dialog opened\n")
ready:close()
reaper.defer(function() end)
