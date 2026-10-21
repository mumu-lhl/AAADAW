-- Run in an unsaved, stopped Linux REAPER 7.82 / Default 7 project.
-- Records Main action 40548 healing and selection behavior.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")

local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"),
  "set AAADAW_REAPER_PROBE_DIR to an output directory")
local output = assert(io.open(output_dir .. "/item-heal-splits-native.tsv", "w"))

local function add_track(name)
  local index = reaper.CountTracks(0)
  reaper.InsertTrackAtIndex(index, true)
  local track = reaper.GetTrack(0, index)
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  return track
end

local function add_midi_item(track, start_time, end_time, selected)
  local item = reaper.CreateNewMIDIItemInProj(track, start_time, end_time, false)
  assert(item, "could not create MIDI probe item")
  reaper.SetMediaItemSelected(item, selected)
  return item
end

local function track_items(track)
  local items = {}
  for index = 0, reaper.CountTrackMediaItems(track) - 1 do
    items[#items + 1] = reaper.GetTrackMediaItem(track, index)
  end
  return items
end

local function write_case(name, tracks, state_before)
  output:write(name, "_state_change_delta\t",
    reaper.GetProjectStateChangeCount(0) - state_before, "\n")
  for track_index, track in ipairs(tracks) do
    local items = track_items(track)
    output:write(name, "\t", track_index - 1, "\t", #items, "\n")
    for item_index, item in ipairs(items) do
      local start_time = reaper.GetMediaItemInfo_Value(item, "D_POSITION")
      local end_time = start_time + reaper.GetMediaItemInfo_Value(item, "D_LENGTH")
      local start_offset = reaper.GetMediaItemTakeInfo_Value(
        reaper.GetActiveTake(item), "D_STARTOFFS")
      output:write(name, "_item\t", track_index - 1, "\t", item_index - 1,
        "\t", string.format("%.9f", start_time),
        "\t", string.format("%.9f", end_time),
        "\t", string.format("%.9f", start_offset),
        "\t", reaper.IsMediaItemSelected(item) and "selected" or "unselected", "\n")
    end
  end
end

local function reset_project()
  for index = reaper.CountTracks(0) - 1, 0, -1 do
    reaper.DeleteTrack(reaper.GetTrack(0, index))
  end
end

local function split_midi_case(name, selection_mode, gap_after_split)
  local track = add_track(name)
  add_midi_item(track, 0, 10, true)
  reaper.SetEditCurPos(5, false, false)
  reaper.Main_OnCommand(40012, 0)
  local items = track_items(track)
  assert(#items == 2, "cursor split should create two adjacent MIDI items")
  if gap_after_split then
    reaper.SetMediaItemInfo_Value(items[1], "D_POSITION", 5.5)
  end
  for _, item in ipairs(items) do
    reaper.SetMediaItemSelected(item, selection_mode == "both")
  end
  if selection_mode == "left" then
    reaper.SetMediaItemSelected(items[0 + 1], true)
  elseif selection_mode == "right" then
    reaper.SetMediaItemSelected(items[1 + 1], true)
  end
  reaper.UpdateArrange()
  return track
end

local function split_audio_case(name, selection_mode, gap_after_split, source_discontinuity)
  local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"),
    "set AAADAW_SPLIT_SOURCE_PATH to a 10-second WAV")
  local track = add_track(name)
  reaper.SetOnlyTrackSelected(track)
  reaper.SetEditCurPos(0, false, false)
  reaper.InsertMedia(source_path, 0)
  reaper.UpdateArrange()
  local items = track_items(track)
  assert(#items == 1, "audio insertion should create one item")
  reaper.SetMediaItemSelected(items[1], true)
  reaper.SetEditCurPos(5, false, false)
  reaper.Main_OnCommand(40012, 0)
  items = track_items(track)
  assert(#items == 2, "cursor split should create two adjacent audio items")
  if gap_after_split then
    reaper.SetMediaItemInfo_Value(items[2], "D_POSITION", 5.5)
  end
  if source_discontinuity then
    reaper.SetMediaItemTakeInfo_Value(reaper.GetActiveTake(items[2]), "D_STARTOFFS", 4.5)
  end
  for _, item in ipairs(items) do
    reaper.SetMediaItemSelected(item, selection_mode == "both")
  end
  if selection_mode == "left" then
    reaper.SetMediaItemSelected(items[1], true)
  elseif selection_mode == "right" then
    reaper.SetMediaItemSelected(items[2], true)
  end
  reaper.UpdateArrange()
  return track
end

local function split_audio_three_case(name)
  local source_path = assert(os.getenv("AAADAW_SPLIT_SOURCE_PATH"),
    "set AAADAW_SPLIT_SOURCE_PATH to a 10-second WAV")
  local track = add_track(name)
  reaper.SetOnlyTrackSelected(track)
  reaper.SetEditCurPos(0, false, false)
  reaper.InsertMedia(source_path, 0)
  reaper.UpdateArrange()
  local items = track_items(track)
  assert(#items == 1, "audio insertion should create one item")
  reaper.SetMediaItemSelected(items[1], true)
  reaper.SetEditCurPos(2.5, false, false)
  reaper.Main_OnCommand(40012, 0)
  reaper.SetEditCurPos(7.5, false, false)
  reaper.Main_OnCommand(40012, 0)
  items = track_items(track)
  assert(#items == 3, "two selected splits should create three audio items")
  for _, item in ipairs(items) do
    reaper.SetMediaItemSelected(item, true)
  end
  reaper.UpdateArrange()
  return track
end

output:write("version\t", reaper.GetAppVersion(), "\n")
output:write("theme\tDefault_7.0.ReaperThemeZip\n")
output:write("action\t40548\tItem: Heal splits in items\n")
output:write("split_setup_action\t40012\tItem: Split items at edit or play cursor (select right)\n")
output:write("transport\tstopped\n")

for _, case in ipairs({
  { "right_only", "right", false },
  { "left_only", "left", false },
  { "both_selected", "both", false },
  { "no_selection", "none", false },
  { "right_with_gap", "both", true },
}) do
  reset_project()
  local track = split_midi_case(case[1], case[2], case[3])
  local state_before = reaper.GetProjectStateChangeCount(0)
  reaper.Main_OnCommand(40548, 0)
  reaper.UpdateArrange()
  write_case(case[1], { track }, state_before)
  output:flush()
end

for _, case in ipairs({
  { "audio_right_only", "right", false, false },
  { "audio_both_selected", "both", false, false },
  { "audio_no_selection", "none", false, false },
  { "audio_right_with_gap", "both", true, false },
  { "audio_source_discontinuity", "both", false, true },
}) do
  reset_project()
  local track = split_audio_case(case[1], case[2], case[3], case[4])
  if case[1] == "audio_both_selected" then
    local before_project = output_dir .. "/item-heal-splits-before.rpp"
    reaper.Main_SaveProjectEx(0, before_project, 0)
  end
  local state_before = reaper.GetProjectStateChangeCount(0)
  reaper.Main_OnCommand(40548, 0)
  reaper.UpdateArrange()
  write_case(case[1], { track }, state_before)
  if case[1] == "audio_both_selected" then
    reaper.GetSet_ArrangeView2(0, true, 0, 0, 0, 10)
    reaper.UpdateArrange()
    local after_project = output_dir .. "/item-heal-splits-after.rpp"
    reaper.Main_SaveProjectEx(0, after_project, 0)
  end
  output:flush()
end

reset_project()
local three_audio_track = split_audio_three_case("Three selected audio segments")
local three_audio_before = reaper.GetProjectStateChangeCount(0)
reaper.Main_OnCommand(40548, 0)
reaper.UpdateArrange()
write_case("audio_three_segments", { three_audio_track }, three_audio_before)
output:flush()

output:write("heal_before_project\titem-heal-splits-before.rpp\n")
output:write("heal_after_project\titem-heal-splits-after.rpp\n")
output:close()
