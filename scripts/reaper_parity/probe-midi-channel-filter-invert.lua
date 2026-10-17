-- Run in REAPER 7.82 with the Default 7 theme on an isolated profile.
-- Open the channel selector, choose Multichannel, and capture the native
-- Filter Events window. Click All to expose the per-channel checks, uncheck
-- Channel 2, and compare the visible notes with Invert off and on. The
-- channel checks should stay fixed while the visible channel set complements.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local output = assert(io.open(root .. "midi-channel-filter-invert-native.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI channel filter invert", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 8, false))
local take = assert(reaper.GetActiveTake(item))
local notes = {
  {0, 60, 96},
  {960, 62, 96},
  {1920, 64, 96},
  {2880, 65, 96},
  {3840, 67, 96},
  {4800, 69, 96},
}
for index, note in ipairs(notes) do
  local channel = (index - 1) % 3
  assert(reaper.MIDI_InsertNote(take, false, false, note[1], note[1] + 480,
    channel, note[2], note[3], false))
end
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive())
assert(reaper.MIDIEditor_GetTake(editor) == take)
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("notes_channel_pitch_tick\t")
for index, note in ipairs(notes) do
  local channel = (index - 1) % 3
  output:write(channel + 1, ",", note[2], ",", note[1], index == #notes and "\n" or ";")
end
output:flush()

local started = reaper.time_precise()
local duration = tonumber(os.getenv("AAADAW_REAPER_PROBE_DELAY_SECONDS") or "180") or 180
local function finish()
  if reaper.time_precise() - started < duration then
    reaper.defer(finish)
  else
    output:close()
  end
end
reaper.defer(finish)
