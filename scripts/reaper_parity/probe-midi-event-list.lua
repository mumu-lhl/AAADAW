-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
-- Creates a small MIDI source, records its event data, then opens action 40056.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-event-list-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI Event List", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
for index, note in ipairs({
  {true, 60, 1}, {true, 64, 2}, {true, 67, 3}, {false, 72, 4},
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
reaper.MIDIEditor_OnCommand(editor, 40056)

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("action\t40056\tMode: Event List\tAlt+3\n")
local _, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
output:write("counts\t", note_count, "\t", cc_count, "\t", text_count, "\n")
for index = 0, note_count - 1 do
  local ok, selected, muted, start, finish, channel, pitch, velocity = reaper.MIDI_GetNote(take, index)
  assert(ok, "could not read MIDI note " .. tostring(index))
  output:write(table.concat({"note", index + 1, selected and 1 or 0, muted and 1 or 0,
    start, finish - start, channel + 1, pitch, velocity}, "\t"), "\n")
end
output:flush()
output:close()
reaper.UpdateArrange()
