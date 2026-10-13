-- Run in Linux REAPER 7.82 with an isolated Default 7 configuration and an empty project.
-- source.wav must be a one-second, 48 kHz, stereo PCM24 file with constant L=0.125/R=0.375.
-- The probe creates three selected/unselected regions and a razor edit, then renders each
-- documented RENDER_BOUNDSFLAG through the native most-recent-settings render action.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local items = {}
local function add_track()
  reaper.InsertTrackAtIndex(0, true)
  local track = reaper.GetTrack(0, 0)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Render fixture", true)
  for index, start in ipairs({ 0, 2, 4 }) do
    local item = reaper.AddMediaItemToTrack(track)
    local take = reaper.AddTakeToMediaItem(item)
    assert(reaper.SetMediaItemTake_Source(take, reaper.PCM_Source_CreateFromFile(root .. "source.wav")))
    reaper.SetMediaItemInfo_Value(item, "D_POSITION", start)
    reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
    reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
    reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
    items[index] = item
  end
  return track
end

local track = add_track()
for index, range in ipairs({ { 0, 1 }, { 2, 3 }, { 4, 5 } }) do
  assert(reaper.AddProjectMarker2(0, true, range[1], range[2], "Region " .. index, -1, 0))
end
reaper.SetMediaItemSelected(items[2], true)
reaper.GetSet_LoopTimeRange2(0, true, false, 1.5, 4.5, false)
reaper.GetSetMediaTrackInfo_String(track, "P_RAZOREDITS", '1.5 2.5 ""', true)
reaper.GetSetProjectInfo_String(0, "RENDER_FILE", root, true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)
for field, value in pairs({
  RENDER_SETTINGS = 0,
  RENDER_SRATE = 48000,
  RENDER_CHANNELS = 2,
  RENDER_TAILFLAG = 0,
  RENDER_NORMALIZE = 0,
}) do
  reaper.GetSetProjectInfo(0, field, value, true)
end

local log = assert(io.open(root .. "render-bounds-native.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\tDefault_7.0.ReaperThemeZip\n")
log:write("audio_device\tDummy Audio; no hardware I/O\n")
log:write("fixture\tThree one-second Audio items and same-span regions at [0,1], [2,3], [4,5]; item 2 selected; time selection [1.5,4.5]; razor [1.5,2.5]\n")
log:write("case\tbounds_flag\tselection\tregion_ranges\ttime_selection\trazor_edit\toutput_files\toutput_notes\n")

for _, flag in ipairs({ 0, 1, 2, 3, 4, 5, 6 }) do
  local folder = root .. "bounds-" .. flag
  reaper.RecursiveCreateDirectory(folder, 0)
  reaper.GetSetProjectInfo(0, "RENDER_BOUNDSFLAG", flag, true)
  reaper.GetSetProjectInfo(0, "RENDER_STARTPOS", 0, true)
  reaper.GetSetProjectInfo(0, "RENDER_ENDPOS", 5, true)
  reaper.GetSetProjectInfo_String(0, "RENDER_FILE", folder .. "/", true)
  reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "bounds-" .. flag .. "-$region-$item", true)
  local selected_count = reaper.CountSelectedMediaItems(0)
  local selected_regions = 0
  for index = 0, reaper.CountProjectMarkers(0) - 1 do
    local _, is_region = reaper.EnumProjectMarkers3(0, index)
    if is_region then selected_regions = selected_regions + 1 end
  end
  local flags_before = reaper.GetSetProjectInfo(0, "RENDER_BOUNDSFLAG", 0, false)
  reaper.Main_OnCommand(42230, 0)
  local files = {}
  local index = 0
  while true do
    local name = reaper.EnumerateFiles(folder, index)
    if not name then break end
    files[#files + 1] = name
    index = index + 1
  end
  table.sort(files)
  log:write("native_bounds_" .. flag, "\t", flags_before, "\tselected_items=", selected_count,
    "\tregions=", selected_regions, "\t1.5..4.5\t1.5..2.5\t", table.concat(files, ";"),
    "\t", #files, " output file(s)\n")
  log:flush()
end

log:close()
reaper.defer(function() end)
