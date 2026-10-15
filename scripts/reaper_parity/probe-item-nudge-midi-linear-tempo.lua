-- Run in a fresh, unsaved Linux REAPER 7.82 / Default 7 project.
-- Set Target=contents, 250 milliseconds, Snap off in Nudge/Set Items, then
-- write `right`, `left`, or `done` to the probe command file.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "item-nudge-midi-linear-tempo-native.tsv", "w"))
local ramp_ok = reaper.SetTempoTimeSigMarker(0, -1, 0.0, -1, -1, 120.0, 4, 4, true)
local end_ok = reaper.SetTempoTimeSigMarker(0, -1, -1, 2, 0.0, 60.0, 4, 4, false)
assert(ramp_ok and end_ok, "could not create the 120-to-60 BPM linear tempo ramp")
reaper.UpdateTimeline()

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Variable tempo MIDI nudge", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 3.9, 8.9, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 960, 1920, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(take, false, false, 2880, 3840, 0, 64, 108, false))
reaper.MIDI_Sort(take)
assert(reaper.SetMediaItemInfo_Value(item, "B_LOOPSRC", 1))
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(3.9, false, false)
reaper.UpdateArrange()

local function write_state(label, state_change_delta)
  local position = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
  local length = reaper.GetMediaItemInfo_Value(item, "D_LENGTH")
  local offset = reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")
  local _, note_count = reaper.MIDI_CountEvts(take)
  local notes = {}
  for index = 0, note_count - 1 do
    local ok, selected, muted, start_ppq, end_ppq, channel, pitch, velocity =
      reaper.MIDI_GetNote(take, index)
    assert(ok, "could not read MIDI note " .. tostring(index))
    notes[#notes + 1] = table.concat({
      index,
      selected and "1" or "0",
      muted and "1" or "0",
      string.format("%.3f", start_ppq),
      string.format("%.3f", end_ppq),
      channel,
      pitch,
      velocity,
    }, ",")
  end
  output:write(label,
    "\tposition_s\t", string.format("%.9f", position),
    "\tlength_s\t", string.format("%.9f", length),
    "\tloop_source\t", reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC"),
    "\tsource_offset_s\t", string.format("%.9f", offset),
    "\tppq_at_item_start\t", string.format("%.3f", reaper.MIDI_GetPPQPosFromProjTime(take, position)),
    "\tppq_at_item_plus_250ms\t", string.format("%.3f", reaper.MIDI_GetPPQPosFromProjTime(take, position + 0.25)),
    "\tppq_at_item_end\t", string.format("%.3f", reaper.MIDI_GetPPQPosFromProjTime(take, position + length)),
    "\tnotes\t", table.concat(notes, ";"),
    "\tselected\t", reaper.IsMediaItemSelected(item) and "1" or "0",
    "\tstate_change_delta\t", tostring(state_change_delta or 0),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("tempo_map\t120 BPM at project start; linear to 60 BPM at measure 3 (5.333333 seconds)\n")
for index = 0, reaper.CountTempoTimeSigMarkers(0) - 1 do
  local ok, timepos, measurepos, beatpos, bpm, numerator, denominator, linear =
    reaper.GetTempoTimeSigMarker(0, index)
  assert(ok, "could not read tempo marker " .. tostring(index))
  output:write("tempo_marker\t", index,
    "\ttime_s\t", string.format("%.9f", timepos),
    "\tmeasure\t", measurepos,
    "\tbeat\t", string.format("%.6f", beatpos),
    "\tbpm\t", string.format("%.6f", bpm),
    "\tmeter\t", numerator, "/", denominator,
    "\tlinear\t", linear and "1" or "0", "\n")
end
output:write("probe\tMIDI loop item from 3.900 to 8.900 seconds starts inside the 0.000-to-5.333333-second linear tempo ramp; 250 ms Nudge remains inside that ramp\n")
output:write("nudge\tTarget contents; 250 milliseconds; Snap off; set in the actual Nudge/Set dialog\n")
output:write("actions\t41249 right; 41250 left\n")
write_state("before", 0)
reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()

local command_path = root .. "item-nudge-midi-linear-tempo-command.txt"
local function poll_command()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    if command == "left" or command == "right" then
      local action = command == "left" and 41250 or 41249
      local before = reaper.GetProjectStateChangeCount(0)
      reaper.Main_OnCommand(action, 0)
      reaper.UpdateArrange()
      write_state("after_" .. command,
        reaper.GetProjectStateChangeCount(0) - before)
    elseif command == "done" then
      output:close()
      return
    end
  end
  reaper.defer(poll_command)
end
poll_command()
