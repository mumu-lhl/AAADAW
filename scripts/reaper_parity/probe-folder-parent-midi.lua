-- Run in an isolated Linux REAPER 7.82 / Default 7 project.
-- A JSFX on a folder parent counts note-on/off events from its MIDI child.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use an empty disposable project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")
assert(reaper.GetPlayStateEx(0) == 0, "Transport must be stopped")

reaper.InsertTrackAtIndex(0, true)
local outer = assert(reaper.GetTrack(0, 0))
assert(reaper.GetSetMediaTrackInfo_String(outer, "P_NAME", "Folder Parent", true))
local inner_index = reaper.CountTracks(0)
reaper.InsertTrackAtIndex(inner_index, true)
local inner = assert(reaper.GetTrack(0, inner_index))
assert(reaper.GetSetMediaTrackInfo_String(inner, "P_NAME", "Nested Folder", true))
local leaf_index = reaper.CountTracks(0)
reaper.InsertTrackAtIndex(leaf_index, true)
local leaf = assert(reaper.GetTrack(0, leaf_index))
assert(reaper.GetSetMediaTrackInfo_String(leaf, "P_NAME", "MIDI Child", true))
reaper.SetMediaTrackInfo_Value(outer, "I_FOLDERDEPTH", 1)
reaper.SetMediaTrackInfo_Value(inner, "I_FOLDERDEPTH", 1)
reaper.SetMediaTrackInfo_Value(leaf, "I_FOLDERDEPTH", -1)

local item = assert(reaper.CreateNewMIDIItemInProj(leaf, 0, 1, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 0, 480, 0, 60, 100, true))
reaper.MIDI_Sort(take)

local fx = reaper.TrackFX_AddByName(outer, "JS: AAADAW Folder Parent MIDI Probe", false, -1000)
assert(fx >= 0, "Install the probe JSFX in the isolated REAPER Effects directory")
reaper.TrackFX_SetEnabled(outer, fx, true)
reaper.gmem_attach("aaadaw_folder_parent_midi_probe")

for field, value in pairs({
  RENDER_SETTINGS = 0,
  RENDER_BOUNDSFLAG = 0,
  RENDER_STARTPOS = 0,
  RENDER_ENDPOS = 1,
  RENDER_SRATE = 48000,
  RENDER_CHANNELS = 2,
  RENDER_TAILFLAG = 0,
  RENDER_NORMALIZE = 0,
  RENDER_FORMAT = "evaw",
}) do
  if field == "RENDER_FORMAT" then
    reaper.GetSetProjectInfo_String(0, field, value, true)
  else
    reaper.GetSetProjectInfo(0, field, value, true)
  end
end
reaper.GetSetProjectInfo_String(0, "RENDER_FILE", root, true)

local function render(label)
  os.remove(root .. "folder-parent-midi-" .. label .. ".wav")
  reaper.gmem_write(0, 0)
  reaper.gmem_write(1, 0)
  reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "folder-parent-midi-" .. label, true)
  reaper.Main_OnCommand(42230, 0)
  return math.floor(reaper.gmem_read(0) + 0.5), math.floor(reaper.gmem_read(1) + 0.5)
end

local enabled_on, enabled_off = render("enabled")
reaper.Main_SaveProjectEx(0, root .. "folder-parent-midi-enabled.rpp", 0)
reaper.SetMediaTrackInfo_Value(leaf, "B_MAINSEND", 0)
local leaf_disabled_on, leaf_disabled_off = render("leaf-parent-disabled")
reaper.Main_SaveProjectEx(0, root .. "folder-parent-midi-leaf-disabled.rpp", 0)
reaper.SetMediaTrackInfo_Value(leaf, "B_MAINSEND", 1)
reaper.SetMediaTrackInfo_Value(inner, "B_MAINSEND", 0)
local inner_disabled_on, inner_disabled_off = render("inner-parent-disabled")
reaper.Main_SaveProjectEx(0, root .. "folder-parent-midi-inner-disabled.rpp", 0)

local output = assert(io.open(root .. "folder-parent-midi-reference.tsv", "w"))
output:write("reference\t", reaper.GetAppVersion(), "; Linux; Default 7\n")
output:write("folder_main_send_enabled_note_on\t", enabled_on, "\n")
output:write("folder_main_send_enabled_note_off\t", enabled_off, "\n")
output:write("leaf_parent_send_disabled_note_on\t", leaf_disabled_on, "\n")
output:write("leaf_parent_send_disabled_note_off\t", leaf_disabled_off, "\n")
output:write("inner_parent_send_disabled_note_on\t", inner_disabled_on, "\n")
output:write("inner_parent_send_disabled_note_off\t", inner_disabled_off, "\n")
output:write("probe\tJSFX counts incoming MIDI in the folder parent during offline project render\n")
output:close()

assert(enabled_on == 1 and enabled_off == 1, "Folder Parent must receive the child note pair")
assert(leaf_disabled_on == 0 and leaf_disabled_off == 0, "Turning off the leaf Main Send must block Parent MIDI")
assert(inner_disabled_on == 0 and inner_disabled_off == 0, "Turning off a nested folder Main Send must block upward MIDI")
reaper.defer(function() end)
