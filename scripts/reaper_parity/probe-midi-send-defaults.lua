-- Run in an isolated Linux REAPER 7.82 / Default 7 empty project.
-- Record the native MIDI routing defaults created with an ordinary Audio Send.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0)
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local function add_track(name)
  local index = reaper.CountTracks(0)
  reaper.InsertTrackAtIndex(index, true)
  local track = assert(reaper.GetTrack(0, index))
  assert(reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true))
  return track
end

local source = add_track("MIDI Source")
local destination = add_track("Instrument")
local send = reaper.CreateTrackSend(source, destination)
assert(send >= 0)
local flags = reaper.GetTrackSendInfo_Value(source, 0, send, "I_MIDIFLAGS")
local mode = reaper.GetTrackSendInfo_Value(source, 0, send, "I_SENDMODE")
local output = assert(io.open(output_dir .. "/midi-send-defaults.tsv", "w"))
output:write("reference\t", reaper.GetAppVersion(), "; Default 7\n")
output:write("new_audio_send_i_midiflags\t", string.format("%.0f", flags), "\n")
output:write("new_audio_send_i_sendmode\t", string.format("%.0f", mode), "\n")
output:write("interpretation\tSee REAPER 7.82 routing window and API bitfield before implementing channel mappings\n")
output:close()
reaper.Main_SaveProjectEx(0, output_dir .. "/midi-send-defaults.rpp", 0)
reaper.SetOnlyTrackSelected(source)
reaper.Main_OnCommand(40293, 0) -- Track: View routing and I/O for current/last touched track.
reaper.UpdateArrange()
