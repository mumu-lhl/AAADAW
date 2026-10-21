-- Run in the unsaved project created by probe-item-nudge-relative-units.lua.
-- Configure the visible Nudge dialog to target `edit cursor`, select the note
-- unit/value and Snap setting, then issue reset|label or right|label through
-- the command file. Results are written to item-nudge-edit-cursor-native.tsv.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local command_path = root .. "item-nudge-edit-cursor-command.txt"
local output = assert(io.open(root .. "item-nudge-edit-cursor-native.tsv", "w"))
local audio_track = assert(reaper.GetTrack(0, 0))
local midi_track = assert(reaper.GetTrack(0, 1))
local audio_item = assert(reaper.GetTrackMediaItem(audio_track, 0))
local midi_item = assert(reaper.GetTrackMediaItem(midi_track, 0))

local function write_state(label)
  output:write(label,
    "\tstate_change_count\t", reaper.GetProjectStateChangeCount(0),
    "\tedit_cursor_s\t", string.format("%.9f", reaper.GetCursorPosition()),
    "\taudio_position_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(audio_item, "D_POSITION")),
    "\taudio_selected\t", reaper.IsMediaItemSelected(audio_item) and "1" or "0",
    "\tmidi_position_s\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(midi_item, "D_POSITION")),
    "\tmidi_selected\t", reaper.IsMediaItemSelected(midi_item) and "1" or "0", "\n")
  output:flush()
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("project_sample_rate\t", reaper.GetSetProjectInfo(0, "PROJECT_SRATE", 0, false), "\n")
output:write("project_bpm\t", reaper.Master_GetTempo(), "\n")
output:write("nudge_right_action\t41249\n")
output:flush()

local function poll_command()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    if command then
      os.remove(command_path)
      local action, label = command:match("^([^|]+)|?(.*)$")
      if action == "reset" then
        reaper.SetEditCurPos(1.27, false, false)
        write_state("baseline_" .. label)
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
