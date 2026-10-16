-- Generate drums.wav, bass.wav, and guitar.wav in the output directory with
-- generate-region-render-matrix-sources.py, then run in Linux REAPER 7.82 with an isolated
-- Default 7 configuration and empty project. After closing the initial audio-device dialog,
-- create a file named `render-ready` in that output directory to start the deferred render.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use an empty disposable project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved project")

local log = assert(io.open(root .. "region-render-matrix-render-native.tsv", "w"))
local function record(key, value)
  log:write(key, "\t", tostring(value), "\n")
  log:flush()
end

local tracks = {}
for index, spec in ipairs({
  { name = "Drums", file = "drums.wav" },
  { name = "Bass", file = "bass.wav" },
  { name = "Guitar", file = "guitar.wav" },
}) do
  reaper.InsertTrackAtIndex(index - 1, true)
  local track = reaper.GetTrack(0, index - 1)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", spec.name, true)
  reaper.SetMediaTrackInfo_Value(track, "D_VOL", 1)
  reaper.SetMediaTrackInfo_Value(track, "D_PAN", 0)
  local item = reaper.AddMediaItemToTrack(track)
  local take = reaper.AddTakeToMediaItem(item)
  assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. spec.file)))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 4)
  reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
  reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
  tracks[index] = track
end

assert(reaper.AddProjectMarker2(0, true, 0, 2, "Intro", 1, 0))
assert(reaper.AddProjectMarker2(0, true, 2, 4, "Verse", 2, 0))
reaper.SetRegionRenderMatrix(0, 1, reaper.GetMasterTrack(0), 1)
reaper.SetRegionRenderMatrix(0, 1, tracks[1], 1)
reaper.SetRegionRenderMatrix(0, 1, tracks[3], 1)
reaper.SetRegionRenderMatrix(0, 2, tracks[2], 1)

for field, value in pairs({
  RENDER_BOUNDSFLAG = 3,
  RENDER_STARTPOS = 0,
  RENDER_ENDPOS = 4,
  RENDER_SETTINGS = 0,
  RENDER_SRATE = 48000,
  RENDER_CHANNELS = 2,
  RENDER_TAILFLAG = 0,
  RENDER_NORMALIZE = 0,
}) do
  reaper.GetSetProjectInfo(0, field, value, true)
end
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)

local function list_files(folder)
  local files = {}
  local index = 0
  while true do
    local name = reaper.EnumerateFiles(folder, index)
    if not name then break end
    files[#files + 1] = name
    index = index + 1
  end
  table.sort(files)
  return table.concat(files, ";")
end

local function render(action, name, settings)
  local folder = root .. name
  reaper.RecursiveCreateDirectory(folder, 0)
  reaper.GetSetProjectInfo_String(0, "RENDER_FILE", folder .. "/", true)
  reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "$region-$track", true)
  reaper.GetSetProjectInfo(0, "RENDER_SETTINGS", settings, true)
  record("action", action .. " / " .. (reaper.kbd_getTextFromCmd(action, 0) or "<missing>"))
  record(name .. "_settings_before", reaper.GetSetProjectInfo(0, "RENDER_SETTINGS", 0, false))
  reaper.Main_OnCommand(action, 0)
  record(name .. "_files", list_files(folder))
  record(name .. "_settings_after", reaper.GetSetProjectInfo(0, "RENDER_SETTINGS", 0, false))
end

local function record_matrix(key)
  for region_index, region_name in ipairs({ "Intro", "Verse" }) do
    local assigned = {}
    local matrix_index = 0
    while true do
      local track = reaper.EnumRegionRenderMatrix(0, region_index, matrix_index)
      if not track then break end
      if track == reaper.GetMasterTrack(0) then
        assigned[#assigned + 1] = "Master mix"
      else
        local _, track_name = reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "", false)
        assigned[#assigned + 1] = track_name
      end
      matrix_index = matrix_index + 1
    end
    record(key .. "_" .. region_name, table.concat(assigned, ";"))
  end
end

record("version", reaper.GetAppVersion())
record("theme", reaper.GetLastColorThemeFile())
record("fixture", "48 kHz PCM24, 4-second constant tracks Drums=0.125, Bass=0.25, Guitar=0.375; Intro [0,2), Verse [2,4)")

local function wait_until_audio_dialog_is_closed()
  local ready = io.open(root .. "render-ready", "r")
  if not ready then
    reaper.defer(wait_until_audio_dialog_is_closed)
    return
  end
  ready:close()
  os.remove(root .. "render-ready")
  record_matrix("matrix_before_render")
  reaper.Main_SaveProjectEx(0, root .. "region-render-matrix-render-native-before.rpp", 0)
  render(42230, "matrix-render", 8)
  record_matrix("matrix_after_render")
  local save_path = root .. "region-render-matrix-render-native.rpp"
  reaper.Main_SaveProjectEx(0, save_path, 0)
  log:close()
  local done = assert(io.open(root .. "region-render-matrix-render-done", "w"))
  done:write("complete\n")
  done:close()
end

reaper.defer(wait_until_audio_dialog_is_closed)
