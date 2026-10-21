-- Run in a fresh Linux REAPER 7.82 / Default 7 project with the MIDI editor
-- grid set to straight 1/32 (the probe asserts the grid before opening Quantize).
-- Set AAADAW_REAPER_PROBE_DIR and AAADAW_REAPER_QUANTIZE_SCOPE=all|selected.
-- In the open Quantize Events dialog choose the requested scope and press OK,
-- then create midi-quantize-opaque-{scope}.done in the output directory to
-- capture immediately (or wait for AAADAW_REAPER_PROBE_DELAY_SECONDS).
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
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Opaque Quantize Probe", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 100, 280, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(take, true, false, 350, 430, 0, 62, 96, false))

local function insert_text(selected, ppq, event_type, message)
  assert(reaper.MIDI_InsertTextSysexEvt(take, selected, false, ppq, event_type, message),
    "could not insert opaque MIDI event")
end

-- The text Meta events collide at 480 in All events. In Selected events, the
-- first moves and the unselected second event stays at 484.
insert_text(true, 475, 1, "selected text at 475")
insert_text(false, 484, 1, "unselected text at 484")
insert_text(false, 715, 6, "unselected marker at 715")

-- The selected and unselected F0 starts collide at 600. Pass payload bytes to
-- REAPER's high-level API; it adds the F0 and F7 framing bytes itself.
insert_text(true, 595, -1, string.char(0x43, 0x12))
insert_text(false, 600, -1, string.char(0x55, 0x66))
reaper.MIDI_Sort(take)
local ok, raw_events = reaper.MIDI_GetAllEvts(take, "")
assert(ok, "could not read raw MIDI events")
local events, offset, position = {}, 1, 0
while offset <= #raw_events do
  local delta, flags, message, next_offset = string.unpack("<i4Bs4", raw_events, offset)
  position = position + delta
  events[#events + 1] = {position = position, flags = flags, message = message}
  offset = next_offset
end
-- Also attempt an unmatched standalone F7 before any F0 event. REAPER omits it
-- from the post-sort raw readback in this mixed-take probe, so it is not used
-- to infer F7 quantize behavior.
events[#events + 1] = {
  position = 50,
  flags = 0,
  message = string.char(0xF7, 0x34, 0xF7),
}
table.sort(events, function(left, right) return left.position < right.position end)
local rewritten, previous = {}, 0
for _, event in ipairs(events) do
  rewritten[#rewritten + 1] = string.pack("<i4Bs4", event.position - previous,
    event.flags, event.message)
  previous = event.position
end
assert(reaper.MIDI_SetAllEvts(take, table.concat(rewritten)), "could not add standalone F7 event")
reaper.MIDI_Sort(take)

reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
assert(reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 41003) -- Grid: set grid type to straight.
assert(math.abs(reaper.MIDI_GetGrid(take) - 0.125) < 0.000001,
  "set the MIDI editor grid to 1/32 before running the probe")

local output_path = root .. "midi-quantize-opaque-" .. scope .. "-native.tsv"
local output = assert(io.open(output_path, "w"))
local function hex(bytes)
  local result = {}
  for index = 1, #bytes do result[#result + 1] = string.format("%02X", bytes:byte(index)) end
  return table.concat(result, " ")
end
local function write_state(label)
  local valid, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
  assert(valid, "could not count MIDI events")
  local notes, opaque = {}, {}
  for index = 0, note_count - 1 do
    local ok_note, selected, muted, start_ppq, end_ppq, channel, pitch =
      reaper.MIDI_GetNote(take, index)
    assert(ok_note, "could not read MIDI note")
    notes[#notes + 1] = table.concat({selected and 1 or 0, pitch,
      string.format("%.0f", start_ppq), string.format("%.0f", end_ppq)}, ",")
  end
  for index = 0, text_count - 1 do
    local ok_text, selected, muted, ppq, event_type, message =
      reaper.MIDI_GetTextSysexEvt(take, index)
    assert(ok_text, "could not read opaque MIDI event")
    opaque[#opaque + 1] = table.concat({selected and 1 or 0,
      string.format("%.0f", ppq), event_type, hex(message)}, ",")
  end
  output:write(label, "\tnotes=", table.concat(notes, ";"),
    "\topaque=", table.concat(opaque, ";"), "\n")
  local raw_ok, current_events = reaper.MIDI_GetAllEvts(take, "")
  assert(raw_ok, "could not read raw MIDI events")
  local raw_offset, raw_position = 1, 0
  while raw_offset <= #current_events do
    local delta, flags, message, next_offset =
      string.unpack("<i4Bs4", current_events, raw_offset)
    raw_position = raw_position + delta
    if #message > 0 then
      output:write("raw\t", string.format("%.0f", raw_position), "\t", flags,
        "\t", hex(message), "\n")
    end
    raw_offset = next_offset
  end
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("scope_to_apply\t", scope, "\n")
output:write("grid_quarter_notes\t", string.format("%.3f", reaper.MIDI_GetGrid(take)), "\n")
output:write("standalone_f7_observation\traw F7 34 F7 attempted at tick 50; it is absent from MIDI_GetAllEvts after MIDI_SetAllEvts and MIDI_Sort, so it is not part of the quantize comparison\n")
write_state("before")
reaper.MIDIEditor_OnCommand(editor, 40009) -- Quantize Events.
output:write("dialog_opened_at\t", string.format("%.3f", reaper.time_precise()), "\n")
output:flush()

local done_path = root .. "midi-quantize-opaque-" .. scope .. ".done"
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
  reaper.Main_SaveProjectEx(0, root .. "midi-quantize-opaque-" .. scope .. "-native.rpp", 0)
  output:close()
  reaper.defer(function() reaper.Main_OnCommand(40004, 0) end)
end
reaper.defer(finish)
