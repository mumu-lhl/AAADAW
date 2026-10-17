-- Run in Linux REAPER 7.82 with an isolated Default 7 configuration and an
-- unsaved project. source.wav must be a deterministic one-second PCM24 file.
-- For the default two-track selection, choose Source > Selected tracks (stems)
-- in the Render dialog. Set AAADAW_REAPER_SELECTED_TRACKS=2 to probe Keys only;
-- that case uses the dialog's default Master mix source.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.OpenColorThemeFile(root .. "config/ColorThemes/Default_7.0.ReaperThemeZip"))
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local tracks = { "Bass", "Keys", "Guitar" }
local selected_track_indexes = {}
for index in string.gmatch(os.getenv("AAADAW_REAPER_SELECTED_TRACKS") or "1,2", "%d+") do
  selected_track_indexes[tonumber(index)] = true
end
for index, name in ipairs(tracks) do
  reaper.InsertTrackAtIndex(index - 1, true)
  local track = reaper.GetTrack(0, index - 1)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  if index < 3 then
    local item = reaper.AddMediaItemToTrack(track)
    local take = reaper.AddTakeToMediaItem(item)
    assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
    reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
    reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
    reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
    reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
  end
end
for index = 0, reaper.CountTracks(0) - 1 do
  reaper.SetTrackSelected(reaper.GetTrack(0, index), selected_track_indexes[index + 1] == true)
end

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
local pattern = os.getenv("AAADAW_REAPER_WILDCARD_PATTERN") or "$seltrack-$track"
reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", pattern, true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)

local log = assert(io.open(root .. "render-selected-track-wildcard-native.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\tDefault_7.0.ReaperThemeZip\n")
log:write("render_source_to_select_in_dialog\tSelected tracks (stems)\n")
log:write("wildcard_pattern\t", pattern, "\n")
log:write("selected_track_indexes\t", os.getenv("AAADAW_REAPER_SELECTED_TRACKS") or "1,2", "\n")
for index, name in ipairs(tracks) do
  log:write(
    "track_",
    index,
    "\t",
    name,
    "\tselected=",
    tostring(reaper.IsTrackSelected(reaper.GetTrack(0, index - 1))),
    "\n"
  )
end
log:close()

reaper.Main_OnCommand(40015, 0)
local ready = assert(io.open(root .. "render-selected-track-wildcard-ready", "w"))
ready:write("Render to File dialog opened\n")
ready:close()
reaper.defer(function() end)
