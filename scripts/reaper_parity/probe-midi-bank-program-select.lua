-- Run in an isolated Linux REAPER 7.82 / Default 7 profile.
-- Set AAADAW_REAPER_PROBE_DIR to a writable directory. The script opens a MIDI
-- Event List with Bank MSB 11, Bank LSB 22, and Program 41 at the same tick.
-- Create `open-properties` to open action 40004; after editing the dialog and
-- closing it, create `capture-bank` to record REAPER's resulting MIDI events.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe requires an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR")) .. "/"
local take
local function open_file(name)
  local file = io.open(root .. name, "r")
  if file then
    file:close()
    os.remove(root .. name)
    return true
  end
  return false
end

local function write_events(name, label)
  local file = assert(io.open(root .. name, "w"))
  local _, note_count, cc_count = reaper.MIDI_CountEvts(take)
  file:write("version\t", reaper.GetAppVersion(), "\n")
  file:write("theme\tDefault_7.0.ReaperThemeZip\n")
  file:write("label\t", label, "\n")
  file:write("counts\t", note_count, "\t", cc_count, "\n")
  for index = 0, cc_count - 1 do
    local ok, selected, muted, tick, kind, channel, msg2, msg3 =
      reaper.MIDI_GetCC(take, index)
    assert(ok, "could not read MIDI event " .. tostring(index))
    file:write(table.concat({
      "event", index + 1, selected and 1 or 0, muted and 1 or 0, tick,
      string.format("0x%02X", kind), channel + 1, msg2, msg3,
    }, "\t"), "\n")
  end
  file:close()
end

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Bank Program Select probe", true)
reaper.SetOnlyTrackSelected(track)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0, 4, false))
take = assert(reaper.GetActiveTake(item))
assert(reaper.MIDI_InsertCC(take, false, false, 480, 0xB0, 2, 0, 11))
assert(reaper.MIDI_InsertCC(take, false, false, 480, 0xB0, 2, 32, 22))
assert(reaper.MIDI_InsertCC(take, true, false, 480, 0xC0, 2, 41, 0))
reaper.MIDI_Sort(take)
reaper.SetMediaItemSelected(item, true)
reaper.SetEditCurPos(0, false, false)
reaper.UpdateItemInProject(item)
reaper.UpdateArrange()
write_events("midi-bank-program-before.tsv", "initial CC0/CC32/PC event stream")

reaper.Main_OnCommand(40153, 0)
local editor = assert(reaper.MIDIEditor_GetActive(), "MIDI editor did not open")
assert(reaper.MIDIEditor_GetTake(editor) == take, "wrong MIDI take opened")
reaper.MIDIEditor_OnCommand(editor, 40056)

local wait_for_capture_request
local function wait_for_properties_request()
  if open_file("open-properties") then
    reaper.MIDIEditor_OnCommand(editor, 40004)
    local ready = assert(io.open(root .. "properties-opened", "w"))
    ready:write("action\t40004\tEdit: Event properties\n")
    ready:close()
    reaper.defer(wait_for_capture_request)
  elseif reaper.MIDIEditor_GetActive() == editor then
    reaper.defer(wait_for_properties_request)
  end
end

wait_for_capture_request = function()
  if open_file("capture-bank") then
    write_events("midi-bank-program-after.tsv", "event stream after native dialog edit")
    local ready = assert(io.open(root .. "bank-capture-complete", "w"))
    ready:write("captured\n")
    ready:close()
  elseif reaper.MIDIEditor_GetActive() == editor then
    reaper.defer(wait_for_capture_request)
  end
end

reaper.defer(wait_for_properties_request)
