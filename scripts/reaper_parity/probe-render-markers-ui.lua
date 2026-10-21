-- Run in Linux REAPER 7.82 with an isolated Default 7 configuration and an empty project.
-- Generate source.wav with probe-render-format-vectors.py into a disposable directory and set
-- AAADAW_REAPER_PROBE_DIR to it. This opens the actual Render dialog with three markers/items.
-- Select All project markers in the Bounds menu and inspect the native file count/output names.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Marker render fixture", true)
for _, start in ipairs({ 0, 2, 4 }) do
  local item = reaper.AddMediaItemToTrack(track)
  local take = reaper.AddTakeToMediaItem(item)
  assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", start)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
  reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
end
for index, position in ipairs({ 0, 2, 4 }) do
  assert(reaper.AddProjectMarker2(0, false, position, 0, "Marker " .. index, -1, 0))
end
for field, value in pairs({
  RENDER_SETTINGS = 0,
  RENDER_BOUNDSFLAG = 0,
  RENDER_STARTPOS = 0,
  RENDER_ENDPOS = 5,
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
  os.getenv("AAADAW_REAPER_WILDCARD_PATTERN") or "markers-$marker-$item",
  true
)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)
reaper.Main_OnCommand(40015, 0)
local ready = assert(io.open(root .. "render-markers-ui-ready", "w"))
ready:write("Render to file dialog opened\n")
ready:close()
reaper.defer(function() end)
