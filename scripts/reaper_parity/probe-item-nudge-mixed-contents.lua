-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- Nudge target/amount are read from the isolated config used to launch REAPER.
-- Write `right`, `left`, `undo`, `redo`, `status`, or `done` to
-- item-nudge-mixed-command.txt after inspecting the real Nudge/Set dialog.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local audio_path = assert(os.getenv("AAADAW_MIXED_NUDGE_AUDIO_PATH"))
local output = assert(io.open(root .. "item-nudge-mixed-contents-native.tsv", "w"))

local function add_track(name)
  reaper.InsertTrackAtIndex(reaper.CountTracks(0), true)
  local track = assert(reaper.GetTrack(0, reaper.CountTracks(0) - 1))
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  return track
end

local audio_track = add_track("Contents Nudge Audio")
local midi_track = add_track("Contents Nudge MIDI")
reaper.SetOnlyTrackSelected(audio_track)
reaper.SetEditCurPos(2, false, false)
reaper.InsertMedia(audio_path, 0)
local audio_item = assert(reaper.GetTrackMediaItem(audio_track, 0))
local audio_take = assert(reaper.GetActiveTake(audio_item))
assert(reaper.SetMediaItemInfo_Value(audio_item, "D_POSITION", 2))
assert(reaper.SetMediaItemInfo_Value(audio_item, "D_LENGTH", 5))
assert(reaper.SetMediaItemInfo_Value(audio_item, "B_LOOPSRC", 1))
assert(reaper.SetMediaItemTakeInfo_Value(audio_take, "D_STARTOFFS", 0.1))

local midi_item = assert(reaper.CreateNewMIDIItemInProj(midi_track, 2, 7, false))
local midi_take = assert(reaper.GetActiveTake(midi_item))
assert(reaper.MIDI_InsertNote(midi_take, false, false, 960, 2880, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(midi_take, false, false, 4800, 6720, 0, 64, 108, false))
reaper.MIDI_Sort(midi_take)
reaper.SetMediaItemSelected(audio_item, true)
reaper.SetMediaItemSelected(midi_item, true)
reaper.UpdateArrange()

local function note_state()
  local notes = {}
  local _, count = reaper.MIDI_CountEvts(midi_take)
  for index = 0, count - 1 do
    local ok, _, _, start_ppq, end_ppq, _, pitch = reaper.MIDI_GetNote(midi_take, index)
    assert(ok, "could not read MIDI note " .. tostring(index))
    notes[#notes + 1] = table.concat({
      index,
      string.format("%.3f", start_ppq),
      string.format("%.3f", end_ppq),
      pitch,
    }, ",")
  end
  return table.concat(notes, ";")
end

local function write_state(label, state_change_delta)
  local audio_valid = reaper.ValidatePtr2(0, audio_item, "MediaItem*")
  local midi_valid = reaper.ValidatePtr2(0, midi_item, "MediaItem*")
  if not audio_valid or not midi_valid then
    output:write(label,
      "\titems_valid\t0",
      "\taudio_item_valid\t", audio_valid and "1" or "0",
      "\tmidi_item_valid\t", midi_valid and "1" or "0",
      "\tstate_change_delta\t", tostring(state_change_delta or 0),
      "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""), "\n")
    output:flush()
    return false
  end
  output:write(label,
    "\taudio_position\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_item, "D_POSITION")),
    "\taudio_length\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_item, "D_LENGTH")),
    "\taudio_loop\t", reaper.GetMediaItemInfo_Value(audio_item, "B_LOOPSRC"),
    "\taudio_source_offset\t", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(audio_take, "D_STARTOFFS")),
    "\taudio_selected\t", reaper.IsMediaItemSelected(audio_item) and "1" or "0",
    "\tmidi_position\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(midi_item, "D_POSITION")),
    "\tmidi_length\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(midi_item, "D_LENGTH")),
    "\tmidi_loop\t", reaper.GetMediaItemInfo_Value(midi_item, "B_LOOPSRC"),
    "\tmidi_source_offset\t", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(midi_take, "D_STARTOFFS")),
    "\tmidi_selected\t", reaper.IsMediaItemSelected(midi_item) and "1" or "0",
    "\tnotes\t", note_state(),
    "\tstate_change_delta\t", tostring(state_change_delta or 0),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""), "\n")
  output:flush()
  return true
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("probe\tAudio and MIDI items both selected; each at 2s for 5s; audio offset 0.1s; MIDI note PPQ preserved\n")
output:write("nudge_right_action\t41249\tItem edit: Nudge right by last nudge dialog settings\n")
output:write("nudge_left_action\t41250\tItem edit: Nudge left by last nudge dialog settings\n")
write_state("before", 0)
reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()

local command_path = root .. "item-nudge-mixed-command.txt"
local function poll_command()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    if command == "status" then
      if not write_state("current", 0) then
        output:close()
        return
      end
    elseif command == "left" or command == "right" or command == "undo" or command == "redo" then
      local actions = {left = 41250, right = 41249, undo = 40029, redo = 40030}
      local before = reaper.GetProjectStateChangeCount(0)
      reaper.Main_OnCommand(actions[command], 0)
      reaper.UpdateArrange()
      if not write_state("after_" .. command, reaper.GetProjectStateChangeCount(0) - before) then
        output:close()
        return
      end
    elseif command == "done" then
      output:close()
      return
    end
  end
  reaper.defer(poll_command)
end
poll_command()
