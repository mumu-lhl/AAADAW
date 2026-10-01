# REAPER-Inspired Desktop UI and Module Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Leave `main.rs` as a small launcher, split the Iced shell by responsibility, and make Arrangement the focused default workspace with REAPER-inspired top menus and persistent transport.

**Architecture:** `app/` owns application state and message routing; nested view modules compose the shell and the Arrangement, Media, and Project workspaces. Project I/O and media/path-dialog orchestration live in focused child modules and continue to use background workers; views only emit messages and all domain edits continue through `DawAction`.

**Tech Stack:** Rust, Iced 0.14, `rfd` 0.15.4, `aaadaw-core`, `aaadaw-storage`, nextest.

**Spec:** `docs/superpowers/specs/2026-10-01-reaper-inspired-desktop-ui-design.md`

## Global Constraints

- Keep all project state changes behind `aaadaw-core::Project` and `DawAction`.
- Keep blocking project, media, hash, and file-dialog work off the Iced update thread and audio callback.
- Do not add wgpu timeline, waveform, docking, mixer, or unsupported REAPER features in this refactor.
- Keep Iced at 0.14 and preserve optional Linux JACK behavior.
- Use `cargo xtest` for workspace tests.

## Review Focus

- A cancelled native picker must preserve the prior path and clear picker-busy state (Task 3).
- A picker/worker error must clear busy state and surface a useful status without mutating the project (Task 3).
- Workspace changes during import/asset maintenance must not lose progress or disable cancellation (Task 2 and Task 3).
- Opening a project must still refuse to discard dirty state; Save As must use the dialog-selected path even when a project is already open, and unrelated existing files must not be silently overwritten (Task 4).
- JACK playback must still block project edits while permitting view navigation, and non-JACK builds must compile without playback-only symbols (Task 2 and Task 5).

---

### Task 1: Make `main.rs` a launcher and establish the app module

**Files:**
- Modify: `crates/aaadaw/src/main.rs`
- Create: `crates/aaadaw/src/app/mod.rs`
- Create: `crates/aaadaw/src/app/messages.rs`
- Create: `crates/aaadaw/src/app/tests.rs`
- Modify: `crates/aaadaw/src/timeline.rs`

**Interfaces:**
- Produces: `pub(crate) fn app::run() -> iced::Result`, `pub(crate) enum Message`, and the internal `App` state/update loop.
- `timeline.rs` consumes `crate::app::Message`; keep message fields internal to the crate.

- [ ] **Step 1: Add and run the startup behavior test**

Add `default_workspace_is_arrangement`, asserting a default `App` selects `WorkspacePage::Arrangement`. Keep `top_menus_toggle_and_workspace_navigation_stays_available_during_jobs` and `keyboard_shortcuts_route_to_existing_app_messages` as behavior contracts.

Run: `cargo xtest -p aaadaw`
Expected: the startup/navigation and existing app tests pass before extraction.

- [ ] **Step 2: Add the app-module contract test and verify red**

Add `app_entrypoint_has_iced_result_signature`, assigning `crate::app::run` to `fn() -> iced::Result` without invoking it.

Run: `cargo xtest -p aaadaw app_entrypoint_has_iced_result_signature`
Expected: compile failure because `crate::app` does not exist yet. This is the intended red signal for the module-boundary extraction.

- [ ] **Step 3: Extract app ownership**

Move `App`, startup/subscription/update routing, shared worker wrappers, and workspace/menu/path-picker message types into `app/`. Move the current inline unit tests to `app/tests.rs`. Keep `main.rs` to module declarations and `app::run()`. Re-export `Message` only as `pub(crate)` for the existing root `timeline` module.

- [ ] **Step 5: Verify the extraction**

Run: `cargo xtest -p aaadaw`
Expected: the same app behavior passes with `main.rs` as a launcher and no visibility widening beyond the crate.

- [ ] **Step 6: Commit**

```bash
git add crates/aaadaw/src/main.rs crates/aaadaw/src/app crates/aaadaw/src/timeline.rs
git commit -m "refactor: move desktop app into focused modules"
```

