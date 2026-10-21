-- Run in an isolated Linux REAPER 7.82 / Default 7 empty project.
-- Verify ordinary Send source-channel filtering and destination-channel remapping.
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
local destination = add_track("MIDI Receiver")
local send = reaper.CreateTrackSend(source, destination)
assert(send >= 0)
local progress = assert(io.open(root .. "midi-send-channel-map-progress.txt", "w"))
progress:write("tracks_created\n")
progress:flush()
local item = assert(reaper.CreateNewMIDIItemInProj(source, 0, 1, false))
local take = assert(reaper.GetActiveTake(item))
for channel = 0, 2 do
  assert(reaper.MIDI_InsertNote(take, false, false, 0, 480, channel, 60 + channel, 100, true))
  assert(reaper.MIDI_InsertCC(take, false, false, 120, 0xB0, channel, 1, 20 + channel))
  assert(reaper.MIDI_InsertCC(take, false, false, 180, 0xE0, channel, 0, 64 + channel))
  assert(reaper.MIDI_InsertCC(take, false, false, 240, 0xC0, channel, 10 + channel, 0))
  assert(reaper.MIDI_InsertCC(take, false, false, 300, 0xA0, channel, 60 + channel, 70 + channel))
  assert(reaper.MIDI_InsertCC(take, false, false, 360, 0xD0, channel, 80 + channel, 0))
end
reaper.MIDI_Sort(take)
local fx = reaper.TrackFX_AddByName(destination, "JS: AAADAW MIDI Send Channel Map Probe", false, -1000)
assert(fx >= 0, "Install the probe JSFX in the isolated REAPER Effects directory")
progress:write("probe_fx_added\n")
progress:flush()
reaper.TrackFX_SetEnabled(destination, fx, true)
reaper.gmem_attach("aaadaw_midi_send_channel_map_probe")

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

local event_kinds = {
  { "note_on", 0 },
  { "note_off", 16 },
  { "cc", 32 },
  { "pitch_bend", 48 },
  { "program", 64 },
  { "poly_aftertouch", 80 },
  { "channel_pressure", 96 },
}
local cases = {
  { "all_to_all", 0 },
  { "channel_2_to_original", 2 },
  { "all_to_6", 6 << 5 },
  { "channel_2_to_6", 2 + (6 << 5) },
}
local observed = {}
local output = assert(io.open(root .. "midi-send-channel-map-native.tsv", "w"))
output:write("reference\t", reaper.GetAppVersion(), "; Linux; Default 7; I_MIDIFLAGS low 5 source / next 5 destination\n")

for _, case in ipairs(cases) do
  local label, flags = case[1], case[2]
  observed[label] = {}
  progress:write("render_", label, "\n")
  progress:flush()
  assert(reaper.SetTrackSendInfo_Value(source, 0, send, "I_MIDIFLAGS", flags))
  local observed_flags = reaper.GetTrackSendInfo_Value(source, 0, send, "I_MIDIFLAGS")
  assert(observed_flags == flags, label .. " MIDI flags did not persist")
  for index = 0, 111 do reaper.gmem_write(index, 0) end
  os.remove(root .. "midi-send-channel-map-" .. label .. ".wav")
  reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "midi-send-channel-map-" .. label, true)
  reaper.Main_OnCommand(42230, 0)
  output:write(label, "_i_midiflags\t", flags, "\n")
  for _, event in ipairs(event_kinds) do
    observed[label][event[1]] = {}
    for channel = 0, 15 do
      local count = math.floor(reaper.gmem_read(event[2] + channel) + 0.5)
      observed[label][event[1]][channel + 1] = count
      output:write(label, "_", event[1], "_channel_", channel + 1, "\t", count, "\n")
    end
  end
  if label == "all_to_all" then
    reaper.Main_SaveProjectEx(0, root .. "midi-send-channel-map-before.rpp", 0)
  elseif label == "channel_2_to_6" then
    reaper.Main_SaveProjectEx(0, root .. "midi-send-channel-map-after.rpp", 0)
  end
end
for _, event in ipairs(event_kinds) do
  local all = observed.all_to_all[event[1]]
  local selected = observed.channel_2_to_original[event[1]]
  local remapped = observed.channel_2_to_6[event[1]]
  local all_remapped = observed.all_to_6[event[1]]
  for channel = 1, 16 do
    if channel == 2 then
      assert(selected[channel] > 0, event[1] .. " selected source channel was dropped")
    else
      assert(selected[channel] == 0, event[1] .. " source filter mismatch")
    end
    if channel == 6 then
      assert(remapped[channel] > 0, event[1] .. " selected channel was not remapped")
      assert(all_remapped[channel] > 0, event[1] .. " All channels were not remapped")
    else
      assert(remapped[channel] == 0, event[1] .. " channel remap mismatch")
      assert(all_remapped[channel] == 0, event[1] .. " All remap mismatch")
    end
  end
end
output:write("interpretation\tSource filter 2 permits only MIDI channel 2; destination 6 rewrites it to channel 6; source All maps every channel to 6\n")
output:close()
progress:write("renders_complete\n")
progress:close()

reaper.SetOnlyTrackSelected(source)
local action_log = assert(io.open(root .. "midi-send-channel-map-actions.tsv", "w"))
action_log:write("selected_track\tMIDI Source\n")
action_log:write("routing_window\tOpened by clicking REAPER Default 7 mixer Route button at x=211,y=645\n")
action_log:close()
reaper.UpdateArrange()
local marker = assert(io.open(root .. "midi-send-channel-map-capture-complete", "w"))
marker:write("complete\n")
marker:close()
reaper.defer(function() end)
