-- Run in Linux REAPER 7.82 with an isolated Default 7 configuration and an empty unsaved project.
-- source.wav must be a one-second, 48 kHz, stereo PCM24 file with constant L=0.125/R=0.375.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
local item = reaper.AddMediaItemToTrack(track)
local take = reaper.AddTakeToMediaItem(item)
reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav"))
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
reaper.SetMediaTrackInfo_Value(track, "D_PAN", 0)

for field, value in pairs({
  RENDER_SETTINGS = 0, -- Master mix
  RENDER_BOUNDSFLAG = 0,
  RENDER_STARTPOS = 0,
  RENDER_ENDPOS = 1,
  RENDER_SRATE = 48000,
  RENDER_TAILFLAG = 0,
  RENDER_NORMALIZE = 0,
}) do
  reaper.GetSetProjectInfo(0, field, value, true)
end
reaper.GetSetProjectInfo_String(0, "RENDER_FILE", root, true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)

local log = assert(io.open(root .. "reference.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("source\t48 kHz PCM24 stereo, one second, constant L=0.125 R=0.375\n")
log:write("master_pan\t", reaper.GetMediaTrackInfo_Value(reaper.GetMasterTrack(0), "D_PAN"), "\n")
for _, channels in ipairs({2, 1}) do
  reaper.GetSetProjectInfo(0, "RENDER_CHANNELS", channels, true)
  local name = channels == 1 and "mono" or "stereo"
  reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", name, true)
  reaper.Main_OnCommand(42230, 0)
  log:write("render\t", name, "\trequested channels=", channels,
    "\treadback channels=", reaper.GetSetProjectInfo(0, "RENDER_CHANNELS", 0, false), "\n")
  log:flush()
end
log:close()
local done = assert(io.open(root .. "done", "w"))
done:write("complete\n")
done:close()
