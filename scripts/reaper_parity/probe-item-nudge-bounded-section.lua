-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- Copy a bounded source loop, disable looping, then let the operator choose
-- Contents / 250 ms in the native Nudge dialog and trigger this probe through
-- AAADAW_REAPER_PROBE_DIR/bounded-nudge-command.txt.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local section_start = tonumber(os.getenv("AAADAW_PROBE_SECTION_START")) or 0
local section_end = tonumber(os.getenv("AAADAW_PROBE_SECTION_END")) or 3
assert(section_start >= 0 and section_end > section_start, "invalid bounded section range")
local output = assert(io.open(root .. "item-nudge-bounded-section-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Bounded nudge probe", true)
reaper.SetOnlyTrackSelected(track)
reaper.SetEditCurPos(0, false, false)
assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert probe audio")
local source_item = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(source_item, "D_LENGTH", 10)
reaper.SetMediaItemSelected(source_item, true)
reaper.GetSet_LoopTimeRange2(0, true, false, section_start, section_end, false)
reaper.Main_OnCommand(40014, 0) -- Item: Copy loop of selected area of audio items
reaper.SetEditCurPos(12, false, false)
reaper.Main_OnCommand(42398, 0) -- Item: Paste items/tracks
local bounded_item = assert(reaper.GetTrackMediaItem(track, 1), "bounded loop paste is missing")
local take = assert(reaper.GetActiveTake(bounded_item), "bounded loop take is missing")
local source = reaper.GetMediaItemTake_Source(take)
local section_ok, section_start, section_length, section_reversed =
  reaper.PCM_Source_GetSectionInfo(source, 0, 0, false)
assert(section_ok and section_length > 0, "pasted item should use a bounded source section")

reaper.SetMediaItemInfo_Value(bounded_item, "B_LOOPSRC", 0)
reaper.SetMediaItemInfo_Value(bounded_item, "D_LENGTH", 3)
reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", 0.1)
for index = 0, reaper.CountTrackMediaItems(track) - 1 do
  reaper.SetMediaItemSelected(reaper.GetTrackMediaItem(track, index), false)
end
reaper.SetMediaItemSelected(bounded_item, true)
reaper.SetEditCurPos(13, false, false)
reaper.UpdateArrange()
reaper.Main_OnCommand(41226, 0) -- Item edit: Nudge/Set...

local function write_state(label, state_change_delta)
  local current_take = reaper.GetActiveTake(bounded_item)
  local current_source = current_take and reaper.GetMediaItemTake_Source(current_take)
  local current_section_ok, current_section_start, current_section_length, current_reversed =
    reaper.PCM_Source_GetSectionInfo(current_source, 0, 0, false)
  output:write(label,
    "\tposition\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(bounded_item, "D_POSITION")),
    "\tlength\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(bounded_item, "D_LENGTH")),
    "\tsource_offset\t", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(current_take, "D_STARTOFFS")),
    "\tloop_source\t", reaper.GetMediaItemInfo_Value(bounded_item, "B_LOOPSRC"),
    "\tsection\t", tostring(current_section_ok),
    "\t", string.format("%.9f", current_section_start),
    "\t", string.format("%.9f", current_section_length),
    "\t", tostring(current_reversed),
    "\tstate_change_delta\t", tostring(state_change_delta or 0), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("dialog_action\t41226\tItem edit: Nudge/Set...\n")
output:write("nudge_right_action\t41249\tItem edit: Nudge right by last nudge dialog settings\n")
output:write("nudge_left_action\t41250\tItem edit: Nudge left by last nudge dialog settings\n")
output:write("probe\tbounded source section, loop disabled\n")
output:write("selection\t", string.format("%.9f", section_start),
  "\t", string.format("%.9f", section_end), "\n")
output:write("section_reversed_before\t", tostring(section_reversed), "\n")
write_state("before", 0)

local command_path = root .. "bounded-nudge-command.txt"
local function poll_command()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    if command == "left" or command == "right" then
      local action = command == "left" and 41250 or 41249
      local state_change_before = reaper.GetProjectStateChangeCount(0)
      reaper.Main_OnCommand(action, 0)
      reaper.UpdateArrange()
      write_state("after_" .. command,
        reaper.GetProjectStateChangeCount(0) - state_change_before)
    end
  end
  reaper.defer(poll_command)
end
poll_command()
