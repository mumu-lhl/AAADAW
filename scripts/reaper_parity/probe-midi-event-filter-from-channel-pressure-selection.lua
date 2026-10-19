-- Run in an isolated Linux REAPER 7.82 / Default 7 profile.
-- Inspect the selected Channel Pressure rows, use Event List > Filter Events >
-- Set filter from selection, then create `capture-after-set` in the probe dir.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
for index = reaper.CountTracks(0) - 1, 0, -1 do
  reaper.DeleteTrack(assert(reaper.GetTrack(0, index)))
end
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Filter From Channel Pressure", true)
reaper.SetOnlyTrackSelected(track)

local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 240, 0, 60, 90, false))
local events = {
  {480, 0xD0, 1, 32, 0, true},
  {600, 0xD0, 1, 64, 0, false},
  {720, 0xB0, 1, 7, 70, false},
  {840, 0xA0, 2, 64, 50, false},
  {960, 0xD0, 2, 100, 0, true},
}
for _, event in ipairs(events) do
  assert(reaper.MIDI_InsertCC(take, event[6], false, event[1], event[2], event[3], event[4], event[5]))
end
reaper.MIDI_Sort(take)

local function write_readback(path)
  local output = assert(io.open(root .. path, "w"))
  local _, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
  output:write("version\t", reaper.GetAppVersion(), "\n")
  output:write("theme\tDefault_7.0.ReaperThemeZip\n")
  output:write("counts\t", note_count, "\t", cc_count, "\t", text_count, "\n")
  for index = 0, cc_count - 1 do
    local ok, selected, muted, tick, status, channel, data1, data2 = reaper.MIDI_GetCC(take, index)
    assert(ok, "could not read MIDI event " .. tostring(index))
    output:write(table.concat({"cc", index, selected and 1 or 0, muted and 1 or 0,
      tick, string.format("0x%02X", status), channel, data1, data2}, "\t"), "\n")
  end
  output:close()
end

write_readback("midi-event-filter-from-channel-pressure-before.tsv")
reaper.Main_SaveProjectEx(0, root .. "midi-event-filter-from-channel-pressure-before.rpp", 0)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
assert(reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 40056)

local function wait_for_capture()
  local request = io.open(root .. "capture-after-set", "r")
  if request then
    request:close()
    os.remove(root .. "capture-after-set")
    write_readback("midi-event-filter-from-channel-pressure-after-set.tsv")
    reaper.Main_SaveProjectEx(0, root .. "midi-event-filter-from-channel-pressure-after-set.rpp", 0)
    local ready = assert(io.open(root .. "capture-complete", "w"))
    ready:write("captured\n")
    ready:close()
    return
  end
  reaper.defer(wait_for_capture)
end
reaper.defer(wait_for_capture)