### Task 2: Extract view composition and establish the REAPER-like shell

**Files:**
- Create: `crates/aaadaw/src/app/view/mod.rs`
- Create: `crates/aaadaw/src/app/view/arrangement.rs`
- Create: `crates/aaadaw/src/app/view/media.rs`
- Create: `crates/aaadaw/src/app/view/project.rs`
- Modify: `crates/aaadaw/src/app/mod.rs`
- Modify: `crates/aaadaw/src/timeline.rs`

**Interfaces:**
- Produces: `pub(super) fn view(app: &App) -> Element<'_, Message>` as the app's view entry point.
- `arrangement.rs`, `media.rs`, and `project.rs` each expose one parent-visible view builder; builders read state and emit `Message`s only.
- The existing timeline item editor remains in `timeline.rs` and is composed by `arrangement.rs`.

- [ ] **Step 1: Add/retain workspace behavior tests**

Assert that the default workspace is Arrangement, selecting Media/Project updates the active workspace, and selecting a workspace remains accepted during a background media job.

- [ ] **Step 2: Run workspace tests before moving view code**

Run: `cargo xtest -p aaadaw`
Expected: workspace selection tests and existing editor/action tests pass.

- [ ] **Step 3: Move screen rendering out of app state**

Move the current `App::view`, `playback_controls`, and track-row rendering into the view modules. Keep File/Edit/Track menus in the shell, put Arrangement/Media/Project navigation below them, and make Arrangement the initial workspace. Keep track controls on the left and the current timeline editor central. Put transport controls in a persistent bottom strip; show only a compact JACK hint when the feature is disabled. Place import, source scan, pack, relink, and source status only in Media; put action search and shortcut help in Project.

- [ ] **Step 4: Verify view extraction and both feature configurations**

Run: `cargo xtest -p aaadaw` and `cargo xtest -p aaadaw --features jack-backend`
Expected: both pass; no changes to project actions or audio behavior.

- [ ] **Step 5: Commit**

```bash
git add crates/aaadaw/src/app crates/aaadaw/src/timeline.rs
git commit -m "refactor: separate desktop workspace views"
```

### Task 3: Isolate media workers and native path-dialog coordination

**Files:**
- Create: `crates/aaadaw/src/app/media.rs`
- Modify: `crates/aaadaw/src/app/mod.rs`
- Modify: `crates/aaadaw/src/app/messages.rs`
- Modify: `crates/aaadaw/src/app/tests.rs`
- Modify: `crates/aaadaw/src/app/view/media.rs`

**Interfaces:**
- `app::media` defines parent-visible `App` methods `pick_path(&mut self, target: PathPickerTarget) -> Task<Message>`, `path_picked(&mut self, target: PathPickerTarget, result: Result<Option<PathBuf>, String>) -> Task<Message>`, `start_audio_import(&mut self) -> Task<Message>`, and `start_audio_asset_management(&mut self, operation: AudioAssetManagementOperation) -> Task<Message>`; existing progress, finish, cancel, and relink handlers move into the same `impl App`.
- `App::update` remains the sole message router; worker results return as typed `Message`s.

- [ ] **Step 1: Add picker cancellation and failure tests**

Add `cancelled_picker_preserves_path_and_clears_busy_state` and `failed_picker_reports_error_and_clears_busy_state`. Exercise the path-result handler directly with `Ok(None)` and `Err(...)`; assert the previous query remains unchanged on cancel and the project revision is unchanged on either result.

- [ ] **Step 2: Run the focused app tests**

Run: `cargo xtest -p aaadaw`
Expected: the two picker-result tests pass against the current behavior.

- [ ] **Step 3: Extract media orchestration**

Move path picking/result handling, audio import lifecycle, source scan/pack worker lifecycle, relink orchestration, and media status updates into `app/media.rs`. Leave UI-local fields and top-level dispatch in `App`. Keep `rfd` calls inside the existing blocking worker boundary; route selected file paths to project open/save, audio import, or relink by `PathPickerTarget`.

