-- Measure Main action 40438 for one selected Audio and one selected MIDI item.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local audio_path = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_A"))
local target_kind = os.getenv("AAADAW_REAPER_PROBE_TARGET") or "audio"
assert(target_kind == "audio" or target_kind == "midi")
local output_prefix = "mixed-audio-midi-implode-" .. target_kind .. "-target"
local output = assert(io.open(output_dir .. "/" .. output_prefix .. "-native.tsv", "w"))
local project_base = output_dir .. "/" .. output_prefix

local function add_track(name)
  reaper.InsertTrackAtIndex(reaper.CountTracks(0), true)
  local track = assert(reaper.GetTrack(0, reaper.CountTracks(0) - 1))
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  return track
end

local target_track_name = target_kind == "audio" and "Top Audio target" or "Top MIDI target"
local donor_track_name = target_kind == "audio" and "Lower MIDI donor" or "Lower Audio donor"
local audio_target = target_kind == "audio"
local audio_track, midi_track
if audio_target then
  audio_track = add_track(target_track_name)
  midi_track = add_track(donor_track_name)
else
  midi_track = add_track(target_track_name)
  audio_track = add_track(donor_track_name)
end
reaper.SetOnlyTrackSelected(audio_track)
reaper.SetEditCurPos(2, false, false)
assert(reaper.InsertMedia(audio_path, 0) == 1, "could not insert audio source")
local audio_item = assert(reaper.GetTrackMediaItem(audio_track, 0))
reaper.SetMediaItemInfo_Value(audio_item, "D_POSITION", audio_target and 2 or 0)
reaper.SetMediaItemInfo_Value(audio_item, "D_LENGTH", audio_target and 2 or 4)
local audio_take = assert(reaper.GetActiveTake(audio_item))
reaper.GetSetMediaItemTakeInfo_String(audio_take, "P_NAME", audio_target and "Audio target" or "Audio donor", true)

local midi_item = assert(reaper.CreateNewMIDIItemInProj(midi_track,
  audio_target and 0 or 2, 4, false))
local midi_take = assert(reaper.GetActiveTake(midi_item))
reaper.GetSetMediaItemTakeInfo_String(midi_take, "P_NAME", audio_target and "MIDI donor" or "MIDI target", true)
assert(reaper.MIDI_InsertNote(midi_take, false, false, 0, 960, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(midi_take, false, false, 960, 1920, 1, 67, 88, false))
reaper.MIDI_Sort(midi_take)
reaper.UpdateItemInProject(midi_item)

reaper.Undo_OnStateChange2(0, "Mixed Audio and MIDI take fixture")
local function describe(label)
  output:write(label, "\ttrack_count=", reaper.CountTracks(0),
    "\titem_count=", reaper.CountMediaItems(0),
    "\tselected_count=", reaper.CountSelectedMediaItems(0), "\n")
  for track_index = 0, reaper.CountTracks(0) - 1 do
    local track = assert(reaper.GetTrack(0, track_index))
    local _, track_name = reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "", false)
    for item_index = 0, reaper.CountTrackMediaItems(track) - 1 do
      local item = assert(reaper.GetTrackMediaItem(track, item_index))
      local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
      output:write(label, "\ttrack=", track_index, "\ttrack_name=", track_name,
        "\titem=", item_index,
        "\tposition=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
        "\tlength=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
        "\tselected=", reaper.IsMediaItemSelected(item) and 1 or 0,
        "\ttake_count=", reaper.CountTakes(item), "\tactive_index=", active_index, "\n")
      for take_index = 0, reaper.CountTakes(item) - 1 do
        local take = assert(reaper.GetTake(item, take_index))
        local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
        local is_midi = reaper.TakeIsMIDI(take)
        local details
        if is_midi then
          local _, note_count = reaper.MIDI_CountEvts(take)
          local notes = {}
          for note_index = 0, note_count - 1 do
            local _, _, _, start_ppq, end_ppq, channel, pitch, velocity =
              reaper.MIDI_GetNote(take, note_index)
            notes[#notes + 1] = string.format("%d:%d:%d:%d:%d", start_ppq, end_ppq,
              channel, pitch, velocity)
          end
          details = "midi=1\tsource_offset_seconds=" ..
            string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")) ..
            "\tnotes=" .. table.concat(notes, ";")
        else
          local source = assert(reaper.GetMediaItemTake_Source(take))
          details = "midi=0\tsource=" .. reaper.GetMediaSourceFileName(source, "") ..
            "\toffset=" .. string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS"))
        end
        output:write(label, "\ttrack=", track_index, "\titem=", item_index,
          "\ttake=", take_index, "\tactive=", take_index == active_index and 1 or 0,
          "\tname=", name, "\t", details, "\n")
      end
    end
  end
  output:flush()
end

local function save(label)
  reaper.Main_SaveProjectEx(0, project_base .. "-" .. label .. ".rpp", 0)
end

reaper.SetMediaItemSelected(audio_item, true)
reaper.SetMediaItemSelected(midi_item, true)
describe("before")
save("before")
reaper.Undo_OnStateChange2(0, "Prepare mixed-media implode probe")
reaper.Main_OnCommand(40438, 0) -- Take: Implode items across tracks into takes.
describe("after")
save("after")
if reaper.CountMediaItems(0) ~= 1 then
  output:write("result\tno-op\n")
else
  reaper.Undo_DoUndo2(0)
  describe("undo")
  save("undo")
  reaper.Undo_DoRedo2(0)
  describe("redo")
  save("redo")
end
output:write("action\t40438\tTake: Implode items across tracks into takes\n")
output:write("target\t", target_kind, "\n")
output:write("reference\tREAPER 7.82/linux-x86_64; Default_7.0.ReaperThemeZip\n")
output:close()
local complete = assert(io.open(output_dir .. "/" .. output_prefix .. "-complete", "w"))
complete:write("complete\n")
complete:close()
reaper.UpdateArrange()
reaper.defer(function() end)
