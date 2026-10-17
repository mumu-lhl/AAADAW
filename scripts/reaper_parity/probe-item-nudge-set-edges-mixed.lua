-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- Use the real Nudge/Set dialog, then write snapshot|case and done to
-- AAADAW_REAPER_PROBE_DIR/item-nudge-set-edges-mixed-command.txt.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local output = assert(io.open(root .. "item-nudge-set-edges-mixed-native.tsv", "w"))
local command_path = root .. "item-nudge-set-edges-mixed-command.txt"

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Set mixed edge targets", true)
reaper.SetOnlyTrackSelected(track)
assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert probe audio")
local audio_item = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(audio_item, "D_POSITION", 2)
reaper.SetMediaItemInfo_Value(audio_item, "D_LENGTH", 10)
reaper.SetMediaItemInfo_Value(audio_item, "B_LOOPSRC", 1)
local audio_take = assert(reaper.GetActiveTake(audio_item))
reaper.SetMediaItemTakeInfo_Value(audio_take, "D_STARTOFFS", 0)
local midi_item = assert(reaper.CreateNewMIDIItemInProj(track, 2, 12, false))
assert(reaper.SetMediaItemInfo_Value(midi_item, "B_LOOPSRC", 1))
local midi_take = assert(reaper.GetActiveTake(midi_item))
assert(reaper.MIDI_InsertNote(midi_take, false, false, 960, 1920, 0, 60, 96, false))
reaper.MIDI_Sort(midi_take)
reaper.SetMediaItemSelected(audio_item, true)
reaper.SetMediaItemSelected(midi_item, true)
reaper.SetEditCurPos(7, false, false)
reaper.UpdateArrange()
reaper.Undo_EndBlock2(0, "Set mixed edge targets probe setup", -1)

local function write_state(label)
  output:write(label,
    "\tstate_change_count\t", reaper.GetProjectStateChangeCount(0),
    "\titem_count\t", reaper.CountMediaItems(0),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""), "\n")
  for index = 0, reaper.CountMediaItems(0) - 1 do
    local item = assert(reaper.GetMediaItem(0, index))
    local take = reaper.GetActiveTake(item)
    local kind = take and reaper.TakeIsMIDI(take) and "midi" or "audio"
    local source_offset = take and reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS") or 0
    local notes = ""
    if kind == "midi" then
      local ok, note_count = reaper.MIDI_CountEvts(take)
      assert(ok)
      local values = {}
      for note_index = 0, note_count - 1 do
        local note_ok, selected, muted, start_ppq, end_ppq, channel, pitch, velocity =
          reaper.MIDI_GetNote(take, note_index)
        assert(note_ok)
        values[#values + 1] = table.concat({
          note_index, selected and "1" or "0", muted and "1" or "0",
          string.format("%.3f", start_ppq), string.format("%.3f", end_ppq),
          channel, pitch, velocity,
        }, ",")
      end
      notes = table.concat(values, ";")
    end
    output:write(label, "_item\t", index,
      "\ttype\t", kind,
      "\tposition_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
      "\tlength_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
      "\tloop_source\t", reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC"),
      "\tsource_offset_s\t", string.format("%.9f", source_offset),
      "\tselected\t", reaper.IsMediaItemSelected(item) and "1" or "0",
      "\tnotes\t", notes, "\n")
  end
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("fixture\tselected loop-source Audio and MIDI items at 2.000s; both 10.000s; MIDI note 960-1920 PPQ\n")
write_state("before")
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
    if action == "snapshot" then
      write_state(label)
    elseif action == "done" then
      output:close()
      return
    end
  end
  reaper.defer(poll_command)
end
poll_command()
