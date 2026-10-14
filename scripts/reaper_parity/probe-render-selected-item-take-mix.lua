-- Run in Linux REAPER 7.82 with an isolated factory Default 7 config.
-- source.wav must be one second of 48 kHz PCM24 stereo at L=0.125/R=0.375.
-- The four selected items have take volume 0.5 and exercise center, 25%R,
-- hard-left, and hard-right balance. In the opened dialog choose Source >
-- Selected media items, confirm the four-file WAV 24-bit PCM setup, then render.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local version = reaper.GetAppVersion()
local theme = reaper.GetLastColorThemeFile()
local pan_cases = {
  { name = "center", value = 0.0 },
  { name = "25-right", value = 0.25 },
  { name = "hard-left", value = -1.0 },
  { name = "hard-right", value = 1.0 },
}
local log = assert(io.open(root .. "selected-item-take-mix-native.tsv", "w"))
log:write("version\t", version, "\n")
log:write("theme\t", theme, "\n")
log:write("source\t48 kHz PCM24 stereo, constant L=0.125/R=0.375\n")
log:write("take_volume_linear\t0.5\n")

for index, case in ipairs(pan_cases) do
  reaper.InsertTrackAtIndex(index - 1, true)
  local track = reaper.GetTrack(0, index - 1)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", case.name, true)
  reaper.SetMediaTrackInfo_Value(track, "D_VOL", 1.0)
  local item = reaper.AddMediaItemToTrack(track)
  local take = reaper.AddTakeToMediaItem(item)
  assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
  reaper.SetMediaItemTakeInfo_Value(take, "D_VOL", 0.5)
  reaper.SetMediaItemTakeInfo_Value(take, "D_PAN", case.value)
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  reaper.SetMediaItemInfo_Value(item, "D_VOL", 1.0)
  reaper.SetMediaItemInfo_Value(item, "D_PAN", 0.0)
  reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
  reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
  reaper.SetMediaItemSelected(item, true)
  log:write(case.name, "\t", case.value, "\t", reaper.GetMediaItemTakeInfo_Value(take, "D_VOL"),
    "\t", reaper.GetMediaItemTakeInfo_Value(take, "D_PAN"), "\n")
end

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
reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "take-mix-$item", true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)
log:write("selected_items\t", reaper.CountSelectedMediaItems(0), "\n")
log:close()

reaper.Main_OnCommand(40015, 0)
local ready = assert(io.open(root .. "take-mix-render-dialog-ready", "w"))
ready:write("Render to File dialog opened\n")
ready:close()
reaper.defer(function() end)
