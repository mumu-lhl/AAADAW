-- Run in an isolated Linux REAPER 7.82 / Default 7 project.
-- Builds the Type 0 marker fixture, waits for #Hidden to be hidden in the
-- Region/Marker Manager, then opens the MIDI export dialog.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

for _, marker in ipairs({
  { position = 0.0, name = "#Intro" },
  { position = 0.5, name = "Verse" },
  { position = 1.0, name = "#Hidden" },
}) do
  assert(reaper.AddProjectMarker2(0, false, marker.position, marker.position, marker.name, -1, 0) >= 0)
end

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Piano", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0.25, 1.25, false))
local take = assert(reaper.GetActiveTake(item))
local start_qn = reaper.TimeMap2_timeToQN(0, 0.25)
local start_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn)
local note_end_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 1.0)
local cc_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 0.25)
local bend_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 0.375)
assert(reaper.MIDI_InsertNote(take, false, false, start_ppq, note_end_ppq, 0, 60, 100, false))
assert(reaper.MIDI_InsertCC(take, false, false, cc_ppq, 0xB0, 0, 1, 64))
assert(reaper.MIDI_InsertCC(take, false, false, bend_ppq, 0xE0, 0, 0, 96))
reaper.MIDI_Sort(take)

local ready = assert(io.open(root .. "midi-export-type0-hidden-ready", "w"))
ready:write("Hide #Hidden in Region/Marker Manager, then create hide-selected-marker\n")
ready:close()
reaper.Main_OnCommand(40326, 0)

local function wait_for_hidden_marker()
  local trigger = io.open(root .. "hide-selected-marker", "r")
  if not trigger then
    reaper.defer(wait_for_hidden_marker)
    return
  end
  trigger:close()
  os.remove(root .. "hide-selected-marker")
  local before = reaper.GetProjectStateChangeCount(0)
  reaper.Main_OnCommand(43484, 0)
  local state_delta = reaper.GetProjectStateChangeCount(0) - before
  assert(state_delta == 1, "Hiding the marker should make one project-state change")
  reaper.Main_SaveProjectEx(0, root .. "midi-export-type0-hidden-markers.rpp", 0)

  local meta = assert(io.open(root .. "midi-export-type0-hidden-native-meta.tsv", "w"))
  meta:write("version\t", reaper.GetAppVersion(), "\n")
  meta:write("theme\tDefault_7.0.ReaperThemeZip\n")
  meta:write("fixture\ttrack Piano; C4 note, CC1, pitch bend; #Intro tick 0; Verse tick 960; hidden #Hidden tick 1920\n")
  meta:write("marker_visibility_action\t43484; state change delta ", state_delta, "\n")
  meta:write("export_action\t40849\n")
  meta:write("export_mode\tType 0; PPQN 960\n")
  meta:write("export_options\tmarkers checked; # filter unchecked; tempo/meter/SMPTE unchecked\n")
  meta:write("hidden_marker_cases\tExport hidden markers unchecked and checked\n")
  meta:close()

  reaper.Main_OnCommand(40849, 0)
  local export_ready = assert(io.open(root .. "midi-export-type0-hidden-export-ready", "w"))
  export_ready:write("Export Type 0 with marker export on, hidden-marker export off, then on\n")
  export_ready:close()
end

reaper.defer(wait_for_hidden_marker)
