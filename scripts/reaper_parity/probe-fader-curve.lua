-- Read-only reference data; run in Linux REAPER 7.82 with factory settings.
local root=assert(os.getenv("AAADAW_REAPER_PROBE_DIR")).."/"
assert(reaper.GetAppVersion():match("^7%.82/linux"), "Expected Linux REAPER 7.82")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"), "Expected Default 7 theme")
assert(reaper.APIExists("SLIDER2DB") and reaper.APIExists("DB2SLIDER"))
local log=assert(io.open(root.."fader-curve-reference.csv","w"))
log:write("slider,db\n")
for index=0,10000 do
 local position=index/10
 log:write(string.format("%.1f,%.15g\n",position,reaper.SLIDER2DB(position)))
end
log:close()
local forward=assert(io.open(root.."fader-forward-reference.csv","w"))
forward:write("db,slider\n")
for index=0,10120 do
 local db=-1000+index/10
 forward:write(string.format("%.1f,%.15g\n",db,reaper.DB2SLIDER(db)))
end
forward:close()
local manifest=assert(io.open(root.."fader-curve-manifest.txt","w"))
manifest:write("Version="..reaper.GetAppVersion().."\nTheme="..reaper.GetLastColorThemeFile().."\n")
for _,db in ipairs({-1000,-150,-72,-60,-48,-36,-24,-18,-12,-6,0,6,12}) do
 manifest:write(string.format("DB2SLIDER(%g)=%.15g\n",db,reaper.DB2SLIDER(db)))
end
manifest:close()
