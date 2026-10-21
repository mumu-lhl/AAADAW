-- Run in a fresh REAPER 7.82 project with an empty arrange view.
-- Select the second and fourth notes below, then use Quantize Events:
-- Selected notes, Position and note length, straight 1/32, Strength 100%,
-- Fix overlaps unchecked. Press OK and touch the matching .done sentinel.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
local notes = {
  {false, 100, 300},
  {true, 180, 300},
  {false, 330, 480},
  {true, 400, 700},
  {false, 530, 650},
  {false, 620, 760},
  {false, 800, 880},
}
for _, note in ipairs(notes) do
  assert(reaper.MIDI_InsertNote(take, note[1], false, note[2], note[3], 0, 60, 96, false))
end

reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40153, 0) -- Open MIDI editor
local editor = assert(reaper.MIDIEditor_GetActive())
reaper.MIDIEditor_OnCommand(editor, 41003) -- Set grid type to straight
assert(math.abs(reaper.MIDI_GetGrid(take) - 0.125) < 0.000001, "expected straight 1/32 grid")

local output_path = root .. "midi-quantize-selected-multiple-overlap-chain-native.tsv"
local output = assert(io.open(output_path, "w"))
local function write_state(label)
  local valid, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
  assert(valid)
  local notes_out = {}
  for index = 0, note_count - 1 do
    local ok, selected, muted, start_ppq, end_ppq, channel, pitch = reaper.MIDI_GetNote(take, index)
    assert(ok)
    notes_out[#notes_out + 1] = table.concat({selected and 1 or 0, channel + 1, pitch,
      string.format("%.0f", start_ppq), string.format("%.0f", end_ppq)}, ",")
  end
  local raw_ok, raw = reaper.MIDI_GetAllEvts(take, "")
  assert(raw_ok)
  local events, offset, position = {}, 1, 0
  while offset <= #raw do
    local delta, flags, message, next_offset = string.unpack("<i4Bs4", raw, offset)
    position = position + delta
    if #message > 0 then
      local bytes = {}
      for byte = 1, #message do bytes[#bytes + 1] = string.format("%02X", message:byte(byte)) end
      events[#events + 1] = table.concat({string.format("%.0f", position), flags,
        table.concat(bytes, " ")}, ",")
    end
    offset = next_offset
  end
  output:write(label, "\tnotes=selected,channel,pitch,start,end:", table.concat(notes_out, ";"),
    "\traw=ppq,flags,msg:", table.concat(events, ";"), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("selection\tSelected notes: note 2 and note 4; same channel and pitch\n")
output:write("operation\tPosition and note length; Strength 100%; Fix overlaps unchecked\n")
output:write("grid_quarter_notes\t", string.format("%.3f", reaper.MIDI_GetGrid(take)), "\n")
write_state("before")
reaper.MIDIEditor_OnCommand(editor, 40009) -- Open Quantize Events

local done_path = root .. "midi-quantize-selected-multiple-overlap-chain.done"
local delay = tonumber(os.getenv("AAADAW_REAPER_PROBE_DELAY_SECONDS") or "120") or 120
local started = reaper.time_precise()
local function finish()
  local done = io.open(done_path, "r")
  if done then done:close() end
  if not done and reaper.time_precise() - started < delay then
    reaper.defer(finish)
    return
  end
  reaper.MIDI_Sort(take)
  write_state("after")
  reaper.Main_SaveProjectEx(0, root .. "midi-quantize-selected-multiple-overlap-chain.rpp", 0)
  output:close()
end
reaper.defer(finish)
