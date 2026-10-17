-- Run in Linux REAPER 7.82 with the Default 7 theme in a fresh, unsaved project.
-- This script records the project data before opening the native Humanize dialog. Set the
-- requested values, click OK, and it records the changed MIDI notes after the modal closes.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI Humanize", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
for _, note in ipairs({
  {true, 170, 260, 0, 60, 70},
  {true, 350, 460, 0, 64, 80},
  {true, 550, 700, 0, 67, 90},
  {false, 730, 920, 0, 69, 100},
}) do
  assert(reaper.MIDI_InsertNote(take, note[1], false, note[2], note[3], note[4], note[5], note[6], false))
end
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
local grid_qn, swing = reaper.MIDI_GetGrid(take)
local project_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, 1) - reaper.MIDI_GetPPQPosFromProjQN(take, 0)
local output = assert(io.open(root .. "midi-humanize-native.tsv", "w"))
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
    assert(ok, "could not read MIDI note " .. tostring(index))
    values[#values + 1] = table.concat({selected and 1 or 0, channel + 1, pitch,
      string.format("%.0f", start_ppq), string.format("%.0f", end_ppq), velocity}, ",")
  end
  output:write(label, "\tnotes=selected,channel,pitch,start,end,velocity:", table.concat(values, ";"), "\n")
  output:flush()
end

write_notes("before")
reaper.Main_SaveProjectEx(0, root .. "midi-humanize-before.rpp", 0)
reaper.MIDIEditor_OnCommand(editor, 40457) -- MIDI editor: Humanize notes
local started = reaper.time_precise()
local delay = tonumber(os.getenv("AAADAW_REAPER_PROBE_DELAY_SECONDS") or "120") or 120
local function finish()
  if reaper.time_precise() - started < delay then
    reaper.defer(finish)
    return
  end
  write_notes("after")
  reaper.Main_SaveProjectEx(0, root .. "midi-humanize-after.rpp", 0)
  output:close()
end
reaper.defer(finish)
