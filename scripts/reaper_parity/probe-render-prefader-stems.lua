-- Run in Linux REAPER 7.82 with a fresh isolated Default 7 configuration and an empty project.
-- source.wav must be a one-second, 48 kHz, stereo PCM24 file with constant L=0.125/R=0.375.
-- Set AAADAW_RENDER_PREFADER=1 for the checked pre-fader source; otherwise it opens post-fader.
-- In the Render dialog choose Source "Selected tracks (stems)" and render both selected tracks.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local pre_fader = os.getenv("AAADAW_RENDER_PREFADER") == "1"
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
  reaper.SetMediaTrackInfo_Value(track, "D_VOL", 0.5)
  reaper.SetTrackSelected(track, true)
end
for field, value in pairs({
  RENDER_SETTINGS = pre_fader and 8194 or 2,
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
reaper.GetSetProjectInfo_String(
  0,
  "RENDER_PATTERN",
  pre_fader and "prefader-$track" or "postfader-$track",
  true
)

local log = assert(io.open(root .. "render-prefader-stems.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("selected_tracks\t", reaper.CountSelectedTracks(0), "\n")
log:write("track_volume_linear\t0.5\n")
log:write("requested_settings\t", pre_fader and 8194 or 2, "\n")
log:write("requested_pre_fader\t", pre_fader and "true" or "false", "\n")
log:close()
reaper.Main_OnCommand(40015, 0)
reaper.defer(function() end)
