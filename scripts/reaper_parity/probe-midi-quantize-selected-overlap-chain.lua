local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local selected_note_index = tonumber(os.getenv("AAADAW_REAPER_PROBE_SELECTED_NOTE_INDEX") or "2")
assert(selected_note_index == 2, "this probe measures the second selected note in its chain")
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
local notes = {
  {100, 300},
  {180, 300},
  {330, 480},
  {400, 520},
  {530, 650},
}
for index, note in ipairs(notes) do
  assert(reaper.MIDI_InsertNote(take, index == selected_note_index, false,
    note[1], note[2], 0, 60, 96, false))
end

reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40153, 0) -- Open MIDI editor
local editor = assert(reaper.MIDIEditor_GetActive())
reaper.MIDIEditor_OnCommand(editor, 41003) -- Set grid type to straight
assert(math.abs(reaper.MIDI_GetGrid(take) - 0.125) < 0.000001, "expected 1/32 grid")

local output = assert(io.open(output_dir .. "midi-quantize-selected-overlap-chain-native.tsv", "w"))
local function write_state(label)
  local _, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
  local notes_out = {}
  for index = 0, note_count - 1 do
    local ok, selected, muted, start_ppq, end_ppq, channel, pitch = reaper.MIDI_GetNote(take, index)
    assert(ok)
    notes_out[#notes_out + 1] = table.concat({selected and 1 or 0, channel + 1, pitch,
      string.format("%.0f", start_ppq), string.format("%.0f", end_ppq)}, ",")
  end
  local raw_events = {}
  local event_count = note_count * 2 + cc_count + text_count
  for index = 0, event_count - 1 do
    local ok, selected, muted, ppq, message = reaper.MIDI_GetEvt(take, index)
    assert(ok)
    local message_hex = {}
    for byte = 1, #message do
      message_hex[#message_hex + 1] = string.format("%02X", string.byte(message, byte))
    end
    raw_events[#raw_events + 1] = table.concat({index, selected and 1 or 0,
      muted and 1 or 0, string.format("%.0f", ppq), table.concat(message_hex)}, ",")
  end
  output:write(label, "\tnotes=selected,channel,pitch,start,end:", table.concat(notes_out, ";"),
    "\tMIDI_events=index,selected,muted,ppq,msg:", table.concat(raw_events, ";"), "\n")
  output:flush()
end

write_state("before")
reaper.MIDIEditor_OnCommand(editor, 40009) -- Open Quantize Events
local started = reaper.time_precise()
local delay = tonumber(os.getenv("AAADAW_REAPER_PROBE_DELAY_SECONDS") or "120") or 120
local function finish()
  if reaper.time_precise() - started < delay then
    reaper.defer(finish)
    return
  end
  write_state("after")
  reaper.Main_SaveProjectEx(0, output_dir .. "midi-quantize-selected-overlap-chain.rpp", 0)
  output:close()
end
reaper.defer(finish)
