-- Run in an isolated Linux REAPER 7.82 / Default 7 instance.
-- Creates the standard MIDI fixture with a nonzero project start-time offset.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

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

local project_file = root .. "midi-export-smpte-offset.rpp"
reaper.Main_SaveProjectEx(0, project_file, 8)

-- REAPER exposes GetProjectTimeOffset but no ReaScript setter. Its native RPP
-- field is PROJOFFS; set 10.5 seconds in this disposable fixture, reopen it in
-- REAPER, and verify the loaded value before collecting the MIDI reference.
local source = assert(io.open(project_file, "r"))
local project_text = source:read("*a")
source:close()
local patched_text, replacements = project_text:gsub("PROJOFFS 0 0 0", "PROJOFFS 10.5 0 0", 1)
assert(replacements == 1, "Expected one default PROJOFFS record in the fresh RPP")
local temporary_file = assert(io.open(project_file .. ".tmp", "w"))
temporary_file:write(patched_text)
temporary_file:close()
assert(os.rename(project_file .. ".tmp", project_file))
reaper.Main_openProject("noprompt:" .. project_file)
local actual_offset_seconds = reaper.GetProjectTimeOffset(0, false)
assert(math.abs(actual_offset_seconds - 10.5) < 1e-9,
  string.format("Loaded project start offset is %.9f seconds; expected 10.500000000", actual_offset_seconds))
reaper.Main_SaveProjectEx(0, project_file, 8)

local meta = assert(io.open(root .. "midi-export-smpte-offset-native-meta.tsv", "w"))
meta:write("version\t", reaper.GetAppVersion(), "\n")
meta:write("theme\tDefault_7.0.ReaperThemeZip\n")
meta:write("fixture\ttrack Piano; C4 note, CC1, pitch bend\n")
meta:write("project_frame_rate\t30 fps; non-drop\n")
meta:write("project_time_offset_seconds\t", actual_offset_seconds, "\n")
meta:write("project_time_offset_timecode\t00:00:10:15\n")
meta:write("export_action\t40849\n")
meta:write("export_mode\tType 1; PPQN 960\n")
meta:write("export_tempo_meter_changes\tunchecked\n")
meta:write("export_smpte_offset\tchecked\n")
meta:write("native_export_path\t/tmp/midi-smpte-offset-native-20261016.mid\n")
meta:close()

local ready = assert(io.open(root .. "midi-export-smpte-offset-ready", "w"))
ready:write("Verify Project Settings shows 0:10.500 and Video frame rate 30, then create open-smpte-offset-export\n")
ready:close()

local function wait_for_export()
  local trigger = io.open(root .. "open-smpte-offset-export", "r")
  if not trigger then
    reaper.defer(wait_for_export)
    return
  end
  trigger:close()
  os.remove(root .. "open-smpte-offset-export")
  reaper.Main_OnCommand(40849, 0)
end

reaper.defer(wait_for_export)
