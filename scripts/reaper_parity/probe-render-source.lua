-- Run in Linux REAPER 7.82 with an isolated Default 7 configuration and an empty unsaved project.
-- source.wav must be a one-second, 48 kHz, stereo PCM24 file with constant L=0.125/R=0.375.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local function add_source_track(index, name, selected)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  local item = reaper.AddMediaItemToTrack(track)
  local take = reaper.AddTakeToMediaItem(item)
  reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav"))
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
  reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
  reaper.SetMediaTrackInfo_Value(track, "D_PAN", 0)
  reaper.SetTrackSelected(track, selected)
  return track
end

local selected = add_source_track(0, "Selected", true)
local unselected = add_source_track(1, "Unselected", false)
reaper.SetTrackSelected(selected, true)
reaper.SetTrackSelected(unselected, false)
for field, value in pairs({
  RENDER_BOUNDSFLAG = 0,
  RENDER_STARTPOS = 0,
  RENDER_ENDPOS = 1,
  RENDER_SRATE = 48000,
  RENDER_CHANNELS = 2,
  RENDER_TAILFLAG = 0,
  RENDER_NORMALIZE = 0,
}) do
  reaper.GetSetProjectInfo(0, field, value, true)
end
reaper.GetSetProjectInfo_String(0, "RENDER_FILE", root, true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)

local log = assert(io.open(root .. "source-reference.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("source\t48 kHz PCM24 stereo, two identical tracks, constant L=0.125 R=0.375\n")
for _, probe in ipairs({
  { name = "master-mix", flags = 0, selection = "one" },
  { name = "selected-via-master-$track", flags = 128, selection = "one" },
  { name = "selected-stems-$track", flags = 128, selection = "both" },
}) do
  reaper.SetTrackSelected(selected, true)
  reaper.SetTrackSelected(unselected, probe.selection == "both")
  reaper.GetSetProjectInfo(0, "RENDER_SETTINGS", probe.flags, true)
  reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", probe.name, true)
  reaper.Main_OnCommand(42230, 0)
  log:write(probe.name, "\tflags=",
    reaper.GetSetProjectInfo(0, "RENDER_SETTINGS", 0, false),
    "\tselected_tracks=", reaper.CountSelectedTracks(0), "\n")
  log:flush()
end
log:close()
local done = assert(io.open(root .. "source-done", "w"))
done:write("complete\n")
done:close()
