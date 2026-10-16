-- Run only in an isolated, unsaved Linux REAPER 7.82 / Default 7 project.
-- Builds the note/CC/pitch-bend fixture inside a 120-to-60 BPM linear ramp,
-- then opens Main action 40849 for a manual path and tempo/meter checkbox.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

local ramp_ok = reaper.SetTempoTimeSigMarker(0, -1, 0.0, -1, -1, 120.0, 4, 4, true)
local end_ok = reaper.SetTempoTimeSigMarker(0, -1, -1, 2, 0.0, 60.0, 4, 4, false)
assert(ramp_ok and end_ok, "Could not create the 120-to-60 BPM linear ramp")
reaper.UpdateTimeline()

local item_start = reaper.TimeMap2_QNToTime(0, 0.5)
local item_end = reaper.TimeMap2_QNToTime(0, 1.5)
reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Piano", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, item_start, item_end, false))
local take = assert(reaper.GetActiveTake(item))
local requested_beat_attach_mode = tonumber(os.getenv("AAADAW_REAPER_ITEM_BEAT_MODE"))
if requested_beat_attach_mode then
  assert(reaper.SetMediaItemInfo_Value(item, "C_BEATATTACHMODE", requested_beat_attach_mode))
  reaper.UpdateItemInProject(item)
end
local item_beat_attach_mode = reaper.GetMediaItemInfo_Value(item, "C_BEATATTACHMODE")
local item_loop_source = reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC")
local project_timebase = reaper.GetSetProjectInfo(0, "PROJECT_TIMEBASE", 0, false)
local start_qn = reaper.TimeMap2_timeToQN(0, item_start)
local start_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn)
local note_end_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 1.0)
local cc_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 0.25)
local bend_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 0.375)
local project_ppq_at_item_start = math.floor(start_qn * 960 + 0.5)
local project_ppq_at_note_end = math.floor((start_qn + 1.0) * 960 + 0.5)
local project_ppq_at_cc = math.floor((start_qn + 0.25) * 960 + 0.5)
local project_ppq_at_pitch_bend = math.floor((start_qn + 0.375) * 960 + 0.5)
assert(reaper.MIDI_InsertNote(take, false, false, start_ppq, note_end_ppq, 0, 60, 100, false))
assert(reaper.MIDI_InsertCC(take, false, false, cc_ppq, 0xB0, 0, 1, 64))
assert(reaper.MIDI_InsertCC(take, false, false, bend_ppq, 0xE0, 0, 0, 96))
reaper.MIDI_Sort(take)

local meta = assert(io.open(root .. "midi-export-tempo-ramp-native-meta.tsv", "w"))
meta:write("version\t", reaper.GetAppVersion(), "\n")
meta:write("theme\tDefault_7.0.ReaperThemeZip\n")
meta:write("fixture\ttrack Piano; MIDI source spans project QN 0.5-1.5; C4 note, CC1, pitch bend\n")
meta:write("take_ppq_at_item_start\t", string.format("%.6f", start_ppq), "\n")
meta:write("take_ppq_at_note_end\t", string.format("%.6f", note_end_ppq), "\n")
meta:write("take_ppq_at_cc\t", string.format("%.6f", cc_ppq), "\n")
meta:write("take_ppq_at_pitch_bend\t", string.format("%.6f", bend_ppq), "\n")
meta:write("project_ppq_at_item_start\t", project_ppq_at_item_start, "\n")
meta:write("project_ppq_at_note_end\t", project_ppq_at_note_end, "\n")
meta:write("project_ppq_at_cc\t", project_ppq_at_cc, "\n")
meta:write("project_ppq_at_pitch_bend\t", project_ppq_at_pitch_bend, "\n")
meta:write("item_requested_beat_attach_mode\t", requested_beat_attach_mode or "project default", "\n")
meta:write("item_beat_attach_mode\t", item_beat_attach_mode, "\n")
meta:write("item_loop_source\t", item_loop_source, "\n")
meta:write("project_timebase\t", project_timebase, "\n")
for _, event in ipairs({
  { name = "note_start", ppq = start_ppq },
  { name = "note_end", ppq = note_end_ppq },
  { name = "cc", ppq = cc_ppq },
  { name = "pitch_bend", ppq = bend_ppq },
}) do
  local name, ppq = event.name, event.ppq
  local event_time = reaper.MIDI_GetProjTimeFromPPQPos(take, ppq)
  local event_qn = reaper.TimeMap2_timeToQN(0, event_time)
  meta:write("take_event_", name, "\t", string.format(
    "time %.9f; QN %.9f; project PPQ %.6f",
    event_time, event_qn, event_qn * 960
  ), "\n")
end
meta:write("tempo_ramp\t120 BPM at QN 0; linear to 60 BPM at QN 8\n")
local tempo_marker_count = reaper.CountTempoTimeSigMarkers(0)
meta:write("tempo_marker_count\t", tempo_marker_count, "\n")
for index = 0, tempo_marker_count - 1 do
  local _, timepos, measurepos, beatpos, bpm, numerator, denominator, lineartempo =
    reaper.GetTempoTimeSigMarker(0, index)
  meta:write("tempo_marker_", index, "\t", string.format(
    "time %.9f; measure %.6f; beat %.6f; %.6f BPM; %d/%d; linear %s",
    timepos, measurepos, beatpos, bpm, numerator, denominator, tostring(lineartempo)
  ), "\n")
end
for _, tick in ipairs({ 0, 60, 120, 180, 7680 }) do
  local timepos = reaper.TimeMap2_QNToTime(0, tick / 960)
  local bpm = reaper.TimeMap2_GetDividedBpmAtTime(0, timepos)
  meta:write("tempo_sample_tick_", tick, "\t", string.format(
    "time %.9f; %.9f BPM", timepos, bpm
  ), "\n")
end
meta:write("export_action\t40849\n")
meta:write("export_settings_to_use\tType 1; PPQN 960; tempo/meter checked; SMPTE/markers unchecked\n")
meta:close()

if os.getenv("AAADAW_REAPER_METADATA_ONLY") == "1" then
  return
end

reaper.Main_OnCommand(40849, 0)
local ready = assert(io.open(root .. "midi-export-tempo-ramp-ready", "w"))
ready:write("Enter an unused .mid path, check Embed project tempo/time signature changes, and click OK\n")
ready:close()
