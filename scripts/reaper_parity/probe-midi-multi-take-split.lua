-- Probe REAPER's split behavior for a MIDI item with distinct active/inactive takes.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, existing_project = reaper.EnumProjects(-1, "")
assert(existing_project == "")
assert(reaper.CountTracks(0) == 0)

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-multi-take-split-native.tsv", "w"))
local project_base = root .. "midi-multi-take-split"

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI multi-take split", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 2, false))
local take_a = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(take_a, "P_NAME", "Take A", true)
assert(reaper.MIDI_InsertNote(take_a, false, false, 960, 2880, 0, 60, 100, false))
assert(reaper.MIDI_InsertNote(take_a, false, false, 3000, 3480, 0, 67, 90, false))
reaper.MIDI_Sort(take_a)
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
local take_b = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(take_b, "P_NAME", "Take B", true)
assert(reaper.MIDI_DeleteNote(take_b, 1))
assert(reaper.MIDI_DeleteNote(take_b, 0))
assert(reaper.MIDI_InsertNote(take_b, false, false, 480, 720, 0, 62, 84, false))
assert(reaper.MIDI_InsertNote(take_b, false, false, 1200, 1440, 0, 64, 92, false))
assert(reaper.MIDI_InsertNote(take_b, false, false, 3360, 3720, 0, 72, 88, false))
reaper.MIDI_Sort(take_b)
local source_offset_seconds = tonumber(os.getenv("AAADAW_REAPER_SPLIT_SOURCE_OFFSET") or "0.5")
reaper.SetMediaItemTakeInfo_Value(take_b, "D_STARTOFFS", source_offset_seconds)
local loop_source = tonumber(os.getenv("AAADAW_REAPER_SPLIT_LOOP_SOURCE") or "1")
assert(loop_source == 0 or loop_source == 1)
reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", loop_source)
local active_take_index = tonumber(os.getenv("AAADAW_REAPER_SPLIT_ACTIVE_TAKE") or "1")
assert(active_take_index == 0 or active_take_index == 1)
reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", active_take_index)
reaper.UpdateItemInProject(item)
reaper.Undo_EndBlock2(0, "MIDI multi-take split fixture setup", -1)

local function capture(label)
  output:write(label, "\titem_count=", reaper.CountMediaItems(0),
    "\tselected_count=", reaper.CountSelectedMediaItems(0), "\n")
  for item_index = 0, reaper.CountMediaItems(0) - 1 do
    local current = assert(reaper.GetMediaItem(0, item_index))
    local active_index = math.floor(reaper.GetMediaItemInfo_Value(current, "I_CURTAKE"))
    output:write(label, "\titem=", item_index,
      "\tposition=", string.format("%.6f", reaper.GetMediaItemInfo_Value(current, "D_POSITION")),
      "\tlength=", string.format("%.6f", reaper.GetMediaItemInfo_Value(current, "D_LENGTH")),
      "\tactive_index=", active_index,
      "\ttake_count=", reaper.CountTakes(current),
      "\tloop_source=", math.floor(reaper.GetMediaItemInfo_Value(current, "B_LOOPSRC")),
      "\tselected=", reaper.IsMediaItemSelected(current) and 1 or 0, "\n")
    for take_index = 0, reaper.CountTakes(current) - 1 do
      local take = assert(reaper.GetTake(current, take_index))
      local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
      local _, note_count = reaper.MIDI_CountEvts(take)
      local source = assert(reaper.GetMediaItemTake_Source(take))
      local source_length = reaper.GetMediaSourceLength(source, false)
      local notes = {}
      for note_index = 0, note_count - 1 do
        local _, selected, muted, start_ppq, end_ppq, channel, pitch, velocity =
          reaper.MIDI_GetNote(take, note_index)
        notes[#notes + 1] = string.format("%s:%d:%d-%d:%d:%d:%d", selected and 1 or 0,
          muted and 1 or 0, start_ppq, end_ppq, channel, pitch, velocity)
      end
      output:write(label, "\titem=", item_index,
        "\ttake=", take_index,
        "\tactive=", active_index == take_index and 1 or 0,
        "\tname=", name,
        "\tstart_offset=", string.format("%.6f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
        "\tsource_length=", string.format("%.6f", source_length),
        "\tnotes=", table.concat(notes, ";"), "\n")
    end
  end
  output:flush()
  reaper.Main_SaveProjectEx(0, project_base .. "-" .. label .. ".rpp", 0)
end

capture("before")
local split_seconds = tonumber(os.getenv("AAADAW_REAPER_SPLIT_SECONDS") or "1")
assert(split_seconds and split_seconds > 0)
reaper.SetEditCurPos(split_seconds, false, false)
reaper.Main_OnCommand(40012, 0) -- Item: Split items at edit or play cursor (select right).
capture("after")
reaper.Undo_DoUndo2(0)
capture("undo")
reaper.Undo_DoRedo2(0)
capture("redo")
output:write("action\t40012\tItem: Split items at edit or play cursor (select right)\n")
output:write("split_seconds\t", string.format("%.9f", split_seconds), "\n")
output:write("reference\tREAPER 7.82/linux-x86_64; Default_7.0.ReaperThemeZip\n")
output:close()
reaper.UpdateArrange()
reaper.defer(function() end)
