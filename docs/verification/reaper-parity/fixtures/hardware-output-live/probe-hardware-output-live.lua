-- Measure REAPER's live Hardware Output path against the same source sent to
-- the Master device output. Run with an isolated REAPER profile and a live
-- PulseAudio sink; scenario.txt selects one of the cases below.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
local scenario_file = assert(io.open(root .. "scenario.txt", "r"))
local scenario = scenario_file:read("*l") or "main"
scenario_file:close()
assert(
  scenario == "main"
    or scenario == "routed"
    or scenario == "muted"
    or scenario == "half"
    or scenario == "phase"
    or scenario == "pan-right"
    or scenario == "mono-1-stereo"
    or scenario == "mono-1-output-2"
    or scenario == "track-half"
    or scenario == "pre-fx"
    or scenario == "post-fx-2"
    or scenario == "post-fx-3"
    or scenario == "track-muted"
    or scenario == "source-solo"
    or scenario == "other-solo"
    or scenario == "master-bypass",
  "Unsupported Hardware Output scenario: " .. scenario
)

local function write(suffix, value)
  local file = assert(io.open(root .. scenario .. "." .. suffix, "w"))
  file:write(tostring(value), "\n")
  file:close()
end

for index = reaper.CountTracks(0) - 1, 0, -1 do
  reaper.DeleteTrack(reaper.GetTrack(0, index))
end
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Live hardware output proof", true)
reaper.SetMediaTrackInfo_Value(track, "B_MAINSEND", scenario == "main" and 1 or 0)
reaper.SetMediaTrackInfo_Value(track, "B_MUTE", scenario == "track-muted" and 1 or 0)
reaper.SetMediaTrackInfo_Value(track, "I_SOLO", scenario == "source-solo" and 1 or 0)
reaper.SetOnlyTrackSelected(track)
reaper.SetEditCurPos(0, false, false)
assert(reaper.InsertMedia(root .. "source.wav", 0) > 0, "Source WAV was not inserted")
local item = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
-- InsertMedia advances the edit cursor to the new item's end.
reaper.SetEditCurPos(0, false, false)

local fx = -1
if scenario == "pre-fx" or scenario == "post-fx-2" or scenario == "post-fx-3" then
  fx = reaper.TrackFX_AddByName(track, "JS: Volume Adjustment", false, -1)
  assert(fx >= 0, "Stock JS: Volume Adjustment was not found")
  assert(reaper.TrackFX_SetParam(track, fx, 0, -6.0), "Could not set the stock JSFX gain to -6 dB")
end
local track_volume = (scenario == "track-half" or fx >= 0) and 0.5 or 1.0
reaper.SetMediaTrackInfo_Value(track, "D_VOL", track_volume)

local send = -1
if scenario ~= "main" then
  send = reaper.CreateTrackSend(track, nil)
  assert(send >= 0, "Hardware Output route creation failed")
  local source_channels = (scenario == "mono-1-stereo" or scenario == "mono-1-output-2") and 1024 or 0
  local destination_channels = scenario == "mono-1-output-2" and 1025 or 0
  assert(reaper.SetTrackSendInfo_Value(track, 2, send, "I_SRCCHAN", source_channels))
  assert(reaper.SetTrackSendInfo_Value(track, 2, send, "I_DSTCHAN", destination_channels))
  assert(reaper.SetTrackSendInfo_Value(track, 2, send, "D_VOL", scenario == "half" and 0.5 or 1.0))
  assert(reaper.SetTrackSendInfo_Value(track, 2, send, "D_PAN", scenario == "pan-right" and 1.0 or 0.0))
  assert(reaper.SetTrackSendInfo_Value(track, 2, send, "B_MUTE", scenario == "muted" and 1 or 0))
  assert(reaper.SetTrackSendInfo_Value(track, 2, send, "B_PHASE", scenario == "phase" and 1 or 0))
  local send_mode = 0
  if scenario == "pre-fx" then send_mode = 1 end
  if scenario == "post-fx-2" then send_mode = 2 end
  if scenario == "post-fx-3" then send_mode = 3 end
  assert(reaper.SetTrackSendInfo_Value(track, 2, send, "I_SENDMODE", send_mode))
