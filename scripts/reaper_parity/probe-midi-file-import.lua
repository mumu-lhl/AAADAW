-- Run in a fresh Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_REAPER_MIDI_FILE to the Type 1 fixture and
-- AAADAW_REAPER_PROBE_DIR to a writable output directory.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local midi_path = assert(os.getenv("AAADAW_REAPER_MIDI_FILE"))
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-file-import-native.tsv", "w"))

local function hex(bytes)
  local result = {}
  for index = 1, #bytes do
    result[#result + 1] = string.format("%02X", string.byte(bytes, index))
  end
  return table.concat(result)
end

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
      selected and 1 or 0, muted and 1 or 0, start_tick, end_tick,
      channel + 1, pitch, velocity, off_velocity}, "\t"), "\n")
  end
  for index = 0, cc_count - 1 do
    local valid, selected, muted, tick, status, channel, message2, message3 =
      reaper.MIDI_GetCC(take, index)
    assert(valid, "could not read imported channel event")
    output:write(table.concat({"cc", track_index + 1, index + 1,
      selected and 1 or 0, muted and 1 or 0, tick,
      string.format("0x%02X", status), channel + 1, message2, message3}, "\t"), "\n")
  end
  for index = 0, text_count - 1 do
    local valid, selected, muted, tick, event_type, message =
      reaper.MIDI_GetTextSysexEvt(take, index)
    assert(valid, "could not read imported text or SysEx event")
    output:write(table.concat({"text_sysex", track_index + 1, index + 1,
      selected and 1 or 0, muted and 1 or 0, tick, event_type,
      hex(message)}, "\t"), "\n")
  end
  local _, all_events = reaper.MIDI_GetAllEvts(take, "")
  local offset, position = 0, 0
  while offset < #all_events do
    local delta, flags, message, next_offset =
      string.unpack("<i4Bs4", all_events, offset + 1)
    position = position + delta
    output:write(table.concat({"raw", track_index + 1, item_index + 1,
      position, flags, hex(message)}, "\t"), "\n")
    offset = next_offset - 1
  end
end

reaper.SetEditCurPos(2.0, false, false)
assert(reaper.InsertMedia(midi_path, 0) == 1, "REAPER did not insert the MIDI file")
reaper.UpdateArrange()

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("cursor_seconds\t2.0\n")
output:write("track_count\t", reaper.CountTracks(0), "\n")
output:write("tempo_marker_count\t", reaper.CountTempoTimeSigMarkers(0), "\n")
local _, marker_count, region_count = reaper.CountProjectMarkers(0)
output:write("project_marker_counts\t", marker_count, "\t", region_count, "\n")
for index = 0, marker_count + region_count - 1 do
  local valid, is_region, position, region_end, name, marker_index =
    reaper.EnumProjectMarkers3(0, index)
  if valid ~= 0 then
    output:write(table.concat({"project_marker", marker_index,
      is_region and 1 or 0, position, region_end, name}, "\t"), "\n")
  end
end
for index = 0, reaper.CountTempoTimeSigMarkers(0) - 1 do
  local valid, time_position, measure_position, beat_position, bpm, numerator, denominator, linear =
    reaper.GetTempoTimeSigMarker(0, index)
  assert(valid, "could not read project tempo marker")
  output:write(table.concat({"tempo", index + 1, time_position,
    measure_position, beat_position, bpm, numerator, denominator,
    linear and 1 or 0}, "\t"), "\n")
end

local selected_take
for track_index = 0, reaper.CountTracks(0) - 1 do
  local track = assert(reaper.GetTrack(0, track_index))
  local _, track_name = reaper.GetTrackName(track, "")
  local item_count = reaper.CountTrackMediaItems(track)
  output:write(table.concat({"track", track_index + 1, track_name, item_count}, "\t"), "\n")
  for item_index = 0, item_count - 1 do
    local item = assert(reaper.GetTrackMediaItem(track, item_index))
    local position = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
    local length = reaper.GetMediaItemInfo_Value(item, "D_LENGTH")
    local take = reaper.GetActiveTake(item)
    local _, take_name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
    output:write(table.concat({"item", track_index + 1, item_index + 1,
      position, length, take_name or ""}, "\t"), "\n")
    if take and reaper.TakeIsMIDI(take) then
      write_take(take, track_index, item_index)
      local _, notes, ccs = reaper.MIDI_CountEvts(take)
      if not selected_take and (notes > 0 or ccs > 0) then
        selected_take = take
        reaper.SetOnlyTrackSelected(track)
        reaper.SetMediaItemSelected(item, true)
      end
    end
  end
end
output:flush()
output:close()

if selected_take then
  reaper.Main_OnCommand(40153, 0)
  local editor = reaper.MIDIEditor_GetActive()
  assert(editor and reaper.MIDIEditor_GetTake(editor) == selected_take,
    "could not open an imported MIDI take")
  reaper.MIDIEditor_OnCommand(editor, 40056)
end
reaper.UpdateArrange()
