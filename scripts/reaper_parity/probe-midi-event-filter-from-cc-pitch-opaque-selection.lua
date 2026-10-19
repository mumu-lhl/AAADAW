-- Run in an isolated Linux REAPER 7.82 / Default 7 profile.
-- Select CC7, Pitch Bend, and Text Meta, use Event List > Filter Events >
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
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Filter From CC Pitch Opaque Selection", true)
reaper.SetOnlyTrackSelected(track)

local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 480, 720, 1, 60, 100, false))
assert(reaper.MIDI_InsertNote(take, false, false, 720, 960, 4, 67, 95, false))
assert(reaper.MIDI_InsertCC(take, true, false, 480, 0xB0, 1, 7, 64))
assert(reaper.MIDI_InsertCC(take, false, false, 720, 0xB0, 3, 10, 70))
assert(reaper.MIDI_InsertCC(take, true, false, 960, 0xE0, 2, 0, 96))
assert(reaper.MIDI_InsertTextSysexEvt(take, false, false, 720, 1, "Unselected Text Meta in range"))
assert(reaper.MIDI_InsertTextSysexEvt(take, true, false, 960, 1, "Selected Text Meta"))
assert(reaper.MIDI_InsertTextSysexEvt(
  take, false, false, 1500, -1, string.char(0xF0, 0x43, 0x12, 0xF7)))
reaper.MIDI_Sort(take)

local function write_readback(path)
  local output = assert(io.open(root .. path, "w"))
  local _, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
  output:write("version\t", reaper.GetAppVersion(), "\n")
  output:write("theme\tDefault_7.0.ReaperThemeZip\n")
  output:write("counts\t", note_count, "\t", cc_count, "\t", text_count, "\n")
  for index = 0, note_count - 1 do
    local ok, selected, muted, startppq, endppq, channel, pitch, velocity = reaper.MIDI_GetNote(take, index)
    assert(ok, "could not read MIDI note " .. tostring(index))
    output:write(table.concat({"note", index, selected and 1 or 0, muted and 1 or 0,
      startppq, endppq, channel, pitch, velocity}, "\t"), "\n")
  end
  for index = 0, cc_count - 1 do
    local ok, selected, muted, tick, status, channel, data1, data2 = reaper.MIDI_GetCC(take, index)
    assert(ok, "could not read MIDI control event " .. tostring(index))
    output:write(table.concat({"cc", index, selected and 1 or 0, muted and 1 or 0,
      tick, string.format("0x%02X", status), channel, data1, data2}, "\t"), "\n")
  end
  for index = 0, text_count - 1 do
    local ok, selected, muted, tick, event_type, message = reaper.MIDI_GetTextSysexEvt(take, index)
    if ok then
      output:write(table.concat({"text", index, selected and 1 or 0, muted and 1 or 0,
        tick, event_type, message}, "\t"), "\n")
    else
      output:write(table.concat({"text-read-failed", index}, "\t"), "\n")
    end
  end
  output:close()
end

local function write_raw_midi(path)
  local ok, blob = reaper.MIDI_GetAllEvts(take, "")
  assert(ok, "could not read raw MIDI source")
  local output = assert(io.open(root .. path, "wb"))
  output:write(blob)
  output:close()
end

write_readback("midi-event-filter-from-cc-pitch-opaque-selection-before.tsv")
write_raw_midi("midi-event-filter-from-cc-pitch-opaque-selection-before.bin")
reaper.Main_SaveProjectEx(0, root .. "midi-event-filter-from-cc-pitch-opaque-selection-before.rpp", 0)
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
    write_readback("midi-event-filter-from-cc-pitch-opaque-selection-after-set.tsv")
    write_raw_midi("midi-event-filter-from-cc-pitch-opaque-selection-after-set.bin")
    reaper.Main_SaveProjectEx(0, root .. "midi-event-filter-from-cc-pitch-opaque-selection-after-set.rpp", 0)
    local ready = assert(io.open(root .. "capture-complete", "w"))
    ready:write("captured\n")
    ready:close()
  end
  local disable_filter = io.open(root .. "capture-funnel-off", "r")
  if disable_filter then
    disable_filter:close()
    os.remove(root .. "capture-funnel-off")
    reaper.MIDIEditor_OnCommand(editor, 40504)
    reaper.UpdateArrange()
    reaper.Main_SaveProjectEx(0, root .. "midi-event-filter-from-cc-pitch-opaque-selection-funnel-off.rpp", 0)
    local ready = assert(io.open(root .. "capture-funnel-off-complete", "w"))
    ready:write("funnel disabled\n")
    ready:close()
    return
  end
  reaper.defer(wait_for_capture)
end
reaper.defer(wait_for_capture)
