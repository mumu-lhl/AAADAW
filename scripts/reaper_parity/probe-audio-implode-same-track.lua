-- Measure Main action 40543 for same-track audio items.
-- REAPER 7.82 / Linux / Default 7 is required.
-- Set AAADAW_REAPER_PROBE_DIR to an existing output directory and
-- AAADAW_REAPER_PROBE_MEDIA_A/B to two PCM WAV sources.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")
local media_a = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_A"))
local media_b = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_B"))
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/audio-implode-same-track-native.tsv", "w"))
local project_base = output_dir .. "/audio-implode-same-track"
local sample_rate = reaper.GetSetProjectInfo(0, "PROJECT_SRATE", 48000, true)
assert(sample_rate == 48000)

local function insert_track(name)
  reaper.InsertTrackAtIndex(reaper.CountTracks(0), true)
  local track = assert(reaper.GetTrack(0, reaper.CountTracks(0) - 1))
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  reaper.SetOnlyTrackSelected(track)
  return track
end

local function insert_item(track, path, position, length, name, offset, volume, pan, rate)
  reaper.SetOnlyTrackSelected(track)
  reaper.SelectAllMediaItems(0, false)
  assert(reaper.InsertMedia(path, 0) == 1)
  local item = assert(reaper.GetSelectedMediaItem(0, 0))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", length)
  local take = assert(reaper.GetActiveTake(item))
  reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", name, true)
  reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", offset)
  reaper.SetMediaItemTakeInfo_Value(take, "D_VOL", volume or 1)
  reaper.SetMediaItemTakeInfo_Value(take, "D_PAN", pan or 0)
  reaper.SetMediaItemTakeInfo_Value(take, "D_PLAYRATE", rate or 1)
  return item
end

local function source_path(take)
  local source = assert(reaper.GetMediaItemTake_Source(take))
  return reaper.GetMediaSourceFileName(source, "")
end

local function describe(label, track)
  local item_count = reaper.CountTrackMediaItems(track)
  output:write(label, "\titem_count=", item_count,
    "\tselected_count=", reaper.CountSelectedMediaItems(0), "\n")
  for item_index = 0, item_count - 1 do
    local item = assert(reaper.GetTrackMediaItem(track, item_index))
    local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
    output:write(label, "\titem=", item_index,
      "\tposition=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
      "\tlength=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
      "\tselected=", reaper.IsMediaItemSelected(item) and 1 or 0,
      "\ttake_count=", reaper.CountTakes(item),
      "\tactive_index=", active_index, "\n")
    for take_index = 0, reaper.CountTakes(item) - 1 do
      local take = assert(reaper.GetTake(item, take_index))
      local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
      output:write(label, "\titem=", item_index,
        "\ttake=", take_index, "\tname=", name,
        "\tsource=", source_path(take),
        "\toffset=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
        "\trate=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_PLAYRATE")),
        "\tvolume=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_VOL")),
        "\tpan=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_PAN")), "\n")
    end
  end
  output:flush()
end

local function save(label)
  reaper.Main_SaveProjectEx(0, project_base .. "-" .. label .. ".rpp", 0)
end

local function run_case(label, track, items)
  reaper.SelectAllMediaItems(0, false)
  for _, item in ipairs(items) do reaper.SetMediaItemSelected(item, true) end
  describe(label .. "_before", track)
  save(label .. "-before")
  reaper.Main_OnCommand(40543, 0) -- Take: Implode items on same track into takes.
  describe(label .. "_after", track)
  save(label .. "-after")
  reaper.Undo_DoUndo2(0)
  describe(label .. "_undo", track)
  save(label .. "-undo")
  reaper.Undo_DoRedo2(0)
  describe(label .. "_redo", track)
  save(label .. "-redo")
end

local overlap_track = insert_track("Staggered items")
local overlap_a = insert_item(overlap_track, media_a, 0, 1.25, "Early A", 0.125, 0.8, -0.5, 0.9)
local overlap_b = insert_item(overlap_track, media_b, 1, 1.5, "Middle B", 0.25, 0.7, 0.25, 1.1)
local overlap_c = insert_item(overlap_track, media_a, 2, 1.25, "Late C", 0.375, 0.6, 0.5, 1.2)
run_case("staggered", overlap_track, { overlap_a, overlap_b, overlap_c })

local aligned_track = insert_track("Aligned items")
local aligned_a = insert_item(aligned_track, media_a, 6, 1, "Aligned A", 0.125, 0.8, -0.5, 0.9)
local aligned_b = insert_item(aligned_track, media_b, 6, 1, "Aligned B", 0.25, 0.7, 0.25, 1.1)
local aligned_c = insert_item(aligned_track, media_a, 6, 1, "Aligned C", 0.375, 0.6, 0.5, 1.2)
run_case("aligned", aligned_track, { aligned_a, aligned_b, aligned_c })

local multi_take_track = insert_track("Existing takes")
local multi_take_item = insert_item(multi_take_track, media_a, 9, 1, "Existing base", 0.5)
reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
local duplicated_take = assert(reaper.GetActiveTake(multi_take_item))
reaper.GetSetMediaItemTakeInfo_String(duplicated_take, "P_NAME", "Existing alternate", true)
local active_source_item = insert_item(multi_take_track, media_b, 10, 1, "New source", 0.625)
reaper.Main_OnCommand(40639, 0) -- Give the donor an inactive take as well.
duplicated_take = assert(reaper.GetActiveTake(active_source_item))
reaper.GetSetMediaItemTakeInfo_String(duplicated_take, "P_NAME", "New source active", true)
run_case("existing-takes", multi_take_track, { multi_take_item, active_source_item })

output:write("action\t40543\tTake: Implode items on same track into takes\n")
output:close()
reaper.UpdateArrange()
reaper.defer(function() end)
