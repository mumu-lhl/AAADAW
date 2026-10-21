-- Measure Main actions 41351/41352 for a MIDI item with two distinct Takes.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/midi-empty-takes-native.tsv", "w"))
local project_base = output_dir .. "/midi-empty-takes"

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI empty takes", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 2, false))
local take_a = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(take_a, "P_NAME", "Take A", true)
assert(reaper.MIDI_InsertNote(take_a, false, false, 0, 960, 0, 60, 100, false))
reaper.MIDI_Sort(take_a)
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
local take_b = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(take_b, "P_NAME", "Take B", true)
assert(reaper.MIDI_DeleteNote(take_b, 0))
assert(reaper.MIDI_InsertNote(take_b, false, false, 480, 1440, 0, 64, 92, false))
reaper.MIDI_Sort(take_b)
reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", 1)
reaper.SetMediaItemSelected(item, true)
reaper.UpdateItemInProject(item)
reaper.Undo_EndBlock2(0, "MIDI empty take fixture setup", -1)

local function capture(label)
  local current = assert(reaper.GetMediaItem(0, 0))
  local active_index = math.floor(reaper.GetMediaItemInfo_Value(current, "I_CURTAKE"))
  output:write(label, "\tactive_index=", active_index, "\ttake_count=", reaper.CountTakes(current),
    "\tselected=", reaper.IsMediaItemSelected(current) and 1 or 0, "\n")
  reaper.Main_SaveProjectEx(0, project_base .. "-" .. label .. ".rpp", 0)
  for index = 0, reaper.CountTakes(current) - 1 do
    local take = reaper.GetMediaItemTake(current, index)
    if not take then
      output:write("take\t", label, "\tindex=", index, "\tapi_take=none\n")
    else
      local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
      local is_midi = reaper.TakeIsMIDI(take)
      local note_count = "n/a"
      local pitch = "n/a"
      if is_midi then
        local _, count = reaper.MIDI_CountEvts(take)
        note_count = count
        if count > 0 then
          local _, _, _, _, _, _, first_pitch = reaper.MIDI_GetNote(take, 0)
          pitch = first_pitch or "none"
        else
          pitch = "none"
        end
      end
      local source = reaper.GetMediaItemTake_Source(take)
      local source_length, length_is_qn, source_type = 0, false, "none"
      if source then
        source_length, length_is_qn = reaper.GetMediaSourceLength(source)
        source_type = reaper.GetMediaSourceType(source, "")
      end
      output:write("take\t", label, "\tindex=", index, "\tname=", name,
        "\tactive=", index == active_index and 1 or 0, "\tmidi=", is_midi and 1 or 0,
        "\tnotes=", note_count, "\tfirst_pitch=", pitch or "none",
        "\tsource_type=", source_type,
        "\tsource_length=", string.format("%.12f", source_length),
        "\tlength_is_qn=", length_is_qn and 1 or 0,
        "\tsource_offset=", string.format("%.12f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")), "\n")
    end
  end
  output:flush()
end

capture("before")
reaper.Main_OnCommand(41351, 0) -- Item: Add an empty take before the active take.
capture("before-active")
reaper.Undo_DoUndo2(0)
capture("undo-before-active")
reaper.Undo_DoRedo2(0)
capture("redo-before-active")
reaper.Undo_DoUndo2(0)
reaper.Main_OnCommand(41352, 0) -- Item: Add an empty take after the active take.
capture("after-active")
reaper.Undo_DoUndo2(0)
capture("undo-after-active")
reaper.Undo_DoRedo2(0)
capture("redo-after-active")
output:write("action_before\t41351\tItem: Add an empty take before the active take\n")
output:write("action_after\t41352\tItem: Add an empty take after the active take\n")
output:close()
reaper.UpdateArrange()
reaper.defer(function() end)
