-- Probe REAPER's Main action 40129 with a non-terminal active MIDI Take.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, existing_project = reaper.EnumProjects(-1, "")
assert(existing_project == "")
assert(reaper.CountTracks(0) == 0)

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-delete-active-take-native.tsv", "w"))
local project_base = root .. "midi-delete-active-take"

local function replace_notes(take, notes)
  local _, note_count = reaper.MIDI_CountEvts(take)
  for note_index = note_count - 1, 0, -1 do
    assert(reaper.MIDI_DeleteNote(take, note_index))
  end
  for _, note in ipairs(notes) do
    assert(reaper.MIDI_InsertNote(take, false, false, note[1], note[2], 0, note[3], note[4], false))
  end
  reaper.MIDI_Sort(take)
end

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI delete take", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 2, false))
local take_a = assert(reaper.GetActiveTake(item))
reaper.SetMediaItemSelected(item, true)
reaper.GetSetMediaItemTakeInfo_String(take_a, "P_NAME", "Take A", true)
replace_notes(take_a, { { 240, 480, 60, 100 } })
reaper.SetMediaItemTakeInfo_Value(take_a, "D_STARTOFFS", 0.125)
reaper.Main_OnCommand(40639, 0)
local take_b = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(take_b, "P_NAME", "Take B", true)
replace_notes(take_b, { { 480, 720, 62, 84 }, { 1440, 1680, 64, 92 } })
reaper.SetMediaItemTakeInfo_Value(take_b, "D_STARTOFFS", 0.375)
reaper.Main_OnCommand(40639, 0)
local take_c = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(take_c, "P_NAME", "Take C", true)
replace_notes(take_c, { { 720, 960, 55, 75 } })
reaper.SetMediaItemTakeInfo_Value(take_c, "D_STARTOFFS", 0.625)
reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", 1)
reaper.SetMediaItemSelected(item, true)
reaper.UpdateItemInProject(item)
reaper.Undo_EndBlock2(0, "MIDI delete active take fixture setup", -1)

local function capture(label)
  local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
  output:write(label, "\titem_count=", reaper.CountMediaItems(0),
    "\tselected_count=", reaper.CountSelectedMediaItems(0),
    "\ttake_count=", reaper.CountTakes(item),
    "\tactive_index=", active_index,
    "\tselected=", reaper.IsMediaItemSelected(item) and 1 or 0, "\n")
  for take_index = 0, reaper.CountTakes(item) - 1 do
    local take = assert(reaper.GetTake(item, take_index))
    local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
    local _, note_count = reaper.MIDI_CountEvts(take)
    local notes = {}
    for note_index = 0, note_count - 1 do
      local _, selected, muted, start_ppq, end_ppq, channel, pitch, velocity =
        reaper.MIDI_GetNote(take, note_index)
      notes[#notes + 1] = string.format("%d-%d:%d:%d:%d:%d:%d", selected and 1 or 0,
        muted and 1 or 0, start_ppq, end_ppq, channel, pitch, velocity)
    end
    output:write(label, "\ttake=", take_index,
      "\tactive=", active_index == take_index and 1 or 0,
      "\tname=", name,
      "\tstart_offset=", string.format("%.6f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
      "\tnotes=", table.concat(notes, ";"), "\n")
  end
  output:flush()
  reaper.Main_SaveProjectEx(0, project_base .. "-" .. label .. ".rpp", 0)
end

capture("before")
reaper.Main_OnCommand(40129, 0) -- Take: Delete active take from items.
capture("after")
reaper.Undo_DoUndo2(0)
capture("undo")
reaper.Undo_DoRedo2(0)
capture("redo")

reaper.Undo_DoUndo2(0)
reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", 0)
reaper.Undo_OnStateChange2(0, "Select first Take for deletion probe")
capture("first-before")
reaper.Main_OnCommand(40129, 0)
capture("first-after")
reaper.Undo_DoUndo2(0)
capture("first-undo")
reaper.Undo_DoRedo2(0)
capture("first-redo")
reaper.Undo_DoUndo2(0)
reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", 2)
reaper.Undo_OnStateChange2(0, "Select last Take for deletion probe")
capture("last-before")
reaper.Main_OnCommand(40129, 0)
capture("last-after")
reaper.Undo_DoUndo2(0)
capture("last-undo")
reaper.Undo_DoRedo2(0)
capture("last-redo")
reaper.Undo_DoUndo2(0)

reaper.SetMediaItemSelected(item, false)
reaper.Undo_BeginBlock2(0)
local single_take_item = assert(reaper.CreateNewMIDIItemInProj(track, 3, 5, false))
local single_take = assert(reaper.GetActiveTake(single_take_item))
reaper.GetSetMediaItemTakeInfo_String(single_take, "P_NAME", "Only Take", true)
replace_notes(single_take, { { 240, 480, 67, 90 } })
reaper.SetMediaItemSelected(single_take_item, true)
reaper.Undo_EndBlock2(0, "MIDI single take deletion fixture setup", -1)
local function capture_single(label)
  output:write(label, "\titem_count=", reaper.CountMediaItems(0),
    "\tselected_count=", reaper.CountSelectedMediaItems(0), "\n")
  output:flush()
  reaper.Main_SaveProjectEx(0, project_base .. "-single-" .. label .. ".rpp", 0)
end
capture_single("before")
reaper.Main_OnCommand(40129, 0)
capture_single("after")
reaper.Undo_DoUndo2(0)
capture_single("undo")
reaper.Undo_DoRedo2(0)
capture_single("redo")

-- Leave the native Arrange in a representative multi-Take deletion state for capture.
reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", 1)
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40129, 0)
capture("visual-after")
output:write("action\t40129\tTake: Delete active take from items\n")
output:write("reference\tREAPER 7.82/linux-x86_64; Default_7.0.ReaperThemeZip\n")
output:close()
reaper.UpdateArrange()
reaper.defer(function() end)
