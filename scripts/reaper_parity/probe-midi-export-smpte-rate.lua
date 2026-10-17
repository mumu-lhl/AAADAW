-- Run in an isolated Linux REAPER 7.82 / Default 7 instance.
-- Creates the standard MIDI fixture at a configurable offset and supported REAPER SMPTE preset.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
local requested_rate = os.getenv("AAADAW_REAPER_FRAME_RATE") or "24"
local requested_offset = tonumber(os.getenv("AAADAW_REAPER_PROJECT_OFFSET_SECONDS") or "10.5")
assert(requested_offset and requested_offset >= 0 and requested_offset < 86400,
  "AAADAW_REAPER_PROJECT_OFFSET_SECONDS must be from 0 to less than 86400")
local offset_text = string.format("%.9f", requested_offset):gsub("0+$", ""):gsub("%.$", "")
local rates = {
  ["23.976"] = {
    fps = 24000 / 1001,
    nominal = 24,
    rate_code = 0,
    case_name = "23976fps",
    drop_frame = false,
    time_mode = "TIMEMODE 1 0 -1 24 2 0 -1 0",
  },
  ["24"] = {
    fps = 24,
    nominal = 24,
    rate_code = 0,
    case_name = "24fps",
    drop_frame = false,
    time_mode = "TIMEMODE 1 5 -1 24 0 0 -1 0",
  },
  ["25"] = {
    fps = 25,
    nominal = 25,
    rate_code = 1,
    case_name = "25fps",
    drop_frame = false,
    time_mode = "TIMEMODE 1 5 -1 25 0 0 -1 0",
  },
  ["29.97DF"] = {
    fps = 30000 / 1001,
    nominal = 30,
    rate_code = 2,
    case_name = "2997drop",
    drop_frame = true,
    time_mode = "TIMEMODE 1 3 -1 30 1 0 -1 0",
  },
  ["29.97ND"] = {
    fps = 30000 / 1001,
    nominal = 30,
    rate_code = 3,
    case_name = "2997nd",
    drop_frame = false,
    time_mode = "TIMEMODE 1 4 -1 30 2 0 -1 0",
  },
}
local rate_spec = assert(rates[requested_rate], "AAADAW_REAPER_FRAME_RATE must be 23.976, 24, 25, 29.97DF, or 29.97ND")
local case_name = rate_spec.case_name
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Run this probe in a fresh, unsaved project")
assert(reaper.CountTracks(0) == 0, "Run this probe in an empty project")

reaper.InsertTrackAtIndex(0, true)
local track = reaper.GetTrack(0, 0)
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Piano", true)
local item = assert(reaper.CreateNewMIDIItemInProj(track, 0.25, 1.25, false))
local take = assert(reaper.GetActiveTake(item))
local start_qn = reaper.TimeMap2_timeToQN(0, 0.25)
local start_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn)
local note_end_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 1.0)
local cc_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 0.25)
local bend_ppq = reaper.MIDI_GetPPQPosFromProjQN(take, start_qn + 0.375)
assert(reaper.MIDI_InsertNote(take, false, false, start_ppq, note_end_ppq, 0, 60, 100, false))
assert(reaper.MIDI_InsertCC(take, false, false, cc_ppq, 0xB0, 0, 1, 64))
assert(reaper.MIDI_InsertCC(take, false, false, bend_ppq, 0xE0, 0, 0, 96))
reaper.MIDI_Sort(take)

local project_file = root .. "project-" .. case_name .. ".rpp"
reaper.Main_SaveProjectEx(0, project_file, 8)

-- REAPER exposes a frame-rate getter but no setter. Patch the disposable RPP,
-- reopen it, and use the getter to verify the loaded project settings.
local source = assert(io.open(project_file, "r"))
local project_text = source:read("*a")
source:close()
local patched_text, offset_replacements = project_text:gsub(
  "PROJOFFS 0 0 0", "PROJOFFS " .. offset_text .. " 0 0", 1)
assert(offset_replacements == 1, "Expected one default PROJOFFS record")
local frame_rate_replacements
patched_text, frame_rate_replacements = patched_text:gsub(
  "TIMEMODE 1 5 %-1 30 0 0 %-1 0",
  rate_spec.time_mode,
  1)
assert(frame_rate_replacements == 1, "Expected one default 30 fps TIMEMODE record")
local temporary_file = assert(io.open(project_file .. ".tmp", "w"))
temporary_file:write(patched_text)
temporary_file:close()
assert(os.rename(project_file .. ".tmp", project_file))
reaper.Main_openProject("noprompt:" .. project_file)

local actual_rate, drop_frame = reaper.TimeMap_curFrameRate(0)
local offset_seconds = reaper.GetProjectTimeOffset(0, false)
assert(math.abs(actual_rate - rate_spec.fps) < 1e-9 and drop_frame == rate_spec.drop_frame,
  string.format("Loaded frame rate is %.9f fps; expected %s with drop-frame=%s", actual_rate, case_name, tostring(rate_spec.drop_frame)))
assert(math.abs(offset_seconds - requested_offset) < 1e-9,
  string.format("Loaded project start offset is %.9f seconds; expected %.9f", offset_seconds, requested_offset))
reaper.Main_SaveProjectEx(0, project_file, 8)

local frame_count = math.floor((requested_offset + 1e-8) * rate_spec.fps)
if rate_spec.drop_frame then
  local cycles = math.floor(frame_count / 17982)
  local remainder = frame_count % 17982
  frame_count = frame_count + 18 * cycles + 2 * math.floor(math.max(remainder - 2, 0) / 1798)
end
local hours = math.floor(frame_count / (rate_spec.nominal * 3600)) % 24
local minutes = math.floor(frame_count / (rate_spec.nominal * 60)) % 60
local seconds = math.floor(frame_count / rate_spec.nominal) % 60
local frames = frame_count % rate_spec.nominal
local expected_smpte_event = string.format("FF 54 05 %02X %02X %02X %02X %02X",
  rate_spec.rate_code * 32 + hours, minutes, seconds, frames, 0)

local meta = assert(io.open(root .. "smpte-" .. case_name .. "-native-meta.tsv", "w"))
meta:write("version\t", reaper.GetAppVersion(), "\n")
meta:write("theme\tDefault_7.0.ReaperThemeZip\n")
meta:write("fixture\ttrack Piano; item 0.25-1.25 seconds; C4 note, CC1, pitch bend\n")
meta:write("project_frame_rate\t", requested_rate, " fps; drop-frame=", tostring(rate_spec.drop_frame), "\n")
meta:write("project_time_offset_seconds\t", string.format("%.9f", offset_seconds), "\n")
meta:write("export_action\t40849\n")
meta:write("export_mode_dialog\tmanual confirmation required; capture Type 1 and PPQN 960 state\n")
meta:write("export_embed_smpte_offset_dialog\tmanual confirmation required\n")
meta:write("expected_smpte_offset_event\t", expected_smpte_event, "\n")
meta:close()

local ready = assert(io.open(root .. "smpte-" .. case_name .. "-ready", "w"))
ready:write("Project reports ", requested_rate, " fps; drop-frame=", tostring(rate_spec.drop_frame),
  "; ", string.format("%.9f", offset_seconds), " seconds offset; open the MIDI export dialog\n")
ready:close()

local function wait_for_export()
  local trigger = io.open(root .. "open-smpte-" .. case_name .. "-export", "r")
  if not trigger then
    reaper.defer(wait_for_export)
    return
  end
  trigger:close()
  os.remove(root .. "open-smpte-" .. case_name .. "-export")
  reaper.Main_OnCommand(40849, 0)
end

reaper.defer(wait_for_export)
