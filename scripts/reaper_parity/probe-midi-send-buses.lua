-- Run in an isolated Linux REAPER 7.82 / Default 7 empty project.
-- Measure I_MIDIFLAGS source/destination bus routing through a bus-aware JSFX.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use an empty disposable project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")
assert(reaper.GetPlayStateEx(0) == 0, "Transport must be stopped")

local function add_track(name)
  local index = reaper.CountTracks(0)
  reaper.InsertTrackAtIndex(index, true)
  local track = assert(reaper.GetTrack(0, index))
  assert(reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true))
  return track
end

local source = add_track("MIDI Source")
local destination = add_track("MIDI Bus Receiver")
local send = reaper.CreateTrackSend(source, destination)
assert(send >= 0)
local source_fx = reaper.TrackFX_AddByName(source, "JS: AAADAW MIDI Send Bus Source Probe", false, -1000)
assert(source_fx >= 0, "Install the source bus probe JSFX in the isolated REAPER Effects directory")
reaper.TrackFX_SetEnabled(source, source_fx, true)
local item = assert(reaper.CreateNewMIDIItemInProj(source, 0, 1, false))
local take = assert(reaper.GetActiveTake(item))
for channel = 0, 2 do
  assert(reaper.MIDI_InsertNote(take, false, false, 0, 480, channel, 60 + channel, 100, true))
end
reaper.MIDI_Sort(take)
local fx = reaper.TrackFX_AddByName(destination, "JS: AAADAW MIDI Send Bus Probe", false, -1000)
assert(fx >= 0, "Install the bus-aware JSFX in the isolated REAPER Effects directory")
reaper.TrackFX_SetEnabled(destination, fx, true)
reaper.gmem_attach("aaadaw_midi_send_bus_probe")

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

local cases = {
  { "all_to_all_preserves_bus_2", 0, 1, 3 },
  { "source_bus_1_to_all", 1 << 14, 0, 0 },
  { "source_bus_2_to_all", 2 << 14, 1, 3 },
  { "all_to_destination_bus_1", 1 << 22, 0, 3 },
  { "all_to_destination_bus_2", 2 << 22, 1, 3 },
  { "source_bus_1_to_destination_bus_2", (1 << 14) | (2 << 22), 0, 0 },
  { "source_bus_2_to_destination_bus_2", (2 << 14) | (2 << 22), 1, 3 },
}
local output = assert(io.open(root .. "midi-send-buses-native.tsv", "w"))
output:write("reference\t", reaper.GetAppVersion(), "; Linux; Default 7; I_MIDIFLAGS source bus bits 14..21, destination bits 22..29; JSFX midi_bus is zero-based\n")
for _, case in ipairs(cases) do
  local label, flags, expected_bus, expected_count = case[1], case[2], case[3], case[4]
  assert(reaper.SetTrackSendInfo_Value(source, 0, send, "I_MIDIFLAGS", flags))
  assert(reaper.GetTrackSendInfo_Value(source, 0, send, "I_MIDIFLAGS") == flags)
  for index = 0, 127 do reaper.gmem_write(index, 0) end
  os.remove(root .. "midi-send-buses-" .. label .. ".wav")
  reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "midi-send-buses-" .. label, true)
  reaper.Main_OnCommand(42230, 0)
  local total = 0
  for bus = 0, 15 do
    local count = math.floor(reaper.gmem_read(112 + bus) + 0.5)
    output:write(label, "_jsfx_bus_", bus + 1, "\t", count, "\n")
    total = total + count
    if bus == expected_bus then
      assert(count == expected_count, label .. " produced an unexpected count on its selected JSFX bus")
    else
      assert(count == 0, label .. " leaked note-ons to another JSFX bus")
    end
  end
  output:write(label, "_total_note_ons\t", total, "\n")
  if label == "all_to_destination_bus_2" then
    reaper.Main_SaveProjectEx(0, root .. "midi-send-buses-after.rpp", 0)
  end
end
output:write("interpretation\tREAPER bus 1 maps to JSFX midi_bus 0 and bus 2 maps to midi_bus 1; destination All preserves a non-normal source bus; explicit source/destination selectors filter and map that bus\n")
output:close()

reaper.SetOnlyTrackSelected(source)
local actions = assert(io.open(root .. "midi-send-buses-actions.tsv", "w"))
actions:write("selected_track\tMIDI Source\n")
actions:write("routing_window\tOpened by clicking REAPER Default 7 mixer Route button at x=211,y=645\n")
actions:close()
reaper.SetTrackSendInfo_Value(source, 0, send, "I_MIDIFLAGS", 2 << 22)
reaper.Main_SaveProjectEx(0, root .. "midi-send-buses-after.rpp", 0)
reaper.UpdateArrange()
local marker = assert(io.open(root .. "midi-send-buses-capture-complete", "w"))
marker:write("complete\n")
marker:close()
reaper.defer(function() end)
