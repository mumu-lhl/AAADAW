-- REAPER 7.82 probe for Set -> Contents on a bounded audio source section.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local section_start = tonumber(os.getenv("AAADAW_PROBE_SECTION_START")) or 1
local section_end = tonumber(os.getenv("AAADAW_PROBE_SECTION_END")) or 4
local target_seconds = tonumber(os.getenv("AAADAW_PROBE_SET_TARGET_SECONDS")) or 3
assert(section_start >= 0 and section_end > section_start, "invalid bounded section range")
assert(target_seconds >= 0, "invalid absolute Contents target")

local output = assert(io.open(root .. "item-nudge-bounded-audio-set-contents.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Bounded Audio Set Contents", true)
reaper.SetOnlyTrackSelected(track)
reaper.SetEditCurPos(0, false, false)
assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert audio source")

local source_item = assert(reaper.GetTrackMediaItem(track, 0))
assert(reaper.SetMediaItemInfo_Value(source_item, "D_LENGTH", 10))
reaper.SetMediaItemSelected(source_item, true)
reaper.GetSet_LoopTimeRange2(0, true, false, section_start, section_end, false)
reaper.Main_OnCommand(40014, 0) -- Item: Copy loop of selected area of audio items
reaper.SetEditCurPos(12, false, false)
reaper.Main_OnCommand(42398, 0) -- Item: Paste items/tracks

local item = assert(reaper.GetTrackMediaItem(track, 1), "bounded loop paste is missing")
local take = assert(reaper.GetActiveTake(item), "bounded loop take is missing")
assert(reaper.SetMediaItemInfo_Value(item, "D_POSITION", 2))
assert(reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 5))
assert(reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 1))
assert(reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", 0.1))
for index = 0, reaper.CountTrackMediaItems(track) - 1 do
  reaper.SetMediaItemSelected(reaper.GetTrackMediaItem(track, index), false)
end
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(7, false, false)
reaper.UpdateArrange()
reaper.Undo_OnStateChange2(0, "Fixture setup")

local function write_state(label)
  item = assert(reaper.GetTrackMediaItem(track, 1), "bounded item is missing")
  local current_take = assert(reaper.GetActiveTake(item))
  local source = assert(reaper.GetMediaItemTake_Source(current_take))
  local section_ok, start, length, reversed = reaper.PCM_Source_GetSectionInfo(source, 0, 0, false)
  local source_length, source_is_qn = reaper.GetMediaSourceLength(source)
  output:write(table.concat({label,
    string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
    string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
    tostring(reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC")),
    string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(current_take, "D_STARTOFFS")),
    tostring(section_ok), string.format("%.9f", start), string.format("%.9f", length),
    tostring(reversed), string.format("%.9f", source_length), tostring(source_is_qn),
    reaper.IsMediaItemSelected(item) and "selected" or "unselected",
    tostring(reaper.GetProjectStateChangeCount(0)), tostring(reaper.Undo_CanUndo2(0) or "")}, "\t"), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("dialog_action\t41226\tItem edit: Nudge/Set...\n")
output:write("source_path\t", source_path, "\n")
output:write("target_seconds\t", string.format("%.9f", target_seconds), "\n")
output:write("columns\tlabel\tposition_s\tlength_s\tloop_source\tsource_offset_s\tsection\tsection_start_s\tsection_length_s\tsection_reversed\tsource_length_s\tsource_length_is_qn\tselection\tstate_change\tundo_label\n")
write_state("before")
reaper.Main_OnCommand(41226, 0) -- Item edit: Nudge/Set...

local command_path = root .. "item-nudge-bounded-audio-set-contents-command.txt"
local function poll()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    if command == "snapshot" then write_state("snapshot") end
    if command == "undo" then
      reaper.Main_OnCommand(40029, 0) -- Edit: Undo
      reaper.UpdateArrange()
      write_state("undo")
    end
    if command == "redo" then
      reaper.Main_OnCommand(40030, 0) -- Edit: Redo
      reaper.UpdateArrange()
      write_state("redo")
    end
    if command == "done" then output:close(); return end
  end
  reaper.defer(poll)
end
poll()
