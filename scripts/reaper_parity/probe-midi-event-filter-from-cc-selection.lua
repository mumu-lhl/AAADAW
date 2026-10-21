-- Run in a disposable Linux REAPER 7.82 / Default 7 project with an isolated profile.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
for index = reaper.CountTracks(0) - 1, 0, -1 do
  reaper.DeleteTrack(assert(reaper.GetTrack(0, index)))
end
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Filter From CC Selection", true)
reaper.SetOnlyTrackSelected(track)

local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 240, 0, 60, 90, false))
local controllers = {
  {480, 7, 64, 1, true},
  {720, 10, 80, 1, false},
  {960, 7, 100, 2, true},
  {1440, 7, 127, 1, false},
}
for _, cc in ipairs(controllers) do
  assert(reaper.MIDI_InsertCC(take, cc[5], false, cc[1], 0xB0, cc[4], cc[2], cc[3]))
end
reaper.MIDI_Sort(take)

local output = assert(io.open(root .. "midi-event-filter-from-cc-native-readback.tsv", "w"))
local _, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
assert(note_count == 1 and cc_count == 4 and text_count == 0)
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("counts\t", note_count, "\t", cc_count, "\t", text_count, "\n")
for index = 0, cc_count - 1 do
  local ok, selected, muted, tick, status, channel, number, value = reaper.MIDI_GetCC(take, index)
  assert(ok, "could not read CC " .. tostring(index))
  output:write(table.concat({"cc", index, selected and 1 or 0, tick, status,
    channel, number, value}, "\t"), "\n")
end
output:close()

reaper.Main_SaveProjectEx(0, root .. "midi-event-filter-from-cc-native.rpp", 0)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
assert(reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 40056)
-- Open Filter Events and press Set filter from selection. Save the project after the filter is derived.
