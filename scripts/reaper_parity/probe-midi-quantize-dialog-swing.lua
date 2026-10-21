-- Run in a fresh REAPER 7.82 / Default 7 project.
-- Set the MIDI editor's swing strength to 50% before running; the script verifies it.
-- CASE=position and position_and_note_end run native actions 40469 and 40729.
-- CASE=all_events runs native action 40728 with all modeled MIDI events selected.
-- CASE=dialog opens Quantize Events; choose an operation/scope and press OK during the capture delay.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
local case = os.getenv("AAADAW_REAPER_PROBE_CASE") or "dialog"
local action_by_case = {
  position = 40469,
  position_and_note_end = 40729,
  all_events = 40728,
}
assert(case == "dialog" or action_by_case[case], "unknown quantize probe case")

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
for _, note in ipairs({
  {true, 170, 260, 60},
  {true, 350, 430, 62},
  {case == "all_events", 550, 670, 64},
}) do
  assert(reaper.MIDI_InsertNote(take, note[1], false, note[2], note[3], 0, note[4], 96, false))
end
for _, cc in ipairs({{475, 1, 64}, {485, 1, 65}, {730, 7, 96}}) do
  assert(reaper.MIDI_InsertCC(take, true, false, cc[1], 0xB0, 0, cc[2], cc[3]))
end
for _, bend in ipairs({{835, 0, 31, 62}, {850, 0, 0, 64}}) do
  assert(reaper.MIDI_InsertCC(take, true, false, bend[1], 0xE0, bend[2], bend[3], bend[4]))
end
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
reaper.MIDIEditor_OnCommand(editor, 41003) -- Grid: Set grid type to straight
reaper.MIDIEditor_OnCommand(editor, 41006) -- Grid: Set grid type to swing
local grid_qn, swing_amount = reaper.MIDI_GetGrid(take)
assert(math.abs(grid_qn - 0.125) < 0.000001, "set the MIDI editor grid to 1/32")
assert(math.abs(swing_amount - 0.5) < 0.000001, "set the MIDI editor swing strength to 50%")
local project_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, 1) - reaper.MIDI_GetPPQPosFromProjQN(take, 0)
local output = assert(io.open(root .. "midi-quantize-swing-" .. case .. ".tsv", "w"))
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("grid_qn\t", tostring(grid_qn), "\n")
output:write("project_ppq\t", string.format("%.0f", project_ppq), "\n")
output:write("swing_amount\t", tostring(swing_amount), "\n")

local function write_events(label)
  local _, note_count, cc_count = reaper.MIDI_CountEvts(take)
  local notes, ccs, bends = {}, {}, {}
  for index = 0, note_count - 1 do
    local ok, selected, muted, start_ppq, end_ppq, channel, pitch = reaper.MIDI_GetNote(take, index)
    assert(ok)
    notes[#notes + 1] = table.concat({selected and 1 or 0, channel + 1, pitch,
      string.format("%.0f", start_ppq), string.format("%.0f", end_ppq)}, ",")
  end
  for index = 0, cc_count - 1 do
    local ok, selected, muted, ppqpos, chanmsg, channel, msg2, msg3 = reaper.MIDI_GetCC(take, index)
    assert(ok)
    local event = table.concat({selected and 1 or 0, channel + 1, chanmsg,
      string.format("%.0f", ppqpos), msg2, msg3}, ",")
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

write_events("before")
if case == "dialog" then
  reaper.MIDIEditor_OnCommand(editor, 40009) -- Quantize Events; set options and press OK
else
  reaper.MIDIEditor_OnCommand(editor, action_by_case[case])
end
local started = reaper.time_precise()
local delay = tonumber(os.getenv("AAADAW_REAPER_PROBE_DELAY_SECONDS") or "60") or 60
local function finish()
  if case == "dialog" then
    if reaper.time_precise() - started < delay then
      reaper.defer(finish)
      return
    end
  end
  reaper.MIDI_Sort(take)
  write_events("after")
  reaper.Main_SaveProjectEx(0, root .. "midi-quantize-swing-" .. case .. ".rpp", 0)
  output:close()
  reaper.defer(function() reaper.Main_OnCommand(40004, 0) end)
end
reaper.defer(finish)
