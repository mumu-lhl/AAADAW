-- Run in Linux REAPER 7.82 with an isolated Default 7 configuration and an empty project.
-- source.wav must be one second of 48 kHz stereo PCM24. For the standard format check use
-- constant L=0.125/R=0.375; probe-render-ulaw-vectors.py creates a segmented μ-law test fixture.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_description = os.getenv("AAADAW_REAPER_PROBE_SOURCE_DESCRIPTION")
  or "48 kHz PCM24 stereo; 48,000 frames; L=0.125/R=0.375"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

for index, name in ipairs({ "Selected", "Unselected" }) do
  reaper.InsertTrackAtIndex(index - 1, true)
  local track = reaper.GetTrack(0, index - 1)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  local item = reaper.AddMediaItemToTrack(track)
  local take = reaper.AddTakeToMediaItem(item)
  reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav"))
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
  reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
end

for field, value in pairs({
  RENDER_SETTINGS = 0,
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
reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "native-format-$track", true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)

local log = assert(io.open(root .. "render-formats.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("source\t", source_description, "\n")
log:write("render_source\tMaster mix\noutput_channels\t2\noutput_rate\t48000\n")
log:close()

reaper.Main_OnCommand(40015, 0)
reaper.defer(function() end)
