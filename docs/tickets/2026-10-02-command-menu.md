# Global Command Menu

## M1 — Rebuild the DAW menu bar

- **Goal:** Replace the current expanding File/Edit/Track panels with a compact, discoverable DAW menu bar modeled on REAPER's command organization.
- **Scope:** File, Edit, View, Insert, Item, Track, and Actions dropdowns; group existing commands by domain, show registered shortcuts, route through existing app messages and `DawAction`, and reflect selection/busy state. Use anchored popovers that do not move the Arrangement; close after commands, outside clicks, or Escape.
- **Out of scope:** Unsupported commands, configurable shortcut maps/macros, a full Action registry, and redesigning the Transport or workspace contents.
- **Dependencies:** A2 complete; existing Project/File, Edit, track context, media import, item edit, and workspace actions.
- **Acceptance:** Every menu item invokes a real supported operation; no placeholder commands appear. View switches preserve Transport and project state. Item commands target the current selection and remain undoable. Open/Save/Import keep their existing busy and dirty-project protections. Menu shortcuts match actual keyboard behavior.
- **Verification:** App message tests, command/selection tests, `cargo xtest -p aaadaw`, formatting and diff checks.
- **UI acceptance:** In the running app, inspect open/closed menus at 1280×800 and 900×620. Popovers align below their menu labels, remain readable, do not shift the timeline, and preserve the compact desktop DAW hierarchy in `DESIGN.md`.
- **Status:** Complete.
