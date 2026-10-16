-- Run in Linux REAPER 7.82 with an isolated Default 7 configuration and an empty project.
-- source.wav must be the deterministic 48 kHz PCM24 fixture in the probe directory.
-- The script opens Render for a selected track nested two folders deep. Choose
-- Source > Selected tracks (stems), then disable Wildcards > Options > Allow /
-- (create directory) in wildcard substitutions before inspecting or rendering.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.OpenColorThemeFile(root .. "config/ColorThemes/Default_7.0.ReaperThemeZip"))
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local tracks = {
  { name = "Drums", folder_depth = 1 },
  { name = "Acoustic", folder_depth = 1 },
  { name = "Kick/Snare", folder_depth = -1 },
  { name = "Room", folder_depth = -1 },
  { name = "Piano", folder_depth = 0 },
}
for index, spec in ipairs(tracks) do
  reaper.InsertTrackAtIndex(index - 1, true)
  local track = reaper.GetTrack(0, index - 1)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", spec.name, true)
  reaper.SetMediaTrackInfo_Value(track, "I_FOLDERDEPTH", spec.folder_depth)
end

local selected = reaper.GetTrack(0, 2)
local item = reaper.AddMediaItemToTrack(selected)
local take = reaper.AddTakeToMediaItem(item)
assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
for index = 0, reaper.CountTracks(0) - 1 do
  reaper.SetTrackSelected(reaper.GetTrack(0, index), false)
end
reaper.SetTrackSelected(selected, true)

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
local pattern = os.getenv("AAADAW_REAPER_WILDCARD_PATTERN")
  or "$track-$trackslashes-$folders-$folders[,]-$parent"
reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", pattern, true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)

local log = assert(io.open(root .. "render-track-folder-wildcards-native.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\tDefault_7.0.ReaperThemeZip\n")
log:write("render_source_requested\tSelected tracks (stems)\n")
log:write("wildcard_pattern\t", pattern, "\n")
for index, spec in ipairs(tracks) do
  log:write(
    "track_",
    index,
    "\t",
    spec.name,
    "\tfolder_depth=",
    spec.folder_depth,
    "\tselected=",
    tostring(reaper.IsTrackSelected(reaper.GetTrack(0, index - 1))),
    "\n"
  )
end
log:close()

reaper.Main_OnCommand(40015, 0)
local ready = assert(io.open(root .. "render-track-folder-wildcards-ready", "w"))
ready:write("Render to File dialog opened\n")
ready:close()
reaper.defer(function() end)
