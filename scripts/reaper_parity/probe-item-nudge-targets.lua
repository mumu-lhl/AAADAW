-- Run in an unsaved, stopped Linux REAPER 7.82 / Default 7 project.
-- Choose a target and amount in the open Nudge/Set dialog, then write a unique
-- case name to AAADAW_REAPER_PROBE_DIR/item-nudge-target-command.txt. The command
-- may be `case|source-offset-seconds|left|0` or
-- `case|source-offset-seconds|right|1`; source offset, direction and loop-source
-- flag default to 0, right and enabled. The script resets the item, runs the
-- matching Nudge action once and records the result. Target and amount are
-- intentionally driven through the real UI.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local command_path = root .. "item-nudge-target-command.txt"
local output = assert(io.open(root .. "item-nudge-targets-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Nudge target probe", true)
reaper.SetOnlyTrackSelected(track)
reaper.SetEditCurPos(2, false, false)
assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert probe audio")
local item = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 2)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 10)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(7, false, false)
reaper.UpdateArrange()
reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("dialog_action\t41226\tItem edit: Nudge/Set...\n")
output:write("nudge_right_action\t41249\tItem edit: Nudge right by last nudge dialog settings\n")
output:flush()

local function write_case(name, state_change_before)
  output:write(name, "_state_change_count\t", reaper.GetProjectStateChangeCount(0), "\n")
  output:write(name, "_state_change_delta\t",
    reaper.GetProjectStateChangeCount(0) - state_change_before, "\n")
  output:write(name, "_edit_cursor\t", string.format("%.9f", reaper.GetCursorPosition()), "\n")
  output:write(name, "_track_item_count\t", reaper.CountTrackMediaItems(track), "\n")
  for index = 0, reaper.CountTrackMediaItems(track) - 1 do
    local current = reaper.GetTrackMediaItem(track, index)
    local take = reaper.GetActiveTake(current)
    output:write(name, "_loop_source\t", reaper.GetMediaItemInfo_Value(current, "B_LOOPSRC"), "\n")
    output:write(name, "_item\t", index,
      "\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(current, "D_POSITION")),
      "\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(current, "D_LENGTH")),
      "\t", take and string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")) or "none",
      "\t", reaper.IsMediaItemSelected(current) and "selected" or "unselected", "\n")
  end
  output:write(name, "_undo_label\t", tostring(reaper.Undo_CanUndo2(0)), "\n")
  output:flush()
end

local function reset_probe_item(source_offset_seconds, loop_source_enabled)
  for index = reaper.CountTrackMediaItems(track) - 1, 1, -1 do
    reaper.DeleteTrackMediaItem(track, reaper.GetTrackMediaItem(track, index))
  end
  item = assert(reaper.GetTrackMediaItem(track, 0), "probe item was removed")
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", 2)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 10)
  reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", loop_source_enabled and 1 or 0)
  local take = reaper.GetActiveTake(item)
  if take then
    reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", source_offset_seconds or 0)
  end
  reaper.SetMediaItemSelected(item, true)
  reaper.SetEditCurPos(7, false, false)
  reaper.UpdateArrange()
end

local function poll_command()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    if command and command ~= "" then
      local name, source_offset_seconds, direction, loop_source =
        command:match("^([^|]+)|([%d%.]+)|([^|]+)|([01])$")
      if not name then
        name, source_offset_seconds, direction = command:match("^([^|]+)|([%d%.]+)|([^|]+)$")
      end
      if not name then name, source_offset_seconds = command:match("^([^|]+)|([%d%.]+)$") end
      if not name then name = command end
      direction = direction == "left" and "left" or "right"
      reset_probe_item(tonumber(source_offset_seconds) or 0, loop_source ~= "0")
      local action = direction == "left" and 41250 or 41249
      output:write(name, "_nudge_action\t", action, "\n")
      local state_change_before = reaper.GetProjectStateChangeCount(0)
      reaper.Main_OnCommand(action, 0)
      reaper.UpdateArrange()
      write_case(name, state_change_before)
    end
  end
  reaper.defer(poll_command)
end
poll_command()
