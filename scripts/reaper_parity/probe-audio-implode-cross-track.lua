-- Measure Main action 40438 across selected audio items on different tracks.
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
local output = assert(io.open(output_dir .. "/audio-implode-cross-track-native.tsv", "w"))
local project_base = output_dir .. "/audio-implode-cross-track"
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
      local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
      output:write(label, "\ttrack=", track_index, "\titem=", item_index,
        "\tposition=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_POSITION")),
        "\tlength=", string.format("%.9f", reaper.GetMediaItemInfo_Value(item, "D_LENGTH")),
        "\tloop_source=", math.floor(reaper.GetMediaItemInfo_Value(item, "B_LOOPSRC")),
        "\tselected=", reaper.IsMediaItemSelected(item) and 1 or 0,
        "\ttake_count=", reaper.CountTakes(item), "\tactive_index=", active_index, "\n")
      for take_index = 0, reaper.CountTakes(item) - 1 do
        local take = assert(reaper.GetTake(item, take_index))
        local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
        output:write(label, "\ttrack=", track_index, "\titem=", item_index,
          "\ttake=", take_index, "\tname=", name, "\tsource=", source_path(take),
          "\toffset=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")),
          "\trate=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_PLAYRATE")),
          "\tvolume=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_VOL")),
          "\tpan=", string.format("%.9f", reaper.GetMediaItemTakeInfo_Value(take, "D_PAN")), "\n")
      end
    end
  end
  output:flush()
end

local function save(label)
  reaper.Main_SaveProjectEx(0, project_base .. "-" .. label .. ".rpp", 0)
end

local function run_case(label, items)
  reaper.SelectAllMediaItems(0, false)
  for _, item in ipairs(items) do reaper.SetMediaItemSelected(item, true) end
  describe(label .. "_before")
  save(label .. "-before")
  local item_count_before = reaper.CountMediaItems(0)
  reaper.Main_OnCommand(40438, 0) -- Take: Implode items across tracks into takes.
  describe(label .. "_after")
  save(label .. "-after")
  if reaper.CountMediaItems(0) == item_count_before then
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

local top_track = insert_track("Top track")
local early_track = insert_track("Earlier item track")
local lower_track = insert_track("Lower track")
local top_item = insert_item(top_track, media_a, 1, 1.25, "Top item", 0.125, 0.8, -0.5, 0.9)
local early_item = insert_item(early_track, media_b, 0, 1.5, "Earlier item", 0.25, 0.7, 0.25, 1.1)
reaper.Main_OnCommand(40639, 0)
local early_alternate = assert(reaper.GetActiveTake(early_item))
reaper.GetSetMediaItemTakeInfo_String(early_alternate, "P_NAME", "Earlier alternate", true)
reaper.SetMediaItemTakeInfo_Value(early_alternate, "D_PLAYRATE", 0.8)
local lower_item = insert_item(lower_track, media_a, 2, 1.25, "Lower item", 0.375, 0.6, 0.5, 1.2)
run_case("staggered", { top_item, early_item, lower_item })

local aligned_top_track = insert_track("Aligned top track")
local aligned_middle_track = insert_track("Aligned middle track")
local aligned_lower_track = insert_track("Aligned lower track")
local aligned_top = insert_item(aligned_top_track, media_a, 6, 1, "Aligned top", 0.125, 0.8, -0.5, 0.9)
reaper.Main_OnCommand(40639, 0)
local top_alternate = assert(reaper.GetActiveTake(aligned_top))
reaper.GetSetMediaItemTakeInfo_String(top_alternate, "P_NAME", "Aligned top alternate", true)
local aligned_middle = insert_item(aligned_middle_track, media_b, 6, 1, "Aligned middle", 0.25, 0.7, 0.25, 1.1)
reaper.Main_OnCommand(40639, 0)
local middle_alternate = assert(reaper.GetActiveTake(aligned_middle))
reaper.GetSetMediaItemTakeInfo_String(middle_alternate, "P_NAME", "Aligned middle active", true)
local aligned_lower = insert_item(aligned_lower_track, media_a, 6, 1, "Aligned lower", 0.375, 0.6, 0.5, 1.2)
run_case("aligned", { aligned_top, aligned_middle, aligned_lower })

local same_track = insert_track("Single track control")
local same_top = insert_item(same_track, media_a, 12, 1.25, "Top same-track item", 0.125, 0.8, -0.5, 0.9)
local same_early = insert_item(same_track, media_b, 11, 1.5, "Earlier same-track item", 0.25, 0.7, 0.25, 1.1)
local same_late = insert_item(same_track, media_a, 13, 1.25, "Later same-track item", 0.375, 0.6, 0.5, 1.2)
run_case("same-track-control", { same_top, same_early, same_late })

local loop_target_track = insert_track("Loop target track")
local loop_donor_track = insert_track("Loop donor track")
local loop_target = insert_item(loop_target_track, media_a, 16, 1.5, "Loop target", 0.125)
local loop_donor = insert_item(loop_donor_track, media_b, 17, 1, "Loop donor", 0.25, 1, 0, 1.2)
reaper.SetMediaItemInfo_Value(loop_donor, "B_LOOPSRC", 1)
reaper.SetMediaItemInfo_Value(loop_target, "D_LENGTH", 1)
run_case("no-overlap-control", { loop_target, loop_donor })
reaper.SetMediaItemInfo_Value(loop_target, "D_LENGTH", 1.5)
run_case("loop-source", { loop_target, loop_donor })

output:write("action\t40438\tTake: Implode items across tracks into takes\n")
output:close()
reaper.UpdateArrange()
reaper.defer(function() end)
