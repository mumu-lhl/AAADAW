-- Owned scratch-project probe. Never run against a personal/open project.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local log = assert(io.open(root .. "numbered-markers.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("case\taction_id\trequested_cursor\tactual_cursor\tmarkers_before\tmarkers_after\n")

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

local function trial(label, action_id, cursor)
  reaper.SetEditCurPos2(0, cursor, false, false)
  log:write(
    label,
    "\t",
    action_id,
    "\t",
    string.format("%.9f", cursor),
    "\t",
    string.format("%.9f", reaper.GetCursorPosition()),
    "\t",
    describe_markers(),
    "\t"
  )
  reaper.Main_OnCommand(action_id, 0)
  log:write(describe_markers(), "\n")
  log:flush()
end

trial("slot_1_insert", 40657, 5)
trial("slot_1_move", 40657, 7.25)
trial("slot_2_insert", 40658, 3.5)

clear_markers()
reaper.AddProjectMarker2(0, true, 2, 8, "1", -1, 0)
trial("region_with_numbered_name", 40657, 9)

clear_markers()
reaper.AddProjectMarker2(0, false, 2, 0, "1", -1, 0)
reaper.AddProjectMarker2(0, false, 7, 0, "1", -1, 0)
trial("duplicate_slot_names", 40657, 10)

clear_markers()
reaper.AddProjectMarker2(0, false, 2, 0, "2", -1, 0)
reaper.AddProjectMarker2(0, false, 7, 0, "1", -1, 0)
trial("slot_1_with_crossed_names", 40657, 10)

clear_markers()
reaper.AddProjectMarker2(0, false, 2, 0, "alpha", -1, 0)
reaper.AddProjectMarker2(0, false, 7, 0, "beta", -1, 0)
trial("slot_1_with_arbitrary_names", 40657, 10)

clear_markers()
trial("slot_10_insert", 40656, 11)

assert(reaper.Main_SaveProjectEx(0, root .. "numbered-markers-probe.rpp", 0))
reaper.defer(function()
  reaper.Main_OnCommand(40004, 0)
end)
