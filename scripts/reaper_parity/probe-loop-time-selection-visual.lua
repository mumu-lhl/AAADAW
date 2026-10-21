-- Run only in an isolated REAPER 7.82 configuration and disposable project.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")

if reaper.GetToggleCommandState(40621) == 1 then
  reaper.Main_OnCommand(40750, 0)
end
if reaper.CountTracks(0) == 0 then
  reaper.InsertTrackAtIndex(0, true)
end
reaper.GetSet_LoopTimeRange2(0, true, false, 1, 2, false)
reaper.GetSet_LoopTimeRange2(0, true, true, 3, 4, false)
reaper.SetEditCurPos2(0, 0, false, false)
reaper.GetSet_ArrangeView2(0, true, 0, 0, 0, 6)
reaper.UpdateArrange()
