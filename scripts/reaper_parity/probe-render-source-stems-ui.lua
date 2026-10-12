-- Run in Linux REAPER 7.82 with a fresh isolated Default 7 configuration and an empty project.
-- source.wav must be a one-second, 48 kHz, stereo PCM24 file with constant L=0.125/R=0.375.
-- This opens the native Render dialog. Render once with "Master mix and stems", then choose
-- "Selected tracks (stems)" and render again to compare Master-stage behavior.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local function add_source_track(index, name)
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
  reaper.SetTrackSelected(track, true)
  return track
end

add_source_track(0, "Selected")
add_source_track(1, "Unselected")
reaper.SetMediaTrackInfo_Value(reaper.GetMasterTrack(0), "D_VOL", 0.5)
for field, value in pairs({
  RENDER_SETTINGS = 1,
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
reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "master-and-stems-$track", true)

local log = assert(io.open(root .. "render-source-stems-ui.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("selected_tracks\t", reaper.CountSelectedTracks(0), "\n")
log:write("master_volume_linear\t", reaper.GetMediaTrackInfo_Value(reaper.GetMasterTrack(0), "D_VOL"), "\n")
log:write("dialog_initial_settings\t", reaper.GetSetProjectInfo(0, "RENDER_SETTINGS", 0, false), "\n")
log:close()
reaper.Main_OnCommand(40015, 0)
reaper.defer(function() end)
