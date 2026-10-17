-- Run in Linux REAPER 7.82 with an isolated factory Default 7 config.
-- source.wav must be one second of 48 kHz PCM24 stereo at L=0.125/R=0.375.
-- The selected item, take, and track are at unity; Master volume and pan are 0.5.
-- In the opened dialog choose Source > Selected media items, then render the WAV.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Selected item", true)
reaper.SetMediaTrackInfo_Value(track, "D_VOL", 1.0)
local item = reaper.AddMediaItemToTrack(track)
local take = reaper.AddTakeToMediaItem(item)
assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
reaper.SetMediaItemTakeInfo_Value(take, "D_VOL", 1.0)
reaper.SetMediaItemTakeInfo_Value(take, "D_PAN", 0.0)
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
reaper.SetMediaItemInfo_Value(item, "D_VOL", 1.0)
reaper.SetMediaItemInfo_Value(item, "D_PAN", 0.0)
reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
reaper.SetMediaItemSelected(item, true)

local master = reaper.GetMasterTrack(0)
reaper.SetMediaTrackInfo_Value(master, "D_VOL", 0.5)
reaper.SetMediaTrackInfo_Value(master, "D_PAN", 0.5)
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
reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "selected-item-master-half-pan-right", true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)

local log = assert(io.open(root .. "selected-item-master-mix-native.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("source\t48 kHz PCM24 stereo, constant L=0.125/R=0.375\n")
log:write("selected_items\t", reaper.CountSelectedMediaItems(0), "\n")
log:write("item_volume_linear\t", reaper.GetMediaItemInfo_Value(item, "D_VOL"), "\n")
log:write("take_volume_linear\t", reaper.GetMediaItemTakeInfo_Value(take, "D_VOL"), "\n")
log:write("track_volume_linear\t", reaper.GetMediaTrackInfo_Value(track, "D_VOL"), "\n")
log:write("master_volume_linear\t", reaper.GetMediaTrackInfo_Value(master, "D_VOL"), "\n")
log:write("master_pan\t", reaper.GetMediaTrackInfo_Value(master, "D_PAN"), "\n")
log:close()

reaper.Main_OnCommand(40015, 0)
local ready = assert(io.open(root .. "selected-item-master-render-dialog-ready", "w"))
ready:write("Render to File dialog opened\n")
ready:close()
reaper.defer(function() end)
