-- Measure Main action 40438 on selected MIDI items across tracks.
-- Run in an empty, stopped Linux REAPER 7.82 / Default 7 project.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/midi-implode-cross-track-native.tsv", "w"))
local project_base = output_dir .. "/midi-implode-cross-track"

local function make_track(name)
  reaper.InsertTrackAtIndex(reaper.CountTracks(0), true)
  local track = assert(reaper.GetTrack(0, reaper.CountTracks(0) - 1))
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  return track
end

local function write_notes(take, pitch, velocity)
  local _, note_count = reaper.MIDI_CountEvts(take)
  for index = note_count - 1, 0, -1 do
    assert(reaper.MIDI_DeleteNote(take, index))
  end
  assert(reaper.MIDI_InsertNote(take, false, false, 120, 360, 0, pitch, velocity, false))
  assert(reaper.MIDI_InsertNote(take, false, false, 720, 960, 0, pitch + 2, velocity - 5, false))
  reaper.MIDI_Sort(take)
end

local function set_take(take, name, offset, pitch, velocity)
  reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", name, true)
  reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", offset)
  write_notes(take, pitch, velocity)
end

local function make_item(track, position, length, name, offset, pitch, velocity)
  local item = assert(reaper.CreateNewMIDIItemInProj(track, position, position + length, false))
  local take = assert(reaper.GetActiveTake(item))
  set_take(take, name, offset, pitch, velocity)
  reaper.UpdateItemInProject(item)
  return item
end

local function describe(label)
  output:write(label, "\ttrack_count=", reaper.CountTracks(0),
    "\titem_count=", reaper.CountMediaItems(0),
    "\tselected_count=", reaper.CountSelectedMediaItems(0), "\n")
  for track_index = 0, reaper.CountTracks(0) - 1 do
    local track = assert(reaper.GetTrack(0, track_index))
    local _, track_name = reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "", false)
    local item_count = reaper.CountTrackMediaItems(track)
    output:write(label, "\ttrack=", track_index, "\ttrack_name=", track_name,
      "\ttrack_item_count=", item_count, "\n")
    for item_index = 0, item_count - 1 do
      local item = assert(reaper.GetTrackMediaItem(track, item_index))
      local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
      output:write(label, "\ttrack=", track_index, "\titem=", item_index,
        "\tposition=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
        "\tlength=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
        "\tselected=", reaper.IsMediaItemSelected(item) and 1 or 0,
        "\ttake_count=", reaper.CountTakes(item), "\tactive_index=", active_index, "\n")
      for take_index = 0, reaper.CountTakes(item) - 1 do
        local take = assert(reaper.GetTake(item, take_index))
        local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
        local _, note_count = reaper.MIDI_CountEvts(take)
        local notes = {}
        for note_index = 0, note_count - 1 do
          local _, _, _, start_ppq, end_ppq, _, pitch, velocity = reaper.MIDI_GetNote(take, note_index)
          notes[#notes + 1] = string.format("%d:%d:%d:%d", start_ppq, end_ppq, pitch, velocity)
        end
        output:write(label, "\ttrack=", track_index, "\titem=", item_index,
          "\ttake=", take_index, "\tactive=", take_index == active_index and 1 or 0,
          "\tname=", name,
          "\toffset=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
          "\tnotes=", table.concat(notes, ";"), "\n")
      end
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

local top_track = make_track("Top MIDI track")
local early_track = make_track("Earlier MIDI track")
local lower_track = make_track("Lower MIDI track")
reaper.Undo_BeginBlock2(0)
local top = make_item(top_track, 1, 2, "Top target", 0.125, 60, 96)
local earlier = make_item(early_track, 0, 1.5, "Earlier donor", 0.25, 67, 88)
reaper.SetMediaItemSelected(earlier, true)
reaper.Main_OnCommand(40639, 0)
local earlier_active = assert(reaper.GetActiveTake(earlier))
set_take(earlier_active, "Earlier donor active", 0.375, 74, 80)
reaper.UpdateItemInProject(earlier)
local lower = make_item(lower_track, 2, 1.5, "Lower donor", 0.5, 79, 76)
reaper.Undo_EndBlock2(0, "MIDI cross-track implode fixture", -1)

reaper.SelectAllMediaItems(0, false)
for _, item in ipairs({ top, earlier, lower }) do reaper.SetMediaItemSelected(item, true) end
assert(reaper.CountSelectedMediaItems(0) == 3)
describe("before")
save("before")

reaper.Undo_BeginBlock2(0)
reaper.SetMediaItemInfo_Value(lower, "D_POSITION", 3)
reaper.SelectAllMediaItems(0, false)
reaper.SetMediaItemSelected(top, true)
reaper.SetMediaItemSelected(lower, true)
reaper.Undo_EndBlock2(0, "Prepare abutting MIDI no-overlap selection", -1)
describe("no_overlap_before")
save("no-overlap-before")
local no_overlap_undo_before = reaper.Undo_CanUndo2(0) or ""
reaper.Main_OnCommand(40438, 0)
describe("no_overlap_after")
save("no-overlap-after")
local no_overlap_undo_after = reaper.Undo_CanUndo2(0) or ""
assert(reaper.CountMediaItems(0) == 3, "abutting MIDI items should not implode")
assert(reaper.CountSelectedMediaItems(0) == 2, "no-op should retain the selected MIDI items")
assert(no_overlap_undo_after == no_overlap_undo_before,
  "native no-op should not add an undo step")
output:write("no_overlap_result\tno-op\n")
output:write("no_overlap_undo_before\t", no_overlap_undo_before, "\n")
output:write("no_overlap_undo_after\t", no_overlap_undo_after, "\n")

reaper.Undo_DoUndo2(0)
reaper.SelectAllMediaItems(0, false)
for _, item in ipairs({ top, earlier, lower }) do reaper.SetMediaItemSelected(item, true) end
reaper.Undo_OnStateChange2(0, "Prepare MIDI cross-track implode probe")

reaper.Main_OnCommand(40438, 0) -- Take: Implode items across tracks into takes.
describe("after")
save("after")
reaper.Undo_DoUndo2(0)
describe("undo")
save("undo")
reaper.Undo_DoRedo2(0)
describe("redo")
save("redo")
output:write("undo_description\t", reaper.Undo_CanUndo2(0) or "", "\n")
output:write("action\t40438\tTake: Implode items across tracks into takes\n")
output:write("reference\tREAPER 7.82/linux-x86_64; Default_7.0.ReaperThemeZip\n")
output:close()

local marker = assert(io.open(output_dir .. "/capture-complete", "w"))
marker:write("ready for live-window capture\n")
marker:close()
reaper.UpdateArrange()
reaper.defer(function() end)
