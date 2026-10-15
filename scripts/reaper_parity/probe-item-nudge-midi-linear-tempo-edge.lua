-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- Set Target=left trim, 250 milliseconds, Snap off in Nudge/Set Items.
-- Write right, left, snapshot|label, or done to the command file.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local output = assert(io.open(root .. "item-nudge-midi-linear-tempo-edge-native.tsv", "w"))
local ramp_ok = reaper.SetTempoTimeSigMarker(0, -1, 0.0, -1, -1, 120.0, 4, 4, true)
local end_ok = reaper.SetTempoTimeSigMarker(0, -1, -1, 2, 0.0, 60.0, 4, 4, false)
assert(ramp_ok and end_ok, "could not create the 120-to-60 BPM linear tempo ramp")
reaper.UpdateTimeline()

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Mixed linear-tempo MIDI edge Nudge", true)
reaper.SetOnlyTrackSelected(track)
assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert Audio probe item")
local audio_item = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(audio_item, "D_POSITION", 2)
reaper.SetMediaItemInfo_Value(audio_item, "D_LENGTH", 10)
reaper.SetMediaItemInfo_Value(audio_item, "B_LOOPSRC", 0)
local audio_take = assert(reaper.GetActiveTake(audio_item))
reaper.SetMediaItemTakeInfo_Value(audio_take, "D_STARTOFFS", 0)

local midi_item = assert(reaper.CreateNewMIDIItemInProj(track, 3.9, 8.9, false))
reaper.SetMediaItemInfo_Value(midi_item, "B_LOOPSRC", 0)
local midi_take = assert(reaper.GetActiveTake(midi_item))
assert(reaper.MIDI_InsertNote(midi_take, false, false, 960, 1920, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(midi_take, false, false, 2880, 3840, 0, 64, 108, false))
assert(reaper.MIDI_InsertCC(midi_take, false, false, 4800, 0xB0, 0, 1, 80))
assert(reaper.MIDI_InsertCC(midi_take, false, false, 5760, 0xE0, 0, 0, 80))
reaper.MIDI_Sort(midi_take)
reaper.SetMediaItemSelected(audio_item, true)
reaper.SetMediaItemSelected(midi_item, true)
reaper.SetEditCurPos(3.9, false, false)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 0, 12)
reaper.UpdateArrange()
reaper.Undo_EndBlock2(0, "Mixed linear-tempo MIDI edge probe setup", -1)

local state_before = reaper.GetProjectStateChangeCount(0)
local function write_state(label, state_change_delta)
  output:write(label,
    "\tstate_change_count\t", reaper.GetProjectStateChangeCount(0),
    "\tstate_change_delta\t", tostring(state_change_delta or 0),
    "\titem_count\t", reaper.CountMediaItems(0),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""), "\n")
  for index = 0, reaper.CountMediaItems(0) - 1 do
    local item = assert(reaper.GetMediaItem(0, index))
    local take = assert(reaper.GetActiveTake(item))
    local is_midi = reaper.TakeIsMIDI(take)
    local source = reaper.GetMediaItemTake_Source(take)
    local source_offset = reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")
    local notes, controllers, midi_counts = "", "", "none"
    if is_midi then
      local ok, note_count, cc_count, text_count = reaper.MIDI_CountEvts(take)
      assert(ok)
      midi_counts = table.concat({ note_count, cc_count, text_count }, ",")
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
    local position = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
    local length = reaper.GetMediaItemInfo_Value(item, "D_LENGTH")
    local source_length = source and reaper.GetMediaSourceLength(source)
    local ppq_at_item_start = is_midi
      and string.format("%.3f", reaper.MIDI_GetPPQPosFromProjTime(take, position))
      or "none"
    output:write(label, "_item\t", index,
      "\ttype\t", is_midi and "midi" or "audio",
      "\tposition_s\t", string.format("%.9f", position),
      "\tlength_s\t", string.format("%.9f", length),
      "\tloop_source\t", reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC"),
      "\tsource_offset_s\t", string.format("%.9f", source_offset),
      "\tsource_length_native_units\t", source_length and string.format("%.9f", source_length) or "none",
      "\tppq_at_item_start\t", ppq_at_item_start,
      "\tselected\t", reaper.IsMediaItemSelected(item) and "1" or "0",
      "\tnotes\t", notes,
      "\tcontrollers\t", controllers,
      "\tmidi_counts\t", midi_counts, "\n")
  end
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("tempo_map\t120 BPM at project start; linear to 60 BPM at measure 3 (5.333333 seconds)\n")
for index = 0, reaper.CountTempoTimeSigMarkers(0) - 1 do
  local ok, timepos, measurepos, beatpos, bpm, numerator, denominator, linear =
    reaper.GetTempoTimeSigMarker(0, index)
  assert(ok)
  output:write("tempo_marker\t", index,
    "\ttime_s\t", string.format("%.9f", timepos),
    "\tmeasure\t", measurepos,
    "\tbeat\t", string.format("%.6f", beatpos),
    "\tbpm\t", string.format("%.6f", bpm),
    "\tmeter\t", numerator, "/", denominator,
    "\tlinear\t", linear and "1" or "0", "\n")
end
output:write("probe\tMixed non-loop Audio at 2s and non-loop MIDI at 3.9s inside a 120-to-60 BPM linear ramp; both selected\n")
output:write("nudge\tTarget Left Trim; 250 milliseconds; Snap off; set in the actual Nudge/Set dialog\n")
output:write("actions\t41249 Nudge right; 41250 Nudge left\n")
write_state("before", 0)
reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()

local command_path = root .. "item-nudge-midi-linear-tempo-edge-command.txt"
local function poll_command()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    if command == "left" or command == "right" then
      local action = command == "left" and 41250 or 41249
      local before = reaper.GetProjectStateChangeCount(0)
      reaper.Main_OnCommand(action, 0)
      reaper.UpdateArrange()
      write_state("after_" .. command,
        reaper.GetProjectStateChangeCount(0) - before)
    elseif command then
      local action, label = command:match("^([^|]+)|?(.*)$")
      if action == "snapshot" then
        write_state(label, 0)
      elseif action == "done" then
        output:close()
        return
      end
    end
  end
  reaper.defer(poll_command)
end
poll_command()
