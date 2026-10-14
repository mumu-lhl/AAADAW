-- Run in Linux REAPER 7.82 with an isolated factory Default 7 config.
-- source.wav must be one second of 48 kHz PCM24 stereo at L=0.125/R=0.375.
-- Master volume and pan are non-neutral; its JS Volume Adjustment is +3 dB.
-- In the opened dialog choose Source > Selected media items, then render the WAV.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.SetMediaTrackInfo_Value(track, "D_VOL", 1.0)
local item = reaper.AddMediaItemToTrack(track)
local take = reaper.AddTakeToMediaItem(item)
assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
reaper.SetMediaItemTakeInfo_Value(take, "D_VOL", 1.0)
reaper.SetMediaItemTakeInfo_Value(take, "D_PAN", 0.0)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
reaper.SetMediaItemInfo_Value(item, "D_VOL", 1.0)
reaper.SetMediaItemInfo_Value(item, "D_PAN", 0.0)
reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
reaper.SetMediaItemSelected(item, true)

local master = reaper.GetMasterTrack(0)
reaper.SetMediaTrackInfo_Value(master, "D_VOL", 0.5)
reaper.SetMediaTrackInfo_Value(master, "D_PAN", 0.5)
local fx = reaper.TrackFX_AddByName(master, "JS: Volume Adjustment", false, 1)
assert(fx >= 0, "REAPER's stock JS Volume Adjustment effect is required")
assert(reaper.TrackFX_SetParamNormalized(master, fx, 0, 0.51))
assert(reaper.TrackFX_SetParamNormalized(master, fx, 1, 1.0))

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
reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "selected-item-master-fx-plus-3db", true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)

local log = assert(io.open(root .. "selected-item-master-fx-native.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("source\t48 kHz PCM24 stereo, constant L=0.125/R=0.375\n")
log:write("master_volume_linear\t", reaper.GetMediaTrackInfo_Value(master, "D_VOL"), "\n")
log:write("master_pan\t", reaper.GetMediaTrackInfo_Value(master, "D_PAN"), "\n")
local _, fx_name = reaper.TrackFX_GetFXName(master, fx, "")
log:write("master_fx\t", fx_name, "\n")
local _, gain_name = reaper.TrackFX_GetParamName(master, fx, 0, "")
local gain_value, gain_min, gain_max = reaper.TrackFX_GetParam(master, fx, 0)
local gain_normalized = reaper.TrackFX_GetParamNormalized(master, fx, 0)
local _, gain_formatted = reaper.TrackFX_GetFormattedParamValue(master, fx, 0, "")
log:write("master_fx_param\t", gain_name, "\t", gain_value, "\t", gain_min, "\t", gain_max,
  "\t", gain_normalized, "\t", gain_formatted, "\n")
local _, limit_name = reaper.TrackFX_GetParamName(master, fx, 1, "")
local limit_value, limit_min, limit_max = reaper.TrackFX_GetParam(master, fx, 1)
local limit_normalized = reaper.TrackFX_GetParamNormalized(master, fx, 1)
local _, limit_formatted = reaper.TrackFX_GetFormattedParamValue(master, fx, 1, "")
log:write("master_fx_param\t", limit_name, "\t", limit_value, "\t", limit_min, "\t", limit_max,
  "\t", limit_normalized, "\t", limit_formatted, "\n")
log:close()

reaper.Main_OnCommand(40015, 0)
local ready = assert(io.open(root .. "selected-item-master-fx-render-dialog-ready", "w"))
ready:write("Render to File dialog opened\n")
ready:close()
reaper.defer(function() end)
