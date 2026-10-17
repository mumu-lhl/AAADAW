-- Run in Linux REAPER 7.82 with an isolated Default 7 configuration and an
-- unsaved project. source.wav must be a deterministic one-second PCM24 file.
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
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Tone", true)
local item = reaper.AddMediaItemToTrack(track)
local take = reaper.AddTakeToMediaItem(item)
assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)

local output_dir = root .. "output/"
assert(os.execute("mkdir -p '" .. output_dir .. "'"))
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
reaper.GetSetProjectInfo_String(0, "RENDER_FILE", output_dir, true)
local pattern = os.getenv("AAADAW_REAPER_WILDCARD_PATTERN")
  or "$uniqueid[1]-$uniqueid[2]-$uniqueid[3]-$uniqueid[4]-$uniqueid[5]-$uniqueid[7]-$uniqueid[8]-$uniqueid[9]-$uniqueid[10]-$uniqueid[12]-$uniqueid[16]-$uniqueid[17]-$uniqueid[31]-$uniqueid[32]-$uniqueid[33]-$uniqueid[64]-$uniqueid-$track"
reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", pattern, true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)

local log = assert(io.open(root .. "render-uniqueid-wildcard-native.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\tDefault_7.0.ReaperThemeZip\n")
log:write("wildcard_pattern\t", pattern, "\n")
log:write("render_bounds_seconds\t0-1\n")
log:close()

reaper.Main_OnCommand(40015, 0)
local ready = assert(io.open(root .. "render-uniqueid-wildcard-ready", "w"))
ready:write("Render to File dialog opened\n")
ready:close()
reaper.defer(function() end)
