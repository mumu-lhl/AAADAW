-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local kind = os.getenv("AAADAW_REAPER_PROBE_KIND") or "note"
assert(kind == "note" or kind == "cc" or kind == "pitch", "unknown multi-selection type")
local output = assert(io.open(root .. "midi-event-properties-multiselect-" .. kind .. "-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI Multi Event Properties", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
if kind == "note" then
  assert(reaper.MIDI_InsertNote(take, true, false, 240, 360, 0, 60, 90, false))
  assert(reaper.MIDI_InsertNote(take, true, false, 480, 720, 2, 64, 100, false))
elseif kind == "cc" then
  assert(reaper.MIDI_InsertCC(take, true, false, 480, 0xB0, 2, 7, 90))
  assert(reaper.MIDI_InsertCC(take, true, false, 960, 0xB0, 5, 10, 100))
else
  assert(reaper.MIDI_InsertCC(take, true, false, 480, 0xE0, 2, 40, 70))
  assert(reaper.MIDI_InsertCC(take, true, false, 960, 0xE0, 5, 16, 78))
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
reaper.MIDIEditor_OnCommand(editor, 40004)

local _, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
if kind == "note" then
  assert(note_count == 2 and cc_count == 0 and text_count == 0)
else
  assert(note_count == 0 and cc_count == 2 and text_count == 0)
end
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("action\t40004\tEdit: Event properties\tCtrl+F2\n")
output:write("mode\t40056\tMode: Event List\tAlt+3\n")
if kind == "note" then
  for index = 0, note_count - 1 do
    local ok, selected, muted, start_tick, end_tick, channel, pitch, velocity =
      reaper.MIDI_GetNote(take, index)
    assert(ok and selected, "both probe notes must be selected")
    output:write(table.concat({"note", index + 1, selected and 1 or 0,
      muted and 1 or 0, start_tick, end_tick - start_tick, channel + 1, pitch, velocity}, "\t"), "\n")
  end
else
  for index = 0, cc_count - 1 do
    local ok, selected, muted, tick, chanmsg, channel, msg2, msg3 = reaper.MIDI_GetCC(take, index)
    assert(ok and selected and chanmsg == (kind == "cc" and 0xB0 or 0xE0),
      "both probe events must be selected and use the expected MIDI type")
    local event_name, value = "cc", msg3
    if kind == "pitch" then
      event_name, value = "pitch", msg2 + msg3 * 128
    end
    output:write(table.concat({event_name, index + 1, selected and 1 or 0,
      muted and 1 or 0, tick, chanmsg, channel + 1, msg2, value}, "\t"), "\n")
  end
end
output:flush()
output:close()
reaper.UpdateArrange()
