-- Run with an isolated factory configuration in a disposable unsaved project.
local root=assert(os.getenv("AAADAW_REAPER_PROBE_DIR")).."/"
assert(reaper.CountTracks(0)==0, "Use an empty disposable project")
local _,path=reaper.EnumProjects(-1, "")
assert(path=="", "Use an unsaved project")
reaper.InsertTrackAtIndex(0,true)
local track=reaper.GetTrack(0,0)
local item=reaper.AddMediaItemToTrack(track)
local take=reaper.AddTakeToMediaItem(item)
reaper.SetMediaItemTake_Source(take,reaper.PCM_Source_CreateFromFile(root.."source.wav"))
for field,value in pairs({D_LENGTH=1,D_FADEINLEN=0,D_FADEOUTLEN=0}) do
 reaper.SetMediaItemInfo_Value(item,field,value)
end
for field,value in pairs({RENDER_BOUNDSFLAG=0,RENDER_STARTPOS=0,RENDER_ENDPOS=1,RENDER_SRATE=48000,RENDER_CHANNELS=2,RENDER_TAILFLAG=0}) do
 reaper.GetSetProjectInfo(0,field,value,true)
end
reaper.GetSetProjectInfo_String(0,"RENDER_FILE",root,true)
local master=reaper.GetMasterTrack(0)
local log=assert(io.open(root.."master-reference.txt","w"))
log:write("Version="..reaper.GetAppVersion().."\n")
for _,field in ipairs({"D_VOL","D_PAN","D_PANLAW","I_PANMODE","B_MUTE","B_PHASE"}) do
 log:write(field.."="..reaper.GetMediaTrackInfo_Value(master,field).."\n")
end
local function render(name,vol,pan,mute,phase)
 for field,value in pairs({D_VOL=vol,D_PAN=pan,B_MUTE=mute,B_PHASE=phase}) do
  local accepted=reaper.SetMediaTrackInfo_Value(master,field,value)
  log:write(name.." "..field.." accepted="..tostring(accepted).." readback="..reaper.GetMediaTrackInfo_Value(master,field).."\n")
 end
 log:flush()
 reaper.GetSetProjectInfo_String(0,"RENDER_PATTERN",name,true)
 reaper.Main_OnCommand(42230,0)
end
render("master-unity",1,0,0,0)
render("master-half",0.5,0,0,0)
render("master-left",1,-0.5,0,0)
render("master-right",1,0.5,0,0)
render("master-muted",1,0,1,0)
render("master-phase",1,0,0,1)
log:close()
