-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated config.
-- Creates a 10-second looped MIDI item offset 4.75 seconds into a five-second
-- source, then opens the real MIDI editor for visual inspection.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-editor-loop-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI source loop", true)
reaper.SetOnlyTrackSelected(track)
local source_offset = tonumber(os.getenv("AAADAW_MIDI_SOURCE_OFFSET_SECONDS") or "4.75")
local item = assert(reaper.CreateNewMIDIItemInProj(track, 2, 7, false))
reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 1)
local take = assert(reaper.GetActiveTake(item))
reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", source_offset)
assert(reaper.MIDI_InsertNote(take, false, false, 960, 2880, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(take, false, false, 4800, 6720, 0, 64, 108, false))
reaper.MIDI_Sort(take)
assert(reaper.SetMediaItemLength(item, 10, true))
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(2, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()

local _, note_count = reaper.MIDI_CountEvts(take)
local notes = {}
for index = 0, note_count - 1 do
  local ok, _, _, start_ppq, end_ppq, _, pitch = reaper.MIDI_GetNote(take, index)
  assert(ok, "could not read MIDI note " .. tostring(index))
  notes[#notes + 1] = table.concat({pitch, start_ppq, end_ppq}, ",")
end
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
local source = reaper.GetMediaItemTake_Source(take)
local source_length, length_is_qn = reaper.GetMediaSourceLength(source)
output:write("item\tposition=", string.format("%.6f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
  "\tlength=", string.format("%.6f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
  "\tloop_source=", tostring(reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC")),
  "\tsource_offset=", string.format("%.6f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")), "\n")
output:write("source_loop\t", string.format("%.6f", source_length), " quarter notes\t5.000000 seconds at 120 BPM\tlength_is_qn=",
  tostring(length_is_qn), "\n")
output:write("source_notes\t", table.concat(notes, ";"), "\n")
output:write("expected_visible_occurrences\t",
  string.format("%.3fs,%.3fs,%.3fs,%.3fs",
    (0.5 - source_offset) % 5,
    (2.5 - source_offset) % 5,
    5 + (0.5 - source_offset) % 5,
    5 + (2.5 - source_offset) % 5), "\n")
output:flush()
reaper.Main_OnCommand(40153, 0)
local editor = reaper.MIDIEditor_GetActive()
if editor then
  for _ = 1, 5 do
    reaper.MIDIEditor_OnCommand(editor, 1011)
  end
  reaper.MIDI_RefreshEditors(take)
end
output = assert(io.open(root .. "midi-editor-loop-native.tsv", "a"))
output:write("editor_has_probe_take\t", tostring(editor and reaper.MIDIEditor_GetTake(editor) == take), "\n")
output:close()
reaper.UpdateArrange()
