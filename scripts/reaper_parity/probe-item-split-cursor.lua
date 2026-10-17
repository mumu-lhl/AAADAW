-- Run in an unsaved, stopped Linux REAPER 7.82 / Default 7 project.
-- Records Main action 40012 selection and split behavior at the edit cursor.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"),
  "set AAADAW_REAPER_PROBE_DIR to an output directory")
local output = assert(io.open(output_dir .. "/item-split-cursor-native.tsv", "w"))

local function add_track(name)
  local index = reaper.CountTracks(0)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  return track
end

local function add_item(track, start_time, end_time, selected)
  local item = reaper.CreateNewMIDIItemInProj(track, start_time, end_time, false)
  assert(item, "could not create MIDI probe item")
  reaper.SetMediaItemSelected(item, selected)
  return item
end

local function write_case(name, tracks, state_before)
  output:write(name, "_state_change_delta\t",
    reaper.GetProjectStateChangeCount(0) - state_before, "\n")
  for track_index, track in ipairs(tracks) do
    local count = reaper.CountTrackMediaItems(track)
    output:write(name, "\t", track_index - 1, "\t", count, "\n")
    for item_index = 0, count - 1 do
      local item = reaper.GetTrackMediaItem(track, item_index)
      local start_time = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
      local end_time = start_time + reaper.GetMediaItemInfo_Value(item, "D_LENGTH")
      output:write(name, "_item\t", track_index - 1, "\t", item_index,
        "\t", string.format("%.9f", start_time),
        "\t", string.format("%.9f", end_time),
        "\t", reaper.IsMediaItemSelected(item) and "selected" or "unselected", "\n")
    end
  end
end

local function reset_project()
  for index = reaper.CountTracks(0) - 1, 0, -1 do
    reaper.DeleteTrack(reaper.GetTrack(0, index))
  end
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("action\t40012\tItem: Split items at edit or play cursor (select right)\n")
output:write("transport\tstopped\n")

local selected_track = add_track("Selected item")
local unselected_track = add_track("Unselected item")
add_item(selected_track, 0, 10, true)
add_item(unselected_track, 0, 10, false)
reaper.SetEditCurPos(5, false, false)
local before_selected = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40012, 0)
reaper.UpdateArrange()
write_case("selected", { selected_track, unselected_track }, before_selected)
output:flush()

reset_project()
local track_a = add_track("No selection A")
local track_b = add_track("No selection B")
add_item(track_a, 0, 10, false)
add_item(track_b, 0, 10, false)
reaper.SetEditCurPos(5, false, false)
local before_empty_selection = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40012, 0)
reaper.UpdateArrange()
write_case("no_selection", { track_a, track_b }, before_empty_selection)
local result_project = output_dir .. "/item-split-cursor-result.rpp"
reaper.Main_SaveProjectEx(0, result_project, 0)
output:write("result_project\titem-split-cursor-result.rpp\n")
output:flush()

reset_project()
local boundary_track = add_track("Cursor on item boundary")
add_item(boundary_track, 0, 5, false)
add_item(boundary_track, 5, 10, false)
reaper.SetEditCurPos(5, false, false)
local before_boundary = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40012, 0)
reaper.UpdateArrange()
write_case("boundary", { boundary_track }, before_boundary)

output:close()
