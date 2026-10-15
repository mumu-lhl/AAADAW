-- Probe Nudge -> Contents at the source-offset boundary for non-loop MIDI.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local source_offset = tonumber(os.getenv("AAADAW_REAPER_PROBE_SOURCE_OFFSET_SECONDS") or "0.1")
assert(source_offset and source_offset >= 0, "source offset must be nonnegative")
local first_note_start = tonumber(os.getenv("AAADAW_REAPER_PROBE_FIRST_NOTE_START_PPQ") or "2880")
assert(first_note_start and first_note_start >= 0, "first note start must be nonnegative")
local output = assert(io.open(root .. "item-nudge-midi-nonloop.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI Nudge Contents", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 2, 7, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 0))
assert(reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", source_offset))
assert(reaper.MIDI_InsertNote(take, false, false, first_note_start, first_note_start + 1920, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(take, false, false, 6720, 8640, 0, 64, 108, false))
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(4, false, false)
reaper.UpdateArrange()
reaper.Undo_OnStateChange2(0, "Fixture setup")

local function refresh()
  track = assert(reaper.GetTrack(0, 0))
  item = assert(reaper.GetTrackMediaItem(track, 0))
  take = assert(reaper.GetActiveTake(item))
end

local function note_state()
  local notes = {}
  local _, note_count = reaper.MIDI_CountEvts(take)
  for index = 0, note_count - 1 do
    local ok, selected, muted, start_ppq, end_ppq, channel, pitch, velocity =
      reaper.MIDI_GetNote(take, index)
    assert(ok, "could not read MIDI note " .. tostring(index))
    notes[#notes + 1] = table.concat({index, selected and 1 or 0, muted and 1 or 0,
      string.format("%.3f", start_ppq), string.format("%.3f", end_ppq), channel, pitch, velocity}, ",")
  end
  return table.concat(notes, ";")
end

local function write_state(label)
  refresh()
  local position = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
  local length = reaper.GetMediaItemInfo_Value(item, "D_LENGTH")
  local source = reaper.GetMediaItemTake_Source(take)
  local source_length, source_length_is_qn = reaper.GetMediaSourceLength(source)
  local phase = reaper.MIDI_GetPPQPosFromProjTime(take, position)
  local endpoint = reaper.MIDI_GetPPQPosFromProjTime(take, position + length)
  output:write(table.concat({label,
    string.format("%.9f", position), string.format("%.9f", length),
    tostring(reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC")),
    string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
    string.format("%.9f", source_length), tostring(source_length_is_qn),
    string.format("%.3f", phase), string.format("%.3f", endpoint), note_state(),
    reaper.IsMediaItemSelected(item) and "selected" or "unselected",
    tostring(reaper.GetProjectStateChangeCount(0)), tostring(reaper.Undo_CanUndo2(0) or "")}, "\t"), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("dialog_action\t41226\tItem edit: Nudge/Set...\n")
output:write("columns\tlabel\tposition_s\tlength_s\tloop_source\tsource_offset_s\tsource_length\tsource_length_is_qn\tphase_ppq\tendpoint_ppq\tnotes\tselection\tstate_change\tundo_label\n")
write_state("before")
reaper.Main_OnCommand(41226, 0)

local command_path = root .. "item-nudge-midi-nonloop-command.txt"
local function poll()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    if command == "snapshot" then write_state("snapshot") end
    if command == "right" then
      reaper.Main_OnCommand(41249, 0)
      reaper.UpdateArrange()
      write_state("right")
    end
    if command == "left" then
      reaper.Main_OnCommand(41250, 0)
      reaper.UpdateArrange()
      write_state("left")
    end
    if command == "done" then output:close(); return end
  end
  reaper.defer(poll)
end
poll()
