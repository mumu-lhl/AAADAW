-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-sysex-f7-edge-cases-native.tsv", "w"))
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

local cases = {
  {
    name = "Unmatched F7",
    source = "F7@240:34 F7",
    events = vlq(240) .. string.char(0xF7) .. vlq(2) .. string.char(0x34, 0xF7),
  },
  {
    name = "Unterminated F0 at end of track",
    source = "F0@240:43 12",
    events = vlq(240) .. string.char(0xF0) .. vlq(2) .. string.char(0x43, 0x12),
  },
  {
    name = "Unterminated F7 continuation at end of track",
    source = "F0@240:43; F7@480:12",
    events = vlq(240) .. string.char(0xF0) .. vlq(1) .. string.char(0x43)
      .. vlq(240) .. string.char(0xF7) .. vlq(1) .. string.char(0x12),
  },
  {
    name = "Second F0 closes first",
    source = "F0@240:43; F0@480:12 F7",
    events = vlq(240) .. string.char(0xF0) .. vlq(1) .. string.char(0x43)
      .. vlq(240) .. string.char(0xF0) .. vlq(2) .. string.char(0x12, 0xF7),
  },
  {
    name = "F7 after Control Change",
    source = "F0@240:43 12; CC@360:B0 07 40; F7@480:34 F7",
    events = vlq(240) .. string.char(0xF0) .. vlq(2) .. string.char(0x43, 0x12)
      .. vlq(120) .. string.char(0xB0, 0x07, 0x40)
      .. vlq(120) .. string.char(0xF7) .. vlq(2) .. string.char(0x34, 0xF7),
  },
  {
    name = "F7 after text Meta",
    source = "F0@240:43 12; text Meta@360:x; F7@480:34 F7",
    events = vlq(240) .. string.char(0xF0) .. vlq(2) .. string.char(0x43, 0x12)
      .. vlq(120) .. string.char(0xFF, 0x01) .. vlq(1) .. "x"
      .. vlq(120) .. string.char(0xF7) .. vlq(2) .. string.char(0x34, 0xF7),
  },
  {
    name = "F7 after terminated F0",
    source = "F0@240:43 F7; F7@480:12",
    events = vlq(240) .. string.char(0xF0) .. vlq(2) .. string.char(0x43, 0xF7)
      .. vlq(240) .. string.char(0xF7) .. vlq(1) .. string.char(0x12),
  },
}

local function encode_file(case)
  local name = case.name
  local track_data = string.char(0x00, 0xFF, 0x03) .. vlq(#name) .. name
    .. case.events .. string.char(0x00, 0xFF, 0x2F, 0x00)
  return "MThd" .. string.pack(">I4I2I2I2", 6, 0, 1, 960)
    .. "MTrk" .. string.pack(">I4", #track_data) .. track_data
end

local function hex(bytes)
  local values = {}
  for i = 1, #bytes do values[#values + 1] = string.format("%02X", bytes:byte(i)) end
  return table.concat(values, " ")
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
for case_index, case in ipairs(cases) do
  local midi_path = root .. string.format("probe-midi-sysex-f7-edge-%02d.mid", case_index)
  local midi_filename = string.format("probe-midi-sysex-f7-edge-%02d.mid", case_index)
  local midi_file = assert(io.open(midi_path, "wb"))
  midi_file:write(encode_file(case))
  midi_file:close()
  assert(reaper.InsertMedia(midi_path, 1) == 1, "REAPER did not insert " .. case.name)

  local track_count = reaper.CountTracks(0)
  assert(track_count == case_index, "InsertMedia did not place each case on a separate track")
  local track = assert(reaper.GetTrack(0, case_index - 1))
  local item = assert(reaper.GetTrackMediaItem(track, 0))
  local take = assert(reaper.GetActiveTake(item))
  local _, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
  local _, all_events = reaper.MIDI_GetAllEvts(take, "")
  local offset, position = 1, 0
  local messages = {}
  while offset <= #all_events do
    local delta, flags, message, next_offset = string.unpack("<i4Bs4", all_events, offset)
    position = position + delta
    if #message > 0 then
      messages[#messages + 1] = string.format("%d:%s", position, hex(message))
    end
    offset = next_offset
  end
  output:write("case\t", case_index, "\t", case.name, "\n")
  output:write("fixture\t", midi_filename, "\n")
  output:write("source_packets\t", case.source, "\n")
  output:write("counts\t", note_count, "\t", cc_count, "\t", text_count, "\n")
  output:write("events\t", table.concat(messages, "; "), "\n")
end
output:flush()
output:close()
reaper.UpdateArrange()
