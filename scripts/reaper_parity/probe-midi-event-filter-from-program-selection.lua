-- Run in an isolated Linux REAPER 7.82 / Default 7 profile.
-- Set AAADAW_REAPER_PROBE_DIR to a writable directory first.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
for index = reaper.CountTracks(0) - 1, 0, -1 do
  reaper.DeleteTrack(assert(reaper.GetTrack(0, index)))
end
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Filter From Program Selection", true)
reaper.SetOnlyTrackSelected(track)

local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 240, 0, 60, 90, false))
local programs = {
  {480, 1, 8, true},
  {600, 1, 32, false},
  {960, 2, 64, true},
}
for _, event in ipairs(programs) do
  assert(reaper.MIDI_InsertCC(take, event[4], false, event[1], 0xC0, event[2], event[3], 0))
end
assert(reaper.MIDI_InsertCC(take, false, false, 720, 0xB0, 1, 7, 64))
reaper.MIDI_Sort(take)

local output = assert(io.open(root .. "midi-event-filter-from-program-native-readback.tsv", "w"))
local _, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
assert(note_count == 1 and cc_count == 4 and text_count == 0)
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("counts\t", note_count, "\t", cc_count, "\t", text_count, "\n")
for index = 0, cc_count - 1 do
  local ok, selected, muted, tick, status, channel, data1, data2 = reaper.MIDI_GetCC(take, index)
  assert(ok, "could not read MIDI channel event " .. tostring(index))
  output:write(table.concat({"cc", index, selected and 1 or 0, tick, status,
    channel, data1, data2}, "\t"), "\n")
end
output:close()
reaper.Main_SaveProjectEx(0, root .. "midi-event-filter-from-program-native-before.rpp", 0)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
assert(reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 40056)

local function save_after_set_when_requested()
  local request = io.open(root .. "save_after_set.request", "r")
  if not request then
    reaper.defer(save_after_set_when_requested)
    return
  end
  request:close()

  local active_editor = assert(reaper.MIDIEditor_GetActive())
  local active_take = assert(reaper.MIDIEditor_GetTake(active_editor))
  local after = assert(io.open(root .. "midi-event-filter-from-program-native-after-set-readback.tsv", "w"))
  local _, after_note_count, after_cc_count, after_text_count = reaper.MIDI_CountEvts(active_take)
  after:write("counts\t", after_note_count, "\t", after_cc_count, "\t", after_text_count, "\n")
  for index = 0, after_cc_count - 1 do
    local ok, selected, muted, tick, status, channel, data1, data2 = reaper.MIDI_GetCC(active_take, index)
    assert(ok, "could not read post-filter MIDI channel event " .. tostring(index))
    after:write(table.concat({"cc", index, selected and 1 or 0, tick, status,
      channel, data1, data2}, "\t"), "\n")
  end
  after:close()
  reaper.Main_SaveProjectEx(0, root .. "midi-event-filter-from-program-native-after-set.rpp", 0)
end

reaper.defer(save_after_set_when_requested)
