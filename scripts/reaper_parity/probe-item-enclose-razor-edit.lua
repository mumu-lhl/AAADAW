-- Run in a fresh, unsaved, stopped Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_REAPER_PROBE_MEDIA to the deterministic probe WAV and
-- AAADAW_REAPER_PROBE_DIR to an output directory.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local media_path = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA"),
  "set AAADAW_REAPER_PROBE_MEDIA to the probe WAV")
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"),
  "set AAADAW_REAPER_PROBE_DIR")
local output = assert(io.open(output_dir .. "/item-enclose-razor-native.tsv", "w"))

local function add_track(name)
  local index = reaper.CountTracks(0)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  return track
end

local function add_audio(track, position, length)
  reaper.SetOnlyTrackSelected(track)
  reaper.SetEditCurPos(position, false, false)
  assert(reaper.InsertMedia(media_path, 0) == 1, "could not insert probe audio")
  local item = reaper.GetTrackMediaItem(track, reaper.CountTrackMediaItems(track) - 1)
  assert(item, "inserted audio item is missing")
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", length)
  reaper.SetMediaItemSelected(item, false)
  return item
end

local function add_midi(track, position, length)
  local item = reaper.CreateNewMIDIItemInProj(track, position, position + length, false)
  assert(item, "could not insert probe MIDI item")
  reaper.SetMediaItemSelected(item, false)
  return item
end

local function razor(track)
  local _, value = reaper.GetSetMediaTrackInfo_String(track, "P_RAZOREDITS", "", false)
  return value
end

local function write_track_state(prefix, tracks)
  for index, track in ipairs(tracks) do
    local value = razor(track)
    if value == "" then value = "(empty)" end
    output:write(prefix, "\t", index - 1, "\t", value, "\n")
  end
end

local track_a = add_track("Enclose probe A")
local track_b = add_track("Enclose probe B")
local track_c = add_track("Enclose probe C")
local first = add_audio(track_a, 2, 2)
local second = add_audio(track_a, 6, 1)
local midi = add_midi(track_a, 10, 2)
local other_track_item = add_audio(track_b, 4, 2)
local unselected = add_audio(track_a, 14, 2)
local tracks = { track_a, track_b, track_c }

for _, track in ipairs(tracks) do
  reaper.GetSetMediaTrackInfo_String(track, "P_RAZOREDITS", "", true)
end
for _, item in ipairs({ first, second, midi, other_track_item }) do
  reaper.SetMediaItemSelected(item, true)
end
local selected_before = reaper.CountSelectedMediaItems(0)
local state_before = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(42409, 0) -- Razor edit: Enclose media items, including space between items
reaper.UpdateArrange()
local state_after_first = reaper.GetProjectStateChangeCount(0)
output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("action\t42409\tRazor edit: Enclose media items, including space between items\n")
output:write("selected_before\t", selected_before, "\n")
output:write("selected_after_first\t", reaper.CountSelectedMediaItems(0), "\n")
output:write("first_state_change_delta\t", state_after_first - state_before, "\n")
write_track_state("first_area", tracks)
output:write("redo_api_type\t", type(reaper.Undo_DoRedo2), "\n")
output:flush()
math.randomseed(os.time() + math.floor(os.clock() * 1000000))
local saved_project_path = output_dir .. "/item-enclose-razor-project-"
  .. tostring(os.time()) .. "-" .. tostring(math.random(1, 1000000000)) .. ".rpp"
reaper.Main_SaveProjectEx(0, saved_project_path, 0)
reaper.Undo_DoUndo2(0)
reaper.UpdateArrange()
output:write("undo_selected\t", reaper.CountSelectedMediaItems(0), "\n")
for track_index = 0, reaper.CountTracks(0) - 1 do
  local track = reaper.GetTrack(0, track_index)
  for item_index = 0, reaper.CountTrackMediaItems(track) - 1 do
    local selected_item = reaper.GetTrackMediaItem(track, item_index)
    if reaper.IsMediaItemSelected(selected_item) then
      output:write("undo_selected_item\t", track_index,
        "\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(selected_item, "D_POSITION")),
        "\t", string.format("%.9f", reaper.GetMediaItemInfo_Value(selected_item, "D_LENGTH")),
        "\n")
    end
  end
end
write_track_state("undo_area", tracks)
output:flush()
reaper.Undo_DoRedo2(0)
reaper.UpdateArrange()
output:write("redo_selected\t", reaper.CountSelectedMediaItems(0), "\n")
write_track_state("redo_area", tracks)
output:flush()

for index = 0, reaper.CountMediaItems(0) - 1 do
  reaper.SetMediaItemSelected(reaper.GetMediaItem(0, index), false)
end
reaper.SetMediaItemSelected(unselected, true)
local second_state_before = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(42409, 0)
reaper.UpdateArrange()
output:write("second_selected\t", reaper.CountSelectedMediaItems(0), "\n")
output:write("second_state_change_delta\t",
  reaper.GetProjectStateChangeCount(0) - second_state_before, "\n")
write_track_state("second_area", tracks)

reaper.SetMediaItemSelected(unselected, false)
local empty_state_before = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(42409, 0)
reaper.UpdateArrange()
output:write("empty_selected\t", reaper.CountSelectedMediaItems(0), "\n")
output:write("empty_state_change_delta\t",
  reaper.GetProjectStateChangeCount(0) - empty_state_before, "\n")
write_track_state("empty_area", tracks)
local undo_description = reaper.Undo_CanUndo2(0)
output:write("undo_after_empty_action\t", undo_description or "", "\n")
reaper.Undo_DoUndo2(0)
reaper.UpdateArrange()
write_track_state("undo_restores_area", tracks)
output:flush()
output:close()
