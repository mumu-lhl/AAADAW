-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- Select Target=contents and an amount in the real Nudge/Set window, then
-- write left or right to item-nudge-midi-command.txt in the probe directory.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "item-nudge-midi-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI contents nudge", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 2, 7, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 960, 2880, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(take, false, false, 4800, 6720, 0, 64, 108, false))
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(4, false, false)
reaper.UpdateArrange()
reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()

local function note_state()
  local notes = {}
  local _, note_count = reaper.MIDI_CountEvts(take)
  for index = 0, note_count - 1 do
    local ok, selected, muted, start_ppq, end_ppq, channel, pitch, velocity =
      reaper.MIDI_GetNote(take, index)
    assert(ok, "could not read MIDI note " .. tostring(index))
    notes[#notes + 1] = table.concat({
      index,
      selected and 1 or 0,
      muted and 1 or 0,
      string.format("%.3f", start_ppq),
      string.format("%.3f", end_ppq),
      channel,
      pitch,
      velocity,
    }, ",")
  end
  return table.concat(notes, ";")
end

local function write_state(label, state_change_delta)
  output:write(label,
    "\tposition\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
    "\tlength\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
    "\tsource_offset\t", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
    "\tloop_source\t", reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC"),
    "\tselected\t", reaper.IsMediaItemSelected(item) and "1" or "0",
    "\tnotes\t", note_state(),
    "\tstate_change_delta\t", tostring(state_change_delta or 0),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("dialog_action\t41226\tItem edit: Nudge/Set...\n")
output:write("nudge_right_action\t41249\tItem edit: Nudge right by last nudge dialog settings\n")
output:write("nudge_left_action\t41250\tItem edit: Nudge left by last nudge dialog settings\n")
output:write("probe\tMIDI Contents Nudge; item at 2s; length 5s; two notes at 0.5-1.5s and 2.5-3.5s\n")
write_state("before", 0)

local command_path = root .. "item-nudge-midi-command.txt"
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
    elseif command == "done" then
      output:close()
      return
    end
  end
  reaper.defer(poll_command)
end
poll_command()
