-- Run in a fresh Linux REAPER 7.82 / Default 7 project with straight 1/32 grid.
-- Set AAADAW_REAPER_PROBE_DIR and AAADAW_REAPER_QUANTIZE_SCOPE=all|selected.
-- Select that scope in Quantize Events, press OK, then create the matching
-- midi-quantize-modeled-{scope}.done file to capture immediately.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
local scope = assert(os.getenv("AAADAW_REAPER_QUANTIZE_SCOPE"), "Set quantize scope")
assert(scope == "all" or scope == "selected", "scope must be all or selected")
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Modeled Quantize Probe", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 100, 280, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(take, true, false, 350, 430, 0, 62, 96, false))

-- Each event type has a selected off-grid event and an unselected event already
-- at its destination, plus an unrelated event to verify the event-type key.
local events = {
  {true, 475, 0xC0, 0, 12, 0},
  {false, 480, 0xC0, 0, 13, 0},
  {false, 730, 0xC0, 3, 14, 0},
  {true, 355, 0xA0, 0, 60, 80},
  {false, 360, 0xA0, 0, 60, 90},
  {false, 730, 0xA0, 5, 64, 70},
  {true, 475, 0xD0, 0, 60, 0},
  {false, 480, 0xD0, 0, 70, 0},
  {false, 730, 0xD0, 7, 75, 0},
}
for _, event in ipairs(events) do
  assert(reaper.MIDI_InsertCC(take, event[1], false, event[2], event[3],
    event[4], event[5], event[6]), "could not insert modeled MIDI event")
end
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
assert(reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 41003) -- Grid: set grid type to straight.
assert(math.abs(reaper.MIDI_GetGrid(take) - 0.125) < 0.000001,
  "set the MIDI editor grid to straight 1/32 before running the probe")

local output_path = root .. "midi-quantize-modeled-" .. scope .. "-native.tsv"
local output = assert(io.open(output_path, "w"))
local function write_state(label)
  local valid, note_count, cc_count = reaper.MIDI_CountEvts(take)
  assert(valid, "could not count MIDI events")
  local notes, channel_events = {}, {}
  for index = 0, note_count - 1 do
    local ok_note, selected, muted, start_ppq, end_ppq, channel, pitch =
      reaper.MIDI_GetNote(take, index)
    assert(ok_note, "could not read MIDI note")
    notes[#notes + 1] = table.concat({selected and 1 or 0, pitch,
      string.format("%.0f", start_ppq), string.format("%.0f", end_ppq)}, ",")
  end
  for index = 0, cc_count - 1 do
    local ok_event, selected, muted, ppq, status, channel, message2, message3 =
      reaper.MIDI_GetCC(take, index)
    assert(ok_event, "could not read modeled MIDI event")
    channel_events[#channel_events + 1] = table.concat({selected and 1 or 0,
      string.format("%.0f", ppq), string.format("%02X", status), channel + 1,
      message2, message3}, ",")
  end
  output:write(label, "\tnotes=", table.concat(notes, ";"),
    "\tchannel_events=", table.concat(channel_events, ";"), "\n")
  local raw_ok, raw = reaper.MIDI_GetAllEvts(take, "")
  assert(raw_ok, "could not read raw MIDI events")
  local offset, position = 1, 0
  while offset <= #raw do
    local delta, flags, message, next_offset = string.unpack("<i4Bs4", raw, offset)
    position = position + delta
    if #message > 0 then
      local hex = {}
      for byte = 1, #message do hex[#hex + 1] = string.format("%02X", message:byte(byte)) end
      output:write("raw\t", string.format("%.0f", position), "\t", flags,
        "\t", table.concat(hex, " "), "\n")
    end
    offset = next_offset
  end
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("scope_to_apply\t", scope, "\n")
output:write("grid_quarter_notes\t", string.format("%.3f", reaper.MIDI_GetGrid(take)), "\n")
write_state("before")
reaper.MIDIEditor_OnCommand(editor, 40009) -- Quantize Events.
output:flush()

local done_path = root .. "midi-quantize-modeled-" .. scope .. ".done"
local capture_delay = tonumber(os.getenv("AAADAW_REAPER_PROBE_DELAY_SECONDS") or "90") or 90
local started = reaper.time_precise()
local function finish()
  local done = io.open(done_path, "r")
  if done then done:close() end
  if not done and reaper.time_precise() - started < capture_delay then
    reaper.defer(finish)
    return
  end
  reaper.MIDI_Sort(take)
  write_state("after")
  reaper.Main_SaveProjectEx(0, root .. "midi-quantize-modeled-" .. scope .. "-native.rpp", 0)
  output:close()
end
reaper.defer(finish)
