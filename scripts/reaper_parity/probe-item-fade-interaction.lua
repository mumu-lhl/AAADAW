-- Run only in a fresh owned scratch instance, with the owned constant source.
local root=assert(os.getenv('AAADAW_REAPER_PROBE_DIR'))..'/'
assert(reaper.GetAppVersion():match('^7%.82/linux'))
assert(reaper.GetLastColorThemeFile():match('Default_7%.0%.ReaperThemeZip$'))
assert(reaper.CountTracks(0)==0)
local _,project_path=reaper.EnumProjects(-1,'');assert(project_path=='')
reaper.InsertTrackAtIndex(0,true)
local track=reaper.GetTrack(0,0)
reaper.SetOnlyTrackSelected(track)
reaper.SetEditCurPos(0,false,false)
reaper.InsertMedia(root..'source.wav',0)
assert(reaper.CountTrackMediaItems(track)==1)
local item=reaper.GetTrackMediaItem(track,0)
reaper.SetMediaItemSelected(item,true)
reaper.GetSet_ArrangeView2(0,true,0,0,0,2.5)
local log=assert(io.open(root..'fade-interaction.csv','w'))
log:write('time,x,y,surface,item_hit,fade_in,fade_out,in_shape,out_shape,item_start,item_length,in_curvature,in_s,out_curvature,out_s,lpf_flags\n')
local previous=''
reaper.atexit(function() log:close() end)
local function observe()
 local stop=io.open(root..'stop-probe','r')
 if stop then stop:close();return end
 local x,y=reaper.GetMousePosition()
 local _,surface=reaper.GetThingFromPoint(x,y)
 local hit=reaper.GetItemFromPoint(x,y,true)
 local row=string.format('%d,%d,%s,%d,%.12g,%.12g,%d,%d,%.12g,%.12g',x,y,surface or '',hit==item and 1 or 0,
  reaper.GetMediaItemInfo_Value(item,'D_FADEINLEN'),reaper.GetMediaItemInfo_Value(item,'D_FADEOUTLEN'),
  reaper.GetMediaItemInfo_Value(item,'C_FADEINSHAPE'),reaper.GetMediaItemInfo_Value(item,'C_FADEOUTSHAPE'),
  reaper.GetMediaItemInfo_Value(item,'D_POSITION'),reaper.GetMediaItemInfo_Value(item,'D_LENGTH'))
 for _,field in ipairs({'D_FADEINDIR_NEW','D_FADEINDIR2_NEW','D_FADEOUTDIR_NEW','D_FADEOUTDIR2_NEW','I_FADELPF'}) do
  row=row..','..string.format('%.12g',reaper.GetMediaItemInfo_Value(item,field))
 end
 if row~=previous then
  log:write(string.format('%.6f,',reaper.time_precise())..row..'\n');log:flush();previous=row
  local ok,chunk=reaper.GetItemStateChunk(item,'',false);assert(ok)
  local state=assert(io.open(root..'fade-state.txt','w'))
  for line in chunk:gmatch('[^\r\n]+') do if line:match('^FADE') then state:write(line..'\n') end end
  state:close()
 end
 reaper.defer(observe)
end
observe()
