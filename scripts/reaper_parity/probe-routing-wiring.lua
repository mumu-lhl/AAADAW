-- Run in a fresh disposable REAPER 7.82 / Linux / Default 7 project.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use an empty disposable project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

local function add_track(name)
  local index = reaper.CountTracks(0)
  reaper.InsertTrackAtIndex(index, true)
  local track = assert(reaper.GetTrack(0, index))
  assert(reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true))
  return track
end

local source = add_track("Source")
local destination = add_track("Destination")
local peer = add_track("Peer")
local audio_send = assert(reaper.CreateTrackSend(source, destination))
local hardware_output = assert(reaper.CreateTrackSend(source, nil))
assert(reaper.GetTrackNumSends(source, 0) == 1)
assert(reaper.GetTrackNumSends(source, 2) == 1)

local log = assert(io.open(root .. "routing-wiring-native.tsv", "w"))
log:write("reference\t", reaper.GetAppVersion(), " / Linux / Default 7\n")
log:write("routing_matrix_action_40251\t", reaper.kbd_getTextFromCmd(40251, 0) or "<missing>", "\n")
log:write("source_ordinary_send_count\t", reaper.GetTrackNumSends(source, 0), "\n")
log:write("source_hardware_output_count\t", reaper.GetTrackNumSends(source, 2), "\n")
log:write("source_audio_send_destination_matches\t", tostring(reaper.GetTrackSendInfo_Value(source, 0, audio_send, "P_DESTTRACK") == destination), "\n")
log:write("hardware_output_source_channels\t", reaper.GetTrackSendInfo_Value(source, 2, hardware_output, "I_SRCCHAN"), "\n")
log:write("hardware_output_destination_channels\t", reaper.GetTrackSendInfo_Value(source, 2, hardware_output, "I_DSTCHAN"), "\n")
log:write("hardware_output_send_mode\t", reaper.GetTrackSendInfo_Value(source, 2, hardware_output, "I_SENDMODE"), "\n")
log:write("unused_peer_hardware_output_count\t", reaper.GetTrackNumSends(peer, 2), "\n")
log:close()

reaper.Main_SaveProjectEx(0, root .. "routing-wiring-native.rpp", 0)
local started = reaper.time_precise()
local function open_matrix()
  if reaper.time_precise() - started < 8 then
    reaper.defer(open_matrix)
    return
  end
  reaper.Main_OnCommand(40251, 0)
  -- Undock the native Docker so both matrix and Wiring views can be captured.
  reaper.Main_OnCommand(40313, 0)
  reaper.UpdateArrange()
  local marker = assert(io.open(root .. "routing-wiring-native-ready", "w"))
  marker:write("ready\n")
  marker:close()
end
reaper.defer(open_matrix)
local function keep_open()
  if reaper.time_precise() - started < 300 then
    reaper.defer(keep_open)
  end
end
reaper.defer(keep_open)
