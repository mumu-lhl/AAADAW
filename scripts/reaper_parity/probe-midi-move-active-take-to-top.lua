-- Run in an empty, stopped Linux REAPER 7.82 / Default 7 project.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")

local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/midi-move-active-take-to-top-native.tsv", "w"))
local project_base = output_dir .. "/midi-move-active-take-to-top"
local function clear_notes(take)
  local _, note_count = reaper.MIDI_CountEvts(take)
  for index = note_count - 1, 0, -1 do
    assert(reaper.MIDI_DeleteNote(take, index))
  end
end

local function set_take(take, name, pitch, source_offset)
  reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", name, true)
  reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", source_offset)
  clear_notes(take)
  assert(reaper.MIDI_InsertNote(take, false, false, 120, 360, 0, pitch, 90, false))
  assert(reaper.MIDI_InsertNote(take, false, false, 720, 960, 0, pitch + 2, 80, false))
  reaper.MIDI_Sort(take)
end

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI move active take", true)

local function make_item(position, labels, active_index)
  reaper.SelectAllMediaItems(0, false)
  local item = assert(reaper.CreateNewMIDIItemInProj(track, position, position + 1.5, false))
  reaper.SetMediaItemSelected(item, true)
  local first = assert(reaper.GetActiveTake(item))
  set_take(first, labels[1], 60 + position * 3, 0.125)
  for index = 2, #labels do
    reaper.Main_OnCommand(40639, 0)
    local take = assert(reaper.GetActiveTake(item))
    set_take(take, labels[index], 60 + position * 3 + index * 2, 0.125 * index)
  end
  reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", active_index)
  reaper.UpdateItemInProject(item)
  return item
end

local function take_state(label, item)
  local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
  output:write(label, "\ttake_count=", reaper.CountTakes(item),
    "\tactive_index=", active_index,
    "\tselected=", reaper.IsMediaItemSelected(item) and 1 or 0, "\n")
  for index = 0, reaper.CountTakes(item) - 1 do
    local take = assert(reaper.GetTake(item, index))
    local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
    local _, note_count = reaper.MIDI_CountEvts(take)
    local notes = {}
    for note_index = 0, note_count - 1 do
      local _, _, _, start_ppq, end_ppq, _, pitch, velocity = reaper.MIDI_GetNote(take, note_index)
      notes[#notes + 1] = string.format("%d:%d:%d:%d", start_ppq, end_ppq, pitch, velocity)
    end
    output:write(label, "\ttake=", index, "\tactive=", index == active_index and 1 or 0,
      "\tname=", name,
      "\tsource_offset=", string.format("%.6f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
      "\tnotes=", table.concat(notes, ";"), "\n")
  end
  output:flush()
end

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local file = assert(io.open(path, "r"))
  file:close()
end

reaper.Undo_BeginBlock2(0)
local item_b_top = make_item(0, { "Take A", "Take B" }, 0)
local item_single = make_item(2, { "Single Take" }, 0)
local item_c = make_item(4, { "Take A", "Take B", "Take C" }, 2)
local item_a = make_item(6, { "Take A", "Take B" }, 1)
reaper.Undo_EndBlock2(0, "MIDI active take ordering fixture", -1)
reaper.SelectAllMediaItems(0, true)
assert(reaper.CountSelectedMediaItems(0) == 4)
local items = { item_b_top, item_single, item_c, item_a }
for index, item in ipairs(items) do take_state("before_" .. index, item) end
save("before")
reaper.Undo_OnStateChange2(0, "Prepare MIDI active take ordering probe")

reaper.Main_OnCommand(41380, 0)
for index, item in ipairs(items) do take_state("after_" .. index, item) end
assert(math.floor(reaper.GetMediaItemInfo_Value(item_a, "I_CURTAKE")) == 0)
assert(math.floor(reaper.GetMediaItemInfo_Value(item_single, "I_CURTAKE")) == 0)
assert(math.floor(reaper.GetMediaItemInfo_Value(item_c, "I_CURTAKE")) == 0)
assert(math.floor(reaper.GetMediaItemInfo_Value(item_b_top, "I_CURTAKE")) == 0)
save("after")
local undo_description = reaper.Undo_CanUndo2(0) or ""
reaper.Undo_DoUndo2(0)
for index, item in ipairs(items) do take_state("undo_" .. index, item) end
save("undo")
reaper.Undo_DoRedo2(0)
for index, item in ipairs(items) do take_state("redo_" .. index, item) end
save("redo")
local no_op_undo = reaper.Undo_CanUndo2(0) or ""
reaper.Main_OnCommand(41380, 0)
assert((reaper.Undo_CanUndo2(0) or "") == no_op_undo,
  "already-top MIDI items must not add an undo entry")
for index, item in ipairs(items) do take_state("already_top_" .. index, item) end
save("already-top")
output:write("undo_description\t", undo_description, "\n")
output:write("action\t41380\tItem: Move active takes to top\n")
output:write("reference\tREAPER 7.82/linux-x86_64; Default_7.0.ReaperThemeZip\n")
output:close()

local marker = assert(io.open(output_dir .. "/capture-complete", "w"))
marker:write("ready for live-window capture\n")
marker:close()
reaper.UpdateArrange()
