-- Run only in an isolated REAPER 7.82 configuration and disposable project.
-- Each ready_* phase waits for one manual mouse gesture and records both ranges.
-- Use the gesture named in the phase: plain or Shift-drag, on the bracket body
-- or at empty bottom-lane time 5.0s; move to 4.0s or 5.5s respectively.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

if reaper.CountTracks(0) == 0 then
  reaper.InsertTrackAtIndex(0, true)
end

local function get_range(is_loop)
  local start_time, end_time = reaper.GetSet_LoopTimeRange2(0, false, is_loop, 0, 0, false)
  return start_time, end_time
end

local function set_range(is_loop, start_time, end_time)
  reaper.GetSet_LoopTimeRange2(0, true, is_loop, start_time, end_time, false)
end

local function linked()
  return reaper.GetToggleCommandState(40621) == 1
end

local function set_link(enabled)
  if linked() ~= enabled then
    reaper.Main_OnCommand(enabled and 40749 or 40750, 0)
  end
end

local function snap_enabled()
  return reaper.GetToggleCommandState(1157) == 1
end

local function set_snap(enabled)
  if snap_enabled() ~= enabled then
    reaper.Main_OnCommand(1157, 0)
  end
end

local function state()
  local ts_start, ts_end = get_range(false)
  local loop_start, loop_end = get_range(true)
  return {
    ts_start = ts_start,
    ts_end = ts_end,
    loop_start = loop_start,
    loop_end = loop_end,
    linked = linked(),
    snap = snap_enabled(),
    cursor = reaper.GetCursorPosition(),
  }
end

local function format_state(value)
  return string.format("%.6f\t%.6f\t%.6f\t%.6f\t%s\t%s\t%.6f",
    value.ts_start, value.ts_end, value.loop_start, value.loop_end,
    value.linked and "linked" or "unlinked", value.snap and "snap" or "no-snap",
    value.cursor)
end

local function same(left, right)
  return math.abs(left.ts_start - right.ts_start) < 0.000001
    and math.abs(left.ts_end - right.ts_end) < 0.000001
    and math.abs(left.loop_start - right.loop_start) < 0.000001
    and math.abs(left.loop_end - right.loop_end) < 0.000001
    and left.linked == right.linked
    and left.snap == right.snap
    and math.abs(left.cursor - right.cursor) < 0.000001
end

local log = assert(io.open(root .. "loop-point-ruler-gestures.tsv", "w"))
local original = state()
local original_cursor = reaper.GetCursorPosition()
local view_start, view_end = reaper.GetSet_ArrangeView2(0, false, 0, 0, 0, 0)
local specs = {
  { "plain-drag-unlinked-loop-body", 3, 4, 1, 2, false, false },
  { "plain-drag-unlinked-empty-lane", 0, 0, 0, 0, false, false },
  { "plain-click-unlinked-empty-lane", 0, 0, 0, 0, false, false },
  { "shift-move-unlinked-loop-body", 3, 4, 1, 2, false, false },
  { "shift-create-unlinked-empty-lane", 0, 0, 0, 0, false, false },
  { "shift-move-unlinked-loop-body-snap", 3, 4, 1, 2, false, true },
  { "shift-create-linked-empty-lane", 0, 0, 0, 0, true, false },
}

local function reset(spec)
  set_link(false)
  set_range(false, spec[4], spec[5])
  set_range(true, spec[2], spec[3])
  set_link(spec[6])
  set_snap(spec[7])
  reaper.SetEditCurPos2(0, 0, false, false)
  reaper.GetSet_ArrangeView2(0, true, 0, 0, 0, 6)
  reaper.UpdateArrange()
end

local index = 1
local baseline
local phase_started

local function cleanup()
  set_link(false)
  set_range(false, original.ts_start, original.ts_end)
  set_range(true, original.loop_start, original.loop_end)
  set_link(original.linked)
  set_snap(original.snap)
  reaper.SetEditCurPos2(0, original_cursor, false, false)
  if view_end > view_start then
    reaper.GetSet_ArrangeView2(0, true, 0, 0, view_start, view_end)
  end
  reaper.UpdateArrange()
  log:close()
end

local function begin_phase()
  local spec = specs[index]
  if not spec then
    cleanup()
    return
  end
  reset(spec)
  baseline = state()
  phase_started = reaper.time_precise()
  log:write("ready_", spec[1], "\t", format_state(baseline), "\n")
  log:flush()
  reaper.defer(poll)
end

function poll()
  local current = state()
  local spec = specs[index]
  if not same(current, baseline) then
    log:write("after_", spec[1], "\t", format_state(current), "\n")
    log:flush()
    index = index + 1
    begin_phase()
  elseif reaper.time_precise() - phase_started > 30 then
    log:write("timeout_", spec[1], "\t", format_state(current), "\n")
    log:flush()
    index = index + 1
    begin_phase()
  else
    reaper.defer(poll)
  end
end

log:write("case\ttime_selection_start\ttime_selection_end\tloop_start\tloop_end\tlink\tsnap\tedit_cursor\n")
begin_phase()
