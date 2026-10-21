local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
for _, note in ipairs({{true, 100, 280, 60}, {false, 350, 430, 62}}) do
  assert(reaper.MIDI_InsertNote(take, note[1], false, note[2], note[3], 0, note[4], 96, false))
end
assert(reaper.MIDI_InsertCC(take, false, false, 475, 0xB0, 0, 1, 64))
assert(reaper.MIDI_InsertCC(take, false, false, 730, 0xB0, 1, 7, 96))
assert(reaper.MIDI_InsertCC(take, false, false, 850, 0xE0, 0, 0, 64))
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
local output = assert(io.open(root .. "midi-quantize-bypass.tsv", "w"))
local capture_delay = tonumber(os.getenv("AAADAW_REAPER_PROBE_DELAY_SECONDS") or "60") or 60

local function write_state(label)
  local _, note_count, cc_count = reaper.MIDI_CountEvts(take)
  local notes, events = {}, {}
  for i = 0, note_count - 1 do
    local ok, selected, muted, start_ppq, end_ppq, channel, pitch = reaper.MIDI_GetNote(take, i)
    assert(ok)
    notes[#notes + 1] = table.concat({selected and 1 or 0, pitch, string.format("%.0f", start_ppq),
      string.format("%.0f", end_ppq)}, ",")
  end
  for i = 0, cc_count - 1 do
    local ok, selected, muted, ppq, chanmsg, channel, msg2, msg3 = reaper.MIDI_GetCC(take, i)
    assert(ok)
    events[#events + 1] = table.concat({selected and 1 or 0, string.format("%.0f", ppq),
      string.format("%02X", chanmsg), channel, msg2, msg3}, ",")
  end
  output:write(label, "\tnotes=", table.concat(notes, ";"), "\tevents=", table.concat(events, ";"), "\n")
  output:flush()
end

write_state("before")
reaper.MIDIEditor_OnCommand(editor, 40009)
local started = reaper.time_precise()
local function finish()
  if reaper.time_precise() - started < capture_delay then
    reaper.defer(finish)
    return
  end
  reaper.MIDI_Sort(take)
  write_state("after")
  output:close()
end
reaper.defer(finish)
