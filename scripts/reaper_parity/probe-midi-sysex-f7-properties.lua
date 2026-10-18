-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-sysex-f7-properties-native.tsv", "w"))
local function vlq(value)
  local bytes = {value & 0x7F}
  value = value >> 7
  while value > 0 do
    table.insert(bytes, 1, (value & 0x7F) | 0x80)
    value = value >> 7
  end
  local encoded = {}
  for _, byte in ipairs(bytes) do encoded[#encoded + 1] = string.char(byte) end
  return table.concat(encoded)
end

-- REAPER's high-level SysEx insertion API creates F0 messages. Import a valid SMF
-- F0 start plus F7 continuation and record the native import result.
local start_payload = string.char(0x43, 0x12)
local continuation_payload = string.char(0x34, 0xF7)
local name = "MIDI SysEx F7 Probe"
local track_data = string.char(0x00, 0xFF, 0x03) .. vlq(#name) .. name
  .. vlq(240) .. string.char(0xF0) .. vlq(#start_payload) .. start_payload
  .. vlq(240) .. string.char(0xF7) .. vlq(#continuation_payload) .. continuation_payload
  .. string.char(0x00, 0xFF, 0x2F, 0x00)
local midi_bytes = "MThd" .. string.pack(">I4I2I2I2", 6, 0, 1, 960)
  .. "MTrk" .. string.pack(">I4", #track_data) .. track_data
local midi_path = root .. "probe-midi-sysex-f7.mid"
local midi_file = assert(io.open(midi_path, "wb"))
midi_file:write(midi_bytes)
midi_file:close()
assert(reaper.InsertMedia(midi_path, 0) == 1, "REAPER did not insert the F7 MIDI probe")

local track = assert(reaper.GetTrack(0, 0))
local item = assert(reaper.GetTrackMediaItem(track, 0))
local take = assert(reaper.GetActiveTake(item))
local function hex(bytes)
  local values = {}
  for i = 1, #bytes do values[#values + 1] = string.format("%02X", bytes:byte(i)) end
  return table.concat(values, " ")
end
local count_ok, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
assert(count_ok and text_count == 2, "expected a track name and one imported SysEx event")
local _, all_events = reaper.MIDI_GetAllEvts(take, "")
local offset, position, rewritten = 0, 0, {}
local native_sysex
while offset < #all_events do
  local delta, flags, message, next_offset = string.unpack("<i4Bs4", all_events, offset + 1)
  position = position + delta
  if #message > 0 and message:byte(1) == 0xF0 then
    assert(not native_sysex, "REAPER produced more than one SysEx event")
    native_sysex = {position = position, flags = flags, message = message}
    flags = flags | 1
  else
    flags = flags & 0xFE
  end
  rewritten[#rewritten + 1] = string.pack("<i4Bs4", delta, flags, message)
  offset = next_offset - 1
end
assert(native_sysex, "REAPER did not retain the imported SysEx message")
assert(reaper.MIDI_SetAllEvts(take, table.concat(rewritten)), "could not select F7 event")
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("counts\t", note_count, "\t", cc_count, "\t", text_count, "\n")
output:write("source_packets\tF0@240:43 12; F7@480:34 F7\n")
output:write("native_event_status\t", string.format("%02X", native_sysex.message:byte(1)), "\n")
output:write("native_event_raw_hex\t", hex(native_sysex.message), "\n")
output:write("native_event_ppqpos\t", native_sysex.position, "\n")
output:write("properties_action\t40004\n")
output:flush()

reaper.Main_OnCommand(40153, 0)
local editor = reaper.MIDIEditor_GetActive()
assert(editor and reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 40056)
reaper.MIDIEditor_OnCommand(editor, 40004)
output:close()
reaper.UpdateArrange()
