-- Measure REAPER 7.82 Paste as takes with multiple copied source items.
-- Set AAADAW_REAPER_PROBE_MEDIA_A/B and AAADAW_REAPER_PROBE_DIR.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")

local media_a = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_A"))
local media_b = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_B"))
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/audio-paste-as-takes-multiple-sources-native.tsv", "w"))
local project_base = output_dir .. "/audio-paste-as-takes-multiple-sources"

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.SetOnlyTrackSelected(track)

local function insert_item(path, position, name)
  reaper.SelectAllMediaItems(0, false)
  assert(reaper.InsertMedia(path, 0) == 1)
  local item = assert(reaper.GetSelectedMediaItem(0, 0))
  reaper.SetMediaItemInfo_Value(item, "D_POSITION", position)
  reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 1)
  reaper.GetSetMediaItemTakeInfo_String(assert(reaper.GetActiveTake(item)), "P_NAME", name, true)
  return item
end

local target_a = insert_item(media_a, 0, "Target A")
local target_b = insert_item(media_b, 2, "Target B")
local source_a = insert_item(media_a, 8, "Source A")
local source_b = insert_item(media_b, 10, "Source B")

local function source_path(take)
  return reaper.GetMediaSourceFileName(assert(reaper.GetMediaItemTake_Source(take)), "")
end

local function describe(label, targets)
  for target_index, item in ipairs(targets) do
    local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
    output:write(label, "\ttarget=", target_index, "\ttake_count=", reaper.CountTakes(item),
      "\tactive_index=", active_index, "\n")
    for take_index = 0, reaper.CountTakes(item) - 1 do
      local take = assert(reaper.GetTake(item, take_index))
      local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
      output:write(label, "\ttarget=", target_index, "\ttake=", take_index,
        "\tname=", name, "\tsource=", source_path(take), "\n")
    end
  end
  output:flush()
end

local function save(label)
  reaper.Main_SaveProjectEx(0, project_base .. "-" .. label .. ".rpp", 0)
end

reaper.SelectAllMediaItems(0, false)
reaper.SetMediaItemSelected(source_a, true)
reaper.SetMediaItemSelected(source_b, true)
assert(reaper.CountSelectedMediaItems(0) == 2)
reaper.Main_OnCommand(40057, 0) -- Edit: Copy items/tracks.
reaper.SelectAllMediaItems(0, false)
reaper.SetMediaItemSelected(target_a, true)
reaper.SetMediaItemSelected(target_b, true)
save("before")
reaper.Main_OnCommand(40603, 0) -- Take: Paste as takes in items.
describe("pair_after", { target_a, target_b })
save("pair-after")

reaper.Undo_DoUndo2(0)
describe("pair_undo", { target_a, target_b })
save("pair-undo")
reaper.Undo_DoRedo2(0)
describe("pair_redo", { target_a, target_b })
save("pair-redo")

reaper.Undo_DoUndo2(0)
reaper.SelectAllMediaItems(0, false)
reaper.SetMediaItemSelected(target_a, true)
reaper.Main_OnCommand(40603, 0)
describe("single_target_after", { target_a })
save("single-target-after")

output:write("action\t40603\tTake: Paste as takes in items\n")
output:close()
reaper.UpdateArrange()
reaper.defer(function() end)
