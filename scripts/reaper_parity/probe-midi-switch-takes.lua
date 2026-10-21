-- Measure Main actions 40125/40126 with a MIDI item that has two distinct takes.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/midi-switch-takes-native.tsv", "w"))
local project_base = output_dir .. "/midi-switch-takes"

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI takes", true)
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
reaper.Undo_EndBlock2(0, "MIDI take switch fixture setup", -1)

local function capture(label)
  local current = assert(reaper.GetMediaItem(0, 0))
  local active_index = math.floor(reaper.GetMediaItemInfo_Value(current, "I_CURTAKE"))
  local active_take = assert(reaper.GetActiveTake(current))
  local _, name = reaper.GetSetMediaItemTakeInfo_String(active_take, "P_NAME", "", false)
  local _, note_count = reaper.MIDI_CountEvts(active_take)
  local _, _, _, _, _, _, pitch = reaper.MIDI_GetNote(active_take, 0)
  output:write(label, "\tactive_index=", active_index, "\ttake_count=", reaper.CountTakes(current),
    "\tname=", name, "\tnote_count=", note_count, "\tfirst_pitch=", pitch or "none",
    "\tselected=", reaper.IsMediaItemSelected(current) and 1 or 0, "\n")
  output:flush()
  reaper.Main_SaveProjectEx(0, project_base .. "-" .. label .. ".rpp", 0)
end

capture("before")
reaper.Main_OnCommand(40125, 0) -- Take: Switch items to next take.
capture("next")
reaper.Undo_DoUndo2(0)
capture("undo")
reaper.Undo_DoRedo2(0)
capture("redo")
reaper.Main_OnCommand(40126, 0) -- Take: Switch items to previous take.
capture("previous")
output:write("action_next\t40125\tTake: Switch items to next take\n")
output:write("action_previous\t40126\tTake: Switch items to previous take\n")
output:close()
reaper.UpdateArrange()
reaper.defer(function() end)
