local root=assert(os.getenv("AAADAW_REAPER_PROBE_DIR")).."/"
assert(reaper.CountTracks(0)==5, "Only the disposable five-track folder probe is allowed")
local _,project_path=reaper.EnumProjects(-1, "")
assert(project_path=="", "Use only an unsaved disposable probe project")
for i,expected in ipairs({"Outer","Inner","Leaf","Sibling","Outside"}) do
 local _,actual=reaper.GetTrackName(reaper.GetTrack(0,i-1))
 assert(actual==expected, "Wrong probe track layout")
end
local outer=reaper.GetTrack(0,0)
local _,name=reaper.GetTrackName(outer)
assert(name=="Outer", "Wrong probe project")
local original_mode=reaper.GetMediaTrackInfo_Value(outer,"I_FOLDERCOMPACT")
local log=assert(io.open(root.."folder-compact.txt","w"))
for _,mode in ipairs({0,1,2}) do
 reaper.SetMediaTrackInfo_Value(outer,"I_FOLDERCOMPACT",mode)
 reaper.TrackList_AdjustWindows(false)
 reaper.UpdateArrange()
 log:write("compact="..mode.."\n")
 for i=0,4 do
  local t=reaper.GetTrack(0,i)
  local _,name=reaper.GetTrackName(t)
  log:write(name.." y="..reaper.GetMediaTrackInfo_Value(t,"I_TCPY").." height="..reaper.GetMediaTrackInfo_Value(t,"I_TCPH").."\n")
 end
end
log:close()

reaper.SetMediaTrackInfo_Value(outer,"I_FOLDERCOMPACT",original_mode)
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()
