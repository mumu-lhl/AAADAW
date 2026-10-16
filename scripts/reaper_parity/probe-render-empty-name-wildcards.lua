-- Run in Linux REAPER 7.82 with a fresh isolated Default 7 configuration.
-- Generate source.wav with probe-render-format-vectors.py in the probe directory.
-- The Render dialog opens with two regions and boundary markers, unnamed by default.
-- Set AAADAW_REAPER_WILDCARD_NAMED_OBJECTS=1 to name them for name-only probes.
-- Select All project regions and inspect the configured wildcard substitutions.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Wildcard fallback fixture", true)
for _, start in ipairs({ 0, 2, 4 }) do
  local item = reaper.AddMediaItemToTrack(track)
  local take = reaper.AddTakeToMediaItem(item)
  assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", start)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
  reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
end

local marker_names = { "Start", "Next", "End" }
for index, position in ipairs({ 0, 2, 4 }) do
  local name = os.getenv("AAADAW_REAPER_WILDCARD_NAMED_OBJECTS") == "1" and marker_names[index] or ""
  assert(reaper.AddProjectMarker2(0, false, position, 0, name, -1, 0))
end
local region_names = { "Verse", "Chorus" }
for index, bounds in ipairs({ { 0, 2 }, { 2, 4 } }) do
  local name = os.getenv("AAADAW_REAPER_WILDCARD_NAMED_OBJECTS") == "1" and region_names[index] or ""
  assert(reaper.AddProjectMarker2(0, true, bounds[1], bounds[2], name, -1, 0))
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
  os.getenv("AAADAW_REAPER_WILDCARD_PATTERN") or "$region-$marker",
  true
)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)
reaper.Main_OnCommand(40015, 0)
local ready = assert(io.open(root .. "render-empty-name-wildcards-ready", "w"))
ready:write("Render to file dialog opened\n")
ready:close()
reaper.defer(function() end)
