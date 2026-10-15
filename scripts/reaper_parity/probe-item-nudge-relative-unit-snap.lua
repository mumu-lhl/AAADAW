-- Run in the unsaved fixture project created by
-- probe-item-nudge-relative-units.lua. Configure the visible native Nudge
-- dialog, issue reset|label|selection or right|label via the command file, and
-- inspect item-nudge-relative-unit-snap-native.tsv.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local command_path = root .. "item-nudge-relative-unit-snap-command.txt"
local output = assert(io.open(root .. "item-nudge-relative-unit-snap-native.tsv", "w"))
local audio_track = assert(reaper.GetTrack(0, 0))
local midi_track = assert(reaper.GetTrack(0, 1))
local audio_items = {
  assert(reaper.GetTrackMediaItem(audio_track, 0)),
  assert(reaper.GetTrackMediaItem(audio_track, 1)),
}
local midi_item = assert(reaper.GetTrackMediaItem(midi_track, 0))

local function write_state(label)
  output:write(label,
    "\tstate_change_count\t", reaper.GetProjectStateChangeCount(0),
    "\tundo_label\t", tostring(reaper.Undo_CanUndo2(0) or ""),
    "\taudio_a_position_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_items[1], "D_POSITION")),
    "\taudio_a_length_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_items[1], "D_LENGTH")),
    "\taudio_a_selected\t", reaper.IsMediaItemSelected(audio_items[1]) and "1" or "0",
    "\taudio_b_position_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_items[2], "D_POSITION")),
    "\taudio_b_length_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_items[2], "D_LENGTH")),
    "\taudio_b_selected\t", reaper.IsMediaItemSelected(audio_items[2]) and "1" or "0",
    "\tmidi_position_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(midi_item, "D_POSITION")),
    "\tmidi_length_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(midi_item, "D_LENGTH")),
    "\tmidi_selected\t", reaper.IsMediaItemSelected(midi_item) and "1" or "0", "\n")
  output:flush()
end

local function reset(label, selection)
  reaper.SetMediaItemInfo_Value(audio_items[1], "D_POSITION", 1.27)
  reaper.SetMediaItemInfo_Value(audio_items[1], "D_LENGTH", 0.5)
  reaper.SetMediaItemInfo_Value(audio_items[2], "D_POSITION", 4.1)
  reaper.SetMediaItemInfo_Value(audio_items[2], "D_LENGTH", 1.0)
  reaper.SetMediaItemInfo_Value(midi_item, "D_POSITION", 0.0)
  reaper.SetMediaItemInfo_Value(midi_item, "D_LENGTH", 8.0)
  reaper.SetMediaItemSelected(audio_items[1], selection:find("A", 1, true) ~= nil)
  reaper.SetMediaItemSelected(audio_items[2], selection:find("B", 1, true) ~= nil)
  reaper.SetMediaItemSelected(midi_item, selection:find("M", 1, true) ~= nil)
  reaper.UpdateArrange()
  write_state("baseline_" .. label)
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("project_sample_rate\t", reaper.GetSetProjectInfo(0, "PROJECT_SRATE", 0, false), "\n")
output:write("project_bpm\t", reaper.GetSetProjectInfo(0, "PROJECT_BPM", 0, false), "\n")
output:write("nudge_right_action\t41249\n")
output:write("fixture\tAudio A at 1.270s / 0.500s; Audio B at 4.100s / 1.000s; MIDI item at 0-8s\n")
output:flush()

local function poll_command()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    if command then
      os.remove(command_path)
      local action, label, selection = command:match("^([^|]+)|?([^|]*)|?([^|]*)$")
      if action == "reset" then
        reset(label, selection)
      elseif action == "right" or action == "left" then
        reaper.Main_OnCommand(action == "right" and 41249 or 41250, 0)
        reaper.UpdateArrange()
        write_state("after_" .. action .. "_" .. label)
      elseif action == "done" then
        output:close()
        return
      end
    end
  end
  reaper.defer(poll_command)
end

poll_command()
