-- Inspect ordinary track Hardware Output creation and channel settings on
-- Linux REAPER 7.82 / Default 7 using a disposable empty project.
local root = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR") .. "/"
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
assert(reaper.CountTracks(0) == 0, "Use an empty disposable project")
local _, project_path = reaper.EnumProjects(-1, "")
assert(project_path == "", "Use an unsaved disposable project")
assert(reaper.GetPlayStateEx(0) == 0, "Transport must be stopped")

reaper.InsertTrackAtIndex(0, true)
local source = assert(reaper.GetTrack(0, 0))
assert(reaper.GetSetMediaTrackInfo_String(source, "P_NAME", "Hardware Output Source", true))

local result = assert(io.open(root .. "hardware-output-native.tsv", "w"))
result:write("reference\t", reaper.GetAppVersion(), "; Linux; Default 7; isolated project\n")
local send = reaper.CreateTrackSend(source, nil)
result:write("CreateTrackSend_nil\t", tostring(send), "\n")
result:write("category_0_count\t", reaper.GetTrackNumSends(source, 0), "\n")
result:write("category_1_count\t", reaper.GetTrackNumSends(source, 1), "\n")
result:write("category_2_count\t", reaper.GetTrackNumSends(source, 2), "\n")

if send >= 0 then
  for _, field in ipairs({ "I_SRCCHAN", "I_DSTCHAN", "D_VOL", "D_PAN", "B_MUTE", "B_PHASE", "I_SENDMODE", "I_MIDIFLAGS" }) do
    local value = reaper.GetTrackSendInfo_Value(source, 2, send, field)
    result:write("default_", field, "\t", string.format("%.9f", value), "\n")
  end
  for _, test in ipairs({
    { "stereo_to_stereo", 0, 0 },
    { "mono_1_to_stereo", 1024, 0 },
    { "mono_2_to_mono_1", 1025, 1024 },
  }) do
    assert(reaper.SetTrackSendInfo_Value(source, 2, send, "I_SRCCHAN", test[2]))
    assert(reaper.SetTrackSendInfo_Value(source, 2, send, "I_DSTCHAN", test[3]))
    result:write(test[1], "\tI_SRCCHAN=", reaper.GetTrackSendInfo_Value(source, 2, send, "I_SRCCHAN"))
    result:write("\tI_DSTCHAN=", reaper.GetTrackSendInfo_Value(source, 2, send, "I_DSTCHAN"), "\n")
    reaper.Main_SaveProjectEx(0, root .. "hardware-output-" .. test[1] .. ".rpp", 0)
  end
  for _, mode in ipairs({ 0, 1, 2, 3 }) do
    local accepted = reaper.SetTrackSendInfo_Value(source, 2, send, "I_SENDMODE", mode)
    result:write("send_mode_", mode, "\taccepted=", tostring(accepted))
    result:write("\treadback=", reaper.GetTrackSendInfo_Value(source, 2, send, "I_SENDMODE"), "\n")
  end
  reaper.SetTrackSendInfo_Value(source, 2, send, "I_SRCCHAN", 0)
  reaper.SetTrackSendInfo_Value(source, 2, send, "I_DSTCHAN", 0)
  reaper.SetTrackSendInfo_Value(source, 2, send, "I_SENDMODE", 0)
end

reaper.SetOnlyTrackSelected(source)
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()
reaper.Main_SaveProjectEx(0, root .. "hardware-output-after.rpp", 0)
result:close()
reaper.Main_OnCommand(40293, 0) -- Track: View routing and I/O for current/last touched track
local complete = assert(io.open(root .. "hardware-output-capture-complete", "w"))
complete:write("complete\n")
complete:close()
reaper.defer(function() end)
