-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
-- Creates one selected note whose Note Off velocity is nonzero, then opens Event List.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-note-off-velocity-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI Note Off Velocity", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))

-- REAPER's packed event format is int32 delta, flags byte, then length-prefixed message.
local raw = table.concat({
  string.pack("<i4Bs4", 0, 1, string.char(0x92, 60, 91)),
  string.pack("<i4Bs4", 240, 1, string.char(0x82, 60, 37)),
  string.pack("<i4Bs4", 0, 0, ""),
})
assert(reaper.MIDI_SetAllEvts(take, raw), "could not set raw note events")
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = reaper.MIDIEditor_GetActive()
assert(editor and reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 40056)

local ok, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
assert(ok and note_count == 1, "expected one note")
local note_ok, selected, muted, start, finish, channel, pitch, velocity = reaper.MIDI_GetNote(take, 0)
assert(note_ok and selected, "probe note was not selected")
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("action\t40056\tMode: Event List\tAlt+3\n")
output:write("counts\t", note_count, "\t", cc_count, "\t", text_count, "\n")
output:write(table.concat({"note", 1, selected and 1 or 0, muted and 1 or 0,
  start, finish - start, channel + 1, pitch, velocity}, "\t"), "\n")
local _, all_events = reaper.MIDI_GetAllEvts(take, "")
local offset, position, raw_note_on, raw_note_off = 0, 0, nil, nil
while offset < #all_events do
  local delta, flags, message, next_offset = string.unpack("<i4Bs4", all_events, offset + 1)
  position = position + delta
  if #message >= 3 then
    local status = message:byte(1)
    if status == 0x92 then raw_note_on = {position, message:byte(3)} end
    if status == 0x82 then raw_note_off = {position, message:byte(3)} end
  end
  offset = next_offset - 1
end
assert(raw_note_on and raw_note_off and raw_note_off[2] == 37,
  "raw MIDI readback did not preserve the nonzero Note Off velocity")
output:write("raw_note_on\t", raw_note_on[1], "\t", raw_note_on[2], "\n")
output:write("raw_note_off\t", raw_note_off[1], "\t", raw_note_off[2], "\n")
output:flush()
output:close()
