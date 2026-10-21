-- Run from a fresh Linux REAPER 7.82 / Default 7 MIDI Event List probe.
-- The script records MIDI note-selection changes while the operator presses
-- Arrow Up/Down and Ctrl+Arrow Up/Down in the Event List.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
local editor = assert(reaper.MIDIEditor_GetActive(), "open the MIDI Event List first")
local take = assert(reaper.MIDIEditor_GetTake(editor), "active MIDI editor has no take")
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-event-list-navigation-selection.tsv", "w"))
local previous = nil
local started = reaper.time_precise()

output:write("elapsed_seconds\tselected_notes\n")

local function watch()
  if reaper.MIDIEditor_GetActive() ~= editor then
    output:close()
    return
  end
  local _, note_count = reaper.MIDI_CountEvts(take)
  local selected = {}
  for index = 0, note_count - 1 do
    local ok, is_selected = reaper.MIDI_GetNote(take, index)
    assert(ok, "could not read MIDI note " .. tostring(index))
    selected[#selected + 1] = is_selected and "1" or "0"
  end
  local state = table.concat(selected, ",")
  if state ~= previous then
    output:write(string.format("%.3f\t%s\n", reaper.time_precise() - started, state))
    output:flush()
    previous = state
  end
  reaper.defer(watch)
end

watch()
