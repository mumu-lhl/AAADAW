-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
-- Creates a four-note Event List between two project endpoints and watches the
-- project edit cursor and MIDI selection while the operator presses Home/End.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-event-list-home-end-navigation.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI Event List Home End", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 4, 8, false))
local take = assert(reaper.GetActiveTake(item))
for index, note in ipairs({
  {false, 60, 1}, {false, 64, 2}, {false, 67, 3}, {false, 72, 4},
}) do
  local start = index * 480
  assert(reaper.MIDI_InsertNote(take, note[1], false, start, start + 240,
    note[3], note[2], 90 + index, false))
end
reaper.MIDI_Sort(take)
local trailing_item = assert(reaper.CreateNewMIDIItemInProj(track, 10, 12, false))
reaper.SetMediaItemSelected(trailing_item, false)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(6, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateItemInProject(trailing_item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive(), "MIDI editor did not open")
assert(reaper.MIDIEditor_GetTake(editor) == take, "wrong MIDI take opened")
reaper.MIDIEditor_OnCommand(editor, 40056)
reaper.SetEditCurPos(6, false, false)

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("action\t40056\tMode: Event List\tAlt+3\n")
output:write("midi_item_start_seconds\t4\nmidi_item_end_seconds\t8\nproject_last_item_end_seconds\t12\n")
output:write("initial_edit_cursor_seconds\t", reaper.GetCursorPosition(), "\n")
output:write("initial\tnote_count\t4\tselection\t0,0,0,0\n")
output:flush()

local previous = nil
local started = reaper.time_precise()
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
  local cursor = reaper.GetCursorPosition()
  local state = string.format("%.3f\t%d\t%s", cursor, note_count, table.concat(selected, ","))
  if state ~= previous then
    output:write(string.format("state\t%.3f\t%s\n", reaper.time_precise() - started, state))
    output:flush()
    previous = state
  end
  reaper.defer(watch)
end
watch()
