-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
-- Creates a MIDI item with three notes and four CCs, then opens action 40056.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-event-filter-cc-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Filter Parameter Value", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
for _, note in ipairs({
  {0, 60, 91}, {480, 64, 92}, {960, 67, 93},
}) do
  assert(reaper.MIDI_InsertNote(take, false, false, note[1], note[1] + 240,
    0, note[2], note[3], false))
end
for _, cc in ipairs({
  {0, 7, 64, true}, {240, 7, 100, false},
  {480, 10, 64, false}, {720, 7, 127, false},
}) do
  assert(reaper.MIDI_InsertCC(take, cc[4], false, cc[1], 0xB0, 0, cc[2], cc[3]))
end
reaper.MIDI_Sort(take)
local _, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
assert(note_count == 3 and cc_count == 4 and text_count == 0)
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("counts\t", note_count, "\t", cc_count, "\t", text_count, "\n")
for index = 0, cc_count - 1 do
  local ok, selected, muted, tick, status, channel, number, value = reaper.MIDI_GetCC(take, index)
  assert(ok, "could not read CC " .. tostring(index))
  output:write(table.concat({"cc", index, selected and 1 or 0, tick, status,
    channel, number, value}, "\t"), "\n")
end
output:flush()
output:close()

reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
assert(reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 40056)
-- In Filter Events, enable Show-only; choose Control Change (CC); enable Control
-- and choose 07 Volume MSB; enable Value and enter 64 to 100. Keep it open for
-- the filtered Event List view, then save the project as the RPP readback.
