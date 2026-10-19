-- Check whether Main action 40642 also explodes MIDI item takes.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/midi-explode-takes-in-place-native.tsv", "w"))
local project_path_base = output_dir .. "/midi-explode-takes-in-place"

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI take probe", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 2, false))
local first_take = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(first_take, "P_NAME", "MIDI take A", true)
assert(reaper.MIDI_InsertNote(first_take, false, false, 0, 960, 0, 60, 100, false))
reaper.MIDI_Sort(first_take)
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
local second_take = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(second_take, "P_NAME", "MIDI take B", true)
assert(reaper.MIDI_InsertNote(second_take, false, false, 0, 960, 0, 64, 100, false))
reaper.MIDI_Sort(second_take)
assert(reaper.CountTakes(item) == 2)
reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", 1)
reaper.UpdateItemInProject(item)
reaper.Undo_EndBlock2(0, "MIDI take fixture setup", -1)

local function describe(label)
  output:write(label, "\ttrack_count=", reaper.CountTracks(0),
    "\titem_count=", reaper.CountMediaItems(0),
    "\tselected_count=", reaper.CountSelectedMediaItems(0), "\n")
  for item_index = 0, reaper.CountMediaItems(0) - 1 do
    local current = assert(reaper.GetMediaItem(0, item_index))
    output:write(label, "\titem=", item_index,
      "\tposition=", string.format("%.9f", reaper.GetMediaItemInfo_Value(current, "D_POSITION")),
      "\tlength=", string.format("%.9f", reaper.GetMediaItemInfo_Value(current, "D_LENGTH")),
      "\tselected=", reaper.IsMediaItemSelected(current) and 1 or 0,
      "\ttake_count=", reaper.CountTakes(current),
      "\tactive_index=", math.floor(reaper.GetMediaItemInfo_Value(current, "I_CURTAKE")), "\n")
    for take_index = 0, reaper.CountTakes(current) - 1 do
      local take = assert(reaper.GetTake(current, take_index))
      local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
      local _, note_count = reaper.MIDI_CountEvts(take)
      local _, _, _, _, _, _, pitch = reaper.MIDI_GetNote(take, 0)
      output:write(label, "\titem=", item_index,
        "\ttake=", take_index,
        "\tis_midi=", reaper.TakeIsMIDI(take) and 1 or 0,
        "\tname=", name,
        "\tnote_count=", note_count,
        "\tfirst_note_pitch=", pitch or "none", "\n")
    end
  end
  output:flush()
end

local function save(label)
  reaper.Main_SaveProjectEx(0, project_path_base .. "-" .. label .. ".rpp", 0)
end

describe("before")
output:write("undo_before_action\t", reaper.Undo_CanUndo2(0), "\n")
save("before")
reaper.Main_OnCommand(40642, 0) -- Take: Explode takes of items in place.
describe("after")
output:write("undo_after_action\t", reaper.Undo_CanUndo2(0), "\n")
save("after")
reaper.Undo_DoUndo2(0)
describe("undo")
output:write("undo_after_undo\t", reaper.Undo_CanUndo2(0), "\n")
save("undo")
reaper.Undo_DoRedo2(0)
describe("redo")
save("redo")
output:write("action\t40642\tTake: Explode takes of items in place\n")
output:close()
reaper.UpdateArrange()
reaper.defer(function() end)
