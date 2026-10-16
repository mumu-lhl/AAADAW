-- Run in Linux REAPER 7.82 with a fresh isolated Default 7 configuration.
-- Generate source.wav with probe-render-format-vectors.py in the probe directory.
-- Saves the disposable project under four nested directory names, then opens Render.
-- Set AAADAW_REAPER_WILDCARD_PATTERN to compare $projectdirectory variants.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, initial_path = reaper.EnumProjects(-1, "")

local project_dir = root .. "ProjectRoot/Albums/Album01/Session"
local project_path = project_dir .. "/Directory Session.rpp"
assert(os.execute("mkdir -p '" .. project_dir .. "'"))
assert(os.execute("mkdir -p '" .. root .. "output'"))

local function open_render_dialog()
  local _, active_path = reaper.EnumProjects(-1, "")
  if active_path ~= project_path then
    reaper.defer(open_render_dialog)
    return
  end
  for field, value in pairs({
    RENDER_SETTINGS = 0,
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
  reaper.GetSetProjectInfo_String(0, "RENDER_FILE", root .. "output/", true)
  reaper.GetSetProjectInfo_String(
    0,
    "RENDER_PATTERN",
    os.getenv("AAADAW_REAPER_WILDCARD_PATTERN")
      or "$projectdirectory-$projectdirectory1-$projectdirectory2-$projectdirectory3",
    true
  )
  reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)
  reaper.Main_OnCommand(40015, 0)

  local ready = assert(io.open(root .. "project-directory-render-dialog-ready", "w"))
  ready:write("Render to File dialog opened\n")
  ready:close()
end

if initial_path == "" then
  assert(reaper.CountTracks(0) == 0, "Start from an empty project")
  reaper.InsertTrackAtIndex(0, true)
  local track = reaper.GetTrack(0, 0)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Directory probe", true)
  reaper.SetTrackSelected(track, true)
  local item = reaper.AddMediaItemToTrack(track)
  local take = reaper.AddTakeToMediaItem(item)
  assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
  reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
  reaper.Main_SaveProjectEx(0, project_path, 0)
  local saved_file = assert(io.open(project_path, "r"), "The nested project must save")
  saved_file:close()
  reaper.Main_openProject(project_path)
  reaper.defer(open_render_dialog)
else
  assert(initial_path == project_path, "Open only the isolated nested project")
  open_render_dialog()
end
