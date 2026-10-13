-- Run only in a disposable, unsaved REAPER 7.82 / Default 7 project while stopped.
-- The script inserts one track and opens its native name editor, then samples
-- the focused editor state while manual keyboard input is sent from the host.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0, "Stop playback before running this probe")

local output = assert(io.open("/tmp/aaadaw-input-focus-native.tsv", "w"))
local function clean(value)
  return tostring(value or ""):gsub("[\t\r\n]", " ")
end
local function track_name(track)
  if not track then return "<none>" end
  local _, name = reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "", false)
  return name
end
local function snapshot(label, elapsed)
  local track = reaper.GetSelectedTrack(0, 0)
  local time_start, time_end = reaper.GetSet_LoopTimeRange2(0, false, false, 0, 0, false)
  local loop_start, loop_end = reaper.GetSet_LoopTimeRange2(0, false, true, 0, 0, false)
  output:write(table.concat({
    label,
    string.format("%.3f", elapsed),
    tostring(reaper.CountTracks(0)),
    tostring(reaper.GetPlayStateEx(0)),
    clean(track_name(track)),
    string.format("%.6f", time_start),
    string.format("%.6f", time_end),
    string.format("%.6f", loop_start),
    string.format("%.6f", loop_end),
    tostring(reaper.GetProjectStateChangeCount(0)),
  }, "\t"), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("step\telapsed_seconds\ttrack_count\tplay_state\tselected_track_name\ttime_start_seconds\ttime_end_seconds\tloop_start_seconds\tloop_end_seconds\tproject_state_change_count\n")
local initial_tracks = reaper.CountTracks(0)
local initial_state = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40001, 0) -- Track: Insert new track
local inserted_track_count = reaper.CountTracks(0)
assert(inserted_track_count == initial_tracks + 1, "Could not insert the temporary test track")
reaper.Main_OnCommand(40696, 0) -- Track: Rename last touched track
snapshot("ready", 0)

local started = reaper.time_precise()
local last_sample = 0
local function sample()
  local elapsed = reaper.time_precise() - started
  if elapsed - last_sample >= 0.1 then
    snapshot("sample", elapsed)
    last_sample = elapsed
  end
  if elapsed < 25 then
    reaper.defer(sample)
  else
    output:write("baseline\t", initial_tracks, "\t", initial_state, "\n")
    output:close()
  end
end
reaper.defer(sample)
