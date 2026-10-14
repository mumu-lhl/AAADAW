-- Run in Linux REAPER 7.82 with the isolated Default 7 profile used by the
-- selected-media-item source probe. Open the real Render dialog and render
-- the one selected item after confirming Source is "Selected media items".
-- source.wav must be one second of 48 kHz PCM24 stereo at L=0.125/R=0.375.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local version = reaper.GetAppVersion()
local theme = reaper.GetLastColorThemeFile()
local environment = assert(io.open(root .. "selected-item-volume-environment.tsv", "w"))
environment:write("version\t", version, "\n")
environment:write("theme\t", theme, "\n")
environment:close()
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Item volume fixture", true)
reaper.SetMediaTrackInfo_Value(track, "D_VOL", 0.5)
local item = reaper.AddMediaItemToTrack(track)
local take = reaper.AddTakeToMediaItem(item)
assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
reaper.SetMediaItemInfo_Value(item, "D_VOL", 0.5)
reaper.SetMediaItemInfo_Value(item, "D_PAN", 0)
reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
reaper.SetMediaItemSelected(item, true)

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
reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "selected-item-processing-track-half-$item", true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)

local log = assert(io.open(root .. "selected-item-volume-native.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("source\t48 kHz PCM24 stereo, constant L=0.125/R=0.375\n")
log:write("selected_items\t", reaper.CountSelectedMediaItems(0), "\n")
log:write("item_volume_linear\t", reaper.GetMediaItemInfo_Value(item, "D_VOL"), "\n")
log:write("item_pan\t", reaper.GetMediaItemInfo_Value(item, "D_PAN"), "\n")
log:write("track_volume_linear\t", reaper.GetMediaTrackInfo_Value(track, "D_VOL"), "\n")
log:write("render_pattern\tselected-item-processing-track-half-$item\n")
log:close()

reaper.Main_OnCommand(40015, 0)
local ready = assert(io.open(root .. "selected-item-volume-render-dialog-ready", "w"))
ready:write("Render to File dialog opened\n")
ready:close()
reaper.defer(function() end)
