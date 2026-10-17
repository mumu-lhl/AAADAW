-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated config.
-- Builds the same-pitch/different-channel case for the MIDI editor's channel filter.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-channel-editor-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI channel filter", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 8, false))
local take = assert(reaper.GetActiveTake(item))
for _, note in ipairs({
  {0, 60, 96},
  {1920, 60, 96},
  {3840, 60, 96},
  {5760, 64, 96},
}) do
  local start = note[1]
  local channel = math.floor(start / 1920)
  assert(reaper.MIDI_InsertNote(take, false, false, start, start + 960, channel % 3, note[2], note[3], false))
end
for _, event in ipairs({
  {0, 0, 0xB0, 20, 100},
  {0, 1, 0xB0, 20, 80},
  {0, 2, 0xB0, 20, 60},
  {480, 0, 0xE0, 0, 64},
  {480, 1, 0xE0, 0, 96},
  {480, 2, 0xE0, 0, 32},
}) do
  local ppq, channel, message, data1, data2 = table.unpack(event)
  assert(reaper.MIDI_InsertCC(take, false, false, ppq, message, channel, data1, data2))
end
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
local _, note_count, cc_count, _ = reaper.MIDI_CountEvts(take)
local channels = {}
for index = 0, note_count - 1 do
  local ok, _, _, start_ppq, _, channel, pitch = reaper.MIDI_GetNote(take, index)
  assert(ok, "could not read MIDI note " .. tostring(index))
  channels[#channels + 1] = table.concat({channel + 1, pitch, start_ppq}, ",")
end
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("notes_channel_pitch_tick\t", table.concat(channels, ";"), "\n")
output:write("controller_and_pitch_bend_events\t", cc_count, "\n")
output:flush()
reaper.Main_OnCommand(40153, 0)
local editor = reaper.MIDIEditor_GetActive()
assert(editor and reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open the probe take")
for _, key in ipairs({
  "default_note_chan", "channel_filter", "filter_channel", "note_channel",
  "last_clicked_cc_lane", "color", "default_note_vel", "default_note_len",
}) do
  local ok, value = pcall(reaper.MIDIEditor_GetSetting_int, editor, key)
  output:write("setting_", key, "\t", tostring(ok), "\t", tostring(value), "\n")
end
for _, value in ipairs({0, 1, 15, 16, -1}) do
  local ok, result = pcall(reaper.MIDIEditor_SetSetting_int, editor, "channel_filter", value)
  local read_ok, current = pcall(reaper.MIDIEditor_GetSetting_int, editor, "channel_filter")
  output:write("channel_filter_set\t", value, "\t", tostring(ok), "\t", tostring(result),
    "\t", tostring(read_ok), "\t", tostring(current), "\n")
end
local selected_channel = tonumber(os.getenv("AAADAW_CHANNEL_FILTER_VALUE") or "1")
local set_ok, set_result = pcall(reaper.MIDIEditor_SetSetting_int, editor, "channel_filter", selected_channel)
local read_ok, selected_value = pcall(reaper.MIDIEditor_GetSetting_int, editor, "channel_filter")
output:write("selected_channel_filter\t", selected_channel, "\t", tostring(set_ok), "\t",
  tostring(set_result), "\t", tostring(read_ok), "\t", tostring(selected_value), "\n")
output:flush()
output:close()
reaper.UpdateArrange()
