-- Run in an isolated, unsaved Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_SNAP_SWING_WAV to a short WAV and AAADAW_SNAP_SWING_OUT to a TSV path.
-- The probe creates one Audio and one MIDI item, selects each explicitly, sets
-- several project swing-grid modes, and invokes Main actions 41182-41184.
local wav = assert(os.getenv("AAADAW_SNAP_SWING_WAV"))
local output_path = assert(os.getenv("AAADAW_SNAP_SWING_OUT"))
local output = assert(io.open(output_path, "w"))

assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")

local function add_track(name, index)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  assert(track)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  return track
end

local audio_track = add_track("Audio", 0)
local midi_track = add_track("MIDI", 1)
local audio_item = reaper.AddMediaItemToTrack(audio_track)
local audio_take = reaper.AddTakeToMediaItem(audio_item)
local audio_source = reaper.PCM_Source_CreateFromFile(wav)
assert(audio_item and audio_take and audio_source)
assert(reaper.SetMediaItemTake_Source(audio_take, audio_source))
reaper.SetMediaItemInfo_Value(audio_item, "D_LENGTH", 0.0625)
local midi_item = reaper.CreateNewMIDIItemInProj(midi_track, 0, 0.0625, false)
assert(midi_item)

local actions = {
  {41182, "left"},
  {41183, "right"},
  {41184, "nearest"},
}
local grid_cases = {
  {"straight", 0, 0.0},
  {"swing_25", 1, 0.25},
  {"swing_50", 1, 0.5},
  {"swing_100", 1, 1.0},
  {"swing_minus_50", 1, -0.5},
}
local item_cases = {
  {"AUDIO", audio_item},
  {"MIDI", midi_item},
}
local input_seconds = {
  0.5,
  0.5625,
  0.625,
  1.0,
  1.3125,
  1.5,
  1.625,
  2.0,
  2.5,
}

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("audio_device\tClosed; no playback or hardware I/O\n")
output:write("project_grid\tGetSetProjectGrid division=0.25 (quarter note), 120 BPM, 4/4\n")
output:write("swing_api\tswingmode=0 straight; swingmode=1 enables swing; swingamt is -1..1\n")
output:write("actions\t41182 left; 41183 right; 41184 nearest\n")
output:write("case\titem_type\tswing_mode\tswing_amount\tsnap_enabled\tinput_seconds\taction_id\taction\titem_before_seconds\titem_after_seconds\titem_length_after_seconds\titem_selected_after\n")

local function set_snap_enabled(enabled)
  local current = reaper.GetToggleCommandStateEx(0, 1157) == 1
  if current ~= enabled then
    reaper.Main_OnCommand(1157, 0)
  end
end

for _, grid_case in ipairs(grid_cases) do
  reaper.GetSetProjectGrid(0, true, 0.25, grid_case[2], grid_case[3])
  local _, division, swing_mode, swing_amount = reaper.GetSetProjectGrid(0, false, 0, 0, 0)
  assert(math.abs(division - 0.25) < 1e-9)
  assert(swing_mode == grid_case[2])
  assert(math.abs(swing_amount - grid_case[3]) < 1e-9)

  for _, snap_enabled in ipairs({true, false}) do
    set_snap_enabled(snap_enabled)
    for _, item_case in ipairs(item_cases) do
      for _, input in ipairs(input_seconds) do
        for _, action in ipairs(actions) do
          reaper.SetMediaItemSelected(audio_item, item_case[1] == "AUDIO")
          reaper.SetMediaItemSelected(midi_item, item_case[1] == "MIDI")
          reaper.SetMediaItemInfo_Value(item_case[2], "D_POSITION", input)
          reaper.SetMediaItemInfo_Value(item_case[2], "D_LENGTH", 0.0625)
          reaper.UpdateItemInProject(item_case[2])
          local before = reaper.GetMediaItemInfo_Value(item_case[2], "D_POSITION")
          assert(math.abs(before - input) < 1e-9)
          reaper.Main_OnCommand(action[1], 0)

          local after = reaper.GetMediaItemInfo_Value(item_case[2], "D_POSITION")
          local length_after = reaper.GetMediaItemInfo_Value(item_case[2], "D_LENGTH")
          local selected_after = reaper.IsMediaItemSelected(item_case[2])
          output:write(string.format(
            "%s\t%s\t%d\t%.3f\t%s\t%.6f\t%d\t%s\t%.9f\t%.9f\t%.9f\t%s\n",
            grid_case[1], item_case[1], swing_mode, swing_amount, tostring(snap_enabled), input,
            action[1], action[2], before, after, length_after, tostring(selected_after)
          ))
        end
      end
    end
  end
end

output:flush()
output:close()
reaper.GetSetProjectGrid(0, true, 0.25, 1, 0.5)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 6.5)
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()
reaper.Main_OnCommand(40071, 0) -- Options: Show snap/grid settings
