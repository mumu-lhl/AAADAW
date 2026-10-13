-- Run only in a fresh, unsaved project and isolated REAPER 7.82 configuration.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use an empty disposable project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local log = assert(io.open(root .. "region-delete-near-cursor.tsv", "w"))
log:write("version\t", reaper.GetAppVersion(), "\n")
log:write("theme\t", reaper.GetLastColorThemeFile(), "\n")
log:write("action_40615\t", reaper.kbd_getTextFromCmd(40615, 0) or "<missing>", "\n")
log:write("case\tedit_cursor\tregions_before\tregions_after\n")

local function entries()
  local count = reaper.CountProjectMarkers(0)
  local result = {}
  for index = 0, count - 1 do
    local valid, is_region, position, region_end, name, number =
      reaper.EnumProjectMarkers3(0, index)
    if valid ~= 0 then
      result[#result + 1] = {
        is_region = is_region,
        position = position,
        region_end = region_end,
        name = name,
        number = number,
      }
    end
  end
  return result
end

local function clear()
  local current = entries()
  table.sort(current, function(left, right) return left.number > right.number end)
  for _, item in ipairs(current) do
    reaper.DeleteProjectMarker(0, item.number, item.is_region)
  end
end

local function describe()
  local result = {}
  for _, item in ipairs(entries()) do
    if item.is_region then
      result[#result + 1] = string.format("%s#%d[%.6f,%.6f)", item.name, item.number,
        item.position, item.region_end)
    else
      result[#result + 1] = string.format("marker:%s#%d@%.6f", item.name, item.number,
        item.position)
    end
  end
  return #result > 0 and table.concat(result, ";") or "-"
end

local cases = {}
for _, cursor in ipairs({
  8, 9, 9.001, 9.5, 9.999, 10, 10.001, 10.5, 11, 15, 19, 19.999, 20, 20.001,
  20.5, 21, 21.001, 22, 25,
}) do
  cases[#cases + 1] = {
    label = string.format("single_%+.3f", cursor - 10),
    regions = {{10, 20, "single"}},
    cursor = cursor,
  }
end
cases[#cases + 1] = { label = "nearest_of_two", regions = {{10, 12, "first"}, {18, 20, "second"}}, cursor = 14.9 }
cases[#cases + 1] = { label = "equidistant_ranges", regions = {{10, 12, "first"}, {18, 20, "second"}}, cursor = 15 }
cases[#cases + 1] = { label = "overlap", regions = {{10, 20, "wide"}, {14, 18, "narrow"}}, cursor = 15 }
cases[#cases + 1] = { label = "overlap_outer_end_closer", regions = {{10, 19, "outer"}, {14, 20, "inner"}}, cursor = 18 }
cases[#cases + 1] = { label = "overlap_latest_start_is_longer", regions = {{10, 18, "short"}, {14, 25, "long"}}, cursor = 15 }
cases[#cases + 1] = { label = "overlap_boundary_tie", regions = {{10, 18, "first"}, {12, 20, "second"}}, cursor = 15 }
cases[#cases + 1] = { label = "overlap_same_bounds", regions = {{10, 20, "first"}, {10, 20, "second"}}, cursor = 15 }
cases[#cases + 1] = { label = "overlap_same_start", regions = {{10, 20, "first"}, {10, 18, "second"}}, cursor = 15 }
cases[#cases + 1] = { label = "adjacent_shared_start", regions = {{10, 15, "first"}, {15, 20, "second"}}, cursor = 15 }
cases[#cases + 1] = { label = "region_and_marker", regions = {{10, 20, "target"}}, markers = {{15, "unrelated"}}, cursor = 15 }
cases[#cases + 1] = { label = "active_time_selection", regions = {{10, 20, "selected"}}, cursor = 15, selection = {10, 20} }
cases[#cases + 1] = { label = "marker_only", markers = {{15, "unrelated"}}, cursor = 15 }
cases[#cases + 1] = { label = "no_region", regions = {}, cursor = 15 }

local case_index = 0
local function next_trial()
  case_index = case_index + 1
  local case = cases[case_index]
  if not case then
    clear()
    log:close()
    assert(reaper.Main_SaveProjectEx(0, root .. "region-delete-near-cursor-probe.rpp", 0))
    reaper.defer(function() reaper.Main_OnCommand(40004, 0) end)
    return
  end

  clear()
  if case.selection then
    reaper.GetSet_LoopTimeRange2(0, true, false, case.selection[1], case.selection[2], false)
  else
    reaper.GetSet_LoopTimeRange2(0, true, false, 0, 0, false)
  end
  for _, region in ipairs(case.regions or {}) do
    reaper.AddProjectMarker2(0, true, region[1], region[2], region[3], -1, 0)
  end
  for _, marker in ipairs(case.markers or {}) do
    reaper.AddProjectMarker2(0, false, marker[1], 0, marker[2], -1, 0)
  end
  reaper.SetEditCurPos2(0, case.cursor, false, false)
  reaper.UpdateArrange()

  reaper.defer(function()
    log:write(case.label, "\t", string.format("%.9f", reaper.GetCursorPosition()), "\t",
      describe(), "\t")
    reaper.Main_OnCommand(40615, 0)
    log:write(describe(), "\n")
    log:flush()
    reaper.defer(next_trial)
  end)
end
next_trial()
