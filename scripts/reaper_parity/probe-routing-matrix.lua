-- Run in a fresh disposable Linux REAPER 7.82 / Default 7 project.
-- Capture the native Routing Matrix and query ordinary Send-cell behavior.
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
local peer = add_track("Peer")
local destination = add_track("Destination")
local first = assert(reaper.CreateTrackSend(source, destination))
local second = assert(reaper.CreateTrackSend(source, destination))
assert(reaper.SetTrackSendInfo_Value(source, 0, second, "D_VOL", 0.5))
local log = assert(io.open(root .. "routing-matrix-native.tsv", "w"))
log:write("reference\t", reaper.GetAppVersion(), " / Linux / Default 7\n")
log:write("action_40251\t", reaper.kbd_getTextFromCmd(40251, 0) or "<missing>", "\n")
local matrix_shortcut_count = reaper.CountActionShortcuts(0, 40251)
log:write("action_40251_default_shortcuts\t", matrix_shortcut_count, "\n")
for index = 0, matrix_shortcut_count - 1 do
  local ok, shortcut = reaper.GetActionShortcutDesc(0, 40251, index)
  assert(ok, "Could not read the Routing Matrix shortcut")
  log:write("action_40251_shortcut_", index + 1, "\t", shortcut, "\n")
end
log:write("action_40313\t", reaper.kbd_getTextFromCmd(40313, 0) or "<missing>", "\n")
local function count_sends_to(track, destination_track)
  local count = 0
  for index = 0, reaper.GetTrackNumSends(track, 0) - 1 do
    if reaper.GetTrackSendInfo_Value(track, 0, index, "P_DESTTRACK") == destination_track then
      count = count + 1
    end
  end
  return count
end
local function record(label)
  local total = reaper.GetTrackNumSends(source, 0)
  local to_peer = count_sends_to(source, peer)
  local to_destination = count_sends_to(source, destination)
  log:write(label, "\t", total, "\t", to_peer, "\t", to_destination, "\n")
  log:flush()
end
record("before_matrix")
local started = reaper.time_precise()
local function open_matrix_after_startup()
  if reaper.time_precise() - started < 10 then
    reaper.defer(open_matrix_after_startup)
    return
  end
  reaper.Main_OnCommand(40251, 0)
  -- Undock the native Docker for a legible capture of its full matrix contents.
  reaper.Main_OnCommand(40313, 0)
  reaper.UpdateArrange()
  reaper.Main_SaveProjectEx(0, root .. "routing-matrix-native-before.rpp", 0)
  local marker = assert(io.open(root .. "routing-matrix-native-capture-complete", "w"))
  marker:write("ready\n")
  marker:close()
end
reaper.defer(open_matrix_after_startup)
local observed = { add = false, open = false, delete = false }
local function poll_interaction_markers()
  local cases = {
    { name = "add", project = "routing-matrix-native-after-add.rpp" },
    { name = "open", project = "routing-matrix-native-after-open.rpp" },
    { name = "delete", project = "routing-matrix-native-after-delete.rpp" },
    { name = "duplicate", project = "routing-matrix-native-after-duplicate.rpp" },
  }
  for _, case in ipairs(cases) do
    if not observed[case.name] then
      local marker_path = root .. "routing-matrix-native-click-" .. case.name
      local marker_file = io.open(marker_path, "r")
      if marker_file then
        marker_file:close()
        observed[case.name] = true
        record("after_" .. case.name .. "_cell_click")
        reaper.Main_SaveProjectEx(0, root .. case.project, 0)
        local complete = assert(io.open(root .. "routing-matrix-native-observed-" .. case.name, "w"))
        complete:write("complete\n")
        complete:close()
      end
    end
  end
  if reaper.time_precise() - started < 60 then reaper.defer(poll_interaction_markers) end
end
reaper.defer(poll_interaction_markers)
local function keep_open()
  if reaper.time_precise() - started < 60 then reaper.defer(keep_open) end
end
keep_open()
