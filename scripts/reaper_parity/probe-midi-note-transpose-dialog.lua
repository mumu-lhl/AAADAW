-- Run in a fresh Linux REAPER 7.82 / Default 7 project with an isolated profile.
-- Opens MIDI Editor action 40759 so its native controls can be inspected.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-note-transpose-dialog-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI transpose dialog", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
local take = assert(reaper.GetActiveTake(item))
for index, note in ipairs({
  {true, 60}, {true, 64}, {true, 67}, {false, 72},
}) do
  local start = (index - 1) * 480
  assert(reaper.MIDI_InsertNote(take, note[1], false, start, start + 240,
    index % 16, note[2], 90 + index, false))
end
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive(), "MIDI editor did not open")
assert(reaper.MIDIEditor_GetTake(editor) == take)

local function write_events(label)
  local _, note_count = reaper.MIDI_CountEvts(take)
  local rows = {}
  for index = 0, note_count - 1 do
    local ok, selected, muted, start, finish, channel, pitch, velocity = reaper.MIDI_GetNote(take, index)
    assert(ok, "could not read MIDI note " .. tostring(index))
    rows[#rows + 1] = table.concat({selected and 1 or 0, muted and 1 or 0,
      start, finish, channel + 1, pitch, velocity}, ",")
  end
  output:write(label, "\t", table.concat(rows, ";"), "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
write_events("before")
output:write("dialog_action\t40759\n")
output:flush()
local started_at = reaper.time_precise()
local function open_dialog()
  if reaper.time_precise() - started_at < 5 then
    reaper.defer(open_dialog)
    return
  end
  reaper.MIDIEditor_OnCommand(editor, 40759)
  local interval_count = 0
  local last_record = reaper.time_precise() - 0.5
  local deadline = reaper.time_precise() + 180
  local function record_events()
    local now = reaper.time_precise()
    if now - last_record < 0.5 then
      reaper.defer(record_events)
      return
    end
    last_record = now
    interval_count = interval_count + 1
    write_events("sample_" .. interval_count)
    if now < deadline then
      reaper.defer(record_events)
    else
      output:close()
    end
  end
  reaper.defer(record_events)
end
reaper.defer(open_dialog)
