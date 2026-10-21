-- Run only in an isolated, empty Linux REAPER 7.82 / Default 7 project.
-- AAADAW_REAPER_MIDI_FILE must select an SMPTE-timed Type 0 fixture.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local path = assert(os.getenv("AAADAW_REAPER_MIDI_FILE"))
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-smpte-native.tsv", "w"))

local inserted = reaper.InsertMedia(path, 0)
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("fixture\t", path, "\n")
output:write("insert_result\t", inserted, "\n")
output:write("project_tempo_marker_count\t", reaper.CountTempoTimeSigMarkers(0), "\n")
for track_index = 0, reaper.CountTracks(0) - 1 do
  local track = assert(reaper.GetTrack(0, track_index))
  local _, name = reaper.GetTrackName(track, "")
  local item_count = reaper.CountTrackMediaItems(track)
  output:write(table.concat({"track", track_index + 1, name, item_count}, "\t"), "\n")
  for item_index = 0, item_count - 1 do
    local item = assert(reaper.GetTrackMediaItem(track, item_index))
    local item_position = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
    local item_length = reaper.GetMediaItemInfo_Value(item, "D_LENGTH")
    local take = assert(reaper.GetActiveTake(item))
    output:write(table.concat({"item", track_index + 1, item_index + 1,
      item_position, item_length}, "\t"), "\n")
    for _, source_tick in ipairs({0, 250, 500, 750, 1000}) do
      output:write(table.concat({"ppq_to_project_time", track_index + 1, item_index + 1,
        source_tick, reaper.MIDI_GetProjTimeFromPPQPos(take, source_tick)}, "\t"), "\n")
    end
    for _, project_time in ipairs({0.25, 0.5, 0.75}) do
      output:write(table.concat({"project_time_to_ppq", track_index + 1, item_index + 1,
        project_time, reaper.MIDI_GetPPQPosFromProjTime(take, project_time)}, "\t"), "\n")
    end
    local _, note_count, _, _ = reaper.MIDI_CountEvts(take)
    for note_index = 0, note_count - 1 do
      local valid, _, _, start_tick, end_tick, channel, pitch, velocity =
        reaper.MIDI_GetNote(take, note_index)
      assert(valid, "could not read imported note")
      local start_seconds = reaper.MIDI_GetProjTimeFromPPQPos(take, start_tick)
      local end_seconds = reaper.MIDI_GetProjTimeFromPPQPos(take, end_tick)
      local start_project_qn = reaper.TimeMap2_timeToQN(0, start_seconds)
      local end_project_qn = reaper.TimeMap2_timeToQN(0, end_seconds)
      output:write(table.concat({"note", track_index + 1, note_index + 1,
        start_tick, end_tick, channel + 1, pitch, velocity, start_seconds,
        end_seconds, start_project_qn, end_project_qn}, "\t"), "\n")
    end
  end
end
output:flush()
output:close()
reaper.UpdateArrange()
