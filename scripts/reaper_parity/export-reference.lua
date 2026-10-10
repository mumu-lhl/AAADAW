-- Run with REAPER 7.82 in a fresh, isolated resource directory.
-- AAADAW_REAPER_REFERENCE_OUTPUT must name an existing output directory.
-- This script reads the registered actions and bindings; it edits no project,
-- shortcut, theme, or preference and does not request application shutdown.
local output = assert(os.getenv("AAADAW_REAPER_REFERENCE_OUTPUT"),
  "Set AAADAW_REAPER_REFERENCE_OUTPUT to an existing output directory")
local version = reaper.GetAppVersion()
assert(version == "7.82/linux-x86_64", "Expected 7.82/linux-x86_64, got " .. version)

local function tsv(value)
  return tostring(value):gsub("[\t\r\n]", " ")
end

local actions = assert(io.open(output .. "/actions.tsv", "w"))
actions:write("section_id\taction_id\tname\tshortcuts\n")
local summary = assert(io.open(output .. "/sections.tsv", "w"))
summary:write("section_id\tactions\tbindings\n")
-- Main, Main (alt recording), MIDI editor, MIDI event list,
-- MIDI inline editor, and Media Explorer. Missing sections are reported.
for _, id in ipairs({0, 100, 32060, 32061, 32062, 32063}) do
  local section = reaper.SectionFromUniqueID(id)
  local count, binding_count = 0, 0
  if section then
    while true do
      local cmd, name = reaper.kbd_enumerateActions(section, count)
      if cmd == 0 then break end
      local shortcuts = {}
      for index = 0, reaper.CountActionShortcuts(section, cmd) - 1 do
        local ok, desc = reaper.GetActionShortcutDesc(section, cmd, index)
        assert(ok, "Could not read shortcut for " .. cmd)
        shortcuts[#shortcuts + 1] = tsv(desc)
        binding_count = binding_count + 1
      end
      actions:write(id, "\t", cmd, "\t", tsv(name), "\t",
        table.concat(shortcuts, " | "), "\n")
      count = count + 1
    end
  end
  summary:write(id, "\t", count, "\t", binding_count, "\n")
end
actions:close()
summary:close()

local manifest = assert(io.open(output .. "/runtime.txt", "w"))
manifest:write("version=", version, "\n",
  "os=", reaper.GetOS(), "\n",
  "resource_path=", reaper.GetResourcePath(), "\n",
  "theme=", reaper.GetLastColorThemeFile(), "\n",
  "scope=action sections and shortcut descriptions only\n")
manifest:close()
