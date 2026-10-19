-- Run in an isolated Linux REAPER 7.82 / Default 7 profile.
-- Inspect Event List > View > Filter events, then click Set filter from selection.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
for index = reaper.CountTracks(0) - 1, 0, -1 do
  reaper.DeleteTrack(assert(reaper.GetTrack(0, index)))
end
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Filter From Pitch Selection", true)
reaper.SetOnlyTrackSelected(track)

local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 240, 0, 60, 90, false))
local bends = {
  {480, 1, 4096, true},
  {600, 1, 10000, false},
  {960, 2, 12288, true},
}
for _, bend in ipairs(bends) do
  local value = bend[3]
  assert(reaper.MIDI_InsertCC(take, bend[4], false, bend[1], 0xE0, bend[2], value & 0x7F, (value >> 7) & 0x7F))
end
assert(reaper.MIDI_InsertCC(take, false, false, 720, 0xB0, 1, 7, 64))
reaper.MIDI_Sort(take)

local output = assert(io.open(root .. "midi-event-filter-from-pitch-native-readback.tsv", "w"))
local _, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
assert(note_count == 1 and cc_count == 4 and text_count == 0)
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("counts\t", note_count, "\t", cc_count, "\t", text_count, "\n")
for index = 0, cc_count - 1 do
  local ok, selected, muted, tick, status, channel, data1, data2 = reaper.MIDI_GetCC(take, index)
  assert(ok, "could not read controller event " .. tostring(index))
  output:write(table.concat({"cc", index, selected and 1 or 0, tick, status,
    channel, data1, data2}, "\t"), "\n")
end
output:close()
reaper.Main_SaveProjectEx(0, root .. "midi-event-filter-from-pitch-native-before.rpp", 0)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
assert(reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 40056)
