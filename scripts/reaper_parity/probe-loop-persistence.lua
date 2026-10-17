-- Run only in an isolated REAPER 7.82 config and disposable project.
local root=assert(os.getenv("AAADAW_REAPER_PROBE_DIR"), "Set AAADAW_REAPER_PROBE_DIR").."/"
assert(reaper.GetAppVersion()=="7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
local _,active_project_path=reaper.EnumProjects(-1,"")
local reopened_case
if active_project_path:match("loop%-time%-selection%-unlinked%-distinct%.rpp$") then reopened_case="unlinked-distinct" end
if active_project_path:match("loop%-time%-selection%-linked%-shared%.rpp$") then reopened_case="linked-shared" end
if reopened_case then
 local log=assert(io.open(root.."loop-time-selection-persistence.tsv","a"))
 local a,b=reaper.GetSet_LoopTimeRange2(0,false,false,0,0,false)
 local c,d=reaper.GetSet_LoopTimeRange2(0,false,true,0,0,false)
 log:write(reopened_case,"\tafter-reopen\t",string.format("%.9f\t%.9f\t%.9f\t%.9f\t%s\t%s\n",a,b,c,d,reaper.GetToggleCommandState(40621)==1 and "linked" or "unlinked",active_project_path))
 log:close()
 return
end
local original_linked=reaper.GetToggleCommandState(40621)==1
local function linked() return reaper.GetToggleCommandState(40621)==1 end
local function set_link(enabled)
 if linked()~=enabled then reaper.Main_OnCommand(40621,0) end
end
local function range(is_loop)
 local a,b=reaper.GetSet_LoopTimeRange2(0,false,is_loop,0,0,false)
 return a,b
end
local function set_range(is_loop,a,b)
 reaper.GetSet_LoopTimeRange2(0,true,is_loop,a,b,false)
end
local function format_state()
 local a,b=range(false); local c,d=range(true)
 return string.format("%.9f\t%.9f\t%.9f\t%.9f\t%s",a,b,c,d,linked() and "linked" or "unlinked")
end
local log=assert(io.open(root.."loop-time-selection-persistence.tsv","w"))
log:write("case\tphase\tselection_start\tselection_end\tloop_start\tloop_end\tlink\tproject_path\n")
local cases={
 {"unlinked-distinct",false,1.25,2.5,3.5,5.75},
 {"linked-shared",true,1.25,2.5,0,0},
}
for _,c in ipairs(cases) do
 set_link(false)
 set_range(false,c[3],c[4]); set_range(true,c[5],c[6])
 if c[2] then set_link(true) end
 local path=root.."loop-time-selection-"..c[1]..".rpp"
 local _,current_path=reaper.EnumProjects(-1,"")
 log:write(c[1],"\tbefore-save\t",format_state(),"\t",current_path,"\n"); log:flush()
 reaper.Main_SaveProjectEx(0,path,0)
 local saved=assert(io.open(path,"r"),"could not save "..path); saved:close()
 local _,saved_path=reaper.EnumProjects(-1,"")
 log:write(c[1],"\tafter-save\t",format_state(),"\t",saved_path,"\n"); log:flush()
end
set_link(original_linked)
log:close()
