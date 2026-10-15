-- Native REAPER 7.82 probe: absolute Set -> Contents for full-source audio.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local out = assert(io.open(root .. "item-nudge-audio-set-contents.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Audio Set Contents", true)
reaper.SetOnlyTrackSelected(track)
reaper.SetEditCurPos(2, false, false)
assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert WAV")
local item = assert(reaper.GetTrackMediaItem(track, 0))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.SetMediaItemInfo_Value(item, "D_POSITION", 2))
assert(reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 5))
assert(reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 1))
assert(reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", 0.1))
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(7, false, false)
reaper.UpdateArrange()
reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()
local function write_state(label)
  local source = reaper.GetMediaItemTake_Source(take)
  local source_length, source_is_qn = reaper.GetMediaSourceLength(source)
  out:write(table.concat({label,
    string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
    string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
    tostring(reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC")),
    string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
    string.format("%.9f", source_length), tostring(source_is_qn),
    reaper.IsMediaItemSelected(item) and "selected" or "unselected",
    tostring(reaper.GetProjectStateChangeCount(0)), tostring(reaper.Undo_CanUndo2(0) or "")}, "\t"), "\n")
  out:flush()
end
out:write("version\t", reaper.GetAppVersion(), "\n")
out:write("theme\tDefault_7.0.ReaperThemeZip\n")
out:write("source_path\t", source_path, "\n")
out:write("columns\tlabel\tposition_s\tlength_s\tloop_source\tsource_offset_s\tsource_length_s\tsource_length_is_qn\tselection\tstate_change\tundo_label\n")
write_state("before_looped")
local command_path = root .. "item-nudge-audio-set-contents-command.txt"
local function poll()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    if command == "snapshot" then write_state("snapshot") end
    if command == "loopoff" then
      reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 0)
      reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", 0.1)
      reaper.UpdateArrange()
      write_state("before_loop_off_set")
    end
    if command == "done" then out:close(); return end
  end
  reaper.defer(poll)
end
poll()
