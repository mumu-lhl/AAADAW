-- Run in an isolated, unsaved Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_MIDI_SUBTICK_OUT to a TSV output path.
-- The probe uses the factory 120 BPM / 4/4 project and moves a selected MIDI
-- item with Main actions 41182-41184 at swing percentages that produce
-- fractional project-PPQ positions on a quarter-note grid.
local output_path = assert(os.getenv("AAADAW_MIDI_SUBTICK_OUT"))
local output = assert(io.open(output_path, "w"))

assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
local theme = reaper.GetLastColorThemeFile()
assert(theme:match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")

-- At the factory tempo, project time 0.5s is quarter-note 1.0.
local quarter_note_at_half_second = reaper.TimeMap2_timeToQN(0, 0.5)
assert(math.abs(quarter_note_at_half_second - 1.0) < 1e-9)
local bpm, numerator, denominator = reaper.TimeMap_GetTimeSigAtTime(0, 0.5)
assert(math.abs(bpm - 120.0) < 1e-9 and numerator == 4 and denominator == 4)

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 0.0625, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 120, 60, 60, 100, false))
reaper.MIDI_Sort(take)

local actions = {
  {41182, "left"},
  {41183, "right"},
  {41184, "nearest"},
}
local swing_percents = {0, 1, 3, 7, 17, 25, 33, 50, 100, -1, -3, -7, -17, -25, -50}

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("audio_device\tClosed; no playback or hardware I/O\n")
output:write("project\tFactory 120 BPM, 4/4; quarter-note grid; empty unsaved project\n")
output:write("ppq\t960; project time 0.5s = quarter note 1.0\n")
output:write("actions\t41182 left; 41183 right; 41184 nearest\n")
output:write("swing\tinteger percentages; both signs; global Snap on/off\n")
output:write("swing_percent\tsnap_enabled\tinput_seconds\taction_id\taction\titem_before_seconds\titem_after_seconds\titem_length_after_seconds\toutput_project_ppq\titem_selected_after\n")

local function set_snap_enabled(enabled)
  local current = reaper.GetToggleCommandStateEx(0, 1157) == 1
  if current ~= enabled then
    reaper.Main_OnCommand(1157, 0)
  end
end

for _, percent in ipairs(swing_percents) do
  local amount = percent / 100
  reaper.GetSetProjectGrid(0, true, 0.25, percent == 0 and 0 or 1, amount)
  local _, division, swing_mode, swing_amount = reaper.GetSetProjectGrid(0, false, 0, 0, 0)
  assert(math.abs(division - 0.25) < 1e-9)
  assert(swing_mode == (percent == 0 and 0 or 1))
  assert(math.abs(swing_amount - amount) < 1e-9)

  for _, snap_enabled in ipairs({true, false}) do
    set_snap_enabled(snap_enabled)
    for _, action in ipairs(actions) do
      -- Left from an even grid point, and Right/Nearest from the odd point.
      -- These positions make the shifted odd grid candidate the action target.
      local input = action[1] == 41182 and 1.0 or 0.5
      reaper.SetMediaItemInfo_Value(item, "D_POSITION", input)
      reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 0.0625)
      reaper.SetMediaItemSelected(item, true)
      reaper.UpdateItemInProject(item)

      local before = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
      assert(math.abs(before - input) < 1e-9)
      assert(reaper.IsMediaItemSelected(item))
      reaper.Main_OnCommand(action[1], 0)

      local after = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
      local length_after = reaper.GetMediaItemInfo_Value(item, "D_LENGTH")
      local project_ppq = reaper.TimeMap2_timeToQN(0, after) * 960
      local selected_after = reaper.IsMediaItemSelected(item)
      output:write(string.format(
        "%d\t%s\t%.6f\t%d\t%s\t%.9f\t%.12f\t%.9f\t%.9f\t%s\n",
        percent,
        tostring(snap_enabled),
        input,
        action[1],
        action[2],
        before,
        after,
        length_after,
        project_ppq,
        tostring(selected_after)
      ))
    end
  end
end

output:flush()
output:close()
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()
