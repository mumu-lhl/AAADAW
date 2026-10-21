-- Run in an isolated Linux REAPER 7.82 / Default 7 empty project.
-- Measure ordinary audio-send source/destination channel mapping through JSFX.
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

local source = add_track("Audio Source")
local destination = add_track("Audio Receiver")
local send = reaper.CreateTrackSend(source, destination)
assert(send >= 0)
local source_fx = reaper.TrackFX_AddByName(source, "JS: AAADAW Audio Channel Map Source Probe", false, -1000)
local destination_fx = reaper.TrackFX_AddByName(destination, "JS: AAADAW Audio Channel Map Destination Probe", false, -1000)
assert(source_fx >= 0 and destination_fx >= 0, "Install both probe JSFX files in the isolated REAPER Effects directory")
reaper.TrackFX_SetEnabled(source, source_fx, true)
reaper.TrackFX_SetEnabled(destination, destination_fx, true)
reaper.gmem_attach("aaadaw_audio_channel_map_probe")

for field, value in pairs({
  RENDER_SETTINGS = 0,
  RENDER_BOUNDSFLAG = 0,
  RENDER_STARTPOS = 0,
  RENDER_ENDPOS = 0.1,
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

local source_modes = {
  { "stereo_1_2", 0 },
  { "mono_1", 1024 },
  { "mono_2", 1025 },
}
local destination_modes = {
  { "stereo_1_2", 0 },
  { "mono_1", 1024 },
  { "mono_2", 1025 },
}
local output = assert(io.open(root .. "audio-send-channel-map-native.tsv", "w"))
output:write("reference\t", reaper.GetAppVersion(), "; Linux; Default 7; I_SRCCHAN/I_DSTCHAN standard REAPER channel encodings\n")
for _, source_mode in ipairs(source_modes) do
  for _, destination_mode in ipairs(destination_modes) do
    local label = source_mode[1] .. "_to_" .. destination_mode[1]
    assert(reaper.SetTrackSendInfo_Value(source, 0, send, "I_SRCCHAN", source_mode[2]))
    assert(reaper.SetTrackSendInfo_Value(source, 0, send, "I_DSTCHAN", destination_mode[2]))
    assert(reaper.GetTrackSendInfo_Value(source, 0, send, "I_SRCCHAN") == source_mode[2])
    assert(reaper.GetTrackSendInfo_Value(source, 0, send, "I_DSTCHAN") == destination_mode[2])
    for index = 0, 4 do reaper.gmem_write(index, 0) end
    reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "audio-send-channel-map-" .. label, true)
    reaper.Main_OnCommand(42230, 0)
    local frames = reaper.gmem_read(4)
    assert(frames > 0, "Destination FX did not receive rendered audio for " .. label)
    output:write(label, "\tI_SRCCHAN=", source_mode[2], "\tI_DSTCHAN=", destination_mode[2])
    output:write("\tmean_channel_1=", string.format("%.9f", reaper.gmem_read(0) / frames))
    output:write("\tmean_channel_2=", string.format("%.9f", reaper.gmem_read(1) / frames))
    output:write("\tmean_abs_channel_1=", string.format("%.9f", reaper.gmem_read(2) / frames))
    output:write("\tmean_abs_channel_2=", string.format("%.9f", reaper.gmem_read(3) / frames), "\n")
    if label == "stereo_1_2_to_stereo_1_2" then
      reaper.Main_SaveProjectEx(0, root .. "audio-send-channel-map-before.rpp", 0)
    end
  end
end
for _, case in ipairs({
  { "stereo_1_2_to_stereo_1_2_pan_right", 0, 0 },
  { "stereo_1_2_to_mono_1_pan_right", 0, 1024 },
  { "mono_1_to_stereo_1_2_pan_right", 1024, 0 },
  { "mono_1_to_mono_2_pan_right", 1024, 1025 },
}) do
  local label, source_channels, destination_channels = table.unpack(case)
  assert(reaper.SetTrackSendInfo_Value(source, 0, send, "I_SRCCHAN", source_channels))
  assert(reaper.SetTrackSendInfo_Value(source, 0, send, "I_DSTCHAN", destination_channels))
  assert(reaper.SetTrackSendInfo_Value(source, 0, send, "D_PAN", 0.5))
  for index = 0, 4 do reaper.gmem_write(index, 0) end
  reaper.GetSetProjectInfo_String(0, "RENDER_PATTERN", "audio-send-channel-map-" .. label, true)
  reaper.Main_OnCommand(42230, 0)
  local frames = reaper.gmem_read(4)
  assert(frames > 0, "Destination FX did not receive rendered audio for " .. label)
  output:write(label, "\tI_SRCCHAN=", source_channels, "\tI_DSTCHAN=", destination_channels, "\tD_PAN=", reaper.GetTrackSendInfo_Value(source, 0, send, "D_PAN"))
  output:write("\tmean_channel_1=", string.format("%.9f", reaper.gmem_read(0) / frames))
  output:write("\tmean_channel_2=", string.format("%.9f", reaper.gmem_read(1) / frames), "\n")
end
assert(reaper.SetTrackSendInfo_Value(source, 0, send, "D_PAN", 0))
output:write("interpretation\tSource probe emits 0.25 on channel 1 and 0.75 on channel 2; destination probe accumulates each channel before its track fader\n")
output:close()
assert(reaper.SetTrackSendInfo_Value(source, 0, send, "I_SRCCHAN", 1024))
assert(reaper.SetTrackSendInfo_Value(source, 0, send, "I_DSTCHAN", 0))
reaper.SetOnlyTrackSelected(source)
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()
reaper.Main_SaveProjectEx(0, root .. "audio-send-channel-map-after.rpp", 0)
local complete = assert(io.open(root .. "audio-send-channel-map-capture-complete", "w"))
complete:write("complete\n")
complete:close()
reaper.defer(function() end)
