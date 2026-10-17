-- Probe the native REAPER 7.82 Nudge units `item lengths`, `item selections`,
-- and `notes` with unequal Audio items, a time selection, and a MIDI item.
-- Run in a fresh unsaved Linux REAPER 7.82 / Default 7 project. Set
-- AAADAW_REAPER_PROBE_DIR and AAADAW_SPLIT_SOURCE_PATH before running.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local output = assert(io.open(root .. "item-nudge-relative-units-native.tsv", "w"))
local command_path = root .. "item-nudge-relative-units-command.txt"

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local audio_track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(audio_track, "P_NAME", "Nudge length units", true)
for _, fixture in ipairs({ { 1.0, 0.5 }, { 4.0, 1.0 } }) do
  reaper.SetOnlyTrackSelected(audio_track)
  assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert probe audio")
  local item = assert(reaper.GetTrackMediaItem(audio_track, reaper.CountTrackMediaItems(audio_track) - 1))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", fixture[1])
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", fixture[2])
  reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 0)
end

reaper.InsertTrackAtIndex(1, true)
local midi_track = assert(reaper.GetTrack(0, 1))
reaper.GetSetMediaTrackInfo_String(midi_track, "P_NAME", "Nudge note units", true)
local midi_item = assert(reaper.CreateNewMIDIItemInProj(midi_track, 0, 8, false))
local midi_take = assert(reaper.GetActiveTake(midi_item))
assert(reaper.MIDI_InsertNote(midi_take, false, false, 0, 480, 0, 60, 96, true))
assert(reaper.MIDI_InsertNote(midi_take, false, false, 960, 1440, 0, 64, 100, true))
assert(reaper.MIDI_InsertNote(midi_take, false, false, 2880, 3840, 0, 67, 108, true))
reaper.MIDI_Sort(midi_take)
reaper.SetMediaItemInfo_Value(midi_item, "B_LOOPSRC", 0)
reaper.GetSet_LoopTimeRange2(0, true, false, 2, 5, false)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 0, 8)
reaper.UpdateArrange()
reaper.Undo_EndBlock2(0, "Nudge relative-unit probe setup", -1)

local audio_items = {
  assert(reaper.GetTrackMediaItem(audio_track, 0)),
  assert(reaper.GetTrackMediaItem(audio_track, 1)),
}
local function write_state(label)
  local _, note_count = reaper.MIDI_CountEvts(midi_take)
  output:write(label,
    "\tstate_change_count\t", reaper.GetProjectStateChangeCount(0),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""),
    "\taudio_a_position_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_items[1], "D_POSITION")),
    "\taudio_a_length_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_items[1], "D_LENGTH")),
    "\taudio_a_selected\t", reaper.IsMediaItemSelected(audio_items[1]) and "1" or "0",
    "\taudio_b_position_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_items[2], "D_POSITION")),
    "\taudio_b_length_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_items[2], "D_LENGTH")),
    "\taudio_b_selected\t", reaper.IsMediaItemSelected(audio_items[2]) and "1" or "0",
    "\tmidi_position_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(midi_item, "D_POSITION")),
    "\tmidi_length_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(midi_item, "D_LENGTH")),
    "\tmidi_selected\t", reaper.IsMediaItemSelected(midi_item) and "1" or "0",
    "\tmidi_note_count\t", note_count, "\tmidi_notes_ppq")
  for index = 0, note_count - 1 do
    local _, _, _, start_ppq, end_ppq, channel, pitch = reaper.MIDI_GetNote(midi_take, index)
    output:write("\t", pitch, ":", channel, ":", start_ppq, ":", end_ppq)
  end
  output:write("\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("project_sample_rate\t", reaper.GetSetProjectInfo(0, "PROJECT_SRATE", 0, false), "\n")
output:write("dialog_action\t41226\n")
output:write("nudge_right_action\t41249\n")
output:write("nudge_left_action\t41250\n")
output:write("fixture\tAudio A: 1.000s / 0.500s; Audio B: 4.000s / 1.000s; MIDI item: 0-8s with 0.250s, 0.250s, and 0.500s notes; time selection 2-5s\n")

local function reset(label, selection)
  reaper.SetMediaItemInfo_Value(audio_items[1], "D_POSITION", 1.0)
  reaper.SetMediaItemInfo_Value(audio_items[1], "D_LENGTH", 0.5)
  reaper.SetMediaItemInfo_Value(audio_items[2], "D_POSITION", 4.0)
  reaper.SetMediaItemInfo_Value(audio_items[2], "D_LENGTH", 1.0)
  reaper.SetMediaItemInfo_Value(midi_item, "D_POSITION", 0.0)
  reaper.SetMediaItemInfo_Value(midi_item, "D_LENGTH", 8.0)
  reaper.SetMediaItemSelected(audio_items[1], selection:find("A", 1, true) ~= nil)
  reaper.SetMediaItemSelected(audio_items[2], selection:find("B", 1, true) ~= nil)
  reaper.SetMediaItemSelected(midi_item, selection:find("M", 1, true) ~= nil)
  reaper.UpdateArrange()
  write_state("baseline_" .. label)
end

reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()
local function poll_command()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    if not command then
      reaper.defer(poll_command)
      return
    end
    os.remove(command_path)
    local action, label, selection = command:match("^([^|]+)|?([^|]*)|?([^|]*)$")
    if action == "reset" then
      reset(label, selection)
    elseif action == "right" or action == "left" then
      reaper.Main_OnCommand(action == "right" and 41249 or 41250, 0)
      reaper.UpdateArrange()
      write_state("after_" .. action .. "_" .. label)
    elseif action == "done" then
      output:close()
      return
    end
  end
  reaper.defer(poll_command)
end
poll_command()