- [ ] **Step 4: Verify media workflows**

Run: `cargo xtest -p aaadaw`
Expected: app worker integration tests, picker result tests, cancellation/progress tests, and existing storage/media tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/aaadaw/src/app
git commit -m "refactor: isolate desktop media workflows"
```

### Task 4: Isolate project open/save I/O

**Files:**
- Create: `crates/aaadaw/src/app/project_io.rs`
- Modify: `crates/aaadaw/src/app/mod.rs`
- Modify: `crates/aaadaw/src/app/messages.rs`
- Modify: `crates/aaadaw/src/app/tests.rs`

**Interfaces:**
- Produces `pub(super) fn open_project(app: &mut App) -> Task<Message>`, `save_project(app: &mut App, save_as: Option<PathBuf>) -> Task<Message>`, `load_project_file(path: PathBuf) -> Result<Project, String>`, `save_project_file(path: PathBuf, snapshot: ProjectSnapshot, can_overwrite: bool) -> Result<(), String>`, and `resolve_save_target(current: Option<&Path>, query: &str, save_as: Option<PathBuf>) -> Option<(PathBuf, bool)>`.

- [ ] **Step 1: Add failing save-target precedence tests**

Add `save_as_target_overrides_the_current_project_path` and `save_as_allows_overwrite_only_for_the_current_file`. Assert that an explicit dialog path takes precedence over both the current path and typed path, that saving to the current path yields `can_overwrite == true`, and that a different selected path yields `false`.

- [ ] **Step 2: Verify the save-target test exposes the current bug**

Run: `cargo xtest -p aaadaw save_as_target_overrides_the_current_project_path`
Expected: FAIL because `resolve_save_target` is not implemented yet; do not change the test to match the current incorrect Save As behavior.

- [ ] **Step 3: Implement target resolution, then move project I/O orchestration**

Implement `resolve_save_target(current, query, save_as)` so an explicit Save As selection wins, otherwise the open project's path wins, otherwise the typed path is used. Set `can_overwrite` only when the destination equals the current project path. Move open/save task creation and blocking storage helpers into `app/project_io.rs`; keep `ProjectLoaded`/`ProjectSaved` handling in the app router because it updates UI state. Preserve save-revision tracking, refusal to overwrite unrelated files, and the dirty-project guard.

- [ ] **Step 4: Verify project I/O behavior**

Run: `cargo xtest -p aaadaw`
Expected: both save-target tests, project open/save round-trip, dirty-project guards, and overwrite guards pass; file-dialog-selected paths use the selected Save As destination.

- [ ] **Step 5: Commit**

```bash
git add crates/aaadaw/src/app
git commit -m "refactor: isolate project file operations"
```

### Task 5: Documentation, manual smoke test, and full verification

**Files:**
- Modify: `README.md`
- Modify: `ROADMAP.md` only if implementation scope or completion state changes.

- [ ] **Step 1: Update the desktop-shell documentation**

Document the top menus, default Arrangement workspace, separate Media/Project workspaces, bottom transport, and native project/audio file pickers. This plan captured the initial shell, when the timeline was list-based; the later spatial viewport is tracked in `docs/tickets/2026-10-01-desktop-arrangement.md`.

- [ ] **Step 2: Run the app for a visual smoke test when a desktop session is available**

Run: `cargo run -p aaadaw`
Check: menus open/close; Arrangement is the initial screen; track list and item controls are usable; Media holds import/scan/pack/relink controls; Open/Save/Import/Relink pickers open and cancellation leaves the current path intact; status/progress remain visible. With JACK available, confirm transport is persistent and project edits remain guarded during playback.

- [ ] **Step 3: Run full verification**

Run:

```bash
cargo xtest
cargo xtest --features aaadaw/jack-backend
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features aaadaw/jack-backend -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Expected: both test configurations pass, Clippy is clean, formatting and diff checks pass.

- [ ] **Step 4: Commit the final documentation/cleanup**

```bash
git add README.md ROADMAP.md
git commit -m "docs: describe desktop workspace layout"
```
