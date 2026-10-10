-- Observe frame/timecode settings on an owned, initially empty scratch project.
local root = assert(os.getenv('AAADAW_REAPER_PROBE_DIR')) .. '/'
assert(reaper.GetAppVersion():match('^7%.82/linux'))
assert(reaper.GetLastColorThemeFile():match('Default_7%.0%.ReaperThemeZip$'))
assert(reaper.CountTracks(0) == 0)
local _, path = reaper.EnumProjects(-1, '')
assert(path == '')
local log = assert(io.open(root .. 'frame-displays.tsv', 'w'))
log:write('# REAPER ',reaper.GetAppVersion(),' owned empty scratch project\n')
log:write('fps\tdrop\tseconds\tposition\tlength\tparsed_position\tparsed_length\n')
local previous
local function observe()
  local fps, drop = reaper.TimeMap_curFrameRate(0)
  local key = tostring(fps)..'/'..tostring(drop)
  if key ~= previous then
    for _, seconds in ipairs({0,1/48000,0.25,0.5,1,1.001,59.99975,60,600,3600.001,86400.125}) do
      local pos = reaper.format_timestr_pos(seconds,'',5)
      local len = reaper.format_timestr_len(seconds,'',0,5)
      log:write(string.format('%.17g\t%s\t%.17g\t%s\t%s\t%.17g\t%.17g\n',fps,tostring(drop),seconds,pos,len,reaper.parse_timestr_pos(pos,5),reaper.parse_timestr_len(len,0,5)))
    end
    log:flush()
    reaper.Main_SaveProjectEx(0,root..'owned-frame-settings.rpp',0)
    previous = key
  end
  reaper.defer(observe)
end
observe()
