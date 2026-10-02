# Arrangement Time Selection

## A3.1 — Create and edit time selections

- **Goal:** Make a distinct musical time range available in the Arrangement for later edit commands.
- **Scope:** Keep range state separate from item/track selection and edit cursor; drag blank Arrangement space to create a snapped range, drag either boundary to adjust it, show its fill and boundaries, and clear it with Escape. Honor the existing 1/16 snap toggle and Shift bypass.
- **Out of scope:** Loop selection, persistence, and exact numeric range editing.
- **Dependencies:** A2 cross-track item drag; command-menu refactor.
- **Acceptance:** Empty-space click still positions the edit cursor. Forward and reverse drags create the same normalized range, with non-negative endpoints. Time selection never changes item or track selection. Escape clears the range while preserving existing menu-dismiss behavior.
- **Verification:** Timeline state, snapping, edge-hit, and drag-endpoint tests; Escape/menu-priority and project-switch tests; `cargo xtest -p aaadaw`.
- **UI acceptance:** Range fill and handles are visually distinct from selected Items and edit/play cursors. Inspect the running app at 1280×800 and 900×620; keep the ruler, TCP, and Transport usable.
- **Status:** Complete.

## A3.2 — Split selected items at cursor or time selection

- **Goal:** Split selected Audio and MIDI Items at the edit cursor or time-selection boundaries as one undoable edit.
- **Scope:** Add executable Item-menu and Actions commands; retain every resulting segment and its source/note content; make boundary/no-op behavior explicit and keep action availability selection-aware.
- **Out of scope:** Removing the time-selected middle segment, loop points, and item slip/stretch editing.
- **Dependencies:** A3.1; existing `DawAction` batch transactions and item selection.
- **Acceptance:** Cursor split creates two segments where the cursor is inside an Item. Time-selection split creates segment boundaries at both range edges, without changing material outside the selected Items. Audio segments preserve source offsets; MIDI notes remain represented in the correct segments. Each invocation is one atomic undo/redo step; invalid or empty splits leave state/history unchanged.
- **Verification:** Core/app tests for Audio, MIDI, mixed selections, boundary no-ops, and undo/redo; `cargo xtest -p aaadaw`.
- **UI acceptance:** Commands stay disabled when no selected Item can be split; command names and availability match between Item menu and Actions search.
- **Status:** Ready.
