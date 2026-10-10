-- Read owned Item parameter/state transitions; never inspect personal projects.
local root=assert(os.getenv('AAADAW_REAPER_PROBE_DIR'))..'/'
assert(reaper.GetAppVersion():match('^7%.82/linux'))
assert(reaper.GetLastColorThemeFile():match('Default_7%.0%.ReaperThemeZip$'))
assert(reaper.CountTracks(0)==0)
local _,path=reaper.EnumProjects(-1,'');assert(path=='')
reaper.InsertTrackAtIndex(0,true)
local item=reaper.AddMediaItemToTrack(reaper.GetTrack(0,0))
reaper.SetMediaItemInfo_Value(item,'D_LENGTH',1)
local log=assert(io.open(root..'fade-parameters.txt','w'))
local function dump(label)
 log:write(label..'\n')
 for _,field in ipairs({'C_FADEINSHAPE','D_FADEINDIR','D_FADEINDIR_NEW','D_FADEINDIR2_NEW'}) do log:write(field..'='..reaper.GetMediaItemInfo_Value(item,field)..'\n') end
 local ok,chunk=reaper.GetItemStateChunk(item,'',false);assert(ok)
 for line in chunk:gmatch('[^\r\n]+') do if line:match('^FADE') then log:write(line..'\n') end end
end
dump('API defaults')
for shape=0,6 do
 reaper.SetMediaItemInfo_Value(item,'C_FADEINSHAPE',shape)
 reaper.SetMediaItemInfo_Value(item,'C_FADEOUTSHAPE',shape)
 dump('compatibility shape '..shape)
end
reaper.SetMediaItemInfo_Value(item,'C_FADEINSHAPE',0)
reaper.SetMediaItemInfo_Value(item,'D_FADEINDIR_NEW',0)
reaper.SetMediaItemInfo_Value(item,'D_FADEINDIR2_NEW',0.5)
dump('current S=0.5')
reaper.SetMediaItemInfo_Value(item,'D_FADEINDIR_NEW',0.25)
dump('current curvature=0.25 S=0.5')
log:close()
