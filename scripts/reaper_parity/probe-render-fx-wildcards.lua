-- Run in Linux REAPER 7.82 with an isolated Default 7 configuration and an empty project.
-- source.wav must be the deterministic 48 kHz PCM24 fixture in the probe directory.
-- The script opens Render with one selected track and two stock track FX. Choose
-- Source > Selected tracks (stems) in the dialog before inspecting or rendering.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.OpenColorThemeFile(root .. "config/ColorThemes/Default_7.0.ReaperThemeZip"))
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "FX Wildcard", true)
local item = reaper.AddMediaItemToTrack(track)
local take = reaper.AddTakeToMediaItem(item)
assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)

local fx_names = { "JS: Volume Adjustment", "ReaEQ (Cockos)" }
for _, name in ipairs(fx_names) do
  local index = reaper.TrackFX_AddByName(track, name, false, -1)
  assert(index >= 0, "Required stock effect is unavailable: " .. name)
end
reaper.SetTrackSelected(track, true)

local output_dir = root .. "output/"
assert(os.execute("mkdir -p '" .. output_dir .. "'"))
for field, value in pairs({
  RENDER_SETTINGS = 2, -- Selected tracks (stems)
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
reaper.GetSetProjectInfo_String(0, "RENDER_FILE", output_dir, true)
reaper.GetSetProjectInfo_String(
  0,
  "RENDER_PATTERN",
  os.getenv("AAADAW_REAPER_WILDCARD_PATTERN") or "$track-$fx-$fx[,]",
  true
)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)

local log = assert(io.open(root .. "render-fx-wildcards-native.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\tDefault_7.0.ReaperThemeZip\n")
log:write("source\t48 kHz PCM24 stereo, one selected track stem\n")
log:write("render_source_requested\tSelected tracks (stems)\n")
log:write("wildcard_pattern\t", os.getenv("AAADAW_REAPER_WILDCARD_PATTERN") or "$track-$fx-$fx[,]", "\n")
for index = 0, reaper.TrackFX_GetCount(track) - 1 do
  local _, fx_name = reaper.TrackFX_GetFXName(track, index, "")
  log:write("track_fx_", index + 1, "\t", fx_name, "\n")
end
log:close()

reaper.Main_OnCommand(40015, 0)
local ready = assert(io.open(root .. "render-fx-wildcards-ready", "w"))
ready:write("Render to File dialog opened\n")
ready:close()
reaper.defer(function() end)
