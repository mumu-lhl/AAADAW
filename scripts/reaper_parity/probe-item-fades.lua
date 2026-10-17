-- Owned scratch-project probe. Never run against a personal/open project.
local root=assert(os.getenv('AAADAW_REAPER_PROBE_DIR'))..'/'
assert(reaper.GetAppVersion():match('^7%.82/linux'))
assert(reaper.GetLastColorThemeFile():match('Default_7%.0%.ReaperThemeZip$'))
assert(reaper.CountTracks(0)==0)
local _,path=reaper.EnumProjects(-1,'');assert(path=='')
local log=assert(io.open(root..'item-fade-defaults.txt','w'))
log:write('Version='..reaper.GetAppVersion()..'\n')
local function dump(label,item)
 log:write(label..'\n')
 for _,field in ipairs({'D_FADEINLEN','D_FADEOUTLEN','D_FADEINLEN_AUTO','D_FADEOUTLEN_AUTO','C_FADEINSHAPE','C_FADEOUTSHAPE','D_FADEINDIR','D_FADEOUTDIR','D_FADEINDIR_NEW','D_FADEOUTDIR_NEW','D_FADEINDIR2_NEW','D_FADEOUTDIR2_NEW','D_VOL','B_LOOPSRC','D_LENGTH'}) do
  log:write(field..'='..reaper.GetMediaItemInfo_Value(item,field)..'\n')
 end
end
reaper.InsertTrackAtIndex(0,true)
local first=reaper.GetTrack(0,0)
local item=reaper.AddMediaItemToTrack(first)
local take=reaper.AddTakeToMediaItem(item)
reaper.SetMediaItemTake_Source(take,reaper.PCM_Source_CreateFromFile(root..'source.wav'))
reaper.SetMediaItemInfo_Value(item,'D_LENGTH',1)
dump('API creation',item)
reaper.InsertTrackAtIndex(1,true)
local second=reaper.GetTrack(0,1)
reaper.SetOnlyTrackSelected(second)
reaper.SetEditCurPos(0,false,false)
reaper.InsertMedia(root..'source.wav',0)
assert(reaper.CountTrackMediaItems(second)==1)
dump('InsertMedia',reaper.GetTrackMediaItem(second,0))
log:close()
-- Offline Master renders isolate the first track and measure manual fade curves.
reaper.SetMediaTrackInfo_Value(second,'B_MUTE',1)
for field,value in pairs({RENDER_BOUNDSFLAG=0,RENDER_STARTPOS=0,RENDER_ENDPOS=1,RENDER_SRATE=48000,RENDER_CHANNELS=2,RENDER_TAILFLAG=0}) do
 reaper.GetSetProjectInfo(0,field,value,true)
end
reaper.GetSetProjectInfo_String(0,'RENDER_FILE',root,true)
local curve_log=assert(io.open(root..'item-fade-curves.txt','w'))
local function render(name,shape,fade_in,fade_out,curvature,s_parameter)
 reaper.SetMediaItemInfo_Value(item,'D_FADEINLEN',fade_in)
 reaper.SetMediaItemInfo_Value(item,'D_FADEOUTLEN',fade_out)
 reaper.SetMediaItemInfo_Value(item,'C_FADEINSHAPE',shape)
 reaper.SetMediaItemInfo_Value(item,'C_FADEOUTSHAPE',shape)
 if curvature then
  reaper.SetMediaItemInfo_Value(item,'D_FADEINDIR_NEW',curvature)
  reaper.SetMediaItemInfo_Value(item,'D_FADEOUTDIR_NEW',curvature)
  reaper.SetMediaItemInfo_Value(item,'D_FADEINDIR2_NEW',s_parameter)
  reaper.SetMediaItemInfo_Value(item,'D_FADEOUTDIR2_NEW',s_parameter)
 end
 curve_log:write(name..' fade_in='..reaper.GetMediaItemInfo_Value(item,'D_FADEINLEN')..' fade_out='..reaper.GetMediaItemInfo_Value(item,'D_FADEOUTLEN')..' shape='..reaper.GetMediaItemInfo_Value(item,'C_FADEINSHAPE')..'\n')
 for _,field in ipairs({'D_FADEINDIR_NEW','D_FADEOUTDIR_NEW','D_FADEINDIR2_NEW','D_FADEOUTDIR2_NEW'}) do curve_log:write(field..'='..reaper.GetMediaItemInfo_Value(item,field)..' ') end
 curve_log:write('\n')
 curve_log:flush()
 reaper.UpdateItemInProject(item)
 reaper.GetSetProjectInfo_String(0,'RENDER_PATTERN',name,true)
 reaper.Main_OnCommand(42230,0)
end
for shape=0,6 do render('item-fade-shape-'..shape,shape,0.25,0.25) end
render('item-fade-overlap-linear',0,0.75,0.75)
render('item-fade-overlap-asymmetric',0,0.5,0.75)
render('item-fade-overlap-full',0,1,1)
render('item-fade-long-in',0,2,0)
render('item-fade-long-out',0,0,2)
for _,curve in ipairs({-0.75,-0.5,-0.25,0.25,0.5,0.75}) do render('item-fade-curve-'..curve,0,0.25,0.25,curve,0) end
for _,s in ipairs({-0.75,-0.5,-0.25,0.25,0.5,0.75}) do render('item-fade-s-'..s,0,0.25,0.25,0,s) end
render('item-fade-combined',0,0.25,0.25,0.25,0.5)
curve_log:close()
