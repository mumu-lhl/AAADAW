-- Owned /tmp REAPER project only. Never run against a personal/open project.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use the disposable marker probe project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path:match("numbered%-markers%-probe%.rpp$"), "Use the owned scratch project")

local root = reaper.GetProjectPath("") .. "/"
local log = assert(io.open(root .. "project-marker-id-slots-native.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("case\tcursor\taction_id\tmarkers\n")

local function markers()
  local marker_count, region_count = reaper.CountProjectMarkers(0)
  local result = {}
  for index = 0, marker_count + region_count - 1 do
    local valid, is_region, position, _, name, marker_number =
      reaper.EnumProjectMarkers3(0, index)
    if valid ~= 0 then
      result[#result + 1] = {
        is_region = is_region,
        position = position,
        name = name,
        number = marker_number,
      }
    end
  end
  table.sort(result, function(left, right)
    if left.position == right.position then return left.number < right.number end
    return left.position < right.position
  end)
  local values = {}
  for _, marker in ipairs(result) do
    values[#values + 1] = string.format(
      "%s:%d@%.3f=%s",
      marker.is_region and "region" or "marker",
      marker.number,
      marker.position,
      marker.name
    )
  end
  return table.concat(values, ";")
end

local function clear_markers()
  local marker_count, region_count = reaper.CountProjectMarkers(0)
  local indexes = {}
  for index = 0, marker_count + region_count - 1 do
    local valid, is_region, _, _, _, marker_number = reaper.EnumProjectMarkers3(0, index)
    if valid ~= 0 then indexes[#indexes + 1] = {is_region = is_region, number = marker_number} end
  end
  for _, marker in ipairs(indexes) do
    reaper.DeleteProjectMarker(0, marker.number, marker.is_region)
  end
end

local function log_case(name, cursor, action_id)
  log:write(name, "\t", string.format("%.3f", cursor), "\t", tostring(action_id or ""), "\t", markers(), "\n")
  log:flush()
end

clear_markers()
reaper.AddProjectMarker2(0, false, 2, 0, "ID2-earlier", 2, 0)
reaper.AddProjectMarker2(0, false, 7, 0, "ID1-later", 1, 0)
log_case("slot_1_explicit_ids_before", 10, "")
reaper.SetEditCurPos2(0, 10, false, false)
reaper.Main_OnCommand(40657, 0)
log_case("slot_1_explicit_ids_after", 10, 40657)

clear_markers()
reaper.AddProjectMarker2(0, false, 2, 0, "ID5", 5, 0)
reaper.AddProjectMarker2(0, false, 7, 0, "ID10", 10, 0)
reaper.SetEditCurPos2(0, 13, false, false)
reaper.Main_OnCommand(40657, 0)
log_case("slot_1_missing_with_sparse_ids", 13, 40657)

clear_markers()
reaper.AddProjectMarker2(0, false, 2, 0, "ID5", 5, 0)
reaper.AddProjectMarker2(0, false, 7, 0, "ID10", 10, 0)
reaper.SetEditCurPos2(0, 13, false, false)
reaper.Main_OnCommand(40157, 0)
log_case("plain_insert_with_sparse_ids", 13, 40157)

clear_markers()
reaper.AddProjectMarker2(0, false, 2, 0, "ID1-earlier", 1, 0)
reaper.AddProjectMarker2(0, false, 7, 0, "ID1-later", 1, 0)
reaper.SetEditCurPos2(0, 10, false, false)
log_case("duplicate_ids_before_slot1", 10, "")
reaper.Main_OnCommand(40657, 0)
log_case("duplicate_ids_after_slot1", 10, 40657)

log:close()
reaper.Main_SaveProjectEx(0, root .. "project-marker-id-slots-probe.rpp", 0)
