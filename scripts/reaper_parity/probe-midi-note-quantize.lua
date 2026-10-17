-- Run in an isolated Linux REAPER 7.82 / Default 7 project.
-- The script records native MIDI editor grid quantization for selected notes.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI note quantize", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
for _, note in ipairs({
  {true, 100, 220, 0, 60},
  {true, 350, 470, 2, 62},
  {false, 550, 670, 0, 64},
  {true, 900, 1020, 15, 65},
}) do
  assert(reaper.MIDI_InsertNote(take, note[1], false, note[2], note[3], note[4], note[5], 96, false))
end
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.Main_SaveProjectEx(0, root .. "midi-note-quantize-before.rpp", 0)
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
local grid_qn, swing = reaper.MIDI_GetGrid(take)
local project_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, 1) - reaper.MIDI_GetPPQPosFromProjQN(take, 0)
local output = assert(io.open(root .. "midi-note-quantize-native.tsv", "w"))
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("project_ppq\t", string.format("%.0f", project_ppq), "\n")
output:write("midi_grid_quarter_notes\t", string.format("%.6f", grid_qn), "\n")
output:write("midi_grid_ticks\t", string.format("%.0f", grid_qn * project_ppq), "\n")
output:write("swing\t", string.format("%.6f", swing), "\n")
local function write_notes(label)
  local _, note_count = reaper.MIDI_CountEvts(take)
  local values = {}
  for index = 0, note_count - 1 do
    local ok, selected, muted, start_ppq, end_ppq, channel, pitch, velocity = reaper.MIDI_GetNote(take, index)
    assert(ok, "could not read note " .. tostring(index))
    values[#values + 1] = table.concat({
      selected and 1 or 0,
      muted and 1 or 0,
      channel + 1,
      pitch,
      string.format("%.0f", start_ppq),
      string.format("%.0f", end_ppq),
      velocity,
    }, ",")
  end
  output:write(label, "\t", table.concat(values, ";"), "\n")
  output:flush()
end
write_notes("before_selected_muted_channel_pitch_start_end_velocity")
reaper.MIDIEditor_OnCommand(editor, 40469)
reaper.MIDI_Sort(take)
write_notes("after_selected_muted_channel_pitch_start_end_velocity")
output:write("action\t40469 Quantize notes position to grid\n")
output:flush()
output:close()
reaper.Main_SaveProjectEx(0, root .. "midi-note-quantize-after.rpp", 0)
reaper.UpdateArrange()
