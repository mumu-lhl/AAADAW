-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- Select a non-loop Audio item at 2s and a non-loop MIDI item at 6s, each 10s
-- long. Set AAADAW_MIDI_SOURCE_OFFSET to 0 or 0.1, then use the visible
-- Nudge/Set dialog for each edge target. Write snapshot|label, undo|label,
-- redo|label, or done to the command file.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local midi_source_offset = tonumber(os.getenv("AAADAW_MIDI_SOURCE_OFFSET") or "0")
assert(midi_source_offset == 0 or midi_source_offset == 0.1,
  "AAADAW_MIDI_SOURCE_OFFSET must be 0 or 0.1")
local source_start_events = os.getenv("AAADAW_SOURCE_START_EVENTS") == "1"
local suffix = (midi_source_offset == 0 and "" or "-offset")
  .. (source_start_events and "-source-start" or "")
local output = assert(io.open(
  root .. "item-nudge-mixed-nonloop-edge-nudge" .. suffix .. "-native.tsv", "w"))
local command_path = root .. "item-nudge-mixed-nonloop-edge-nudge" .. suffix .. "-command.txt"

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Mixed non-loop MIDI edge Nudge", true)
reaper.SetOnlyTrackSelected(track)

assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert Audio probe item")
local audio_item = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(audio_item, "D_POSITION", 2)
reaper.SetMediaItemInfo_Value(audio_item, "D_LENGTH", 10)
reaper.SetMediaItemInfo_Value(audio_item, "B_LOOPSRC", 0)
local audio_take = assert(reaper.GetActiveTake(audio_item))
reaper.SetMediaItemTakeInfo_Value(audio_take, "D_STARTOFFS", 0)

local midi_item = assert(reaper.CreateNewMIDIItemInProj(track, 6, 16, false))
reaper.SetMediaItemInfo_Value(midi_item, "B_LOOPSRC", 0)
local midi_take = assert(reaper.GetActiveTake(midi_item))
reaper.SetMediaItemTakeInfo_Value(midi_take, "D_STARTOFFS", midi_source_offset)
local note_start, cc_tick, pitch_bend_tick = 960, 2880, 3840
if source_start_events then
  note_start, cc_tick, pitch_bend_tick = 0, 0, 240
end
assert(reaper.MIDI_InsertNote(midi_take, false, false, note_start, note_start + 960, 0, 60, 96, false))
assert(reaper.MIDI_InsertCC(midi_take, false, false, cc_tick, 0xB0, 0, 1, 80))
assert(reaper.MIDI_InsertCC(midi_take, false, false, pitch_bend_tick, 0xE0, 0, 0, 80))
reaper.MIDI_Sort(midi_take)
reaper.SetMediaItemSelected(audio_item, true)
reaper.SetMediaItemSelected(midi_item, true)
reaper.SetEditCurPos(7, false, false)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 0, 20)
reaper.UpdateArrange()
reaper.Undo_EndBlock2(0, "Mixed non-loop Nudge edge probe setup", -1)

local state_before = reaper.GetProjectStateChangeCount(0)
local function write_state(label)
  output:write(label,
    "\tstate_change_count\t", reaper.GetProjectStateChangeCount(0),
    "\tstate_change_delta\t", reaper.GetProjectStateChangeCount(0) - state_before,
    "\titem_count\t", reaper.CountMediaItems(0),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""), "\n")
  for index = 0, reaper.CountMediaItems(0) - 1 do
    local item = assert(reaper.GetMediaItem(0, index))
    local take = reaper.GetActiveTake(item)
    local is_midi = take and reaper.TakeIsMIDI(take)
    local source = take and reaper.GetMediaItemTake_Source(take)
    local source_offset = take and reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS") or 0
    local notes, controllers, midi_counts = "", "", "none"
    if is_midi then
      local ok, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
      assert(ok)
      midi_counts = table.concat({ note_count, cc_count, text_count }, "\t")
      local note_values, cc_values = {}, {}
      for event = 0, note_count - 1 do
        local event_ok, selected, muted, start_ppq, end_ppq, channel, pitch, velocity =
          reaper.MIDI_GetNote(take, event)
        assert(event_ok)
        note_values[#note_values + 1] = table.concat({
          event, selected and "1" or "0", muted and "1" or "0",
          string.format("%.3f", start_ppq), string.format("%.3f", end_ppq),
          channel, pitch, velocity,
        }, ",")
      end
      for event = 0, cc_count - 1 do
        local event_ok, selected, muted, ppq, chanmsg, channel, msg2, msg3 =
          reaper.MIDI_GetCC(take, event)
        assert(event_ok)
        cc_values[#cc_values + 1] = table.concat({
          event, selected and "1" or "0", muted and "1" or "0",
          string.format("%.3f", ppq), chanmsg, channel, msg2, msg3,
        }, ",")
      end
      notes, controllers = table.concat(note_values, ";"), table.concat(cc_values, ";")
    end
    local source_length = source and reaper.GetMediaSourceLength(source)
    output:write(label, "_item\t", index,
      "\ttype\t", is_midi and "midi" or "audio",
      "\tposition_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
      "\tlength_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
      "\tloop_source\t", reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC"),
      "\tsource_offset_s\t", string.format("%.9f", source_offset),
      "\tsource_length_native_units\t", source_length and string.format("%.9f", source_length) or "none",
      "\tselected\t", reaper.IsMediaItemSelected(item) and "1" or "0",
      "\tnotes\t", notes,
      "\tcontrollers\t", controllers,
      "\tmidi_counts\t", midi_counts, "\n")
  end
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("dialog_action\t41226\n")
output:write("fixture\tnon-loop Audio at 2s and non-loop MIDI at 6s; each 10s; MIDI source offset ",
  string.format("%.1f", midi_source_offset),
  "s; source-start events ", source_start_events and "enabled" or "disabled",
  "; source-length API units are seconds for Audio and QN for MIDI; note, CC, and pitch bend\n")
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
    if action == "snapshot" then
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
