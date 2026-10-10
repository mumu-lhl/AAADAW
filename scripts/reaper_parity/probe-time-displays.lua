-- Numeric formatting facts only, from a fresh owned scratch instance.
local root = assert(os.getenv('AAADAW_REAPER_PROBE_DIR')) .. '/'
assert(reaper.GetAppVersion():match('^7%.82/linux'))
assert(reaper.GetLastColorThemeFile():match('Default_7%.0%.ReaperThemeZip$'))
assert(reaper.CountTracks(0) == 0)
local _, path = reaper.EnumProjects(-1, '')
assert(path == '')
reaper.GetSetProjectInfo(0, 'PROJECT_SRATE', 48000, true)
reaper.GetSetProjectInfo(0, 'PROJECT_SRATE_USE', 1, true)
local fps, drop = reaper.TimeMap_curFrameRate(0)
local log = assert(io.open(root .. 'time-displays.tsv', 'w'))
log:write('# REAPER ', reaper.GetAppVersion(), ' sample_rate=48000 fps=', tostring(fps), ' drop=', tostring(drop), '\n')
log:write('case\tbpm\tnumerator\tdenominator\tstart_seconds\tlength_seconds\tmode\tposition\tlength\n')
local cases = {
  {'default-exact-millisecond', 120, 4, 4, 1.001, 0.001},
  {'default-below-millisecond-1ns', 120, 4, 4, 0.000999999, 0.000999999},
  {'default-below-millisecond-10ns', 120, 4, 4, 0.00099999, 0.00099999},
  {'default-below-millisecond-11ns', 120, 4, 4, 0.000999989, 0.000999989},
  {'default-below-millisecond-25ns', 120, 4, 4, 0.000999975, 0.000999975},
  {'default-below-millisecond-50ns', 120, 4, 4, 0.00099995, 0.00099995},
  {'default-below-millisecond-75ns', 120, 4, 4, 0.000999925, 0.000999925},
  {'default-below-millisecond-99ns', 120, 4, 4, 0.000999901, 0.000999901},
  {'default-below-millisecond-100ns-less-1ps', 120, 4, 4, 0.000999900001, 0.000999900001},
  {'default-below-millisecond-100ns-plus-1ps', 120, 4, 4, 0.000999899999, 0.000999899999},
  {'default-below-millisecond-100ns', 120, 4, 4, 0.0009999, 0.0009999},
  {'default-below-millisecond-1us', 120, 4, 4, 0.000999, 0.000999},
  {'default-below-second-ms-1ns', 120, 4, 4, 1.000999999, 0.000999999},
  {'default-manual-fade-time', 120, 4, 4, 28846.1549042245/48000, 7692.307974459873/48000},
  {'default-hour-boundary', 120, 4, 4, 3600.001, 1},
  {'default-before-hour', 120, 4, 4, 3599.99975, 3600.001},
  {'default-day', 120, 4, 4, 86400.125, 86400.125},
  {'default-half-second', 120, 4, 4, 0.5, 0.25},
  {'default-16001-samples', 120, 4, 4, 16001/48000, 12000/48000},
  {'default-one-sample', 120, 4, 4, 1/48000, 1/48000},
  {'default-half-sample', 120, 4, 4, 0.5/48000, 0.5/48000},
  {'default-minute-boundary', 120, 4, 4, 59.99975, 0.00025},
  {'default-bar-crossing', 120, 4, 4, 1.5, 1},
  {'six-eight-quarter-second', 120, 6, 8, 0.25, 0.25},
  {'six-eight-bar-crossing', 120, 6, 8, 1.25, 0.75},
  {'three-four-bar-crossing', 120, 3, 4, 1.25, 0.75},
  {'sixty-bpm', 60, 4, 4, 0.5, 0.25},
}
for _, case in ipairs(cases) do
  local label, bpm, numerator, denominator, start, length = table.unpack(case)
  local marker = reaper.CountTempoTimeSigMarkers(0) == 0 and -1 or 0
  assert(reaper.SetTempoTimeSigMarker(0, marker, 0, -1, -1, bpm, numerator, denominator, false))
  for _, mode in ipairs({0, 2, 4, 5}) do
    local position = reaper.format_timestr_pos(start, '', mode)
    local duration = reaper.format_timestr_len(length, '', start, mode)
    log:write(string.format('%s\t%.17g\t%d\t%d\t%.17g\t%.17g\t%d\t%s\t%s\n', label, bpm, numerator, denominator, start, length, mode, position, duration))
  end
end
log:close()
