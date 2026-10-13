-- Run only in an isolated REAPER 7.82 configuration and disposable project.
-- During phase 1 drag the left Loop point from 3.000s to 2.500s. During phase 2
-- drag the right Loop point from 4.000s to 4.500s. The probe restores the
-- unlinked baseline between the two manual gestures and restores the original
-- link preference with empty ranges when it finishes.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local log = assert(io.open(root .. "loop-point-drag.tsv", "w"))
local initial_linked = reaper.GetToggleCommandState(40621) == 1

local function linked()
  return reaper.GetToggleCommandState(40621) == 1
end

local function set_link(enabled)
  if linked() ~= enabled then
    reaper.Main_OnCommand(enabled and 40749 or 40750, 0)
  end
end

local function set_range(is_loop, start_time, end_time)
  reaper.GetSet_LoopTimeRange2(0, true, is_loop, start_time, end_time, false)
end

local function state()
  local ts_start, ts_end = reaper.GetSet_LoopTimeRange2(0, false, false, 0, 0, false)
  local loop_start, loop_end = reaper.GetSet_LoopTimeRange2(0, false, true, 0, 0, false)
  return {
    ts_start = ts_start,
    ts_end = ts_end,
    loop_start = loop_start,
    loop_end = loop_end,
    linked = linked(),
  }
end

local function format_state(value)
  return string.format("%.3f\t%.3f\t%.3f\t%.3f\t%s",
    value.ts_start, value.ts_end, value.loop_start, value.loop_end,
    value.linked and "linked" or "unlinked")
end

local function matches(left, right)
  return math.abs(left.ts_start - right.ts_start) < 0.000001
    and math.abs(left.ts_end - right.ts_end) < 0.000001
    and math.abs(left.loop_start - right.loop_start) < 0.000001
    and math.abs(left.loop_end - right.loop_end) < 0.000001
    and left.linked == right.linked
end

local function reset_ranges()
  set_link(false)
  set_range(false, 1, 2)
  set_range(true, 3, 4)
  reaper.UpdateArrange()
end

local function cleanup()
  set_link(false)
  set_range(false, 0, 0)
  set_range(true, 0, 0)
  set_link(initial_linked)
  reaper.SetEditCurPos2(0, 0, false, false)
  reaper.UpdateArrange()
  log:close()
end

assert(reaper.CountTracks(0) > 0, "Insert a track so the Arrange ruler is visible")
reset_ranges()
reaper.SetEditCurPos2(0, 0, false, false)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 0, 6)
log:write("case\ttime_selection_start\ttime_selection_end\tloop_start\tloop_end\tlink\n")
log:write("before_left\t", format_state(state()), "\n")
log:flush()

local phase = "left"
local baseline = state()
local phase_started = reaper.time_precise()

local function poll()
  local current = state()
  if not matches(current, baseline) then
    log:write("after_", phase, "\t", format_state(current), "\n")
    log:flush()
    if phase == "left" then
      reset_ranges()
      phase = "right"
      baseline = state()
      phase_started = reaper.time_precise()
      log:write("before_right\t", format_state(baseline), "\n")
      log:flush()
      reaper.defer(poll)
    else
      cleanup()
    end
  elseif reaper.time_precise() - phase_started > 30 then
    log:write("timeout_", phase, "\t", format_state(current), "\n")
    cleanup()
  else
    reaper.defer(poll)
  end
end

poll()
