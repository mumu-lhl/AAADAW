-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
-- Measures selected-note semitone/octave actions and their range behavior.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-note-transpose-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI note transpose", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 8, false))
local take = assert(reaper.GetActiveTake(item))
for index, note in ipairs({
  {true, 60, 0}, {true, 61, 1}, {true, 62, 2},
  {false, 0, 0}, {false, 127, 15},
}) do
  local start = (index - 1) * 480
  assert(reaper.MIDI_InsertNote(take, note[1], false, start, start + 240,
    note[3], note[2], 90 + index, false))
end
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = reaper.MIDIEditor_GetActive()
assert(editor and reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")

local function write_events(label)
  local _, note_count = reaper.MIDI_CountEvts(take)
  local rows = {}
  for index = 0, note_count - 1 do
    local ok, selected, muted, start, finish, channel, pitch, velocity = reaper.MIDI_GetNote(take, index)
    assert(ok, "could not read MIDI note " .. tostring(index))
    rows[#rows + 1] = table.concat({selected and 1 or 0, muted and 1 or 0,
      start, finish, channel + 1, pitch, velocity}, ",")
  end
  output:write(label, "\t", table.concat(rows, ";"), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
write_events("before")
local actions = {
  {"up_semitone", 40177}, {"down_semitone", 40178},
  {"up_octave", 40179}, {"down_octave", 40180},
  {"up_semitone_ignore_scale", 41026}, {"down_semitone_ignore_scale", 41027},
  {"overflow_up", 40177, {true, false, true, false, true}},
  {"top_edge_up", 40177, {false, false, false, false, true}},
  {"bottom_edge_down", 40178, {false, false, false, true, false}},
}
local started_at = reaper.time_precise()
local action_index = 1
local awaiting_result = false
local function measure_next_action()
  -- Let first-run dialogs and REAPER's UI command queue settle before reading.
  if reaper.time_precise() - started_at < 5 then
    reaper.defer(measure_next_action)
    return
  end
  if action_index > #actions then
    output:close()
    reaper.UpdateArrange()
    return
  end
  local action = actions[action_index]
  if awaiting_result then
    write_events(action[1])
    action_index = action_index + 1
    awaiting_result = false
  else
    if action[3] then
      local _, note_count = reaper.MIDI_CountEvts(take)
      for index = 0, note_count - 1 do
        local ok, _, muted, start, finish, channel, pitch, velocity = reaper.MIDI_GetNote(take, index)
        assert(ok)
        assert(reaper.MIDI_SetNote(take, index, action[3][index + 1], muted,
          start, finish, channel, pitch, velocity, true))
      end
    end
    reaper.MIDIEditor_OnCommand(editor, action[2])
    awaiting_result = true
  end
  reaper.defer(measure_next_action)
end
reaper.defer(measure_next_action)
