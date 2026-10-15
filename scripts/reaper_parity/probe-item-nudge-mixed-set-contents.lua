-- Probe absolute Set -> Contents for a mixed Audio/MIDI selection.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"))
local output = assert(io.open(root .. "item-nudge-mixed-set-contents.tsv", "w"))

reaper.InsertTrackAtIndex(0, true)
local audio_track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(audio_track, "P_NAME", "Set Audio", true)
reaper.SetOnlyTrackSelected(audio_track)
reaper.SetEditCurPos(2, false, false)
assert(reaper.InsertMedia(source_path, 0) == 1, "could not insert WAV")
local audio_item = assert(reaper.GetTrackMediaItem(audio_track, 0))
local audio_take = assert(reaper.GetActiveTake(audio_item))
assert(reaper.SetMediaItemInfo_Value(audio_item, "D_POSITION", 2))
assert(reaper.SetMediaItemInfo_Value(audio_item, "D_LENGTH", 5))
assert(reaper.SetMediaItemInfo_Value(audio_item, "B_LOOPSRC", 1))
assert(reaper.SetMediaItemTakeInfo_Value(audio_take, "D_STARTOFFS", 0.1))

reaper.InsertTrackAtIndex(1, true)
local midi_track = assert(reaper.GetTrack(0, 1))
reaper.GetSetMediaTrackInfo_String(midi_track, "P_NAME", "Set MIDI", true)
local midi_item = assert(reaper.CreateNewMIDIItemInProj(midi_track, 2, 7, false))
local midi_take = assert(reaper.GetActiveTake(midi_item))
assert(reaper.SetMediaItemInfo_Value(midi_item, "B_LOOPSRC", 1))
assert(reaper.MIDI_InsertNote(midi_take, false, false, 0, 960, 0, 60, 100, false))
assert(reaper.MIDI_InsertNote(midi_take, false, false, 4800, 5760, 0, 64, 100, false))
reaper.MIDI_Sort(midi_take)
reaper.SetMediaItemSelected(audio_item, true)
reaper.SetMediaItemSelected(midi_item, true)
reaper.SetEditCurPos(7, false, false)
reaper.UpdateArrange()

local function write_state(label)
  local source = reaper.GetMediaItemTake_Source(audio_take)
  local source_length = reaper.GetMediaSourceLength(source)
  local midi_source = reaper.GetMediaItemTake_Source(midi_take)
  local midi_source_length, midi_length_is_qn = reaper.GetMediaSourceLength(midi_source)
  local _, note_count = reaper.MIDI_CountEvts(midi_take)
  local audio_selected = reaper.IsMediaItemSelected(audio_item) and "selected" or "unselected"
  local midi_selected = reaper.IsMediaItemSelected(midi_item) and "selected" or "unselected"
  output:write(table.concat({label,
    string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_item, "D_POSITION")),
    string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_item, "D_LENGTH")),
    tostring(reaper.GetMediaItemInfo_Value(audio_item, "B_LOOPSRC")),
    string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(audio_take, "D_STARTOFFS")),
    string.format("%.9f", source_length), audio_selected,
    string.format("%.9f", reaper.GetMediaItemInfo_Value(midi_item, "D_POSITION")),
    string.format("%.9f", reaper.GetMediaItemInfo_Value(midi_item, "D_LENGTH")),
    tostring(reaper.GetMediaItemInfo_Value(midi_item, "B_LOOPSRC")),
    string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(midi_take, "D_STARTOFFS")),
    string.format("%.9f", midi_source_length), tostring(midi_length_is_qn),
    tostring(note_count), midi_selected,
    tostring(reaper.GetProjectStateChangeCount(0)), tostring(reaper.Undo_CanUndo2(0) or "")}, "\t"), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("source_path\t", source_path, "\n")
output:write("columns\tlabel\taudio_position_s\taudio_length_s\taudio_loop_source\taudio_offset_s\taudio_source_length_s\taudio_selection\tmidi_position_s\tmidi_length_s\tmidi_loop_source\tmidi_offset_s\tmidi_source_length\tmidi_length_is_qn\tmidi_note_count\tmidi_selection\tstate_change\tundo_label\n")
write_state("before")
reaper.Main_OnCommand(41226, 0)

local command_path = root .. "item-nudge-mixed-set-contents-command.txt"
local function poll()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    if command == "snapshot" then write_state("snapshot") end
    if command == "undo" then
      reaper.Main_OnCommand(40029, 0)
      write_state("undo")
    end
    if command == "redo" then
      reaper.Main_OnCommand(40030, 0)
      write_state("redo")
    end
    if command == "done" then output:close(); return end
  end
  reaper.defer(poll)
end
poll()
