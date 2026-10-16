-- Run only in a fresh, disposable REAPER 7.82 profile and project.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use an empty disposable project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")
assert(reaper.APIExists("SetRegionRenderMatrix"))
assert(reaper.APIExists("EnumRegionRenderMatrix"))

local track_names = { "Drums", "Bass", "Guitar" }
local tracks = {}
for index, name in ipairs(track_names) do
  reaper.InsertTrackAtIndex(index - 1, true)
  local track = reaper.GetTrack(0, index - 1)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  tracks[#tracks + 1] = track
end
reaper.AddProjectMarker2(0, true, 0, 4, "Intro", 1, 0)
reaper.AddProjectMarker2(0, true, 4, 8, "Verse", 2, 0)

-- REAPER's render-matrix API uses the visible region number as its index.
reaper.SetRegionRenderMatrix(0, 1, tracks[1], 1)
reaper.SetRegionRenderMatrix(0, 1, tracks[3], 1)
reaper.SetRegionRenderMatrix(0, 1, reaper.GetMasterTrack(0), 1)
reaper.SetRegionRenderMatrix(0, 2, tracks[2], 1)

local log = assert(io.open(root .. "region-render-matrix-native.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("action_41888\t", reaper.kbd_getTextFromCmd(41888, 0) or "<missing>", "\n")
log:write("region_index\tregion\tassigned_tracks\n")
local regions = {}
local marker_index = 0
while true do
  local valid, is_region, start_time, end_time, name = reaper.EnumProjectMarkers3(0, marker_index)
  if valid == 0 then break end
  if is_region then regions[#regions + 1] = name end
  marker_index = marker_index + 1
end
for region_index, name in ipairs(regions) do
  local assigned = {}
  local matrix_index = 0
  while true do
    local track = reaper.EnumRegionRenderMatrix(0, region_index, matrix_index)
    if not track then break end
    local track_name
    if track == reaper.GetMasterTrack(0) then
      track_name = "Master mix"
    else
      local _
      _, track_name = reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "", false)
    end
    assigned[#assigned + 1] = track_name
    matrix_index = matrix_index + 1
  end
  log:write(region_index - 1, "\t", name, "\t", table.concat(assigned, ";"), "\n")
end
log:flush()
log:close()

reaper.UpdateArrange()
local save_path = root .. "region-render-matrix-native.rpp"
reaper.Main_SaveProjectEx(0, save_path, 0)
reaper.Main_OnCommand(41888, 0)
local started = reaper.time_precise()
local function keep_open()
  if reaper.time_precise() - started < 45 then reaper.defer(keep_open) end
end
keep_open()
