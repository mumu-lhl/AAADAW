-- Run in an isolated Linux REAPER 7.82 / Default 7 project.
-- Build a step tempo/meter map and export it through a 0.5-2.5 second selection.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
local item_end = tonumber(os.getenv("AAADAW_REAPER_ITEM_END_SECONDS")) or 1.25
local add_post_tempo_note = os.getenv("AAADAW_REAPER_POST_TEMPO_NOTE") == "1"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

assert(
    reaper.SetTempoTimeSigMarker(0, -1, 2.0, -1, -1, 90.0, 3, 4, false),
    "Could not add the 90 BPM / 3/4 step at 2 seconds"
)
reaper.UpdateTimeline()

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Piano", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0.25, item_end, false))
local take = assert(reaper.GetActiveTake(item))
local start_qn = reaper.TimeMap2_timeToQN(0, 0.25)
local note_start = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn)
local note_end = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 1.0)
local cc_tick = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 0.25)
local bend_tick = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 0.375)
assert(reaper.MIDI_InsertNote(take, false, false, note_start, note_end, 0, 60, 100, false))
assert(reaper.MIDI_InsertCC(take, false, false, cc_tick, 0xB0, 0, 1, 64))
assert(reaper.MIDI_InsertCC(take, false, false, bend_tick, 0xE0, 0, 0, 96))
if add_post_tempo_note then
    local post_item = assert(reaper.CreateNewMIDIItemInProj(track, 2.125, 2.375, false))
    local post_take = assert(reaper.GetActiveTake(post_item))
    local post_note_start_qn = reaper.TimeMap2_timeToQN(0, 2.125)
    local post_note_end_qn = reaper.TimeMap2_timeToQN(0, 2.375)
    local post_note_start = reaper.MIDI_GetPPQPosFromProjQN(post_take, post_note_start_qn)
    local post_note_end = reaper.MIDI_GetPPQPosFromProjQN(post_take, post_note_end_qn)
    assert(reaper.MIDI_InsertNote(post_take, false, false, post_note_start, post_note_end, 0, 62, 100, false))
end
reaper.MIDI_Sort(take)

reaper.GetSet_LoopTimeRange2(0, true, false, 0.5, 2.5, false)
reaper.UpdateArrange()
local save_project = os.getenv("AAADAW_REAPER_SAVE_PROJECT")
if save_project then
    reaper.Main_SaveProjectEx(0, save_project, 0)
end

local meta = assert(io.open(root .. "midi-export-tempo-selection-native-meta.tsv", "w"))
meta:write("version\t", reaper.GetAppVersion(), "\n")
meta:write("theme\tDefault_7.0.ReaperThemeZip\n")
meta:write("time_selection_seconds\t0.5-2.5\n")
meta:write("time_selection_project_ticks\t960-4560\n")
meta:write("midi_item_seconds\t0.25-", tostring(item_end), "\n")
meta:write("tempo_meter_change\t2 seconds; project tick 3840; output tick 2880; 90 BPM; 3/4\n")
meta:write("midi_events\tC4 project ticks 480-1440; CC1 tick 720; pitch bend tick 840\n")
if add_post_tempo_note then
    meta:write("post_tempo_note\tseparate D4 item at 2.125-2.375 seconds; inside the time selection\n")
end
meta:write("export_action\t40849\n")
meta:close()

reaper.Main_OnCommand(40849, 0)
local ready = assert(io.open(root .. "midi-export-tempo-selection-ready", "w"))
ready:write("Choose Type 1 or Type 0, Time selection only, PPQN 960, check tempo/meter changes, then export\n")
ready:close()
