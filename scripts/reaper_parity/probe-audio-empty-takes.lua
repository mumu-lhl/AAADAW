-- Measure Main actions 41351/41352 for an Audio item with two distinct Takes.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.GetPlayStateEx(0) == 0)
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "")
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local audio_path = assert(os.getenv("AAADAW_REAPER_PROBE_MEDIA_A"))
local output = assert(io.open(output_dir .. "/audio-empty-takes-native.tsv", "w"))
local project_base = output_dir .. "/audio-empty-takes"

reaper.Undo_BeginBlock2(0)
reaper.InsertTrackAtIndex(0, true)
local track = assert(reaper.GetTrack(0, 0))
reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "Audio empty takes", true)
reaper.SetOnlyTrackSelected(track)
reaper.SetEditCurPos(0, false, false)
assert(reaper.InsertMedia(audio_path, 0) == 1, "could not insert Audio source")
local item = assert(reaper.GetTrackMediaItem(track, 0))
reaper.SetMediaItemInfo_Value(item, "D_LENGTH", 2)
local take_a = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(take_a, "P_NAME", "Take A", true)
reaper.SetMediaItemSelected(item, true)
reaper.Main_OnCommand(40639, 0) -- Take: Duplicate active take.
local take_b = assert(reaper.GetActiveTake(item))
reaper.GetSetMediaItemTakeInfo_String(take_b, "P_NAME", "Take B", true)
reaper.SetMediaItemTakeInfo_Value(take_b, "D_STARTOFFS", 0.25)
reaper.SetMediaItemInfo_Value(item, "I_CURTAKE", 1)
reaper.SetMediaItemSelected(item, true)
reaper.UpdateItemInProject(item)
reaper.Undo_EndBlock2(0, "Audio empty takes fixture setup", -1)

local function capture(label)
  local current = assert(reaper.GetMediaItem(0, 0))
  local active_index = math.floor(reaper.GetMediaItemInfo_Value(current, "I_CURTAKE"))
  output:write(label, "\tactive_index=", active_index,
    "\ttake_count=", reaper.CountTakes(current),
    "\tselected=", reaper.IsMediaItemSelected(current) and 1 or 0, "\n")
  reaper.Main_SaveProjectEx(0, project_base .. "-" .. label .. ".rpp", 0)
  for index = 0, reaper.CountTakes(current) - 1 do
    local take = reaper.GetMediaItemTake(current, index)
    if not take then
      output:write("take\t", label, "\tindex=", index, "\tapi_take=none\n")
    else
      local _, name = reaper.GetSetMediaItemTakeInfo_String(take, "P_NAME", "", false)
      local source = reaper.GetMediaItemTake_Source(take)
      local source_length, length_is_qn, source_type, source_path = 0, false, "none", ""
      if source then
        source_length, length_is_qn = reaper.GetMediaSourceLength(source)
        source_type = reaper.GetMediaSourceType(source, "")
        source_path = reaper.GetMediaSourceFileName(source, "")
      end
      output:write("take\t", label, "\tindex=", index, "\tname=", name,
        "\tactive=", index == active_index and 1 or 0,
        "\tmidi=", reaper.TakeIsMIDI(take) and 1 or 0,
        "\tsource_type=", source_type, "\tsource_path=", source_path,
        "\tsource_length=", string.format("%.12f", source_length),
        "\tlength_is_qn=", length_is_qn and 1 or 0,
        "\tsource_offset=", string.format("%.12f", reaper.GetMediaItemTakeInfo_Value(take, "D_STARTOFFS")), "\n")
    end
  end
  output:flush()
end

local function save(label)
  reaper.Main_SaveProjectEx(0, project_base .. "-" .. label .. ".rpp", 0)
end

capture("before")
reaper.Main_OnCommand(41351, 0) -- Item: Add an empty take before the active take.
capture("before-active")
reaper.Undo_DoUndo2(0)
capture("undo-before-active")
reaper.Undo_DoRedo2(0)
capture("redo-before-active")
reaper.Undo_DoUndo2(0)
reaper.Main_OnCommand(41352, 0) -- Item: Add an empty take after the active take.
capture("after-active")
reaper.Undo_DoUndo2(0)
capture("undo-after-active")
reaper.Undo_DoRedo2(0)
capture("redo-after-active")
output:write("action_before\t41351\tItem: Add an empty take before the active take\n")
output:write("action_after\t41352\tItem: Add an empty take after the active take\n")
output:write("reference\tREAPER 7.82/linux-x86_64; Default_7.0.ReaperThemeZip\n")
output:close()
local complete = assert(io.open(output_dir .. "/audio-empty-takes-capture-complete", "w"))
complete:write("complete\n")
complete:close()
reaper.UpdateArrange()
reaper.defer(function() end)
