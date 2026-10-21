-- Enumerate the REAPER 7.82 Main-section take and paste actions.
-- Set AAADAW_REAPER_PROBE_DIR to an existing output directory.
assert(reaper.GetAppVersion() == "7.82/linux-x86_64")
assert(reaper.GetLastColorThemeFile():match("Default_7%.0%.ReaperThemeZip$"))
local output_dir = assert(os.getenv("AAADAW_REAPER_PROBE_DIR"))
local output = assert(io.open(output_dir .. "/audio-take-action-catalog.tsv", "w"))
output:write("reference\t", reaper.GetAppVersion(), "\t", reaper.GetLastColorThemeFile(), "\n")
output:write("section\tcommand_id\taction_name\n")
local index = 0
while index < 100000 do
  local command_id, action_name = reaper.kbd_enumerateActions(0, index)
  if not command_id or command_id == 0 then
    break
  end
  local lower_name = string.lower(action_name or "")
  if lower_name:find("take", 1, true)
      or lower_name:find("paste", 1, true)
      or lower_name:find("copy items", 1, true)
      or lower_name:find("source", 1, true) then
    output:write("Main\t", tostring(command_id), "\t", action_name or "", "\n")
  end
  index = index + 1
end
output:close()
local marker = assert(io.open(output_dir .. "/action-catalog-complete", "w"))
marker:write("complete\n")
marker:close()
reaper.defer(function() end)
