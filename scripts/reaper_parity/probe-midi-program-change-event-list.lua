-- Run in an isolated Linux REAPER 7.82 / Default 7 profile.
-- Set AAADAW_REAPER_PROBE_DIR to a writable directory. The probe leaves the
-- MIDI Event List open; create `open-properties` there to raise the selected
-- Program Change properties dialog for a second screenshot.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-program-change-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Program Change probe", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 240, 0, 60, 96, false))
assert(reaper.MIDI_InsertCC(take, true, false, 480, 0xC0, 2, 41, 0))
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive(), "MIDI editor did not open")
assert(reaper.MIDIEditor_GetTake(editor) == take, "wrong MIDI take opened")
reaper.MIDIEditor_OnCommand(editor, 40056)

local _, note_count, cc_count = reaper.MIDI_CountEvts(take)
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("action\t40056\tMode: Event List\tAlt+3\n")
output:write("counts\t", note_count, "\t", cc_count, "\n")
for index = 0, cc_count - 1 do
  local ok, selected, muted, position, kind, channel, msg2, msg3 =
    reaper.MIDI_GetCC(take, index)
  assert(ok, "could not read MIDI event " .. tostring(index))
  output:write(table.concat({
    "event", index + 1, selected and 1 or 0, muted and 1 or 0, position,
    string.format("0x%02X", kind), channel + 1, msg2, msg3,
  }, "\t"), "\n")
end
output:flush()
output:close()

local function wait_for_properties_request()
  local request = io.open(root .. "open-properties", "r")
  if request then
    request:close()
    os.remove(root .. "open-properties")
    reaper.MIDIEditor_OnCommand(editor, 40004)
    local ready = assert(io.open(root .. "properties-opened", "w"))
    ready:write("command\t40004\tEdit: Event properties\n")
    ready:close()
    return
  end
  if reaper.MIDIEditor_GetActive() == editor then
    reaper.defer(wait_for_properties_request)
  end
end
reaper.defer(wait_for_properties_request)
