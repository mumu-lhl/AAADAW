-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-import-same-tick-order-native.tsv", "w"))
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

local name = "Same Tick Order Probe"
local source_events = {
  {tick = 240, kind = "F0", bytes = "43 12", data = string.char(0xF0, 0x02, 0x43, 0x12)},
  {tick = 240, kind = "Text Meta", bytes = "x", data = string.char(0xFF, 0x01, 0x01) .. "x"},
  {tick = 240, kind = "Pitch Bend", bytes = "01 40", data = string.char(0xE0, 0x01, 0x40)},
  {tick = 240, kind = "CC7", bytes = "40", data = string.char(0xB0, 0x07, 0x40)},
  {tick = 240, kind = "Program Change", bytes = "05", data = string.char(0xC0, 0x05)},
  {tick = 240, kind = "Poly Aftertouch", bytes = "60 20", data = string.char(0xA0, 0x60, 0x20)},
  {tick = 240, kind = "Channel Pressure", bytes = "40", data = string.char(0xD0, 0x40)},
  {tick = 240, kind = "Note On", bytes = "60 64", data = string.char(0x90, 0x60, 0x64)},
  {tick = 240, kind = "CC16", bytes = "20", data = string.char(0xB0, 0x10, 0x20)},
  {tick = 480, kind = "Note On", bytes = "64 5A", data = string.char(0x90, 0x64, 0x5A)},
  {tick = 480, kind = "Note Off", bytes = "60 20", data = string.char(0x80, 0x60, 0x20)},
  {tick = 480, kind = "CC7", bytes = "50", data = string.char(0xB0, 0x07, 0x50)},
  {tick = 720, kind = "Note Off", bytes = "64 18", data = string.char(0x80, 0x64, 0x18)},
}

local track_data = string.char(0x00, 0xFF, 0x03) .. vlq(#name) .. name
local previous_tick = 0
for _, event in ipairs(source_events) do
  local delta = event.tick - previous_tick
  if event.kind == "F0" then
    track_data = track_data .. vlq(delta) .. event.data
  elseif event.kind == "Text Meta" then
    track_data = track_data .. vlq(delta) .. event.data
  else
    track_data = track_data .. vlq(delta) .. event.data
  end
  previous_tick = event.tick
end
track_data = track_data .. string.char(0x00, 0xFF, 0x2F, 0x00)
local midi_bytes = "MThd" .. string.pack(">I4I2I2I2", 6, 0, 1, 960)
  .. "MTrk" .. string.pack(">I4", #track_data) .. track_data
local midi_path = root .. "midi-import-same-tick-order.mid"
local midi_file = assert(io.open(midi_path, "wb"))
midi_file:write(midi_bytes)
midi_file:close()
assert(reaper.InsertMedia(midi_path, 0) == 1, "REAPER did not insert the same-tick fixture")

local track = assert(reaper.GetTrack(0, 0))
local item = assert(reaper.GetTrackMediaItem(track, 0))
local take = assert(reaper.GetActiveTake(item))
local _, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
local ok, all_events = reaper.MIDI_GetAllEvts(take, "")
assert(ok, "could not read native imported events")
local function hex(bytes)
  local values = {}
  for i = 1, #bytes do values[#values + 1] = string.format("%02X", bytes:byte(i)) end
  return table.concat(values, " ")
end
local offset, position, native_events = 1, 0, {}
while offset <= #all_events do
  local delta, flags, message, next_offset = string.unpack("<i4Bs4", all_events, offset)
  position = position + delta
  if #message > 0 then
    native_events[#native_events + 1] = string.format("%d:%s", position, hex(message))
  end
  offset = next_offset
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("division\t960 PPQ\n")
output:write("source_order\tF0; Text Meta; Pitch Bend; CC7; Program Change; Poly Aftertouch; Channel Pressure; Note On; CC16 at tick 240; Note On; Note Off; CC7 at tick 480; Note Off at tick 720\n")
output:write("counts\t", note_count, "\t", cc_count, "\t", text_count, "\n")
output:write("native_midi_get_all_evts\t", table.concat(native_events, "; "), "\n")
output:flush()

reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = reaper.MIDIEditor_GetActive()
assert(editor and reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 40056)
output:close()
