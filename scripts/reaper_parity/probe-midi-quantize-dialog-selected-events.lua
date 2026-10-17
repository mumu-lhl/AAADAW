-- Run in Linux REAPER 7.82 with a fresh project and isolated Default 7 profile.
-- Select "Selected events" and press OK during the capture delay.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
for _, note in ipairs({{true, 170, 260, 60}, {false, 350, 430, 62}, {false, 550, 670, 64}}) do
  assert(reaper.MIDI_InsertNote(take, note[1], false, note[2], note[3], 0, note[4], 96, false))
end
-- The selected CC and pitch bend land on a tick already occupied by an unselected event.
for _, cc in ipairs({{true, 475, 1, 64}, {false, 480, 1, 65}, {false, 730, 7, 96}}) do
  assert(reaper.MIDI_InsertCC(take, cc[1], false, cc[2], 0xB0, 0, cc[3], cc[4]))
end
for _, bend in ipairs({{true, 835, 64, 62}, {false, 840, 0, 64}}) do
  assert(reaper.MIDI_InsertCC(take, bend[1], false, bend[2], 0xE0, 0, bend[3], bend[4]))
end
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
reaper.MIDIEditor_OnCommand(editor, 41003) -- Grid: Set grid type to straight
local grid_qn = reaper.MIDI_GetGrid(take)
assert(math.abs(grid_qn - 0.125) < 0.000001, "set the MIDI editor grid to 1/32")

local output = assert(io.open(root .. "midi-quantize-selected-events-native.tsv", "w"))
local function write_state(label)
  local _, note_count, cc_count = reaper.MIDI_CountEvts(take)
  local notes, ccs, bends = {}, {}, {}
  for index = 0, note_count - 1 do
    local ok, selected, _, start_ppq, end_ppq, channel, pitch = reaper.MIDI_GetNote(take, index)
    assert(ok)
    notes[#notes + 1] = table.concat({selected and 1 or 0, channel + 1, pitch,
      string.format("%.0f", start_ppq), string.format("%.0f", end_ppq)}, ",")
  end
  for index = 0, cc_count - 1 do
    local ok, selected, _, ppq, chanmsg, channel, msg2, msg3 = reaper.MIDI_GetCC(take, index)
    assert(ok)
    local event = table.concat({selected and 1 or 0, string.format("%.0f", ppq),
      string.format("%02X", chanmsg), channel + 1, msg2, msg3}, ",")
    if chanmsg == 0xE0 then
      bends[#bends + 1] = event
    else
      ccs[#ccs + 1] = event
    end
  end
  output:write(label, "\tnotes=", table.concat(notes, ";"),
    "\tcc=", table.concat(ccs, ";"), "\tpitch_bends=", table.concat(bends, ";"), "\n")
  output:flush()
end

write_state("before")
reaper.MIDIEditor_OnCommand(editor, 40009) -- Quantize Events dialog
local started = reaper.time_precise()
local delay = tonumber(os.getenv("AAADAW_REAPER_PROBE_DELAY_SECONDS") or "90") or 90
local function finish()
  if reaper.time_precise() - started < delay then
    reaper.defer(finish)
    return
  end
  reaper.MIDI_Sort(take)
  write_state("after")
  reaper.Main_SaveProjectEx(0, root .. "midi-quantize-selected-events-native.rpp", 0)
  output:close()
  reaper.defer(function() reaper.Main_OnCommand(40004, 0) end)
end
reaper.defer(finish)
