-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- Configure `pixels` and Snap in the native Nudge/Set dialog, then use the
-- command file to compare the same item at different Arrangement zoom levels.
-- Set AAADAW_REAPER_PROBE_DIR and AAADAW_SPLIT_SOURCE_PATH first.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local output = assert(io.open(root .. "item-nudge-pixels-native.tsv", "w"))
local command_path = root .. "item-nudge-pixels-command.txt"

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Nudge pixels", true)
reaper.SetOnlyTrackSelected(track)
assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert probe audio")
local item = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 1.27)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 0.5)
reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 0)
local take = assert(reaper.GetActiveTake(item))
reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", 0.2)
reaper.SetMediaItemSelected(item, true)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 0, 6)
reaper.UpdateArrange()
reaper.Undo_EndBlock2(0, "Nudge pixels probe setup", -1)

local state_before = reaper.GetProjectStateChangeCount(0)
local function write_state(label)
  local current_take = reaper.GetActiveTake(item)
  local view_start, view_end = reaper.GetSet_ArrangeView2(0, false, 0, 0, 0, 0)
  output:write(label,
    "\tstate_change_count\t", reaper.GetProjectStateChangeCount(0),
    "\tstate_change_delta\t", reaper.GetProjectStateChangeCount(0) - state_before,
    "\tposition_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
    "\tlength_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
    "\tsource_offset_s\t", current_take and string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(current_take, "D_STARTOFFS")) or "none",
    "\tselected\t", reaper.IsMediaItemSelected(item) and "1" or "0",
    "\tview_start_s\t", string.format("%.9f", view_start),
    "\tview_end_s\t", string.format("%.9f", view_end),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("project_sample_rate\t", reaper.GetSetProjectInfo(0, "PROJECT_SRATE", 0, false), "\n")
output:write("dialog_action\t41226\n")
output:write("nudge_right_action\t41249\n")
output:write("fixture\tselected non-loop Audio item at 1.270s; length 0.500s; source offset 0.200s; Arrangement view set to 0-6s\n")
write_state("baseline")
reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()

local function poll_command()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    local action, label, first, last
    if command then action, label, first, last = command:match("^([^|]+)|?([^|]*)|?([^|]*)|?([^|]*)$") end
    if action == "reset" then
      reaper.SetMediaItemInfo_Value(item, "D_POSITION", 1.27)
      reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 0.5)
      reaper.SetMediaItemTakeInfo_Value(assert(reaper.GetActiveTake(item)), "D_STARTOFFS", 0.2)
      reaper.SetMediaItemSelected(item, true)
      reaper.UpdateItemInProject(item)
      reaper.UpdateArrange()
      state_before = reaper.GetProjectStateChangeCount(0)
      write_state("baseline_" .. label)
    elseif action == "view" then
      local view_start, view_end = tonumber(first), tonumber(last)
      assert(view_start and view_end and view_end > view_start, "view command requires start and end seconds")
      reaper.GetSet_ArrangeView2(0, true, 0, 0, view_start, view_end)
      reaper.UpdateArrange()
      write_state("view_" .. label)
    elseif action == "right" or action == "left" then
      reaper.Main_OnCommand(action == "right" and 41249 or 41250, 0)
      reaper.UpdateArrange()
      write_state("after_" .. action .. "_" .. label)
    elseif action == "snapshot" then
      write_state(label)
    elseif action == "undo" then
      reaper.Main_OnCommand(40029, 0)
      reaper.UpdateArrange()
      write_state("undo_" .. label)
    elseif action == "redo" then
      reaper.Main_OnCommand(40030, 0)
      reaper.UpdateArrange()
      write_state("redo_" .. label)
    elseif action == "done" then
      output:close()
      return
    end
  end
  reaper.defer(poll_command)
end
poll_command()
