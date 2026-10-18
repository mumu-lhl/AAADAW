-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-import-same-tick-order-reversed-native.tsv", "w"))
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

local name = "Same Tick Order Reversed Probe"
local source_events = {
  {kind = "Note On", data = string.char(0x90, 0x60, 0x64)},
  {kind = "CC16", data = string.char(0xB0, 0x10, 0x20)},
  {kind = "Channel Pressure", data = string.char(0xD0, 0x40)},
  {kind = "Poly Aftertouch", data = string.char(0xA0, 0x60, 0x20)},
  {kind = "Program Change", data = string.char(0xC0, 0x05)},
  {kind = "CC7", data = string.char(0xB0, 0x07, 0x40)},
  {kind = "Pitch Bend", data = string.char(0xE0, 0x01, 0x40)},
  {kind = "Text Meta", data = string.char(0xFF, 0x01, 0x01) .. "x"},
  {kind = "F0", data = string.char(0xF0, 0x02, 0x43, 0x12)},
  {kind = "CC7 at 480", tick = 480, data = string.char(0xB0, 0x07, 0x50)},
  {kind = "Note Off", tick = 480, data = string.char(0x80, 0x60, 0x20)},
  {kind = "Note On", tick = 480, data = string.char(0x90, 0x64, 0x5A)},
  {kind = "Note Off", tick = 720, data = string.char(0x80, 0x64, 0x18)},
}

local track_data = string.char(0x00, 0xFF, 0x03) .. vlq(#name) .. name
local previous_tick = 0
for _, event in ipairs(source_events) do
  local tick = event.tick or 240
  track_data = track_data .. vlq(tick - previous_tick) .. event.data
  previous_tick = tick
end
track_data = track_data .. string.char(0x00, 0xFF, 0x2F, 0x00)
local midi_bytes = "MThd" .. string.pack(">I4I2I2I2", 6, 0, 1, 960)
  .. "MTrk" .. string.pack(">I4", #track_data) .. track_data
local midi_path = root .. "midi-import-same-tick-order-reversed.mid"
local midi_file = assert(io.open(midi_path, "wb"))
midi_file:write(midi_bytes)
midi_file:close()
assert(reaper.InsertMedia(midi_path, 0) == 1, "REAPER did not insert the reversed-order fixture")

local take = assert(reaper.GetActiveTake(reaper.GetTrackMediaItem(reaper.GetTrack(0, 0), 0)))
local ok, all_events = reaper.MIDI_GetAllEvts(take, "")
assert(ok, "could not read native imported events")
local offset, position, native_events = 1, 0, {}
while offset <= #all_events do
  local delta, flags, message, next_offset = string.unpack("<i4Bs4", all_events, offset)
  position = position + delta
  if #message > 0 then
    local bytes = {}
    for i = 1, #message do bytes[#bytes + 1] = string.format("%02X", message:byte(i)) end
    native_events[#native_events + 1] = string.format("%d:%s", position, table.concat(bytes, " "))
  end
  offset = next_offset
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("source_order\tNote On; CC16; Channel Pressure; Poly Aftertouch; Program Change; CC7; Pitch Bend; Text Meta; F0 at tick 240; CC7; Note Off; Note On at tick 480\n")
output:write("native_midi_get_all_evts\t", table.concat(native_events, "; "), "\n")
output:flush()
reaper.Main_OnCommand(40153, 0)
local editor = reaper.MIDIEditor_GetActive()
assert(editor and reaper.MIDIEditor_GetTake(editor) == take, "MIDI editor did not open")
reaper.MIDIEditor_OnCommand(editor, 40056)
output:close()
