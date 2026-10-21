-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- Probe Set edge targets with selected loop-source Audio items at 2s and 6s.
-- In the real Nudge/Set dialog, run Left Trim/Left Edge at 4s, End Position
-- and Right Trim at 14s, and End Position with Relative Set enabled at 14s.
-- Relative Set still takes an absolute destination for the last selected edge.
-- Write reset|case or snapshot|case to the command file to record states.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local output = assert(io.open(root .. "item-nudge-audio-set-edge-multiselect-native.tsv", "w"))
local command_path = root .. "item-nudge-audio-set-edge-multiselect-command.txt"

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Set edge multi-select", true)
reaper.SetOnlyTrackSelected(track)
assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert first probe audio item")
assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert second probe audio item")
local items = {
  assert(reaper.GetTrackMediaItem(track, 0)),
  assert(reaper.GetTrackMediaItem(track, 1)),
}
for i, item in ipairs(items) do
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", i == 1 and 2 or 6)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 10)
  reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 1)
  local take = assert(reaper.GetActiveTake(item))
  reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", 0)
  reaper.SetMediaItemSelected(item, true)
end
reaper.SetEditCurPos(7, false, false)
reaper.UpdateArrange()
reaper.Undo_EndBlock2(0, "Set edge multi-select probe setup", -1)

local state_before = reaper.GetProjectStateChangeCount(0)
local function write_state(label)
  output:write(label,
    "\tstate_change_count\t", reaper.GetProjectStateChangeCount(0),
    "\tstate_change_delta\t", reaper.GetProjectStateChangeCount(0) - state_before,
    "\titem_count\t", reaper.CountMediaItems(0),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""), "\n")
  for i, item in ipairs(items) do
    local take = reaper.GetActiveTake(item)
    output:write(label, "_item_", i,
      "\tposition_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
      "\tlength_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
      "\tloop_source\t", reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC"),
      "\tsource_offset_s\t", take and string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")) or "none",
      "\tselected\t", reaper.IsMediaItemSelected(item) and "1" or "0", "\n")
  end
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("dialog_action\t41226\n")
output:write("fixture\ttwo selected loop-source Audio items at 2s and 6s; each 10s long; source offsets 0s\n")
write_state("baseline")
reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()

local function poll_command()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    local action, label
    if command then action, label = command:match("^([^|]+)|?([^|]*)$") end
    if action == "reset" then
      reaper.Undo_BeginBlock2(0)
      for i, item in ipairs(items) do
        reaper.SetMediaItemInfo_Value(item, "D_POSITION", i == 1 and 2 or 6)
        reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 10)
        reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 1)
        local take = reaper.GetActiveTake(item)
        if take then reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", 0) end
        reaper.SetMediaItemSelected(item, true)
        reaper.UpdateItemInProject(item)
      end
      reaper.Undo_EndBlock2(0, "Set edge multi-select probe reset", -1)
      state_before = reaper.GetProjectStateChangeCount(0)
      reaper.UpdateArrange()
      write_state("baseline_" .. label)
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
