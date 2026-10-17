-- Run in an unsaved, stopped Linux REAPER 7.82 / Default 7 project.
-- Records the Nudge/Set dialog default and action 41249 behavior.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/item-nudge-native.tsv", "w"))

local function add_track(name)
  reaper.InsertTrackAtIndex(reaper.CountTracks(0), true)
  local track = reaper.GetTrack(0, reaper.CountTracks(0) - 1)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  return track
end

local function write_case(name, track, before)
  output:write(name, "_state_change_delta\t", reaper.GetProjectStateChangeCount(0) - before, "\n")
  for index = 0, reaper.CountTrackMediaItems(track) - 1 do
    local item = reaper.GetTrackMediaItem(track, index)
    local take = reaper.GetActiveTake(item)
    output:write(name, "_item\t", index,
      "\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
      "\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
      "\t", take and string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")) or "none",
      "\t", reaper.IsMediaItemSelected(item) and "selected" or "unselected", "\n")
  end
end

local function reset_project()
  if reaper.GetToggleCommandStateEx(0, 41226) == 1 then
    reaper.Main_OnCommand(41227, 0)
  end
  for index = reaper.CountTracks(0) - 1, 0, -1 do
    reaper.DeleteTrack(reaper.GetTrack(0, index))
  end
end

local function undo_and_record(name, track)
  local available, undo_label = reaper.APIExists("Undo_CanUndo2")
  local label_ok, label = false, "unavailable"
  if available then label_ok, label = pcall(reaper.Undo_CanUndo2, 0) end
  output:write(name, "_undo_label\t", label_ok and tostring(label) or tostring(label), "\n")
  output:flush()
  if label_ok and type(label) == "string" and label:lower():find("nudge", 1, true) then
    local undo_ok, undo_result = pcall(reaper.Undo_DoUndo2, 0)
    output:write(name, "_undo_result\t", undo_ok and tostring(undo_result) or tostring(undo_result), "\n")
    reaper.UpdateArrange()
    write_case(name .. "_after_undo", track, reaper.GetProjectStateChangeCount(0))
    local redo_ok, redo_result = pcall(reaper.Undo_DoRedo2, 0)
    output:write(name, "_redo_result\t", redo_ok and tostring(redo_result) or tostring(redo_result), "\n")
    reaper.UpdateArrange()
    write_case(name .. "_after_redo", track, reaper.GetProjectStateChangeCount(0))
  else
    output:write(name, "_undo_result\tno dedicated nudge undo entry\n")
  end
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("dialog_action\t41226\tItem edit: Nudge/Set...\n")
output:write("nudge_right_action\t41249\tItem edit: Nudge right by last nudge dialog settings\n")
output:write("transport\tstopped\n")

reset_project()
local audio_track = add_track("Selected audio")
reaper.SetOnlyTrackSelected(audio_track)
reaper.SetEditCurPos(0, false, false)
reaper.InsertMedia(source_path, 0)
local audio_item = reaper.GetTrackMediaItem(audio_track, 0)
assert(audio_item)
reaper.SetMediaItemSelected(audio_item, true)
reaper.UpdateArrange()
reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()
local before = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(41249, 0)
reaper.UpdateArrange()
write_case("audio_right_default", audio_track, before)
undo_and_record("audio_right_default", audio_track)
output:flush()

reset_project()
local midi_track = add_track("Selected MIDI")
local midi_item = reaper.CreateNewMIDIItemInProj(midi_track, 0, 5, false)
assert(midi_item)
reaper.SetMediaItemSelected(midi_item, true)
reaper.Main_OnCommand(41226, 0)
local midi_before = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(41249, 0)
reaper.UpdateArrange()
write_case("midi_right_default", midi_track, midi_before)
undo_and_record("midi_right_default", midi_track)
output:flush()

reset_project()
local none_track = add_track("No selected items")
reaper.SetOnlyTrackSelected(none_track)
reaper.SetEditCurPos(0, false, false)
reaper.InsertMedia(source_path, 0)
local none_item = reaper.GetTrackMediaItem(none_track, 0)
assert(none_item)
reaper.SetMediaItemSelected(none_item, false)
reaper.Main_OnCommand(41226, 0)
local none_before = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(41249, 0)
reaper.UpdateArrange()
write_case("no_selection_right_default", none_track, none_before)
output:flush()
output:close()
