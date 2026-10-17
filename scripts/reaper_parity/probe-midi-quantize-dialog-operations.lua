local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
local strength_probe = os.getenv("AAADAW_REAPER_PROBE_CASE") == "strength50"

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
local notes = strength_probe and {
  {true, 170, 260, 60},
  {true, 350, 430, 62},
  {false, 550, 670, 67},
  {true, 900, 970, 64},
} or {
  {true, 100, 280, 60},
  {true, 350, 430, 62},
  {false, 550, 670, 67},
  {true, 900, 970, 64},
}
for _, note in ipairs(notes) do
  assert(reaper.MIDI_InsertNote(take, note[1], false, note[2], note[3], 0, note[4], 96, false))
end
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
local mode = assert(os.getenv("AAADAW_REAPER_OPERATION_NAME") or "operation")
local output = assert(io.open(root .. "midi-quantize-dialog-" .. mode .. ".tsv", "w"))
local capture_delay = tonumber(os.getenv("AAADAW_REAPER_PROBE_DELAY_SECONDS") or "60") or 60

local function write_notes(label)
  local _, count = reaper.MIDI_CountEvts(take)
  local values = {}
  for i = 0, count - 1 do
    local ok, selected, muted, start_ppq, end_ppq, channel, pitch, velocity = reaper.MIDI_GetNote(take, i)
    assert(ok)
    values[#values + 1] = table.concat({selected and 1 or 0, pitch, string.format("%.0f", start_ppq),
      string.format("%.0f", end_ppq), channel, velocity}, ",")
  end
  output:write(label, "\t", table.concat(values, ";"), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
write_notes("before_selected_pitch_start_end_channel_velocity")
reaper.MIDIEditor_OnCommand(editor, 40009)
output:write("dialog_command_returned\t", tostring(reaper.time_precise()), "\n")
output:flush()

-- Choose an operation/strength in Quantize Events and press OK before this timer expires.
local started = reaper.time_precise()
local function finish()
  if reaper.time_precise() - started < capture_delay then
    reaper.defer(finish)
    return
  end
  reaper.MIDI_Sort(take)
  write_notes("after_selected_pitch_start_end_channel_velocity")
  output:close()
  reaper.UpdateArrange()
end
reaper.defer(finish)
