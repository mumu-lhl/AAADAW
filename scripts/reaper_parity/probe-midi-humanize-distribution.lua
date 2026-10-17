-- Reproducible sampling probe for the REAPER 7.82 MIDI Editor Humanize dialog.
-- Launch in a fresh project and use the GUI to set the requested values, click
-- New random seed when testing a new stream, then click OK. Each run records
-- raw MIDI note readbacks before and after the native action.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local sample_id = assert(os.getenv("AAADAW_REAPER_HUMANIZE_SAMPLE_ID"))
assert(sample_id:match("^[%w%-_]+$"), "sample id must be filename-safe")
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local project, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")
local requested_project_grid_division =
  tonumber(os.getenv("AAADAW_REAPER_PROJECT_GRID_DIVISION"))
if requested_project_grid_division then
  assert(requested_project_grid_division > 0)
  reaper.SetProjectGrid(project, requested_project_grid_division)
end
local requested_tempo = tonumber(os.getenv("AAADAW_REAPER_HUMANIZE_TEMPO"))
if requested_tempo then
  assert(requested_tempo > 0)
  assert(reaper.SetTempoTimeSigMarker(project, -1, 0, -1, -1, requested_tempo,
    4, 4, false))
end

local velocity_text = os.getenv("AAADAW_REAPER_HUMANIZE_VELOCITIES") or "1,2,64,126,127"
local velocities = {}
for value in velocity_text:gmatch("[^,]+") do
  local velocity = assert(tonumber(value), "invalid velocity in sample list")
  assert(velocity >= 0 and velocity <= 127 and velocity % 1 == 0)
  velocities[#velocities + 1] = velocity
end
assert(#velocities >= 1 and #velocities <= 64)
local note_count = tonumber(os.getenv("AAADAW_REAPER_HUMANIZE_NOTE_COUNT") or #velocities)
local start_tick = tonumber(os.getenv("AAADAW_REAPER_HUMANIZE_START_TICK") or "110")
local interval_ticks = tonumber(os.getenv("AAADAW_REAPER_HUMANIZE_INTERVAL_TICKS") or "180")
local item_length_seconds = tonumber(os.getenv("AAADAW_REAPER_HUMANIZE_ITEM_LENGTH_SECONDS") or "4")
assert(note_count and note_count >= 1 and note_count <= 512 and note_count % 1 == 0)
assert(start_tick and start_tick >= 0 and start_tick % 1 == 0)
assert(interval_ticks and interval_ticks > 0 and interval_ticks % 1 == 0)
assert(item_length_seconds and item_length_seconds > 0)

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI Humanize RNG", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, item_length_seconds, false))
local take = assert(reaper.GetActiveTake(item))
local item_end_tick = math.floor(reaper.MIDI_GetPPQPosFromProjTime(take, item_length_seconds))
local unselected_start_tick = item_end_tick - 300
assert(start_tick + (note_count - 1) * interval_ticks + 90 < unselected_start_tick)
for index = 1, note_count do
  local velocity = velocities[((index - 1) % #velocities) + 1]
  local start_ppq = start_tick + (index - 1) * interval_ticks
  assert(reaper.MIDI_InsertNote(take, true, false, start_ppq, start_ppq + 90,
    0, 60 + ((index - 1) % 60), velocity, false))
end
assert(reaper.MIDI_InsertNote(take, false, false, unselected_start_tick,
  unselected_start_tick + 90, 0, 100, 64, false))
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
local requested_midi_grid_division =
  tonumber(os.getenv("AAADAW_REAPER_HUMANIZE_GRID_DIVISION"))
if requested_midi_grid_division then
  assert(requested_midi_grid_division > 0)
  reaper.SetMIDIEditorGrid(project, requested_midi_grid_division)
end
local grid_qn, swing = reaper.MIDI_GetGrid(take)
local project_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, 1)
  - reaper.MIDI_GetPPQPosFromProjQN(take, 0)
local _, project_grid_division, project_swing_mode, project_swing_amount =
  reaper.GetSetProjectGrid(project, false, 0, 0, 0)
local output = assert(io.open(root .. "midi-humanize-sample-" .. sample_id .. ".tsv", "w"))
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("sample_id\t", sample_id, "\n")
output:write("project_ppq\t", string.format("%.0f", project_ppq), "\n")
output:write("midi_grid_quarter_notes\t", string.format("%.6f", grid_qn), "\n")
output:write("midi_grid_ticks\t", string.format("%.0f", grid_qn * project_ppq), "\n")
output:write("swing\t", string.format("%.6f", swing), "\n")
output:write("selected_note_count\t", note_count, "\n")
output:write("selected_start_tick\t", start_tick, "\n")
output:write("selected_interval_ticks\t", interval_ticks, "\n")
output:write("item_length_seconds\t", string.format("%.3f", item_length_seconds), "\n")
if requested_tempo then
  output:write("requested_tempo_bpm\t", string.format("%.3f", requested_tempo), "\n")
end
output:write("item_end_tick\t", string.format("%.0f", item_end_tick), "\n")
output:write("project_grid_division\t", string.format("%.6f", project_grid_division), "\n")
output:write("project_swing_mode\t", project_swing_mode, "\n")
output:write("project_swing_amount\t", string.format("%.6f", project_swing_amount), "\n")
if requested_midi_grid_division then
  output:write("requested_midi_grid_division\t",
    string.format("%.6f", requested_midi_grid_division), "\n")
end
if requested_project_grid_division then
  output:write("requested_project_grid_division\t",
    string.format("%.6f", requested_project_grid_division), "\n")
end

local function write_notes(label)
  local _, note_count = reaper.MIDI_CountEvts(take)
  local values = {}
  for index = 0, note_count - 1 do
    local ok, selected, muted, start_ppq, end_ppq, channel, pitch, velocity =
      reaper.MIDI_GetNote(take, index)
    assert(ok, "could not read MIDI note " .. tostring(index))
    values[#values + 1] = table.concat({selected and 1 or 0, channel + 1, pitch,
      string.format("%.0f", start_ppq), string.format("%.0f", end_ppq), velocity}, ",")
  end
  output:write(label, "\tnotes=selected,channel,pitch,start,end,velocity:",
    table.concat(values, ";"), "\n")
  output:flush()
end

write_notes("before")
reaper.Main_SaveProjectEx(0, root .. "midi-humanize-sample-" .. sample_id .. "-before.rpp", 0)
reaper.MIDIEditor_OnCommand(editor, 40457)
local started = reaper.time_precise()
local delay = tonumber(os.getenv("AAADAW_REAPER_PROBE_DELAY_SECONDS") or "120") or 120
local function finish()
  if reaper.time_precise() - started < delay then
    reaper.defer(finish)
    return
  end
  write_notes("after")
  reaper.Main_SaveProjectEx(0, root .. "midi-humanize-sample-" .. sample_id .. "-after.rpp", 0)
  output:close()
end
reaper.defer(finish)
