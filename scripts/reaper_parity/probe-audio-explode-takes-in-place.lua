-- Measure Main action 40642, Take: Explode takes of items in place.
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
local output = assert(io.open(output_dir .. "/audio-explode-takes-in-place-native.tsv", "w"))
local project_base = output_dir .. "/audio-explode-takes-in-place"
assert(reaper.GetSetProjectInfo(0, "PROJECT_SRATE", 48000, true) == 48000)

local function insert_track(name)
  reaper.InsertTrackAtIndex(reaper.CountTracks(0), true)
  local track = assert(reaper.GetTrack(0, reaper.CountTracks(0) - 1))
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
  return track
end

local function insert_item(track, path, position, length, name, take_count, active_index)
  reaper.SetOnlyTrackSelected(track)
  reaper.SelectAllMediaItems(0, false)
  assert(reaper.InsertMedia(path, 0) == 1)
  local item = assert(reaper.GetSelectedMediaItem(0, 0))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", length)
  for index = 0, take_count - 1 do
    if index > 0 then
      reaper.SetMediaItemSelected(item, true)
      reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
    end
    local take = assert(reaper.GetActiveTake(item))
    reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", name .. " take " .. (index + 1), true)
    reaper.SetMediaItemTakeInfo_Value(take, "D_STARTOFFS", index * 0.125)
    reaper.SetMediaItemTakeInfo_Value(take, "D_PLAYRATE", 0.8 + index * 0.2)
  end
  reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", active_index)
  reaper.UpdateItemInProject(item)
  return item
end

local function source_path(take)
  local source = assert(reaper.GetMediaItemTake_Source(take))
  return reaper.GetMediaSourceFileName(source, "")
end

local function describe(label)
  output:write(label, "\ttrack_count=", reaper.CountTracks(0),
    "\titem_count=", reaper.CountMediaItems(0),
    "\tselected_count=", reaper.CountSelectedMediaItems(0), "\n")
  for track_index = 0, reaper.CountTracks(0) - 1 do
    local track = assert(reaper.GetTrack(0, track_index))
    local _, track_name = reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "", false)
    local item_count = reaper.CountTrackMediaItems(track)
    output:write(label, "\ttrack=", track_index, "\ttrack_name=", track_name,
      "\ttrack_item_count=", item_count, "\n")
    for item_index = 0, item_count - 1 do
      local item = assert(reaper.GetTrackMediaItem(track, item_index))
      output:write(label, "\ttrack=", track_index, "\titem=", item_index,
        "\tposition=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
        "\tlength=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
        "\tselected=", reaper.IsMediaItemSelected(item) and 1 or 0,
        "\ttake_count=", reaper.CountTakes(item),
        "\tactive_index=", math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE")), "\n")
      for take_index = 0, reaper.CountTakes(item) - 1 do
        local take = assert(reaper.GetTake(item, take_index))
        local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
        output:write(label, "\ttrack=", track_index, "\titem=", item_index,
          "\ttake=", take_index, "\tname=", name, "\tsource=", source_path(take),
          "\toffset=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
          "\trate=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_PLAYRATE")), "\n")
      end
    end
  end
  output:flush()
end

local function save(label)
  reaper.Main_SaveProjectEx(0, project_base .. "-" .. label .. ".rpp", 0)
end

local function state_signature()
  local parts = { tostring(reaper.CountTracks(0)), tostring(reaper.CountMediaItems(0)) }
  for track_index = 0, reaper.CountTracks(0) - 1 do
    local track = assert(reaper.GetTrack(0, track_index))
    for item_index = 0, reaper.CountTrackMediaItems(track) - 1 do
      local item = assert(reaper.GetTrackMediaItem(track, item_index))
      parts[#parts + 1] = table.concat({
        string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
        tostring(reaper.CountTakes(item)),
        tostring(math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))),
      }, ":")
    end
  end
  return table.concat(parts, "|")
end

local function run_case(label, items)
  reaper.SelectAllMediaItems(0, false)
  for _, item in ipairs(items) do reaper.SetMediaItemSelected(item, true) end
  describe(label .. "_before")
  save(label .. "-before")
  local before = state_signature()
  reaper.Main_OnCommand(40642, 0) -- Take: Explode takes of items in place.
  describe(label .. "_after")
  save(label .. "-after")
  if state_signature() == before then
    output:write(label, "\tresult=no-op\n")
    output:flush()
    return
  end
  reaper.Undo_DoUndo2(0)
  describe(label .. "_undo")
  save(label .. "-undo")
  reaper.Undo_DoRedo2(0)
  describe(label .. "_redo")
  save(label .. "-redo")
end

local multi_track_a = insert_track("Multi-selection A")
local multi_track_b = insert_track("Multi-selection B")
local multi_a = insert_item(multi_track_a, media_a, 1, 1.5, "Multi A", 2, 1)
local multi_b = insert_item(multi_track_b, media_b, 4, 2, "Multi B", 2, 0)
run_case("multiple-items", { multi_a, multi_b })

local mixed_track = insert_track("Mixed-take selection")
local mixed_multi = insert_item(mixed_track, media_a, 8, 1.25, "Mixed multi", 2, 1)
local mixed_single = insert_item(mixed_track, media_b, 10, 1.25, "Mixed single", 1, 0)
run_case("mixed-items", { mixed_multi, mixed_single })

local single_track = insert_track("Single-take control")
local single_item = insert_item(single_track, media_b, 12, 1.25, "Single", 1, 0)
run_case("single-take-control", { single_item })

local takes_track = insert_track("Three-take item")
local three_takes = insert_item(takes_track, media_a, 6, 2, "Three", 3, 1)
run_case("three-take-item", { three_takes })

output:write("action\t40642\tTake: Explode takes of items in place\n")
output:close()
reaper.UpdateArrange()
reaper.defer(function() end)
