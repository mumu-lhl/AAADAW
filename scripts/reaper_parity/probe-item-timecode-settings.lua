-- Owned media and numeric observations only; no application assets captured.
local root=assert(os.getenv('AAADAW_REAPER_PROBE_DIR'))..'/'
assert(reaper.GetAppVersion():match('^7%.82/linux'))
assert(reaper.GetLastColorThemeFile():match('Default_7%.0%.ReaperThemeZip$'))
assert(reaper.CountTracks(0)==0)
local _,path=reaper.EnumProjects(-1,'');assert(path=='')
reaper.GetSetProjectInfo(0,'PROJECT_SRATE',48000,true)
reaper.GetSetProjectInfo(0,'PROJECT_SRATE_USE',1,true)
reaper.InsertTrackAtIndex(0,false)
local item=reaper.AddMediaItemToTrack(reaper.GetTrack(0,0))
local take=reaper.AddTakeToMediaItem(item)
reaper.SetMediaItemTake_Source(take,assert(reaper.PCM_Source_CreateFromFile(root..'source.wav')))
reaper.SetMediaItemInfo_Value(item,'D_POSITION',16001/48000)
reaper.SetMediaItemInfo_Value(item,'D_LENGTH',12001/48000)
reaper.SetMediaItemTakeInfo_Value(take,'D_STARTOFFS',6001/48000)
reaper.SetMediaItemSelected(item,true)
reaper.UpdateArrange()
local log=assert(io.open(root..'item-timecode-settings.tsv','w'))
log:write('# REAPER ',reaper.GetAppVersion(),' sample_rate=48000 owned media\n')
log:write('fps\tdrop\tposition\tlength\tstart_in_source\tin_curve\tin_s\n')
local previous
local function observe()
 local fps,drop=reaper.TimeMap_curFrameRate(0)
 local row=string.format('%.17g\t%s\t%.17g\t%.17g\t%.17g\t%.17g\t%.17g',fps,tostring(drop),reaper.GetMediaItemInfo_Value(item,'D_POSITION'),reaper.GetMediaItemInfo_Value(item,'D_LENGTH'),reaper.GetMediaItemTakeInfo_Value(take,'D_STARTOFFS'),reaper.GetMediaItemInfo_Value(item,'D_FADEINDIR_NEW'),reaper.GetMediaItemInfo_Value(item,'D_FADEINDIR2_NEW'))
 if row~=previous then log:write(row,'\n');log:flush();previous=row end
 reaper.defer(observe)
end
observe()
reaper.Main_OnCommand(40009,0)
