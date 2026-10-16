-- Run in Linux REAPER 7.82 with an isolated Default 7 configuration and an empty unsaved project.
-- Opens the native Render dialog with selected MIDI items on a ReaSynth track. Set the Render
-- source to Selected media items, preview the pattern, then render in the disposable probe folder.
-- The middle same-named MIDI item is unselected so $namenumber scope can be compared directly.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")
assert(reaper.RecursiveCreateDirectory(root .. "selected-midi-items", 0) >= 0)

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI render fixture", true)
reaper.SetTrackSelected(track, true)
local synth = reaper.TrackFX_AddByName(track, "ReaSynth", false, -1000)
assert(synth >= 0, "The installed ReaSynth instrument must be available")

local midi_items = {}
for index, start in ipairs({ 0, 2, 4 }) do
  local item = assert(reaper.CreateNewMIDIItemInProj(track, start, start + 1, false))
  local take = assert(reaper.GetActiveTake(item))
  reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "MIDI Hit", true)
  assert(reaper.MIDI_InsertNote(take, false, false, 0, 960, 0, 60, 100, true))
  assert(reaper.MIDI_Sort(take))
  reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
  reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
  reaper.SetMediaItemSelected(item, index ~= 2)
  midi_items[#midi_items + 1] = item
end
reaper.UpdateArrange()
for field, value in pairs({
  RENDER_SETTINGS = 0,
  RENDER_BOUNDSFLAG = 0,
  RENDER_STARTPOS = 0,
  RENDER_ENDPOS = 5,
  RENDER_SRATE = 48000,
  RENDER_CHANNELS = 2,
  RENDER_TAILFLAG = 0,
  RENDER_NORMALIZE = 0,
}) do
  reaper.GetSetProjectInfo(0, field, value, true)
end
reaper.GetSetProjectInfo_String(0, "RENDER_FILE", root .. "selected-midi-items/", true)
reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "$item-$namenumber-$itemnumber", true)
reaper.GetSetProjectInfo_String(0, "RENDER_FORMAT", "evaw", true)
reaper.Main_OnCommand(40015, 0)

local ready = assert(io.open(root .. "midi-item-render-dialog-ready", "w"))
ready:write("Selected MIDI items with a ReaSynth track instrument; the middle matching name is unselected\n")
ready:close()
reaper.defer(function() end)
