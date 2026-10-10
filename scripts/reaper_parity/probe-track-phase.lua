-- Run only in a new disposable project with an isolated REAPER configuration.
local root=assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR").."/"
assert(reaper.CountTracks(0)==0, "Routing probe requires an empty disposable project")
local _,project_path=reaper.EnumProjects(-1, "")
assert(project_path=="", "Use an unsaved disposable probe project")
local function track(name,file)
 reaper.InsertTrackAtIndex(reaper.CountTracks(0),true)
 local t=reaper.GetTrack(0,reaper.CountTracks(0)-1)
 reaper.GetSetMediaTrackInfo_String(t,"P_NAME",name,true)
 local i=reaper.AddMediaItemToTrack(t)
 local take=reaper.AddTakeToMediaItem(i)
 reaper.SetMediaItemTake_Source(take,reaper.PCM_Source_CreateFromFile(root..file..".wav"))
 reaper.SetMediaItemInfo_Value(i,"D_LENGTH",1)
 reaper.SetMediaItemInfo_Value(i,"D_FADEINLEN",0)
 reaper.SetMediaItemInfo_Value(i,"D_FADEOUTLEN",0)
 return t
end
local src=track("Source","source")
local dst=track("Receiver","receiver")
local log=assert(io.open(root.."phase-reference.txt","w"))
log:write("Source pan law: "..reaper.GetMediaTrackInfo_Value(src,"D_PANLAW").."\n")
log:write("Project pan law: "..reaper.GetSetProjectInfo(0,"PANLAW",0,false).."\n")
reaper.GetSetProjectInfo(0,"RENDER_BOUNDSFLAG",0,true)
reaper.GetSetProjectInfo(0,"RENDER_STARTPOS",0,true)
reaper.GetSetProjectInfo(0,"RENDER_ENDPOS",1,true)
reaper.GetSetProjectInfo(0,"RENDER_SRATE",48000,true)
reaper.GetSetProjectInfo(0,"RENDER_CHANNELS",2,true)
reaper.GetSetProjectInfo(0,"RENDER_TAILFLAG",0,true)
reaper.GetSetProjectInfo_String(0,"RENDER_FILE",root,true)
local function render(name)
 reaper.GetSetProjectInfo_String(0,"RENDER_PATTERN",name,true)
 reaper.Main_OnCommand(42230,0)
end
reaper.SetMediaTrackInfo_Value(src,"B_MAINSEND",0)
reaper.SetMediaTrackInfo_Value(dst,"D_VOL",1)
-- Keep the receiver's own media silent, retaining only the routed signal.
local receiver_item=reaper.GetTrackMediaItem(dst,0)
reaper.SetMediaItemInfo_Value(receiver_item,"B_MUTE",1)
local send=reaper.CreateTrackSend(src,dst)
reaper.SetMediaTrackInfo_Value(src,"D_VOL",0.5)
reaper.SetMediaTrackInfo_Value(src,"D_PAN",0)
for _,phase in ipairs({0,1}) do
 reaper.SetMediaTrackInfo_Value(src,"B_PHASE",phase)
 log:write("phase="..reaper.GetMediaTrackInfo_Value(src,"B_PHASE").."\n")
 for _,mode in ipairs({0,1,3}) do
  reaper.SetTrackSendInfo_Value(src,0,send,"I_SENDMODE",mode)
  render("track-phase-"..phase.."-tap-"..mode)
 end
end
log:close()
