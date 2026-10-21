-- Owned scratch-project probe. Never run against a personal/open project.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local log = assert(io.open(root .. "marker-delete.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("case\trequested_cursor\tactual_cursor\tmarkers_before\tmarkers_after\n")

local function markers()
  local count = reaper.CountProjectMarkers(0)
  local result = {}
  for index = 0, count - 1 do
    local valid, is_region, position, region_end, name, marker_index =
      reaper.EnumProjectMarkers3(0, index)
    if valid ~= 0 then
      result[#result + 1] = {
        is_region = is_region,
        position = position,
        region_end = region_end,
        name = name,
        index = marker_index,
      }
    end
  end
  return result
end

local function clear_markers()
  for _, marker in ipairs(markers()) do
    reaper.DeleteProjectMarker(0, marker.index, marker.is_region)
  end
end

local function describe_markers()
  local descriptions = {}
  for _, marker in ipairs(markers()) do
    descriptions[#descriptions + 1] = string.format(
      "%s:%s@%.6f",
      marker.is_region and "region" or "marker",
      marker.name,
      marker.position
    )
  end
  return #descriptions > 0 and table.concat(descriptions, ";") or "-"
end

local function trial(label, positions, cursor)
  clear_markers()
  for index, position in ipairs(positions) do
    reaper.AddProjectMarker2(0, false, position, 0, label .. "-" .. index, -1, 0)
  end
  reaper.SetEditCurPos2(0, cursor, false, false)
  log:write(label, "\t", string.format("%.9f", cursor), "\t",
    string.format("%.9f", reaper.GetCursorPosition()), "\t", describe_markers(), "\t")
  reaper.Main_OnCommand(40613, 0)
  log:write(describe_markers(), "\n")
  log:flush()
end

for _, distance in ipairs({
  -2, -1.001, -1.000001, -1, -0.999999, -0.99, -0.95, -0.9, -0.85, -0.8, -0.7501, -0.75, -0.7499,
  -0.7, -0.5001, -0.5, -0.2501, -0.25, -0.1, -0.0001, -0.00005, -0.00002,
  -0.00001, -0.000001, 0, 0.000001, 0.00001, 0.00002, 0.00005, 0.0001,
  0.0005, 0.001, 0.005, 0.01,
  0.1, 0.25, 0.2501, 0.5, 0.5001,
  0.7, 0.7499, 0.75, 0.7501, 0.8, 0.85, 0.9, 0.95, 0.99, 1, 1.001, 2,
}) do
  trial(string.format("single_%+.6f", distance), { 10 }, 10 + distance)
end

trial("nearest_of_two", { 10, 10.5 }, 10.42)
trial("nearest_marker_wins_over_farther_future_marker", { 10, 10.5 }, 10.1)
trial("equidistant_markers", { 9.5, 10.5 }, 10)
clear_markers()
reaper.AddProjectMarker2(0, false, 10, 0, "marker", -1, 0)
reaper.AddProjectMarker2(0, true, 10.25, 11.25, "region", -1, 0)
reaper.SetEditCurPos2(0, 10.1, false, false)
log:write("region_is_not_marker\t10.100000000\t", string.format("%.9f", reaper.GetCursorPosition()), "\t", describe_markers(), "\t")
reaper.Main_OnCommand(40613, 0)
log:write(describe_markers(), "\n")
log:flush()
log:close()

assert(reaper.Main_SaveProjectEx(0, root .. "marker-delete-probe.rpp", 0))
reaper.defer(function()
  reaper.Main_OnCommand(40004, 0)
end)
