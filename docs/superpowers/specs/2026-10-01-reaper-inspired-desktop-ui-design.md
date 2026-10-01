# REAPER-Inspired Desktop UI and Module Split

**Status:** Design approved in conversation; awaiting written-spec review  
**Scope:** Desktop shell information architecture and source-module boundaries. This does not attempt a full REAPER feature or visual clone.

## Context

The desktop shell has accumulated application state, message dispatch, project I/O, media-worker coordination, file dialogs, and all screen rendering in `crates/aaadaw/src/main.rs`. This makes the file difficult to navigate and puts project/media controls, item editing, and utility actions into one crowded page.

The repository architecture calls for a REAPER-inspired Iced shell with a top menu, track controls on the left, an arrangement viewport in the center, and a transport bar. Domain mutations must continue to go through `aaadaw-core`'s `Project` interface and `DawAction`; file access, decoding, and other blocking work must stay off the UI/audio callback paths.

## Goals

- Make Arrangement the default workspace, with track controls at left and the timeline/editor as the primary central area.
- Keep global commands in a compact top menu and transport controls in a persistent transport area.
- Move audio import, source scanning, packing, and relinking out of the arrangement view into a dedicated Media workspace.
- Keep project commands/action search in a Project workspace; use a native chooser appropriate to each path field. Current project/media fields are files; add a folder chooser only for future directory-valued workflows.
- Split rendering and background-work coordination into modules with clear responsibilities, leaving `main.rs` as a small application entry point.
- Preserve current project actions, undo/redo, progress/cancellation, save-before-maintenance rules, and playback edit guards.

## Non-goals

- Implementing the planned wgpu timeline canvas, waveforms, dockers, mixer, or a complete REAPER toolbar in this refactor.
- Changing the project/domain model, SQLite format, audio callback, or media worker semantics.
- Replacing Iced or introducing a new UI framework.
- Reproducing REAPER pixel-for-pixel. The target is its information hierarchy and workflow, within the current feature set.

## User-facing layout

1. **Top menu:** File (open/save/new path), Edit (undo/redo), Track (track creation and related commands), and View/workspace navigation. Menus remain compact when closed; their command panels appear only when opened.
2. **Persistent transport:** playback/stop/restart, seek, and playhead status remain available without occupying the editing canvas. When JACK is not enabled, show a compact backend hint rather than a row of inert controls.
3. **Arrangement workspace (default):** left track-control panel and central timeline/item editor. Track and item operations stay near their objects; project-wide utility controls are not placed here.
4. **Media workspace:** import, scan, pack, source-status display, and missing-link repair. Native file dialogs supply current project/media file paths; path values remain editable where useful. A folder chooser is added when a directory-valued workflow exists. Blocking dialog and media/storage work stays off the Iced update thread.
5. **Project workspace:** action search, project information, and shortcut help.
6. **Status area:** one compact status line reports the latest operation and save state.

The initial shell kept the previous list-based editor. The wgpu timeline viewport has since replaced it in the Arrangement; see `ROADMAP.md` and `docs/tickets/2026-10-01-desktop-arrangement.md` for the delivered viewport scope and remaining direct-edit work.

## Module boundaries

The implementation will preserve one-way UI-to-application flow and group code by responsibility:

```text
crates/aaadaw/src/
  main.rs                 application bootstrap only
  app/
    mod.rs                App state, subscriptions, and top-level message routing
    messages.rs           UI message and workspace/menu types
    project_io.rs         open/save orchestration and blocking storage helpers
    media.rs              import, scan, pack, relink, and path-picker orchestration
    view/
      mod.rs              shell, top menu, transport, workspace selection
      arrangement.rs      track panel + arrangement workspace composition
      media.rs            media workspace composition
      project.rs          project/action-search workspace composition
  timeline.rs             existing timeline item controls, consumed by arrangement.rs
```

- Views receive read-only project/UI state and emit `Message`s; they do not mutate project or storage state.
- `App` owns UI state transitions and routes messages to focused project-I/O/media/action handlers.
- `Project` changes continue through `DawAction`; storage-only maintenance remains behind `ProjectStore` APIs.
- `rfd` dialogs return path selections as messages. Long-running dialog calls, project I/O, hashing, decoding, and asset work remain on background workers.
- Module visibility stays narrow (`pub(super)` where needed); this is an internal desktop-shell refactor, not a new public workspace API.

## Migration and verification

The change will be performed as behavior-preserving extractions before visual refinements: first move bootstrap/state and message routing, then screen composition, then media and project-I/O orchestration. Each step should keep the application compiling and the existing interactions intact.

Verification uses the repository's preferred commands:

- `cargo xtest`
- `cargo xtest --features aaadaw/jack-backend`
- Clippy for default and JACK configurations with `-D warnings`
- `cargo fmt --all -- --check`
- `git diff --check`

Tests will cover message routing/workspace switching and existing media/project workflows. Native OS dialogs cannot be driven by headless tests; their result handling will be unit-tested, and the dialogs themselves will be manually smoke-tested when a desktop session is available.

## Risks and constraints

- Splitting `App` across Rust modules can accidentally widen visibility. Keep view and worker APIs narrow and avoid making fields public merely to simplify extraction.
- Layout changes must preserve access to background progress and cancellation while allowing workspace navigation.
- System dialog behavior varies by desktop/platform; cancellation must leave the current path unchanged, and failures must report status without blocking the UI.
- REAPER's full interface is much broader than current AAADAW functionality. This slice establishes its core information hierarchy without implying that unsupported editing features already exist.
