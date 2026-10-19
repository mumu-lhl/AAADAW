-- Run in an empty, stopped Linux REAPER 7.82 / Default 7 project.
-- Set AAADAW_REAPER_PROBE_MEDIA_A/B and AAADAW_REAPER_PROBE_DIR.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
assert(reaper.CountTracks(0) == 0, "probe requires an empty project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "probe must run in an unsaved project")

local media_a = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_A"))
local media_b = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_B"))
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/audio-paste-as-takes-native.tsv", "w"))
local project_base = output_dir .. "/audio-paste-as-takes"

local function take_name(take)
  local ok, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
  assert(ok, "could not read take name")
  return name
end

local function take_source(take)
  local source = assert(reaper.GetMediaItemTake_Source(take))
  return reaper.GetMediaSourceFileName(source, "")
end

local function item_state(label, item)
  local count = reaper.CountTakes(item)
  local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
  local active = assert(reaper.GetActiveTake(item))
  output:write(label, "\tcount=", count, "\tactive_index=", active_index,
    "\tactive_name=", take_name(active), "\tactive_source=", take_source(active), "\n")
  for index = 0, count - 1 do
    local take = assert(reaper.GetTake(item, index))
    output:write(label, "_take_", index, "\tname=", take_name(take),
      "\tsource=", take_source(take), "\n")
  end
  output:flush()
end

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local saved = io.open(path, "r")
  assert(saved, "could not save project " .. suffix)
  saved:close()
  return path
end

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.SetOnlyTrackSelected(track)
assert(reaper.InsertMedia(media_a, 0) == 1, "could not insert target source WAV")
local target = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(target, "D_POSITION", 0)
reaper.SetMediaItemInfo_Value(target, "D_LENGTH", 2)
reaper.SetMediaItemSelected(target, true)
local target_take = assert(reaper.GetActiveTake(target))
reaper.GetSetMediaItemTakeInfo_String(target_take, "P_NAME", "Target A", true)

assert(reaper.InsertMedia(media_b, 0) == 1, "could not insert clipboard source WAV")
assert(reaper.CountTrackMediaItems(track) == 2, "expected target and clipboard source items")
local clipboard_source_item
for index = 0, reaper.CountTrackMediaItems(track) - 1 do
  local item = assert(reaper.GetTrackMediaItem(track, index))
  local take = assert(reaper.GetActiveTake(item))
  if take_source(take) == media_b then
    clipboard_source_item = item
  end
end
clipboard_source_item = assert(clipboard_source_item, "could not find clipboard source item")
reaper.SetMediaItemInfo_Value(clipboard_source_item, "D_POSITION", 4)
reaper.SetMediaItemInfo_Value(clipboard_source_item, "D_LENGTH", 2)
local clipboard_take = assert(reaper.GetActiveTake(clipboard_source_item))
reaper.GetSetMediaItemTakeInfo_String(clipboard_take, "P_NAME", "Source B", true)

reaper.SelectAllMediaItems(0, false)
reaper.SetMediaItemSelected(clipboard_source_item, true)
reaper.Main_OnCommand(40057, 0) -- Edit: Copy items/tracks.
reaper.SelectAllMediaItems(0, false)
reaper.SetMediaItemSelected(target, true)
item_state("before_paste_as_takes", target)
save("before")

reaper.Undo_BeginBlock2(0)
reaper.Main_OnCommand(40603, 0) -- Take: Paste as takes in items.
reaper.Undo_EndBlock2(0, "Paste as takes from alternate source", -1)
item_state("after_paste_as_takes", target)
output:write("track_item_count_after", "\t", reaper.CountTrackMediaItems(track), "\n")
save("after")

local undo_description = reaper.Undo_CanUndo2(0)
output:write("undo_before\t", undo_description or "", "\n")
reaper.Undo_DoUndo2(0)
item_state("after_undo", target)
save("undo")
reaper.Undo_DoRedo2(0)
item_state("after_redo", target)
save("redo")

local marker = assert(io.open(output_dir .. "/paste-as-takes-complete", "w"))
marker:write("ready for live-window capture\n")
marker:close()
output:close()
reaper.UpdateArrange()
