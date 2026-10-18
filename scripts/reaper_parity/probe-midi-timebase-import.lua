-- Run only in an isolated, empty Linux REAPER 7.82 / Default 7 project.
-- AAADAW_REAPER_MIDI_FILE selects a generated Format 2 or SMPTE fixture.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local path = assert(os.getenv("AAADAW_REAPER_MIDI_FILE"))
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-timebase-native.tsv", "w"))

local function write_take(take, track_index, item_index)
  local ok, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
  assert(ok, "could not count imported MIDI events")
  output:write(table.concat({"take", track_index + 1, item_index + 1,
    note_count, cc_count, text_count}, "\t"), "\n")
  for index = 0, note_count - 1 do
    local valid, selected, muted, start_tick, end_tick, channel, pitch, velocity, off_velocity =
      reaper.MIDI_GetNote(take, index)
    assert(valid, "could not read imported note")
    output:write(table.concat({"note", track_index + 1, index + 1,
      start_tick, end_tick, channel + 1, pitch, velocity, off_velocity}, "\t"), "\n")
  end
  local _, all_events = reaper.MIDI_GetAllEvts(take, "")
  local offset, position = 1, 0
  while offset <= #all_events do
    local delta, flags, message, next_offset = string.unpack("<i4Bs4", all_events, offset)
    position = position + delta
    if #message > 0 then
      local bytes = {}
      for index = 1, #message do
        bytes[#bytes + 1] = string.format("%02X", message:byte(index))
      end
      output:write(table.concat({"raw", track_index + 1, item_index + 1,
        position, flags, table.concat(bytes, " ")}, "\t"), "\n")
    end
    offset = next_offset
  end
end

local inserted = reaper.InsertMedia(path, 0)
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("fixture\t", path, "\n")
output:write("insert_result\t", inserted, "\n")
output:write("track_count\t", reaper.CountTracks(0), "\n")
output:write("tempo_marker_count\t", reaper.CountTempoTimeSigMarkers(0), "\n")
for track_index = 0, reaper.CountTracks(0) - 1 do
  local track = assert(reaper.GetTrack(0, track_index))
  local _, name = reaper.GetTrackName(track, "")
  local item_count = reaper.CountTrackMediaItems(track)
  output:write(table.concat({"track", track_index + 1, name, item_count}, "\t"), "\n")
  for item_index = 0, item_count - 1 do
    local item = assert(reaper.GetTrackMediaItem(track, item_index))
    local position = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
    local length = reaper.GetMediaItemInfo_Value(item, "D_LENGTH")
    local take = assert(reaper.GetActiveTake(item))
    output:write(table.concat({"item", track_index + 1, item_index + 1,
      position, length}, "\t"), "\n")
    if reaper.TakeIsMIDI(take) then
      write_take(take, track_index, item_index)
    end
  end
end
output:flush()
output:close()
reaper.UpdateArrange()
