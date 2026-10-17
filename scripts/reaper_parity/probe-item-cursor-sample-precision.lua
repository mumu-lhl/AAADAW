-- Run in a fresh, stopped REAPER 7.82 / Default 7 project; audio playback is not needed.
-- Set AAADAW_REAPER_CURSOR_WAV to a 48 kHz WAV and AAADAW_REAPER_PROBE_DIR to an
-- output directory. The script creates one selected Audio item 12 samples after
-- 4 seconds, runs Main 41167/41168/41173/41174 with snapping on and off, and
-- records cursor time at full ReaScript precision without saving the project.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)

local sample_rate = 48000
local start_sample = 192012
local end_sample = start_sample + sample_rate
local start_seconds = start_sample / sample_rate
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "AAADAW_REAPER_PROBE_DIR is required")
local wave_path = assert(os.getenv("AAADAW_REAPER_CURSOR_WAV"), "AAADAW_REAPER_CURSOR_WAV is required")
local track = reaper.InsertTrackAtIndex(0, true)
track = reaper.GetTrack(0, 0)
assert(track, "could not create the Audio item track")

local item = reaper.AddMediaItemToTrack(track)
assert(item, "could not create the Audio item")
local take = reaper.AddTakeToMediaItem(item)
assert(take, "could not create the Audio item take")
local source = reaper.PCM_Source_CreateFromFile(wave_path)
assert(source, "could not open the PCM WAV source")
reaper.SetMediaItemTake_Source(take, source)
reaper.SetMediaItemInfo_Value(item, "D_POSITION", start_seconds)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1.0)
reaper.SetMediaItemSelected(item, true)
assert(reaper.CountSelectedMediaItems(0) == 1)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 6.0)
reaper.UpdateArrange()

local output_path = output_dir .. "/cursor-positions.tsv"
local out = assert(io.open(output_path, "w"))
out:write("version\t", reaper.GetAppVersion(), "\n")
out:write("theme\tDefault_7.0.ReaperThemeZip\n")
out:write("audio_device\tDummy Audio or closed; no playback or hardware I/O\n")
out:write("audio_source\t", wave_path, "\n")
out:write("sample_rate\t", sample_rate, "\n")
out:write("item_start_sample\t", start_sample, "\n")
out:write("item_end_sample\t", end_sample, "\n")
out:write("item_take_source\t", reaper.GetMediaSourceFileName(reaper.GetMediaItemTake_Source(take), ""), "\n")
out:write("snap_command\t1157\n")
out:write("case\taction_id\tsnap_enabled\tcursor_seconds\tcursor_sample_rounded\tproject_state_delta\n")
out:flush()

local function set_snap(enabled)
  local state = reaper.GetToggleCommandStateEx(0, 1157)
  assert(state == 0 or state == 1, "could not read snap state")
  if (state == 1) ~= enabled then reaper.Main_OnCommand(1157, 0) end
  local updated = reaper.GetToggleCommandStateEx(0, 1157)
  assert(updated == (enabled and 1 or 0), "could not set snap state")
end

local function run_case(label, action_id, snap_enabled, cursor_override)
  reaper.SetEditCurPos(cursor_override, false, false)
  local before = reaper.GetProjectStateChangeCount(0)
  reaper.Main_OnCommand(action_id, 0)
  local cursor = reaper.GetCursorPosition()
  local sample = math.floor(cursor * sample_rate + 0.5)
  out:write(table.concat({
    label,
    tostring(action_id),
    tostring(snap_enabled),
    string.format("%.15f", cursor),
    tostring(sample),
    tostring(reaper.GetProjectStateChangeCount(0) - before),
  }, "\t"), "\n")
  out:flush()
end

for _, snap_enabled in ipairs({true, false}) do
  set_snap(snap_enabled)
  local suffix = snap_enabled and "snap_on" or "snap_off"
  run_case(suffix .. "_selected_start", 41173, snap_enabled, 0.0)
  run_case(suffix .. "_selected_end", 41174, snap_enabled, 0.0)
  run_case(suffix .. "_nearest_right", 41168, snap_enabled, start_seconds - 0.5)
  run_case(suffix .. "_nearest_left", 41167, snap_enabled, start_seconds + 0.5)
end

reaper.UpdateArrange()
out:close()
