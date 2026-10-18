-- Run only in an isolated, unsaved Linux REAPER 7.82 / Default 7 project.
-- Imports the paired same-tick fixture, records its native take ordering, and
-- opens Main action 40849 so the exported SMF can be compared with AAADAW.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local fixture = root .. "midi-file-import-same-tick-order.mid"
local output_path = root .. "midi-export-same-tick-note-off.mid"
assert(reaper.InsertMedia(fixture, 0) == 1, "REAPER did not insert the same-tick fixture")
local track = assert(reaper.GetTrack(0, 0))
local item = assert(reaper.GetTrackMediaItem(track, 0))
local take = assert(reaper.GetActiveTake(item))
local ok, all_events = reaper.MIDI_GetAllEvts(take, "")
assert(ok, "could not read imported events")

local function hex(bytes)
  local values = {}
  for i = 1, #bytes do values[#values + 1] = string.format("%02X", bytes:byte(i)) end
  return table.concat(values, " ")
end
local position, offset, native_events = 0, 1, {}
while offset <= #all_events do
  local delta, flags, message, next_offset = string.unpack("<i4Bs4", all_events, offset)
  position = position + delta
  if #message > 0 then
    native_events[#native_events + 1] = string.format("%d:%s", position, hex(message))
  end
  offset = next_offset
end

local meta = assert(io.open(root .. "midi-export-same-tick-note-off-native.tsv", "w"))
meta:write("version\t", reaper.GetAppVersion(), "\n")
meta:write("theme\tDefault_7.0.ReaperThemeZip\n")
meta:write("fixture\tmidi-file-import-same-tick-order.mid\n")
meta:write("raw_take_events\t", table.concat(native_events, "; "), "\n")
meta:write("export_action\t40849\n")
meta:write("export_defaults\tType 1; PPQN 960; tempo/meter/SMPTE/markers unchecked\n")
meta:write("export_path\t", output_path, "\n")
meta:close()

reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
reaper.Main_OnCommand(40849, 0)
local ready = assert(io.open(root .. "midi-export-same-tick-note-off-ready", "w"))
ready:write("Save the project MIDI export to ", output_path, " using the default Type 1 / 960 PPQN options\n")
ready:close()
