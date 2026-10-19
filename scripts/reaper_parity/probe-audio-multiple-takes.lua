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
local output_path = output_dir .. "/audio-multiple-takes-native.tsv"
local output = assert(io.open(output_path, "w"))
local project_base = output_dir .. "/audio-multiple-takes"

local function save(suffix)
  local path = project_base .. "-" .. suffix .. ".rpp"
  reaper.Main_SaveProjectEx(0, path, 0)
  local saved = io.open(path, "r")
  assert(saved, "could not save project " .. suffix)
  saved:close()
  return path
end

local function take_index(item)
  return math.floor(reaper.GetMediaItemInfo_Value(item, "I_CURTAKE"))
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

local track
local item
local function state(label)
  local count = reaper.CountTakes(item)
  local active = take_index(item)
  local take = assert(reaper.GetActiveTake(item))
  output:write(label, "\tcount=", count, "\tactive_index=", active,
    "\tactive_name=", take_name(take), "\tactive_source=", take_source(take), "\n")
  for index = 0, count - 1 do
    local current = assert(reaper.GetTake(item, index))
    output:write(label, "_take_", index, "\tname=", take_name(current),
      "\tsource=", take_source(current), "\n")
  end
  output:flush()
end

reaper.InsertTrackAtIndex(0, true)
track = assert(reaper.GetTrack(0, 0))
reaper.SetOnlyTrackSelected(track)
assert(reaper.InsertMedia(media_a, 0) == 1, "could not insert first source WAV")
item = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(item, "D_POSITION", 0)
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 2)
reaper.SetMediaItemSelected(item, true)
local initial_take = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(initial_take, "P_NAME", "Take A", true)
state("before_duplicate")
save("before")

reaper.Undo_BeginBlock2(0)
reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take
assert(reaper.CountTakes(item) == 2, "duplicate did not add a take")
local new_take_index = 1 - take_index(item)
local new_take = assert(reaper.GetTake(item, new_take_index))
local alternate_source = assert(reaper.PCM_Source_CreateFromFile(media_b), "could not load second source")
assert(reaper.SetMediaItemTake_Source(new_take, alternate_source), "could not assign second source")
reaper.GetSetMediaItemTakeInfo_String(new_take, "P_NAME", "Take B", true)
reaper.Undo_EndBlock2(0, "Duplicate active take with alternate source", -1)
state("after_duplicate")
save("after-duplicate")

reaper.Main_OnCommand(40435, 0) -- Options: Show all takes (when room)
state("show_all_takes")
reaper.UpdateArrange()

local before_next = take_index(item)
reaper.Main_OnCommand(40125, 0) -- Take: Switch items to next take
local after_next = take_index(item)
output:write("next_action\t", before_next, "->", after_next, "\n")
state("after_next")
save("after-next")

reaper.Main_OnCommand(40126, 0) -- Take: Switch items to previous take
local after_previous = take_index(item)
output:write("previous_action\t", after_next, "->", after_previous, "\n")
state("after_previous")
save("after-previous")

local undo_description = reaper.Undo_CanUndo2(0)
output:write("undo_before\t", undo_description or "", "\n")
reaper.Undo_DoUndo2(0)
state("after_undo")
save("after-undo")

local redo_description = reaper.Undo_CanRedo2(0)
output:write("redo_before\t", redo_description or "", "\n")
reaper.Undo_DoRedo2(0)
state("after_redo")
save("after-redo")

reaper.Undo_DoUndo2(0) -- Undo Previous take.
state("undo_previous")
save("undo-previous")
reaper.Undo_DoUndo2(0) -- Undo Next take.
state("undo_next")
save("undo-next")
reaper.Undo_DoUndo2(0) -- Undo Duplicate active take.
state("undo_duplicate")
save("undo-duplicate")

reaper.Undo_DoRedo2(0)
state("redo_duplicate")
save("redo-duplicate")
reaper.Undo_DoRedo2(0)
state("redo_next")
save("redo-next")
reaper.Undo_DoRedo2(0)
state("redo_previous")
save("redo-previous")

local _, saved_path = reaper.EnumProjects(-1, "")
output:write("project\t", saved_path or "", "\n")
output:close()
local marker = assert(io.open(output_dir .. "/capture-complete", "w"))
marker:write("ready for live-window capture\n")
marker:close()
reaper.UpdateArrange()
