local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local capture_case = assert(os.getenv("AAADAW_REAPER_PROBE_CASE"),
  "set AAADAW_REAPER_PROBE_CASE to checked or unchecked")
assert(capture_case == "checked" or capture_case == "unchecked")
local selected_indexes_env = os.getenv("AAADAW_REAPER_PROBE_SELECTED_NOTE_INDEXES")
local include_third_note = os.getenv("AAADAW_REAPER_PROBE_INCLUDE_THIRD_NOTE") ~= "false"
local selected_indexes = {}
if selected_indexes_env then
  for index in string.gmatch(selected_indexes_env, "[^,]+") do
    selected_indexes[tonumber(index)] = true
  end
end
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
-- Deliberately overlapping same-pitch notes exercise Fix overlaps. Do not call
-- MIDI_Sort here: it resolves those overlaps before the dialog can see them.
local probe_notes = {{100, 300}, {180, 350}, {330, 480}}
if not include_third_note then
  probe_notes[3] = nil
end
for index, note in ipairs(probe_notes) do
  local selected = selected_indexes_env == nil or selected_indexes[index] == true
  assert(reaper.MIDI_InsertNote(take, selected, false, note[1], note[2], 0, 60, 96, false))
end
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
local output = assert(io.open(root .. "midi-quantize-fix-overlaps-" .. capture_case .. ".tsv", "w"))
local capture_delay = tonumber(os.getenv("AAADAW_REAPER_PROBE_DELAY_SECONDS") or "60") or 60

local function write_state(label)
  local _, note_count = reaper.MIDI_CountEvts(take)
  local notes = {}
  for i = 0, note_count - 1 do
    local ok, selected, muted, start_ppq, end_ppq, channel, pitch = reaper.MIDI_GetNote(take, i)
    assert(ok)
    notes[#notes + 1] = table.concat({selected and 1 or 0, channel, pitch,
      string.format("%.0f", start_ppq), string.format("%.0f", end_ppq)}, ",")
  end
  output:write(label, "\tnotes=", table.concat(notes, ";"), "\n")
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
  write_state("after")
  output:close()
end
reaper.defer(finish)
