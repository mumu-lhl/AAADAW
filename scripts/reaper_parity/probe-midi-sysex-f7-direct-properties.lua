-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-sysex-f7-direct-properties-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Standalone F7 Probe", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 2, false))
local take = assert(reaper.GetActiveTake(item))

-- Directly create an F7 raw MIDI event. File import drops unmatched F7 events,
-- so MIDI_SetAllEvts lets the native editor's standalone-event behavior be tested.
local message = string.char(0xF7, 0x34, 0xF7)
local raw_events = string.pack("<i4Bs4", 480, 1, message)
  .. string.pack("<i4Bs4", 480, 0, "")
assert(reaper.MIDI_SetAllEvts(take, raw_events), "could not write standalone F7 event")
reaper.MIDI_Sort(take)

local ok, all_events = reaper.MIDI_GetAllEvts(take, "")
assert(ok, "could not read standalone F7 event")
local offset, position, readback = 1, 0, {}
while offset <= #all_events do
  local delta, flags, bytes, next_offset = string.unpack("<i4Bs4", all_events, offset)
  position = position + delta
  if #bytes > 0 then
    local hex = {}
    for index = 1, #bytes do hex[#hex + 1] = string.format("%02X", bytes:byte(index)) end
    readback[#readback + 1] = string.format("%d:%d:%s", position, flags, table.concat(hex, " "))
  end
  offset = next_offset
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("source_event\tF7 34 F7 at tick 480\n")
output:write("native_event_readback\t", table.concat(readback, "; "), "\n")
output:write("properties_action\t40004\n")
output:flush()

reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = reaper.MIDIEditor_GetActive()
assert(editor and reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 40056)
reaper.MIDIEditor_OnCommand(editor, 40004)
output:close()
