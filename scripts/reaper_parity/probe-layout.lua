local root=assert(os.getenv("AAADAW_REAPER_PROBE_DIR")).."/"
assert(reaper.CountTracks(0)==0, "Use a disposable empty project")
local _,path=reaper.EnumProjects(-1, "")
assert(path=="", "Use an unsaved project")
for i,name in ipairs({"Audio 1", "Audio 2"}) do
 reaper.InsertTrackAtIndex(i-1,true)
 reaper.GetSetMediaTrackInfo_String(reaper.GetTrack(0,i-1),"P_NAME",name,true)
end
reaper.TrackList_AdjustWindows(false)
local start=reaper.time_precise()
local function measure()
 if reaper.time_precise()-start<0.3 then reaper.defer(measure); return end
 local log=assert(io.open(root.."layout-reference.txt","w"))
 log:write("Version="..reaper.GetAppVersion().." theme="..reaper.GetLastColorThemeFile().."\n")
 for _,track in ipairs({reaper.GetMasterTrack(0),reaper.GetTrack(0,0),reaper.GetTrack(0,1)}) do
  local _,name=reaper.GetTrackName(track)
  log:write(name.."\n")
  for _,field in ipairs({"I_TCPY","I_TCPH","I_TCPW","I_MCPX","I_MCPY","I_MCPW","I_MCPH"}) do
   log:write(field.."="..reaper.GetMediaTrackInfo_Value(track,field).."\n")
  end
 end
 log:close()
end
reaper.defer(measure)
