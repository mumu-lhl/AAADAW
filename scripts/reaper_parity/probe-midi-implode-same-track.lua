-- Measure Main action 40543 on same-track MIDI items.
-- Run in an empty, stopped Linux REAPER 7.82 / Default 7 project.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/midi-implode-same-track-native.tsv", "w"))
local project_base = output_dir .. "/midi-implode-same-track"

local function clear_notes(take)
  local _, note_count = reaper.MIDI_CountEvts(take)
  for index = note_count - 1, 0, -1 do
    assert(reaper.MIDI_DeleteNote(take, index))
  end
end

local function write_notes(take, pitch, velocity)
  clear_notes(take)
  assert(reaper.MIDI_InsertNote(take, false, false, 120, 360, 0, pitch, velocity, false))
  assert(reaper.MIDI_InsertNote(take, false, false, 720, 960, 0, pitch + 2, velocity - 5, false))
  reaper.MIDI_Sort(take)
end

local function name_take(take, name, offset, pitch, velocity)
  reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", name, true)
  reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", offset)
  write_notes(take, pitch, velocity)
end

local function make_item(track, position, length, take_name, offset, pitch, velocity)
  reaper.SelectAllMediaItems(0, false)
  local item = assert(reaper.CreateNewMIDIItemInProj(track, position, position + length, false))
  reaper.SetMediaItemSelected(item, true)
  local take = assert(reaper.GetActiveTake(item))
  name_take(take, take_name, offset, pitch, velocity)
  reaper.UpdateItemInProject(item)
  return item
end

local function describe(label, track)
  local item_count = reaper.CountTrackMediaItems(track)
  output:write(label, "\titem_count=", item_count,
    "\tselected_count=", reaper.CountSelectedMediaItems(0), "\n")
  for item_index = 0, item_count - 1 do
    local item = assert(reaper.GetTrackMediaItem(track, item_index))
    local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
    output:write(label, "\titem=", item_index,
      "\tposition=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
      "\tlength=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
      "\tselected=", reaper.IsMediaItemSelected(item) and 1 or 0,
      "\ttake_count=", reaper.CountTakes(item),
      "\tactive_index=", active_index, "\n")
    for take_index = 0, reaper.CountTakes(item) - 1 do
      local take = assert(reaper.GetTake(item, take_index))
      local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
      local _, note_count = reaper.MIDI_CountEvts(take)
      local notes = {}
      for note_index = 0, note_count - 1 do
        local _, _, _, start_ppq, end_ppq, _, pitch, velocity = reaper.MIDI_GetNote(take, note_index)
        notes[#notes + 1] = string.format("%d:%d:%d:%d", start_ppq, end_ppq, pitch, velocity)
      end
      output:write(label, "\titem=", item_index,
        "\ttake=", take_index, "\tactive=", take_index == active_index and 1 or 0,
        "\tname=", name,
        "\toffset=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
        "\tnotes=", table.concat(notes, ";"), "\n")
    end
  end
  output:flush()
end

local function save(label)
  local path = project_base .. "-" .. label .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local file = assert(io.open(path, "r"))
  file:close()
end

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI same-track implode", true)
reaper.Undo_BeginBlock2(0)
local base = make_item(track, 0, 6, "Base A", 0.125, 60, 96)
local donor = make_item(track, 2, 1.5, "Donor B", 0.25, 67, 88)
reaper.Main_OnCommand(40639, 0) -- Duplicate the donor's active MIDI Take.
local donor_active = assert(reaper.GetActiveTake(donor))
name_take(donor_active, "Donor B active", 0.375, 74, 80)
reaper.UpdateItemInProject(donor)
local final_item = make_item(track, 4, 1.5, "Final C", 0.5, 79, 76)
reaper.Undo_EndBlock2(0, "MIDI same-track implode fixture", -1)

reaper.SelectAllMediaItems(0, false)
for _, item in ipairs({ base, donor, final_item }) do reaper.SetMediaItemSelected(item, true) end
assert(reaper.CountSelectedMediaItems(0) == 3)
describe("before", track)
save("before")
reaper.Undo_OnStateChange2(0, "Prepare MIDI same-track implode probe")

reaper.Main_OnCommand(40543, 0) -- Take: Implode items on same track into takes.
describe("after", track)
save("after")
local undo_description = reaper.Undo_CanUndo2(0) or ""
reaper.Undo_DoUndo2(0)
describe("undo", track)
save("undo")
reaper.Undo_DoRedo2(0)
describe("redo", track)
save("redo")
output:write("undo_description\t", undo_description, "\n")
output:write("action\t40543\tTake: Implode items on same track into takes\n")
output:write("reference\tREAPER 7.82/linux-x86_64; Default_7.0.ReaperThemeZip\n")
output:close()

local marker = assert(io.open(output_dir .. "/capture-complete", "w"))
marker:write("ready for live-window capture\n")
marker:close()
reaper.UpdateArrange()
