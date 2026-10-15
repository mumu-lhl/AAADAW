-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- Set Mode=Set; probe Left Trim/Left Edge at 4s, End Position/Right Trim
-- at 14s, and End Position with Relative Set enabled at 14s. Relative Set
-- takes an absolute destination for the last selected item's right edge.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "item-nudge-midi-set-edge-multiselect-native.tsv", "w"))
local command_path = root .. "item-nudge-midi-set-edge-multiselect-command.txt"
local loop_source_enabled = os.getenv("AAADAW_MIDI_PROBE_LOOP") ~= "0"
local loop_source_value = loop_source_enabled and 1 or 0
local positions = { 2, 6 }
local items = {}
local takes = {}

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Set edge multi-select MIDI", true)
reaper.SetOnlyTrackSelected(track)
for index, position in ipairs(positions) do
  local item = assert(reaper.CreateNewMIDIItemInProj(track, position, position + 10, false))
  assert(reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", loop_source_value))
  local take = assert(reaper.GetActiveTake(item))
  assert(reaper.MIDI_InsertNote(take, false, false, 960, 1920, 0, 60, 96, false))
  assert(reaper.MIDI_InsertNote(take, false, false, 2880, 3840, 0, 64, 108, false))
  reaper.MIDI_Sort(take)
  reaper.SetMediaItemSelected(item, true)
  items[index] = item
  takes[index] = take
end
reaper.SetEditCurPos(7, false, false)
reaper.UpdateArrange()
reaper.GetSet_ArrangeView2(0, true, 0, 0, 0, 20)
reaper.UpdateArrange()
reaper.Undo_EndBlock2(0, "Set edge MIDI multi-select probe setup", -1)

local state_before = reaper.GetProjectStateChangeCount(0)
local function write_state(label)
  output:write(label,
    "\tstate_change_count\t", reaper.GetProjectStateChangeCount(0),
    "\tstate_change_delta\t", reaper.GetProjectStateChangeCount(0) - state_before,
    "\titem_count\t", reaper.CountMediaItems(0),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""), "\n")
  for index, item in ipairs(items) do
    local take = takes[index]
    local _, note_count = reaper.MIDI_CountEvts(take)
    local notes = {}
    for note_index = 0, note_count - 1 do
      local ok, selected, muted, start_ppq, end_ppq, channel, pitch, velocity =
        reaper.MIDI_GetNote(take, note_index)
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
    local source = reaper.GetMediaItemTake_Source(take)
    local source_length, length_is_qn = reaper.GetMediaSourceLength(source)
    output:write(label, "_item_", index,
      "\tposition_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
      "\tlength_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
      "\tloop_source\t", reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC"),
      "\tsource_offset_s\t", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
      "\tsource_length\t", string.format("%.9f", source_length),
      "\tlength_is_qn\t", tostring(length_is_qn),
      "\tselected\t", reaper.IsMediaItemSelected(item) and "1" or "0",
      "\tnotes\t", table.concat(notes, ";"), "\n")
  end
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("fixture\ttwo selected MIDI items at 2s and 6s; each 10s long; loop_source=", loop_source_value,
  "; notes at 0.5s and 1.5s PPQ offsets\n")
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
      assert(reaper.Undo_CanUndo2(0) == "Nudge items",
        "reset requires a preceding Nudge items edge action")
      reaper.Main_OnCommand(40029, 0)
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
