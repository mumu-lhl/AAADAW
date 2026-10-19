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
local output = assert(io.open(output_dir .. "/audio-move-active-take-to-top-native.tsv", "w"))
local project_base = output_dir .. "/audio-move-active-take-to-top"

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local saved = io.open(path, "r")
  assert(saved, "could not save project " .. suffix)
  saved:close()
  return path
end

local function take_name(take)
  local ok, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
  assert(ok, "could not read take name")
  return name
end

local function take_source(take)
  local source = assert(reaper.GetMediaItemTake_Source(take))
  return reaper.GetMediaSourceFileName(source, "")
end

local function state(label, item)
  local count = reaper.CountTakes(item)
  local active_index = math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
  output:write(label, "\ttake_count=", count, "\tactive_index=", active_index, "\n")
  for index = 0, count - 1 do
    local take = assert(reaper.GetTake(item, index))
    output:write(label, "\ttake_", index, "\tname=", take_name(take),
      "\tsource=", take_source(take), "\n")
  end
  output:flush()
end

reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.SetOnlyTrackSelected(track)
assert(reaper.InsertMedia(media_a, 0) == 1, "could not insert first source WAV")
local item = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 2)
local original = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(original, "P_NAME", "Take A", true)
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
assert(reaper.CountTakes(item) == 2, "duplicate did not add a second take")
local alternate = assert(reaper.GetTake(item, 0))
local source_b = assert(reaper.PCM_Source_CreateFromFile(media_b), "could not load second source")
assert(reaper.SetMediaItemTake_Source(alternate, source_b), "could not assign second source")
reaper.GetSetMediaItemTakeInfo_String(alternate, "P_NAME", "Take B", true)
assert(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE") == 1, "Take A should stay active")
reaper.Undo_OnStateChange2(0, "Prepare active-take ordering probe")
save("before")
state("before", item)

reaper.Undo_BeginBlock2(0)
reaper.Main_OnCommand(41380, 0) -- Item: Move active takes to top.
reaper.Undo_EndBlock2(0, "Move active take to top", -1)
state("after", item)
save("after")

local undo_before = reaper.Undo_CanUndo2(0) or ""
reaper.Undo_DoUndo2(0)
state("undo", item)
save("undo")
local redo_before = reaper.Undo_CanRedo2(0) or ""
reaper.Undo_DoRedo2(0)
state("redo", item)
save("redo")
output:write("undo_description\t", undo_before, "\n")
output:write("redo_description\t", redo_before, "\n")

local no_op_undo = reaper.Undo_CanUndo2(0) or ""
reaper.Undo_BeginBlock2(0)
reaper.Main_OnCommand(41380, 0) -- Already active at index 0; should not reorder.
reaper.Undo_EndBlock2(0, "Move already top take", -1)
state("already_top", item)
output:write("already_top_undo_before\t", no_op_undo, "\n")
output:write("already_top_undo_after\t", reaper.Undo_CanUndo2(0) or "", "\n")
output:write("action\t41380\tItem: Move active takes to top\n")
output:close()

local marker = assert(io.open(output_dir .. "/capture-complete", "w"))
marker:write("ready for live-window capture\n")
marker:close()
reaper.UpdateArrange()