end

if scenario == "other-solo" then
  reaper.InsertTrackAtIndex(1, true)
  local other = assert(reaper.GetTrack(0, 1))
  reaper.GetSetMediaTrackInfo_String(other, "P_NAME", "Soloed track without Hardware Output", true)
  reaper.SetMediaTrackInfo_Value(other, "B_MAINSEND", 0)
  reaper.SetMediaTrackInfo_Value(other, "I_SOLO", 1)
  reaper.SetOnlyTrackSelected(other)
  reaper.SetEditCurPos(0, false, false)
  assert(reaper.InsertMedia(root .. "source.wav", 0) > 0, "Soloed source WAV was not inserted")
  local other_item = assert(reaper.GetTrackMediaItem(other, 0))
  reaper.SetMediaItemInfo_Value(other_item, "D_POSITION", 0)
  reaper.SetEditCurPos(0, false, false)
end

if scenario == "master-bypass" then
  local master = reaper.GetMasterTrack(0)
  reaper.SetMediaTrackInfo_Value(master, "D_VOL", 0.5)
  local master_fx = reaper.TrackFX_AddByName(master, "JS: Volume Adjustment", false, -1)
  assert(master_fx >= 0, "Could not add JSFX to Master")
  assert(reaper.TrackFX_SetParam(master, master_fx, 0, -6.0), "Could not set Master JSFX gain to -6 dB")
end

reaper.Main_SaveProjectEx(0, root .. scenario .. ".rpp", 0)
write(
  "state",
  "reaper=" .. reaper.GetAppVersion()
    .. "; main_send=" .. tostring(reaper.GetMediaTrackInfo_Value(track, "B_MAINSEND"))
    .. "; hardware_send=" .. tostring(send)
    .. "; send_mode=" .. tostring(scenario == "pre-fx" and 1 or (scenario == "post-fx-2" and 2 or (scenario == "post-fx-3" and 3 or 0)))
    .. "; track_volume=" .. tostring(track_volume)
    .. "; track_muted=" .. tostring(scenario == "track-muted")
    .. "; source_solo=" .. tostring(scenario == "source-solo")
    .. "; master_fx_and_fader=" .. tostring(scenario == "master-bypass")
)
local peaks = assert(io.open(root .. scenario .. ".peaks.tsv", "w"))
peaks:write("wall_seconds\tplay_seconds\ttrack_l\ttrack_r\tmaster_l\tmaster_r\n")
reaper.Main_OnCommand(1007, 0) -- Transport: Play
local started = reaper.time_precise()
local function sample()
  local elapsed = reaper.time_precise() - started
  local track_left = reaper.Track_GetPeakInfo(track, 0) or 0
  local track_right = reaper.Track_GetPeakInfo(track, 1) or 0
  local master = reaper.GetMasterTrack(0)
  local master_left = reaper.Track_GetPeakInfo(master, 0) or 0
  local master_right = reaper.Track_GetPeakInfo(master, 1) or 0
  peaks:write(
    string.format(
      "%.6f\t%.6f\t%.9f\t%.9f\t%.9f\t%.9f\n",
      elapsed,
      reaper.GetPlayPosition(),
      track_left,
      track_right,
      master_left,
      master_right
    )
  )
  peaks:flush()
  -- The PulseAudio null sink may be suspended during the first master-output
  -- wake-up; keep transport running long enough to measure the full source.
  if elapsed < 3.2 then
    reaper.defer(sample)
    return
  end
  reaper.Main_OnCommand(1016, 0) -- Transport: Stop
  peaks:close()
  write("complete", "play_state=" .. tostring(reaper.GetPlayState()) .. "; elapsed=" .. tostring(elapsed))
end
reaper.defer(sample)
