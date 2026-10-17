-- Run only against a fresh owned probe project; all output belongs in scratch.
-- Numeric/text observations only; never commit exported native project files or assets.
local root=assert(os.getenv('AAADAW_REAPER_PROBE_DIR'))..'/'
assert(reaper.GetAppVersion():match('^7%.82/linux'))
assert(reaper.GetLastColorThemeFile():match('Default_7%.0%.ReaperThemeZip$'))
assert(reaper.CountTracks(0)==0)
local _,path=reaper.EnumProjects(-1,'');assert(path=='')
local log=assert(io.open(root..'notes-events.txt','w'))
local last
local function observe()
 local notes=reaper.GetSetProjectNotes(0,false,'')
 local _,title=reaper.GetSetProjectInfo_String(0,'PROJECT_TITLE','',false)
 local _,author=reaper.GetSetProjectInfo_String(0,'PROJECT_AUTHOR','',false)
 local current=notes.."\0"..title.."\0"..author
 if current~=last then
  log:write(string.format('%q',notes),'\t',string.format('%q',title),'\t',string.format('%q',author),'\n');log:flush()
  reaper.Main_SaveProjectEx(0,root..'observed.rpp',0)
  last=current
 end
 reaper.defer(observe)
end
observe()
reaper.Main_OnCommand(40021,0)
