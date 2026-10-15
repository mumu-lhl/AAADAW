local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, path = reaper.EnumProjects(-1, "")
assert(path == "", "probe must run in an unsaved project")

local out = assert(io.open(root .. "nudge-set-contents.tsv", "w"))
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "MIDI Set Contents", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 2, 7, false))
local take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertNote(take, false, false, 960, 2880, 0, 60, 96, false))
assert(reaper.MIDI_InsertNote(take, false, false, 4800, 6720, 0, 64, 108, false))
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(4, false, false)
reaper.UpdateArrange()
reaper.Main_OnCommand(41226, 0)
reaper.UpdateArrange()

local function write_state(label)
  local pos = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
  local len = reaper.GetMediaItemInfo_Value(item, "D_LENGTH")
  local offset = reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")
  local phase = reaper.MIDI_GetPPQPosFromProjTime(take, pos)
  local endpoint = reaper.MIDI_GetPPQPosFromProjTime(take, pos + len)
  out:write(table.concat({label, string.format("%.9f", pos), string.format("%.9f", len),
    string.format("%.9f", offset), string.format("%.3f", phase),
    string.format("%.3f", endpoint), reaper.GetProjectStateChangeCount(0),
    reaper.IsMediaItemSelected(item) and "selected" or "unselected"}, "\t"), "\n")
  out:flush()
end

out:write("version\t", reaper.GetAppVersion(), "\n")
out:write("theme\tDefault_7.0.ReaperThemeZip\n")
out:write("columns\tlabel\tposition_s\tlength_s\tsource_offset_s\tphase_ppq\tendpoint_ppq\tstate_change\tselection\n")
write_state("before")

local command_path = root .. "nudge-set-contents-command.txt"
local function poll()
  local input = io.open(command_path, "r")
  if input then
    local command = input:read("*l")
    input:close()
    os.remove(command_path)
    if command == "snapshot" then write_state("snapshot") end
    if command == "done" then out:close(); return end
  end
  reaper.defer(poll)
end
poll()
