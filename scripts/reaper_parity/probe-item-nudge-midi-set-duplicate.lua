-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- Set Mode=Set, Target=duplicate, and a target in the visible Nudge/Set
-- dialog. Write snapshot_absolute/snapshot_relative/snapshot_snap/undo/redo/
-- done to the command file in AAADAW_REAPER_PROBE_DIR.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "Probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "item-nudge-midi-set-duplicate-native.tsv", "w"))
reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Set Duplicate MIDI", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 3.9, 8.9, false))
assert(reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 1))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 960, 1920, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(take, false, false, 2880, 3840, 0, 64, 108, false))
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(3.9, false, false)
reaper.UpdateArrange()
reaper.Undo_EndBlock2(0, "Probe setup", -1)

local function write_state(label)
  output:write(label,
    "\titem_count\t", reaper.CountMediaItems(0),
    "\tstate_change_count\t", reaper.GetProjectStateChangeCount(0),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""), "\n")
  for index = 0, reaper.CountMediaItems(0) - 1 do
    local current = assert(reaper.GetMediaItem(0, index))
    local current_take = reaper.GetActiveTake(current)
    local note_count = 0
    local notes = {}
    if current_take then
      local note_ok
      note_ok, note_count = reaper.MIDI_CountEvts(current_take)
      assert(note_ok)
      for note_index = 0, note_count - 1 do
        local ok, selected, muted, start_ppq, end_ppq, channel, pitch, velocity =
          reaper.MIDI_GetNote(current_take, note_index)
        assert(ok)
        notes[#notes + 1] = table.concat({
          note_index,
          selected and "1" or "0",
          muted and "1" or "0",
          string.format("%.3f", start_ppq),
          string.format("%.3f", end_ppq),
          channel,
          pitch,
          velocity,
        }, ",")
      end
    end
    output:write("item\t", index,
      "\tposition_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(current, "D_POSITION")),
      "\tlength_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(current, "D_LENGTH")),
      "\tloop_source\t", reaper.GetMediaItemInfo_Value(current, "B_LOOPSRC"),
      "\tsource_offset_s\t", current_take and string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(current_take, "D_STARTOFFS")) or "n/a",
      "\tselected\t", reaper.IsMediaItemSelected(current) and "1" or "0",
      "\tnotes\t", table.concat(notes, ";"), "\n")
  end
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("dialog\tMode Set; Target duplicate; absolute target 4.150 seconds; Snap off\n")
write_state("before")
reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()

local command_path = root .. "item-nudge-midi-set-duplicate-command.txt"
local function poll_command()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    if command == "snapshot_absolute" then
      write_state("after_absolute_set")
    elseif command == "snapshot_relative" then
      write_state("after_relative_set")
    elseif command == "snapshot_snap" then
      write_state("after_snap_set")
    elseif command == "undo" then
      reaper.Main_OnCommand(40029, 0)
      reaper.UpdateArrange()
      write_state("after_undo")
    elseif command == "redo" then
      reaper.Main_OnCommand(40030, 0)
      reaper.UpdateArrange()
      write_state("after_redo")
    elseif command == "done" then
      output:close()
      return
    end
  end
  reaper.defer(poll_command)
end
poll_command()
