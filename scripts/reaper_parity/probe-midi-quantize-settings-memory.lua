-- Run in a fresh REAPER 7.82 / Default 7 project. The script creates one MIDI
-- note and opens Quantize Events. Re-run it after closing the dialog to inspect
-- which settings REAPER remembers across Cancel, OK, and an application restart.
local output_root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))

local editor = reaper.MIDIEditor_GetActive()
if not editor then
  assert(reaper.CountTracks(0) == 0, "first run requires a fresh empty project")
  reaper.InsertTrackAtIndex(0, true)
  local track = assert(reaper.GetTrack(0, 0))
  local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
  local take = assert(reaper.GetActiveTake(item))
  assert(reaper.MIDI_InsertNote(take, true, false, 180, 420, 0, 60, 96, false))
  reaper.SetMediaItemSelected(item, true)
  reaper.Main_OnCommand(40153, 0) -- Open MIDI editor.
  editor = assert(reaper.MIDIEditor_GetActive())
end

local output = assert(io.open(output_root .. "midi-quantize-settings-memory-runs.tsv", "a"))
output:write(os.date("!%Y-%m-%dT%H:%M:%SZ"), "\topen_quantize_dialog\n")
output:close()
reaper.MIDIEditor_OnCommand(editor, 40009) -- Open Quantize Events.
