-- Run in a fresh Linux REAPER 7.82 / Default 7 project.
-- Create one Receiver with two independent incoming Audio Sends for UI capture.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, current_project = reaper.EnumProjects(-1, "")
assert(current_project == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local function add_track(name)
  local index = reaper.CountTracks(0)
  reaper.InsertTrackAtIndex(index, true)
  local track = assert(reaper.GetTrack(0, index))
  assert(reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true))
  return track
end

local first = add_track("Audio 1")
local second = add_track("Audio 2")
local receiver = add_track("Receiver")
assert(reaper.CreateTrackSend(first, receiver) >= 0)
assert(reaper.CreateTrackSend(second, receiver) >= 0)
local save_path = output_dir .. "/routing-receive-remove-native.rpp"
reaper.Main_SaveProjectEx(0, save_path, 0)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateArrange()

local output = assert(io.open(output_dir .. "/routing-receive-remove-native.tsv", "w"))
output:write("reference\t", reaper.GetAppVersion(), "; Default 7\n")
output:write("transport\tstopped\n")
output:write("tracks\tAudio 1; Audio 2; Receiver\n")
output:write("sends\tAudio 1 to Receiver; Audio 2 to Receiver\n")
output:write("capture\tOpen Receiver I/O from its TCP or MCP I/O button; inspect its Receive list\n")
output:close()
