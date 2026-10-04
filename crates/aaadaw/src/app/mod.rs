use crate::timeline::{self, TimelineState};
use aaadaw_app::{
    AudioAssetManagementOperation, AudioAssetManagementWorker, AudioAssetSourceStatusEntry,
    AudioItemImportWorker, AudioWaveformResult, AudioWaveformWorker, ClapPluginScanReport,
    add_quarter_note, adjust_midi_note_pitch, adjust_midi_note_velocity,
    create_four_beat_midi_item, default_clap_search_paths, delete_midi_note, duplicate_audio_item,
    move_midi_item_by_beat, move_midi_note_by_sixteenth, quantize_midi_item_to_sixteenth,
    set_audio_item_start_sample,
};
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
use aaadaw_app::{AudioCaptureControl, AudioRecordingWorker, RunningAudioInput};
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
use aaadaw_app::{
    PlaybackBackend, PlaybackBuildError, PreparedAudioPlayback, RunningAudioPlayback,
    prepare_audio_playback_at,
};
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
use aaadaw_core::ProjectSnapshot;
use aaadaw_core::{AudioItem, DawAction, FxParameterChange, ItemId, MidiItem, Project, TrackId};
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
use aaadaw_engine::{ClapEffectOwner, ClapInstrumentOwner, ClapParameterSender};
use aaadaw_engine::{ClapParameterInfo, ClapPluginGuiOwner};
use aaadaw_media::AudioWaveform;
use aaadaw_storage::ProjectStore;
use iced::Task;
use iced::widget::pane_grid::{self, Axis, Split};
use std::collections::{HashMap, HashSet};
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

mod clap_plugin_cache;
mod clap_plugin_config;
mod clap_plugin_settings;
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
mod clap_plugin_state;
mod clap_track_fx;
mod clap_track_instrument;
mod commands;
mod config_paths;
mod keyboard_config;
mod media;
mod messages;
mod project_io;
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
mod recording;
mod recording_recovery;
#[cfg(test)]
mod tests;
mod view;
mod x11_plugin_editor;

pub(crate) use messages::{MainMenu, Message, PathPickerTarget, SettingsCategory};

pub(crate) fn run() -> iced::Result {
    iced::daemon(App::new, App::update, view::view_for_window)
        .title(App::window_title)
        .theme(|_: &App, _| iced::Theme::Dark)
        .subscription(App::subscription)
        .run()
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
#[derive(Default)]
struct ClapEffectOwners(HashMap<u64, ClapEffectOwner>);

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
#[derive(Default)]
struct ClapInstrumentOwners(HashMap<u64, ClapInstrumentOwner>);

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
impl Deref for ClapEffectOwners {
    type Target = HashMap<u64, ClapEffectOwner>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
impl DerefMut for ClapEffectOwners {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
impl Drop for ClapEffectOwners {
    fn drop(&mut self) {
        for owner in self.0.values_mut() {
            let _ = owner.try_deactivate_unused();
        }
    }
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
impl Deref for ClapInstrumentOwners {
    type Target = HashMap<u64, ClapInstrumentOwner>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
impl DerefMut for ClapInstrumentOwners {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
impl Drop for ClapInstrumentOwners {
    fn drop(&mut self) {
        for owner in self.0.values_mut() {
            let _ = owner.try_deactivate_unused();
        }
    }
}

#[derive(Default)]
struct MidiNoteClipboard {
    notes: Vec<aaadaw_core::MidiNoteData>,
    source_item_id: Option<ItemId>,
    span_ticks: u64,
    last_paste: Option<(ItemId, u64)>,
}

#[derive(Default)]
struct App {
    project: Project,
    action_query: String,
    shortcut_bindings: Arc<std::sync::RwLock<commands::ShortcutBindings>>,
    shortcut_binding_edits: commands::ShortcutBindings,
    shortcut_defaults_restored: HashSet<String>,
    media_panel_dock: MediaPanelDock,
    project_path_query: String,
    project_path: Option<PathBuf>,
    track_name_edits: HashMap<TrackId, String>,
    track_volume_edits: HashMap<TrackId, String>,
    track_pan_edits: HashMap<TrackId, String>,
    track_mix_gesture: Option<TrackMixGesture>,
    audio_item_start_edits: HashMap<ItemId, String>,
    active_menu: Option<MainMenu>,
    main_window_id: Option<iced::window::Id>,
    settings_window_id: Option<iced::window::Id>,
    fx_chain_window_id: Option<iced::window::Id>,
    fx_chain_track_id: Option<TrackId>,
    fx_chain_selected_index: Option<usize>,
    fx_chain_native_parent: Option<u64>,
    fx_chain_window_size: iced::Size,
    fx_chain_window_scale_factor: f32,
    fx_chain_editor_host: Option<x11_plugin_editor::X11PluginEditorHost>,
    fx_chain_plugin_gui: Option<ClapPluginGuiOwner>,
    fx_chain_plugin_gui_identity: Option<(TrackId, usize, String)>,
    fx_chain_editor_status: String,
    fx_chain_parameters: Vec<ClapParameterInfo>,
    fx_parameter_gesture: Option<FxParameterGesture>,
    fx_parameter_end_requested: bool,
    fx_parameter_value_edits: HashMap<u32, String>,
    fx_parameter_value_edit_pending: HashSet<u32>,
    midi_editor_window_id: Option<iced::window::Id>,
    midi_editor_item_id: Option<ItemId>,
    midi_editor_selected_notes: HashSet<aaadaw_core::NoteId>,
    midi_editor_origin_tick: u64,
    midi_editor_high_pitch: u8,
    midi_editor_pixels_per_beat: f32,
    midi_note_clipboard: MidiNoteClipboard,
    plugin_picker_window_id: Option<iced::window::Id>,
    plugin_picker_track_id: Option<TrackId>,
    plugin_picker_instrument_track_id: Option<TrackId>,
    plugin_picker_search: String,
    shortcut_capture_id: Option<String>,
    shortcut_editor_feedback: String,
    settings_category: SettingsCategory,
    clap_plugin_paths: Vec<PathBuf>,
    clap_plugin_default_paths: HashSet<PathBuf>,
    clap_plugin_cache_path: Option<PathBuf>,
    clap_plugin_scan: ClapPluginScanReport,
    clap_plugin_scan_busy: bool,
    clap_plugin_scan_is_cached: bool,
    clap_plugin_scan_paths: Vec<PathBuf>,
    clap_plugin_settings_feedback: String,
    timeline: TimelineState,
    path_picker_busy: bool,
    audio_asset_source_statuses: HashMap<String, AudioAssetSourceStatusEntry>,
    audio_asset_management_worker: Option<AudioAssetManagementWorker>,
    audio_asset_management_busy: bool,
    audio_asset_management_finalizing: bool,
    audio_asset_management_cancel_requested: bool,
    audio_asset_management_operation: Option<AudioAssetManagementOperation>,
    audio_asset_management_status: String,
    audio_waveforms: HashMap<String, Arc<AudioWaveform>>,
    audio_waveform_worker: Option<AudioWaveformWorker>,
    relink_source_path_query: String,
    revision: u64,
    saved_revision: u64,
    project_generation: u64,
    io_busy: bool,
    audio_file_path_query: String,
    import_busy: bool,
    import_finalizing: bool,
    import_cancel_requested: bool,
    import_worker: Option<PendingAudioImport>,
    import_bytes: u64,
    import_total_bytes: Option<u64>,
    recording_recovery_candidates: Vec<aaadaw_app::RecordingRecoveryCandidate>,
    recording_recovery_scanning: bool,
    recording_recovery_busy: bool,
    pending_recording_cleanup: Vec<PendingRecordingCleanup>,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    recording: Option<ActiveRecording>,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    pending_recording: Option<ActiveRecording>,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    recording_starting: bool,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    recording_cancel_requested: bool,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    recording_stopping: bool,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    recording_cancelled_transport_start: bool,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    recording_start_sample: u64,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    recording_tracks: Vec<TrackId>,
    record_import_tracks: Option<RecordImportTarget>,
    status: String,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    playback: Option<RunningAudioPlayback>,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    playback_graph_dirty: bool,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    playback_busy: bool,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    playback_playing: bool,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    playhead_sample: u64,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    seek_sample_query: String,
    #[cfg(all(feature = "pipewire-backend", feature = "jack-backend"))]
    playback_backend: PlaybackBackend,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    clap_effect_owners: ClapEffectOwners,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    clap_effect_parameter_senders: HashMap<u64, ClapParameterSender>,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    clap_effect_targets: HashMap<u64, (TrackId, usize, String)>,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    clap_effect_parameter_targets: HashMap<u64, (TrackId, usize)>,
    pending_fx_parameter_sync: Option<FxParameterChange>,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    clap_effect_state_overrides: HashSet<(TrackId, usize, String)>,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    clap_instrument_owners: ClapInstrumentOwners,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    clap_instrument_targets: HashMap<u64, TrackId>,
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    clap_plugin_warnings: Vec<String>,
}

#[derive(Debug)]
struct FxParameterGesture {
    track_id: TrackId,
    chain_index: usize,
    parameter_id: u32,
    before: f64,
    after: f64,
    before_state: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrackMixParameter {
    Volume,
    Pan,
}

#[derive(Clone, Copy, Debug)]
struct TrackMixGesture {
    track_id: TrackId,
    parameter: TrackMixParameter,
    before_volume_db: f32,
    before_pan: f32,
    after_volume_db: f32,
    after_pan: f32,
}

struct PendingAudioImport {
    worker: AudioItemImportWorker,
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
struct ActiveRecording {
    input: RunningAudioInput,
    writer: AudioRecordingWorker,
    control: AudioCaptureControl,
    recovery_manifest_path: PathBuf,
}

struct RecordImportTarget {
    track_ids: Vec<TrackId>,
    source_paths: Vec<PathBuf>,
    next_segment_index: usize,
    next_start_sample: u64,
    imported_actions: Vec<DawAction>,
    project_path: PathBuf,
    sample_rate: u32,
    recovery_manifest_path: PathBuf,
    project_generation: u64,
    recovery_discarded_frames: u64,
    recovery_discarded_tail_bytes: u64,
    recovery_start_sample_is_estimate: bool,
}

struct PendingRecordingCleanup {
    manifest_path: PathBuf,
    project_generation: u64,
    saved_revision: u64,
    media_refs: Vec<String>,
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
#[derive(Clone)]
pub(super) struct SharedRecordingStart(Arc<Mutex<Option<Result<ActiveRecording, String>>>>);

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
impl std::fmt::Debug for SharedRecordingStart {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedRecordingStart(..)")
    }
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
#[derive(Clone)]
pub(super) struct SharedRecordingPositionSaved(
    Arc<Mutex<Option<Result<(ActiveRecording, u64), String>>>>,
);

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
impl std::fmt::Debug for SharedRecordingPositionSaved {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedRecordingPositionSaved(..)")
    }
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
type RecordingStopResult = Result<(Vec<PathBuf>, PathBuf), String>;

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
#[derive(Clone)]
pub(super) struct SharedRecordingStop(Arc<Mutex<Option<RecordingStopResult>>>);

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
impl std::fmt::Debug for SharedRecordingStop {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedRecordingStop(..)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainPane {
    Arrangement,
    MediaBrowser,
}

struct MediaPanelDock {
    panes: Option<pane_grid::State<MainPane>>,
    split: Option<Split>,
    open: bool,
    main_ratio: f32,
}

impl Default for MediaPanelDock {
    fn default() -> Self {
        Self {
            panes: None,
            split: None,
            open: false,
            main_ratio: 0.72,
        }
    }
}

impl MediaPanelDock {
    fn toggle(&mut self) {
        if self.panes.is_none() {
            let (mut panes, arrangement) = pane_grid::State::new(MainPane::Arrangement);
            let (_, split) = panes
                .split(Axis::Vertical, arrangement, MainPane::MediaBrowser)
                .expect("the arrangement pane is present");
            panes.resize(split, self.main_ratio);
            self.panes = Some(panes);
            self.split = Some(split);
        }
        self.open = !self.open;
    }

    fn resize(&mut self, split: Split, ratio: f32) {
        if self.split == Some(split) && ratio.is_finite() {
            self.main_ratio = ratio.clamp(0.55, 0.86);
            if let Some(panes) = &mut self.panes {
                panes.resize(split, self.main_ratio);
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct SharedAudioImportWorker(
    Arc<Mutex<Option<Result<AudioItemImportWorker, String>>>>,
);

impl std::fmt::Debug for SharedAudioImportWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedAudioImportWorker(..)")
    }
}

#[derive(Clone)]
pub(crate) struct SharedAudioAssetManagementWorker(
    Arc<Mutex<Option<Result<AudioAssetManagementWorker, String>>>>,
);

impl std::fmt::Debug for SharedAudioAssetManagementWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedAudioAssetManagementWorker(..)")
    }
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
#[derive(Clone)]
pub(crate) struct SharedPreparedPlayback(Arc<Mutex<Option<Result<PreparedAudioPlayback, String>>>>);

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
impl std::fmt::Debug for SharedPreparedPlayback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedPreparedPlayback(..)")
    }
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let mut app = Self::default();
        let (main_window_id, main_window_task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(1280.0, 800.0),
            min_size: Some(iced::Size::new(900.0, 620.0)),
            ..iced::window::Settings::default()
        });
        app.main_window_id = Some(main_window_id);
        let main_window_task = main_window_task.discard();
        match keyboard_config::load() {
            Ok(bindings) => {
                if let Ok(bindings) = commands::validate_bindings(&bindings) {
                    app.shortcut_binding_edits = bindings.clone();
                    *app.shortcut_bindings
                        .write()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = bindings;
                } else {
                    app.status =
                        "Keyboard shortcut config has conflicts; using defaults".to_owned();
                }
            }
            Err(error) => {
                app.status = format!("Keyboard shortcut config unavailable: {error}");
            }
        }
        let default_plugin_paths = default_clap_search_paths();
        app.clap_plugin_default_paths = default_plugin_paths.iter().cloned().collect();
        app.clap_plugin_paths = default_plugin_paths;
        let mut plugin_settings_warnings = Vec::new();
        match clap_plugin_config::load() {
            Ok(paths) => {
                app.clap_plugin_paths = clap_plugin_settings::merge_clap_plugin_paths(
                    app.clap_plugin_paths.clone(),
                    paths,
                );
            }
            Err(error) => {
                plugin_settings_warnings
                    .push(format!("CLAP search path config unavailable: {error}"));
            }
        }
        let plugin_scan_task = app.initialize_clap_plugin_scan(
            clap_plugin_cache::default_path(),
            plugin_settings_warnings,
        );
        let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
            return (app, Task::batch([main_window_task, plugin_scan_task]));
        };
        app.project_path_query = path.to_string_lossy().into_owned();
        app.io_busy = true;
        app.status = format!("Opening {}…", path.display());
        let message_path = path.clone();
        let task = Task::perform(
            run_blocking("aaadaw-project-open", move || {
                project_io::load_project_file(path)
            }),
            move |result| Message::ProjectLoaded(message_path, Arc::new(Mutex::new(Some(result)))),
        );
        (app, Task::batch([main_window_task, plugin_scan_task, task]))
    }

    fn window_title(&self, window_id: iced::window::Id) -> String {
        if self.settings_window_id == Some(window_id) {
            "AAADAW Settings".to_owned()
        } else if self.fx_chain_window_id == Some(window_id) {
            self.fx_chain_track_id
                .and_then(|track_id| {
                    self.project
                        .tracks()
                        .iter()
                        .position(|track| track.id() == track_id)
                        .map(|index| format!("Track {} FX Chain", index + 1))
                })
                .unwrap_or_else(|| "Track FX Chain".to_owned())
        } else if self.plugin_picker_window_id == Some(window_id) {
            "Add a CLAP Plugin".to_owned()
        } else if self.midi_editor_window_id == Some(window_id) {
            self.midi_editor_item_id
                .and_then(|item_id| {
                    self.project
                        .midi_items()
                        .iter()
                        .find(|item| item.id() == item_id)
                        .map(|_| "MIDI Editor".to_owned())
                })
                .unwrap_or_else(|| "MIDI Editor".to_owned())
        } else {
            "AAADAW".to_owned()
        }
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        let playback_active = self.playback.is_some();
        #[cfg(not(any(feature = "jack-backend", feature = "pipewire-backend")))]
        let playback_active = false;
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        let recording_active =
            self.recording.is_some() || self.recording_starting || self.recording_stopping;
        #[cfg(not(any(feature = "jack-backend", feature = "pipewire-backend")))]
        let recording_active = false;

        let background_ticks = if self.import_busy
            || playback_active
            || recording_active
            || self.audio_asset_management_busy
            || self.audio_waveform_worker.is_some()
        {
            iced::time::every(Duration::from_millis(100)).map(|_| Message::BackgroundTick)
        } else {
            iced::Subscription::none()
        };
        iced::Subscription::batch([
            iced::event::listen_with(runtime_keyboard_event),
            iced::window::close_events().map(Message::WindowClosed),
            iced::window::close_requests().map(Message::WindowCloseRequested),
            iced::window::resize_events()
                .map(|(window_id, size)| Message::FxChainWindowResized(window_id, size)),
            background_ticks,
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        if !matches!(
            &message,
            Message::Timeline(
                timeline::TimelineEvent::OpenTrackContextMenu(_)
                    | timeline::TimelineEvent::ToggleTrackContextMenu(_)
            ) | Message::Escape
        ) {
            self.timeline.context_track = None;
        }
        if !matches!(
            &message,
            Message::Timeline(timeline::TimelineEvent::OpenItemContextMenu { .. })
                | Message::Escape
        ) {
            self.timeline.context_item = None;
            self.timeline.context_item_position = None;
        }
        if !matches!(
            &message,
            Message::ToggleMainMenu(_)
                | Message::DismissMainMenu
                | Message::Escape
                | Message::ActionQueryChanged(_)
        ) {
            self.active_menu = None;
        }
        let window_safe_message = matches!(
            &message,
            Message::OpenSettings
                | Message::OpenTrackFxChain(_)
                | Message::OpenTrackInstrumentPicker(_)
                | Message::OpenPluginPicker
                | Message::OpenMidiEditor(_)
                | Message::CloseMidiEditor
                | Message::CloseTrackFxChain
                | Message::ClosePluginPicker
                | Message::PluginPickerSearchChanged(_)
                | Message::SelectFxChainPlugin(_)
                | Message::ExecuteCommand(commands::CommandId::OpenSettings)
                | Message::ToggleMediaBrowserPanel
                | Message::ExecuteCommand(commands::CommandId::ToggleMediaBrowserPanel)
                | Message::WindowClosed(_)
                | Message::WindowCloseRequested(_)
                | Message::StartShortcutCapture(_)
                | Message::ClearShortcutBinding(_)
                | Message::RestoreShortcutDefault(_)
                | Message::SelectSettingsCategory(_)
                | Message::CancelShortcutCapture
                | Message::ShortcutCaptureKey { .. }
                | Message::SaveShortcutBindings
                | Message::ResetShortcutBindings
                | Message::RemoveClapPluginPath(_)
                | Message::RescanClapPlugins
                | Message::ClapPluginsScanned(_)
                | Message::PickPath(PathPickerTarget::AddClapPluginPath)
        );
        let allowed_during_io = window_safe_message
            || matches!(
                &message,
                Message::ProjectLoaded(..)
                    | Message::ProjectSaved(..)
                    | Message::ToggleMainMenu(_)
                    | Message::DismissMainMenu
                    | Message::Escape
                    | Message::ToggleMediaBrowserPanel
                    | Message::Timeline(_)
                    | Message::TcpScrolled { .. }
                    | Message::TimelineScrolled { .. }
                    | Message::PathPicked(..)
                    | Message::AudioItemRelinked(..)
                    | Message::BackgroundTick
            );
        if matches!(
            &message,
            Message::Timeline(
                timeline::TimelineEvent::BeginItemDrag { .. }
                    | timeline::TimelineEvent::UpdateItemDrag { .. }
                    | timeline::TimelineEvent::EndItemDrag
            )
        ) && let Some(status) = item_drag_edit_guard_status(
            self.path_picker_busy,
            self.import_busy,
            self.audio_asset_management_busy,
            self.playback_busy(),
            self.playback_active(),
            self.io_busy,
        ) {
            self.timeline
                .handle(timeline::TimelineEvent::CancelItemDrag);
            self.status = status.to_owned();
            return Task::none();
        }
        if self.path_picker_busy
            && !window_safe_message
            && !matches!(
                &message,
                Message::PathPicked(..)
                    | Message::ToggleMainMenu(_)
                    | Message::DismissMainMenu
                    | Message::Escape
                    | Message::ToggleMediaBrowserPanel
                    | Message::Timeline(_)
                    | Message::TcpScrolled { .. }
                    | Message::TimelineScrolled { .. }
                    | Message::BackgroundTick
            )
        {
            self.status = "Wait for the file dialog to finish".to_owned();
            return Task::none();
        }
        if self.recording_recovery_busy
            && !matches!(
                &message,
                Message::RecordingRecoveryPrepared(_)
                    | Message::RecordingRecoveryDiscarded(..)
                    | Message::RecordingRecoveryCleaned(_)
                    | Message::BackgroundTick
                    | Message::ToggleMainMenu(_)
                    | Message::DismissMainMenu
                    | Message::Escape
            )
        {
            self.status = "Wait for recording recovery to finish".to_owned();
            return Task::none();
        }
        if self.import_busy
            && !window_safe_message
            && !matches!(
                &message,
                Message::AudioFilePathChanged(_)
                    | Message::ToggleMainMenu(_)
                    | Message::Escape
                    | Message::ToggleMediaBrowserPanel
                    | Message::Timeline(_)
                    | Message::TcpScrolled { .. }
                    | Message::TimelineScrolled { .. }
                    | Message::CancelAudioImport
                    | Message::AudioImportStarted(_)
                    | Message::AudioImportFinished(_)
                    | Message::BackgroundTick
            )
        {
            self.status = "Wait for audio import to finish or cancel it".to_owned();
            return Task::none();
        }
        if self.audio_asset_management_busy
            && !window_safe_message
            && !matches!(
                &message,
                Message::ToggleMainMenu(_)
                    | Message::DismissMainMenu
                    | Message::Escape
                    | Message::ToggleMediaBrowserPanel
                    | Message::Timeline(_)
                    | Message::TcpScrolled { .. }
                    | Message::TimelineScrolled { .. }
                    | Message::AudioAssetManagementStarted(_)
                    | Message::AudioAssetManagementFinished(_)
                    | Message::CancelAudioAssetManagement
                    | Message::BackgroundTick
            )
        {
            self.status = "Wait for audio asset maintenance to finish or cancel it".to_owned();
            return Task::none();
        }
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        {
            if (self.recording.is_some() || self.recording_starting || self.recording_stopping)
                && !window_safe_message
                && !matches!(
                    &message,
                    Message::StopPlayback
                        | Message::StopRecording
                        | Message::RecordingStarted(_)
                        | Message::RecordingPositionSaved(_)
                        | Message::RecordingStopped(_)
                        | Message::BackgroundTick
                        | Message::ToggleMainMenu(_)
                        | Message::DismissMainMenu
                        | Message::Escape
                        | Message::Timeline(_)
                        | Message::TcpScrolled { .. }
                        | Message::TimelineScrolled { .. }
                )
            {
                self.status = "Stop recording before changing the project".to_owned();
                return Task::none();
            }
            if self.playback_busy
                && !window_safe_message
                && !matches!(
                    &message,
                    Message::PlaybackPrepared { .. }
                        | Message::StopRecording
                        | Message::RecordingStarted(_)
                        | Message::RecordingPositionSaved(_)
                        | Message::RecordingStopped(_)
                        | Message::ToggleMainMenu(_)
                        | Message::DismissMainMenu
                        | Message::Escape
                        | Message::ToggleMediaBrowserPanel
                        | Message::BackgroundTick
                )
            {
                self.status = "Wait for playback preparation to finish".to_owned();
                return Task::none();
            }
            let unsupported_playback_history_edit = match &message {
                Message::Undo => !self.project.can_undo_track_mix(),
                Message::Redo => !self.project.can_redo_track_mix(),
                _ => false,
            };
            if self.playback.is_some()
                && (unsupported_playback_history_edit
                    || matches!(
                        &message,
                        Message::AddTrack
                            | Message::AddMidiItem
                            | Message::AddMidiNote(_)
                            | Message::AddMidiNoteAt(..)
                            | Message::EditMidiNotes(..)
                            | Message::DeleteMidiNotes(..)
                            | Message::SetMidiControllers(..)
                            | Message::DeleteMidiItem(_)
                            | Message::NudgeMidiItem(..)
                            | Message::NudgeMidiNote(..)
                            | Message::AdjustMidiNotePitch(..)
                            | Message::AdjustMidiNoteVelocity(..)
                            | Message::DeleteMidiNote(..)
                            | Message::QuantizeMidiItem(_)
                            | Message::DeleteTrack(_)
                            | Message::MoveTrack(..)
                            | Message::TrackNameChanged(..)
                            | Message::CommitTrackName(_)
                            | Message::ToggleMute(_)
                            | Message::ToggleSolo(_)
                            | Message::ToggleRecordArm(_)
                            | Message::AddScannedPlugin(_)
                            | Message::ClearTrackInstrument(_)
                            | Message::SelectScannedInstrument(_)
                            | Message::ToggleFxChainPlugin(_)
                            | Message::RemoveSelectedFxPlugin
                            | Message::FxChainWindowNativeHandle(..)
                            | Message::FxChainWindowScaleFactor(..)
                            | Message::FxChainWindowResized(..)
                            | Message::NudgeAudioItem(..)
                            | Message::BeginAudioItemStartSampleEdit(_)
                            | Message::AudioItemStartSampleChanged(..)
                            | Message::CommitAudioItemStartSample(_)
                            | Message::CancelAudioItemStartSampleEdit(_)
                            | Message::DeleteAudioItem(_)
                            | Message::DeleteSelectedItems
                            | Message::DuplicateAudioItem(_)
                            | Message::DuplicateMidiItem(_)
                            | Message::SplitSelectedItemsAtCursor
                            | Message::SplitSelectedItemsAtTimeSelection
                            | Message::RunActionQuery
                            | Message::RunAudioAssetManagement(_)
                            | Message::CancelAudioAssetManagement
                            | Message::ReimportAudioItem(_)
                            | Message::RelinkAudioItem(_)
                    ))
            {
                self.status = format!(
                    "Close {} output before editing the project",
                    self.playback_name()
                );
                return Task::none();
            }
        }
        if self.io_busy && !allowed_during_io {
            self.status = "Wait for current project operation to finish".to_owned();
            return Task::none();
        }
        let mut task = Task::none();
        match message {
            Message::ToggleMainMenu(menu) => {
                self.active_menu = (self.active_menu != Some(menu)).then_some(menu);
            }
            Message::OpenSettings => task = self.open_settings(),
            Message::WindowClosed(window_id) => {
                if self.settings_window_id == Some(window_id) {
                    self.settings_window_id = None;
                    self.shortcut_capture_id = None;
                    self.shortcut_editor_feedback.clear();
                } else if self.fx_chain_window_id == Some(window_id) {
                    self.close_fx_editor_resources();
                    self.fx_chain_window_id = None;
                    self.fx_chain_track_id = None;
                    self.fx_chain_selected_index = None;
                    if let Some(picker_window_id) = self.plugin_picker_window_id.take() {
                        self.plugin_picker_track_id = None;
                        self.plugin_picker_instrument_track_id = None;
                        self.plugin_picker_search.clear();
                        task = iced::window::close(picker_window_id);
                    }
                } else if self.plugin_picker_window_id == Some(window_id) {
                    self.plugin_picker_window_id = None;
                    self.plugin_picker_track_id = None;
                    self.plugin_picker_instrument_track_id = None;
                    self.plugin_picker_search.clear();
                } else if self.midi_editor_window_id == Some(window_id) {
                    self.midi_editor_window_id = None;
                    self.midi_editor_item_id = None;
                    self.midi_editor_selected_notes.clear();
                } else if self.main_window_id == Some(window_id) {
                    self.close_fx_editor_resources();
                    task = iced::exit();
                }
            }
            Message::WindowCloseRequested(window_id) => {
                if self.fx_chain_window_id == Some(window_id)
                    || self.main_window_id == Some(window_id)
                {
                    self.close_fx_editor_resources();
                }
            }
            Message::OpenTrackFxChain(track_id) => task = self.open_track_fx_chain(track_id),
            Message::OpenMidiEditor(item_id) => task = self.open_midi_editor(item_id),
            Message::CloseMidiEditor => {
                if let Some(window_id) = self.midi_editor_window_id.take() {
                    self.midi_editor_item_id = None;
                    self.midi_editor_selected_notes.clear();
                    task = iced::window::close(window_id);
                }
            }
            Message::SelectMidiNotes(note_ids) => {
                self.midi_editor_selected_notes = note_ids;
            }
            Message::CopyMidiNotes(item_id, note_ids) => {
                if let Some(item) = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                {
                    let selected = item
                        .notes()
                        .iter()
                        .filter(|note| note_ids.contains(&note.id()))
                        .collect::<Vec<_>>();
                    if let Some(first_tick) = selected.iter().map(|note| note.tick()).min() {
                        self.midi_note_clipboard.notes = selected
                            .iter()
                            .map(|note| aaadaw_core::MidiNoteData {
                                pitch: note.pitch(),
                                tick: note.tick() - first_tick,
                                duration: note.duration(),
                                velocity: note.velocity(),
                            })
                            .collect();
                        self.midi_note_clipboard
                            .notes
                            .sort_by_key(|note| (note.tick, note.pitch));
                        self.midi_note_clipboard.span_ticks = self
                            .midi_note_clipboard
                            .notes
                            .iter()
                            .map(|note| note.tick.saturating_add(note.duration))
                            .max()
                            .unwrap_or(0);
                        self.midi_note_clipboard.source_item_id = Some(item_id);
                        self.midi_note_clipboard.last_paste = None;
                        self.status =
                            format!("Copied {} MIDI notes", self.midi_note_clipboard.notes.len());
                    }
                }
            }
            Message::PasteMidiNotes(item_id) => {
                let Some(item) = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                else {
                    return task;
                };
                if self.midi_note_clipboard.notes.is_empty() {
                    return task;
                }
                let grid = (u64::from(self.project.settings().ppq()) / 4).max(1);
                let tick = self
                    .midi_note_clipboard
                    .last_paste
                    .filter(|(last_item, _)| *last_item == item_id)
                    .map_or_else(
                        || {
                            if self.midi_note_clipboard.source_item_id == Some(item_id) {
                                snap_tick_up(self.midi_note_clipboard.span_ticks, grid)
                            } else {
                                (self.midi_editor_origin_tick.saturating_add(grid / 2) / grid)
                                    * grid
                            }
                        },
                        |(_, previous)| {
                            snap_tick_up(
                                previous.saturating_add(self.midi_note_clipboard.span_ticks),
                                grid,
                            )
                        },
                    );
                let notes = self
                    .midi_note_clipboard
                    .notes
                    .iter()
                    .map(|note| aaadaw_core::MidiNoteData {
                        tick: tick.saturating_add(note.tick),
                        ..*note
                    })
                    .collect::<Vec<_>>();
                if notes
                    .iter()
                    .any(|note| note.tick.saturating_add(note.duration) > item.length_ticks())
                {
                    self.status =
                        "Paste rejected: notes would extend beyond the MIDI item".to_owned();
                } else {
                    let old_ids = item
                        .notes()
                        .iter()
                        .map(|note| note.id())
                        .collect::<HashSet<_>>();
                    let revision = self.revision;
                    self.apply_action(
                        DawAction::AddMidiNotes { item_id, notes },
                        "MIDI notes pasted",
                    );
                    if self.revision != revision {
                        if let Some(item) = self
                            .project
                            .midi_items()
                            .iter()
                            .find(|item| item.id() == item_id)
                        {
                            self.midi_editor_selected_notes = item
                                .notes()
                                .iter()
                                .map(|note| note.id())
                                .filter(|id| !old_ids.contains(id))
                                .collect();
                        }
                        self.midi_note_clipboard.last_paste = Some((item_id, tick));
                    }
                }
            }
            Message::AddMidiNoteAt(item_id, data) => {
                self.apply_action(
                    DawAction::AddMidiNotes {
                        item_id,
                        notes: vec![data],
                    },
                    "MIDI note added",
                );
            }
            Message::EditMidiNotes(item_id, edits) => {
                let actions = edits
                    .into_iter()
                    .map(|(note_id, data)| DawAction::EditMidiNote {
                        item_id,
                        note_id,
                        data,
                    })
                    .collect();
                self.apply_action(
                    DawAction::BatchTransaction {
                        tx_id: item_id.value(),
                        actions,
                    },
                    "MIDI notes edited",
                );
            }
            Message::DeleteMidiNotes(item_id, note_ids) => {
                if !note_ids.is_empty() {
                    self.apply_action(
                        DawAction::DeleteMidiNotes { item_id, note_ids },
                        "MIDI notes deleted",
                    );
                    self.midi_editor_selected_notes.clear();
                }
            }
            Message::SetMidiControllers(item_id, controllers) => {
                self.apply_action(
                    DawAction::SetMidiControllers {
                        item_id,
                        controllers,
                    },
                    "MIDI controller lane edited",
                );
            }
            Message::PianoRollPan(beats) => {
                let delta = i128::from(beats) * i128::from(self.project.settings().ppq());
                self.midi_editor_origin_tick = (i128::from(self.midi_editor_origin_tick) + delta)
                    .clamp(0, i128::from(u64::MAX))
                    as u64;
            }
            Message::PianoRollZoom(factor) if factor.is_finite() && factor > 0.0 => {
                self.midi_editor_pixels_per_beat =
                    (self.midi_editor_pixels_per_beat * factor).clamp(24.0, 300.0);
            }
            Message::PianoRollZoom(_) => {}
            Message::PianoRollPitchScroll(delta) => {
                self.midi_editor_high_pitch = (i16::from(self.midi_editor_high_pitch)
                    + i16::from(delta))
                .clamp(35, 127) as u8;
            }
            Message::OpenTrackInstrumentPicker(track_id) => {
                task = self.open_track_instrument_picker(track_id)
            }
            Message::ClearTrackInstrument(track_id) => self.clear_track_instrument(track_id),
            Message::OpenPluginPicker => task = self.open_plugin_picker(),
            Message::CloseTrackFxChain => {
                self.close_fx_editor_resources();
                let close_chain = self
                    .fx_chain_window_id
                    .take()
                    .map(iced::window::close)
                    .unwrap_or_else(Task::none);
                let close_picker = self
                    .plugin_picker_window_id
                    .take()
                    .map(iced::window::close)
                    .unwrap_or_else(Task::none);
                task = Task::batch([close_chain, close_picker]);
                self.fx_chain_track_id = None;
                self.fx_chain_selected_index = None;
                self.plugin_picker_track_id = None;
                self.plugin_picker_instrument_track_id = None;
                self.plugin_picker_search.clear();
            }
            Message::ClosePluginPicker => {
                if let Some(window_id) = self.plugin_picker_window_id.take() {
                    task = iced::window::close(window_id);
                }
                self.plugin_picker_track_id = None;
                self.plugin_picker_instrument_track_id = None;
                self.plugin_picker_search.clear();
            }
            Message::PluginPickerSearchChanged(query) => self.plugin_picker_search = query,
            Message::AddScannedPlugin(plugin_id) => task = self.add_scanned_plugin(&plugin_id),
            Message::SelectScannedInstrument(plugin_id) => {
                task = self.select_scanned_instrument(&plugin_id)
            }
            Message::SelectFxChainPlugin(index) => task = self.select_fx_chain_plugin(index),
            Message::FxParameterChanged(id, value) => self.change_fx_parameter(id, value),
            Message::FxParameterEnded(id) => self.end_fx_parameter_gesture(id),
            Message::FxParameterValueTextChanged(id, value) => {
                self.fx_parameter_value_edits.insert(id, value);
                self.fx_parameter_value_edit_pending.insert(id);
            }
            Message::CommitFxParameterValue(id) => self.commit_fx_parameter_value(id),
            Message::ResetFxParameterValue(id) => self.reset_fx_parameter_value(id),
            Message::ToggleFxChainPlugin(index) => self.toggle_fx_chain_plugin(index),
            Message::RemoveSelectedFxPlugin => task = self.remove_selected_fx_plugin(),
            Message::FxChainWindowNativeHandle(window_id, handle) => {
                if self.fx_chain_window_id == Some(window_id) {
                    self.fx_chain_native_parent = handle;
                    task = self.open_selected_fx_plugin_gui();
                }
            }
            Message::FxChainWindowScaleFactor(window_id, scale_factor) => {
                if self.fx_chain_window_id == Some(window_id)
                    && scale_factor.is_finite()
                    && scale_factor > 0.0
                {
                    self.fx_chain_window_scale_factor = scale_factor;
                    self.resize_fx_editor_host();
                }
            }
            Message::FxChainWindowResized(window_id, size) => {
                if self.fx_chain_window_id == Some(window_id) {
                    self.fx_chain_window_size = size;
                    self.resize_fx_editor_host();
                }
            }
            Message::StartShortcutCapture(action_id) => {
                self.shortcut_capture_id = Some(action_id);
                self.shortcut_editor_feedback = "Press a shortcut, or Escape to cancel".to_owned();
            }
            Message::ClearShortcutBinding(action_id) => self.clear_shortcut_binding(action_id),
            Message::RestoreShortcutDefault(action_id) => self.restore_shortcut_default(action_id),
            Message::SelectSettingsCategory(category) => {
                self.settings_category = category;
                if self.shortcut_capture_id.take().is_some() {
                    self.shortcut_editor_feedback = "Shortcut recording cancelled".to_owned();
                }
            }
            Message::RemoveClapPluginPath(path) => {
                task = self.remove_clap_plugin_path(path);
            }
            Message::RescanClapPlugins => task = self.start_clap_plugin_scan(),
            Message::ClapPluginsScanned(result) => self.finish_clap_plugin_scan(result),
            Message::CancelShortcutCapture => {
                self.shortcut_capture_id = None;
                self.shortcut_editor_feedback = "Shortcut recording cancelled".to_owned();
            }
            Message::ShortcutCaptureKey {
                action_id,
                key,
                modifiers,
            } => self.capture_shortcut_key(action_id, &key, modifiers),
            Message::RuntimeKeyboardEvent(event, status, window_id) => {
                let message = plugin_window_escape_message(
                    &event,
                    status,
                    window_id,
                    self.fx_chain_window_id,
                    self.plugin_picker_window_id,
                )
                .or_else(|| {
                    keyboard_shortcut_event(
                        event,
                        status,
                        window_id,
                        self.main_window_id,
                        self.settings_window_id,
                        self.shortcut_capture_id.as_deref(),
                    )
                });
                if let Some(message) = message {
                    task = self.update(message);
                }
            }
            Message::DismissMainMenu => self.active_menu = None,
            Message::Escape => {
                if self.active_menu.take().is_none() {
                    if self.timeline.context_item.take().is_some()
                        || self.timeline.context_track.take().is_some()
                    {
                        self.timeline.context_item_position = None;
                    } else {
                        self.timeline
                            .handle(timeline::TimelineEvent::ClearTimeSelection);
                    }
                }
            }
            Message::NewProject => self.new_project(),
            Message::Timeline(timeline::TimelineEvent::EndItemDrag) => self.finish_item_drag(),
            Message::Timeline(timeline::TimelineEvent::CancelItemDrag) => {
                self.timeline
                    .handle(timeline::TimelineEvent::CancelItemDrag);
                self.status = "Item drag cancelled".to_owned();
            }
            Message::Timeline(event) => self.timeline.handle(event),
            Message::BeginTrackNameEdit(track_id) => task = self.begin_track_name_edit(track_id),
            Message::TcpScrolled { offset, height } => {
                let offset_changed = (offset - self.timeline.vertical_scroll).abs() > 0.5;
                self.timeline.vertical_scroll = offset;
                self.timeline.viewport_height = height;
                if offset_changed {
                    task = scroll_arrangement_to(timeline::TIMELINE_SCROLL_ID, offset);
                }
            }
            Message::TimelineScrolled { offset, height } => {
                let offset_changed = (offset - self.timeline.vertical_scroll).abs() > 0.5;
                self.timeline.vertical_scroll = offset;
                self.timeline.viewport_height = height;
                if offset_changed {
                    task = scroll_arrangement_to(timeline::TCP_SCROLL_ID, offset);
                }
            }
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::TogglePlayback => {
                if self.playback_playing {
                    self.stop_playback();
                } else {
                    task = self.start_playback();
                }
            }
            Message::AddTrack => {
                self.active_menu = None;
                self.add_track();
            }
            Message::AddMidiItem => {
                let action = create_four_beat_midi_item(&self.project);
                self.apply_edit(action, "Four-beat MIDI item created");
            }
            Message::AddMidiNote(item_id) => {
                let action = add_quarter_note(&self.project, item_id);
                self.apply_edit(action, "C4 MIDI note added");
            }
            Message::DeleteMidiItem(item_id) => {
                self.apply_action(DawAction::DeleteMidiItem { item_id }, "MIDI item deleted");
            }
            Message::NudgeMidiItem(item_id, direction) => {
                let action = move_midi_item_by_beat(&self.project, item_id, direction);
                self.apply_edit(action, "MIDI item moved by one beat");
            }
            Message::NudgeMidiNote(item_id, note_id, direction) => {
                let action =
                    move_midi_note_by_sixteenth(&self.project, item_id, note_id, direction);
                self.apply_edit(action, "MIDI note changed");
            }
            Message::AdjustMidiNotePitch(item_id, note_id, delta) => {
                let action = adjust_midi_note_pitch(&self.project, item_id, note_id, delta);
                self.apply_edit(action, "MIDI note changed");
            }
            Message::AdjustMidiNoteVelocity(item_id, note_id, delta) => {
                let action = adjust_midi_note_velocity(&self.project, item_id, note_id, delta);
                self.apply_edit(action, "MIDI note changed");
            }
            Message::DeleteMidiNote(item_id, note_id) => {
                self.apply_action(delete_midi_note(item_id, note_id), "MIDI note deleted");
            }
            Message::QuantizeMidiItem(item_id) => {
                let action = quantize_midi_item_to_sixteenth(item_id);
                self.apply_edit(action, "MIDI item quantized to 1/16");
            }
            Message::DeleteTrack(track_id) => self.delete_track(track_id),
            Message::MoveTrack(track_id, direction) => self.move_track(track_id, direction),
            Message::TrackNameChanged(track_id, name) => {
                self.track_name_edits.insert(track_id, name);
            }
            Message::CommitTrackName(track_id) => self.commit_track_name(track_id),
            Message::ToggleMute(track_id) => {
                if let Some(track) = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                {
                    self.apply_action(
                        DawAction::SetTrackMute {
                            track_id,
                            muted: !track.is_muted(),
                        },
                        "Track mute changed",
                    );
                }
            }
            Message::ToggleSolo(track_id) => {
                if let Some(track) = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                {
                    self.apply_action(
                        DawAction::SetTrackSolo {
                            track_id,
                            solo: !track.is_solo(),
                        },
                        "Track solo changed",
                    );
                }
            }
            Message::ToggleRecordArm(track_id) => {
                if let Some(track) = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                {
                    self.apply_action(
                        DawAction::SetTrackRecordArm {
                            track_id,
                            armed: !track.is_record_armed(),
                        },
                        "Track record arm changed",
                    );
                }
            }
            Message::PreviewTrackVolume(track_id, volume_db) => {
                self.preview_track_mix(track_id, TrackMixParameter::Volume, volume_db);
            }
            Message::CommitTrackVolume(track_id) => {
                self.finish_track_mix_gesture(track_id, TrackMixParameter::Volume);
            }
            Message::PreviewTrackPan(track_id, pan) => {
                self.preview_track_mix(track_id, TrackMixParameter::Pan, pan);
            }
            Message::CommitTrackPan(track_id) => {
                self.finish_track_mix_gesture(track_id, TrackMixParameter::Pan);
            }
            Message::TrackVolumeTextChanged(track_id, value) => {
                self.track_volume_edits.insert(track_id, value);
            }
            Message::TrackPanTextChanged(track_id, value) => {
                self.track_pan_edits.insert(track_id, value);
            }
            Message::CommitTrackVolumeText(track_id) => {
                self.commit_track_volume_text(track_id);
            }
            Message::CommitTrackPanText(track_id) => {
                self.commit_track_pan_text(track_id);
            }
            Message::ResetTrackVolume(track_id) => {
                self.cancel_track_mix_gesture();
                self.track_volume_edits.remove(&track_id);
                if self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                    .is_some_and(|track| track.volume_db() != 0.0)
                {
                    self.apply_action(
                        DawAction::SetTrackVolume {
                            track_id,
                            volume_db: 0.0,
                        },
                        "Track volume reset",
                    );
                }
            }
            Message::ResetTrackPan(track_id) => {
                self.cancel_track_mix_gesture();
                self.track_pan_edits.remove(&track_id);
                if self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                    .is_some_and(|track| track.pan() != 0.0)
                {
                    self.apply_action(
                        DawAction::SetTrackPan { track_id, pan: 0.0 },
                        "Track pan centered",
                    );
                }
            }
            Message::NudgeAudioItem(item_id, direction, milliseconds) => {
                self.nudge_audio_item(item_id, direction, milliseconds)
            }
            Message::BeginAudioItemStartSampleEdit(item_id) => {
                if let Some(item) = self
                    .project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                {
                    self.audio_item_start_edits
                        .entry(item_id)
                        .or_insert_with(|| item.start_sample().to_string());
                }
            }
            Message::AudioItemStartSampleChanged(item_id, query) => {
                self.audio_item_start_edits.insert(item_id, query);
            }
            Message::CommitAudioItemStartSample(item_id) => {
                self.commit_audio_item_start_sample(item_id);
            }
            Message::CancelAudioItemStartSampleEdit(item_id) => {
                self.audio_item_start_edits.remove(&item_id);
            }
            Message::DeleteAudioItem(item_id) => {
                self.audio_item_start_edits.remove(&item_id);
                self.apply_action(DawAction::DeleteAudioItem { item_id }, "Audio item deleted");
            }
            Message::DeleteSelectedItems => self.delete_selected_items(),
            Message::SplitSelectedItemsAtCursor => self.split_selected_items(false),
            Message::SplitSelectedItemsAtTimeSelection => self.split_selected_items(true),
            Message::DuplicateAudioItem(item_id) => {
                let action = duplicate_audio_item(&self.project, item_id);
                self.apply_edit(action, "Audio item duplicated");
            }
            Message::DuplicateMidiItem(item_id) => {
                self.apply_action(
                    DawAction::DuplicateMidiItem { item_id },
                    "MIDI item duplicated",
                );
            }
            Message::Undo => {
                self.active_menu = None;
                self.undo();
                self.sync_all_track_mix_to_playback();
            }
            Message::Redo => {
                self.active_menu = None;
                self.redo();
                self.sync_all_track_mix_to_playback();
            }
            Message::ActionQueryChanged(query) => self.action_query = query,
            Message::ShortcutPressed(key, modifiers) => {
                let key = if key == " " {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Space)
                } else {
                    iced::keyboard::Key::Character(key.as_str())
                };
                let shortcut = {
                    let bindings = self
                        .shortcut_bindings
                        .read()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    commands::from_shortcut(&key, modifiers, &bindings).map(Message::ExecuteCommand)
                };
                if let Some(message) = shortcut {
                    task = self.update(message);
                }
            }
            Message::SaveShortcutBindings => self.save_shortcut_bindings(),
            Message::ResetShortcutBindings => self.reset_shortcut_bindings(),
            Message::RunActionQuery => task = self.run_action_query(),
            Message::ExecuteCommand(command) => task = commands::dispatch(self, command),
            Message::ToggleMediaBrowserPanel => self.media_panel_dock.toggle(),
            Message::MediaPanelResized(split, ratio) => {
                self.media_panel_dock.resize(split, ratio);
            }
            Message::PickPath(target) => task = self.pick_path(target),
            Message::PathPicked(target, result) => task = self.path_picked(target, result),
            Message::OpenProject => task = self.open_project_command(),
            Message::SaveProject => {
                task = self.save_project_command();
            }
            Message::ImportAudio => task = self.start_audio_import(),
            Message::AudioFilePathChanged(path) => self.audio_file_path_query = path,
            Message::ReimportAudioItem(item_id) => task = self.reimport_audio_item(item_id),
            Message::CancelAudioImport => self.cancel_audio_import(),
            Message::AudioImportStarted(worker) => self.audio_import_started(worker),
            Message::AudioImportFinished(result) => task = self.finish_audio_import(result),
            Message::RunAudioAssetManagement(operation) => {
                task = self.start_audio_asset_management(operation);
            }
            Message::CancelAudioAssetManagement => self.cancel_audio_asset_management(),
            Message::AudioAssetManagementStarted(worker) => {
                self.audio_asset_management_started(worker);
            }
            Message::AudioAssetManagementFinished(result) => {
                self.finish_audio_asset_management(result);
            }
            Message::RelinkSourcePathChanged(path) => self.relink_source_path_query = path,
            Message::RelinkAudioItem(item_id) => task = self.relink_audio_item(item_id),
            Message::AudioItemRelinked(item_id, result) => {
                self.finish_audio_item_relink(item_id, result);
            }
            Message::BackgroundTick => {
                #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
                {
                    if self.fx_parameter_end_requested
                        && let Some(gesture) = self.fx_parameter_gesture.as_ref()
                    {
                        self.end_fx_parameter_gesture(gesture.parameter_id);
                    }
                    if let Some(change) = self.pending_fx_parameter_sync {
                        self.sync_fx_parameter_change(change);
                    }
                    self.update_playback_stats();
                    let recording_failed = self
                        .recording
                        .as_ref()
                        .is_some_and(|recording| recording.control.has_failed())
                        && !self.recording_stopping;
                    self.update_audio_waveforms();
                    task = if recording_failed {
                        Task::batch([
                            self.stop_recording(),
                            self.update_audio_import(),
                            self.update_audio_asset_management(),
                        ])
                    } else {
                        Task::batch([
                            self.update_audio_import(),
                            self.update_audio_asset_management(),
                        ])
                    };
                }
                #[cfg(not(any(feature = "jack-backend", feature = "pipewire-backend")))]
                {
                    self.update_audio_waveforms();
                    task = Task::batch([
                        self.update_audio_import(),
                        self.update_audio_asset_management(),
                    ]);
                }
            }
            Message::ProjectLoaded(path, result) => {
                self.io_busy = false;
                let result = result.lock().ok().and_then(|mut result| result.take());
                match result {
                    Some(Ok(project)) => {
                        self.project = project;
                        self.midi_note_clipboard.source_item_id = None;
                        self.midi_note_clipboard.last_paste = None;
                        self.timeline.rebuild(&self.project);
                        self.timeline.selected_item = None;
                        self.timeline.selected_track = None;
                        self.timeline.time_selection = None;
                        self.timeline.origin_tick = 0;
                        self.timeline.edit_cursor_tick = 0;
                        self.timeline.vertical_scroll = 0.0;
                        task = Task::batch([
                            scroll_arrangement_to(timeline::TCP_SCROLL_ID, 0.0),
                            scroll_arrangement_to(timeline::TIMELINE_SCROLL_ID, 0.0),
                        ]);
                        self.track_name_edits.clear();
                        self.track_volume_edits.clear();
                        self.track_pan_edits.clear();
                        self.track_mix_gesture = None;
                        self.audio_item_start_edits.clear();
                        self.audio_asset_source_statuses.clear();
                        self.project_path_query = path.to_string_lossy().into_owned();
                        self.project_path = Some(path.clone());
                        self.recording_recovery_candidates.clear();
                        self.project_generation = self.project_generation.wrapping_add(1);
                        self.start_audio_waveform_scan(true);
                        self.revision = 0;
                        self.saved_revision = 0;
                        self.status = format!("Opened {}", path.display());
                        self.recording_recovery_scanning = true;
                        let recovery_path = path.clone();
                        task = Task::batch([
                            task,
                            Task::perform(
                                run_blocking("aaadaw-recording-recovery-scan", move || {
                                    let result = aaadaw_app::scan_recording_recoveries(
                                        recovery_path.clone(),
                                    )
                                    .map_err(|error| error.to_string());
                                    Ok((recovery_path, result))
                                }),
                                |result| match result {
                                    Ok((path, result)) => {
                                        Message::RecordingRecoveryScanned(path, result)
                                    }
                                    Err(error) => Message::RecordingRecoveryScanned(
                                        PathBuf::new(),
                                        Err(error),
                                    ),
                                },
                            ),
                        ]);
                    }
                    Some(Err(error)) => self.status = format!("Open failed: {error}"),
                    None => self.status = "Project open result was unavailable".to_owned(),
                }
            }
            Message::ProjectSaved(path, revision, result, plugin_state_warning) => {
                self.io_busy = false;
                match result {
                    Ok(()) => {
                        self.project_path_query = path.to_string_lossy().into_owned();
                        self.project_path = Some(path.clone());
                        self.saved_revision = revision;
                        self.status = if self.revision == revision {
                            format!("Saved {}", path.display())
                        } else {
                            format!("Saved {}; newer edits remain unsaved", path.display())
                        };
                        if let Some(warning) = plugin_state_warning {
                            self.status.push_str(&format!(
                                "; some plugin state was not captured; previously saved state was kept ({warning})"
                            ));
                        }
                        let ready = self
                            .pending_recording_cleanup
                            .iter()
                            .filter(|cleanup| {
                                cleanup.project_generation == self.project_generation
                                    && cleanup.saved_revision <= revision
                                    && cleanup.media_refs.iter().all(|media_ref| {
                                        self.project
                                            .audio_items()
                                            .iter()
                                            .any(|item| item.media_ref() == media_ref)
                                    })
                            })
                            .map(|cleanup| cleanup.manifest_path.clone())
                            .collect::<Vec<_>>();
                        if !ready.is_empty() {
                            task = Task::perform(
                                run_blocking("aaadaw-recording-recovery-cleanup", move || {
                                    for manifest_path in &ready {
                                        aaadaw_app::discard_recording_recovery(manifest_path)
                                            .map_err(|error| error.to_string())?;
                                    }
                                    Ok(ready)
                                }),
                                Message::RecordingRecoveryCleaned,
                            );
                        }
                    }
                    Err(error) => self.status = format!("Save failed: {error}"),
                }
            }
            Message::RecordingRecoveryScanned(path, result) => {
                if self
                    .project_path
                    .as_ref()
                    .is_some_and(|current| same_path(current, &path))
                {
                    self.recording_recovery_scanning = false;
                    match result {
                        Ok(candidates) => self.recording_recovery_candidates = candidates,
                        Err(error) => {
                            self.status = format!("Recording recovery scan failed: {error}")
                        }
                    }
                }
            }
            Message::RecoverRecording(path) => task = self.recover_recording(path),
            Message::DiscardRecording(path) => task = self.discard_recording_recovery(path),
            Message::RecordingRecoveryPrepared(result) => {
                task = self.recording_recovery_prepared(result);
            }
            Message::RecordingRecoveryDiscarded(path, result) => {
                self.recording_recovery_busy = false;
                match result {
                    Ok(()) => {
                        self.recording_recovery_candidates
                            .retain(|candidate| candidate.manifest_path != path);
                        self.status = "Incomplete recording discarded".to_owned();
                    }
                    Err(error) => self.status = format!("Recording discard failed: {error}"),
                }
            }
            Message::RecordingRecoveryCleaned(result) => match result {
                Ok(paths) => {
                    self.pending_recording_cleanup
                        .retain(|cleanup| !paths.contains(&cleanup.manifest_path));
                    self.recording_recovery_candidates
                        .retain(|candidate| !paths.contains(&candidate.manifest_path));
                }
                Err(error) => {
                    self.status = format!(
                        "Project saved, but recording source cleanup failed; saving again will retry: {error}"
                    );
                }
            },
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::StartPlayback => task = self.start_playback(),
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::StopPlayback => {
                if self.recording.is_some() {
                    task = self.stop_recording();
                } else {
                    self.stop_playback();
                }
            }
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::PanicMidi => self.panic_midi(),
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::StartRecording => task = self.start_recording(),
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::StopRecording => task = self.stop_recording(),
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::RecordingStarted(result) => task = self.finish_recording_start(result),
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::RecordingPositionSaved(result) => {
                self.finish_recording_position_saved(result);
            }
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::RecordingStopped(result) => task = self.finish_recording_stop(result),
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::RestartPlayback => task = self.restart_playback(),
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::SeekSampleChanged(sample) => self.seek_sample_query = sample,
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::SeekToItem(sample) => {
                self.seek_sample_query = sample.to_string();
                task = self.prepare_playback(sample, self.playback_playing);
            }
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::SeekToSample => task = self.seek_to_sample(),
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::ClosePlayback => self.close_playback(),
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::PlaybackPrepared {
                target_sample,
                start_when_ready,
                result,
            } => {
                self.finish_playback_preparation(target_sample, start_when_ready, result);
                if self.pending_recording.is_some() {
                    task = self.begin_pending_recording();
                }
            }
            #[cfg(all(feature = "jack-backend", feature = "pipewire-backend"))]
            Message::SelectPlaybackBackend(backend) => {
                if self.playback.is_some() {
                    self.status = "Close the current output before switching backends".to_owned();
                } else {
                    self.playback_backend = backend;
                    self.status = format!("{} selected for playback", backend.name());
                }
            }
        }
        task
    }

    fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    fn new_project(&mut self) {
        if self.io_busy
            || self.import_busy
            || self.audio_asset_management_busy
            || self.path_picker_busy
        {
            self.status = "Wait for the current project operation to finish".to_owned();
            return;
        }
        if self.is_dirty() {
            self.status = "Save the current project before creating a new one".to_owned();
            return;
        }
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        if self.playback.is_some() {
            self.status = format!(
                "Close {} output before creating a new project",
                self.playback_name()
            );
            return;
        }

        self.project = Project::new();
        self.project_generation = self.project_generation.wrapping_add(1);
        self.recording_recovery_candidates.clear();
        self.recording_recovery_scanning = false;
        self.midi_note_clipboard.source_item_id = None;
        self.midi_note_clipboard.last_paste = None;
        self.project_path = None;
        self.project_path_query.clear();
        self.revision = 0;
        self.saved_revision = 0;
        self.timeline.rebuild(&self.project);
        self.timeline.selected_track = None;
        self.timeline.selected_item = None;
        self.timeline.selected_items.clear();
        self.timeline.time_selection = None;
        self.timeline.origin_tick = 0;
        self.timeline.edit_cursor_tick = 0;
        self.timeline.vertical_scroll = 0.0;
        self.track_name_edits.clear();
        self.track_volume_edits.clear();
        self.track_pan_edits.clear();
        self.track_mix_gesture = None;
        self.audio_item_start_edits.clear();
        self.audio_waveforms.clear();
        self.audio_asset_source_statuses.clear();
        self.status = "New project created".to_owned();
    }

    fn open_settings(&mut self) -> Task<Message> {
        if let Some(window_id) = self.settings_window_id {
            return iced::window::gain_focus(window_id);
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(760.0, 620.0),
            min_size: Some(iced::Size::new(640.0, 460.0)),
            ..iced::window::Settings::default()
        });
        self.settings_window_id = Some(window_id);
        task.discard()
    }

    fn open_midi_editor(&mut self, item_id: ItemId) -> Task<Message> {
        if !self
            .project
            .midi_items()
            .iter()
            .any(|item| item.id() == item_id)
        {
            self.status = "The selected MIDI item no longer exists".to_owned();
            return Task::none();
        }
        if let Some(window_id) = self.midi_editor_window_id {
            if self.midi_editor_item_id != Some(item_id) {
                self.midi_editor_origin_tick = 0;
            }
            self.midi_editor_item_id = Some(item_id);
            self.midi_editor_selected_notes.clear();
            return iced::window::gain_focus(window_id);
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(1000.0, 620.0),
            min_size: Some(iced::Size::new(720.0, 420.0)),
            ..iced::window::Settings::default()
        });
        self.midi_editor_window_id = Some(window_id);
        self.midi_editor_item_id = Some(item_id);
        self.midi_editor_selected_notes.clear();
        self.midi_editor_origin_tick = 0;
        self.midi_editor_high_pitch = 84;
        self.midi_editor_pixels_per_beat = 96.0;
        task.discard()
    }

    fn clear_shortcut_binding(&mut self, action_id: String) {
        let mut candidate = self.shortcut_binding_edits.clone();
        candidate.insert(action_id.clone(), String::new());
        match commands::validate_bindings(&candidate) {
            Ok(bindings) => {
                self.shortcut_binding_edits = bindings;
                self.shortcut_defaults_restored.remove(&action_id);
                self.shortcut_capture_id = None;
                self.shortcut_editor_feedback =
                    "Shortcut cleared. Save to apply this change.".to_owned();
            }
            Err(error) => {
                self.shortcut_editor_feedback = format!("Shortcut could not be cleared: {error}");
            }
        }
    }

    fn restore_shortcut_default(&mut self, action_id: String) {
        let mut candidate = self.shortcut_binding_edits.clone();
        candidate.remove(&action_id);
        match commands::validate_bindings(&candidate) {
            Ok(bindings) => {
                self.shortcut_binding_edits = bindings;
                self.shortcut_defaults_restored.insert(action_id.clone());
                self.shortcut_capture_id = None;
                self.shortcut_editor_feedback = format!(
                    "{} restored to its default. Save to apply this change.",
                    commands::label_for_id(&action_id).unwrap_or(&action_id)
                );
            }
            Err(error) => {
                self.shortcut_editor_feedback = format!(
                    "Default shortcut could not be restored: {}",
                    commands::friendly_shortcut_error(&error)
                );
            }
        }
    }

    fn capture_shortcut_key(
        &mut self,
        action_id: String,
        key: &str,
        modifiers: iced::keyboard::Modifiers,
    ) {
        let binding = match commands::capture_binding(key, modifiers) {
            Ok(binding) => binding,
            Err(error) => {
                self.shortcut_editor_feedback = error;
                return;
            }
        };
        let mut candidate = self.shortcut_binding_edits.clone();
        candidate.insert(action_id, binding.clone());
        match commands::validate_bindings(&candidate) {
            Ok(bindings) => {
                if let Some(action_id) = self.shortcut_capture_id.take() {
                    self.shortcut_defaults_restored.remove(&action_id);
                }
                self.shortcut_binding_edits = bindings;
                self.shortcut_editor_feedback = format!(
                    "Recorded {}. Save to apply this change.",
                    binding.replace("Mod+", "Ctrl/Cmd+")
                );
            }
            Err(error) => {
                self.shortcut_editor_feedback = format!(
                    "{}; press another key or Escape",
                    commands::friendly_shortcut_error(&error).replace("Mod+", "Ctrl/Cmd+")
                );
            }
        }
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    fn selected_playback_backend(&self) -> PlaybackBackend {
        #[cfg(all(feature = "jack-backend", feature = "pipewire-backend"))]
        {
            self.playback_backend
        }
        #[cfg(all(feature = "jack-backend", not(feature = "pipewire-backend")))]
        {
            PlaybackBackend::Jack
        }
        #[cfg(all(feature = "pipewire-backend", not(feature = "jack-backend")))]
        {
            PlaybackBackend::PipeWire
        }
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    fn playback_name(&self) -> &'static str {
        self.selected_playback_backend().name()
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    fn start_playback(&mut self) -> Task<Message> {
        self.recording_cancelled_transport_start = false;
        if self.playback.is_some() && !self.playback_graph_dirty {
            self.clap_plugin_warnings.clear();
            if let Err(error) = self.persist_clap_plugin_states() {
                self.clap_plugin_warnings.push(format!(
                    "Some plugin state could not be captured; previously saved state was kept ({error})"
                ));
            }
        }
        if self.playback.is_some() && self.playback_graph_dirty {
            return self.prepare_playback(self.playhead_sample, true);
        }
        if let Some(playback) = self.playback.as_mut() {
            return match playback.play() {
                Ok(()) => {
                    self.playback_playing = true;
                    self.status = "Playback started".to_owned();
                    if !self.clap_plugin_warnings.is_empty() {
                        self.status.push_str("; ");
                        self.status.push_str(&self.clap_plugin_warnings.join("; "));
                    }
                    Task::none()
                }
                Err(error) => {
                    self.status = format!("{} play failed: {error}", self.playback_name());
                    Task::none()
                }
            };
        }
        self.prepare_playback(self.playhead_sample, true)
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    fn stop_playback(&mut self) {
        let Some(playback) = self.playback.as_mut() else {
            self.status = format!("{} output is not open", self.playback_name());
            return;
        };
        match playback.stop() {
            Ok(()) => {
                self.playback_playing = false;
                self.status = "Playback stopped".to_owned();
            }
            Err(error) => self.status = format!("{} stop failed: {error}", self.playback_name()),
        }
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    fn panic_midi(&mut self) {
        let Some(playback) = self.playback.as_mut() else {
            self.status = format!("{} output is not open", self.playback_name());
            return;
        };
        match playback.panic_midi() {
            Ok(()) => {
                self.status = "MIDI Panic sent: CC resets to MIDI-capable ports, held notes released on all instruments; playhead unchanged".to_owned()
            }
            Err(error) => {
                self.status = format!("{} MIDI Panic failed: {error}", self.playback_name())
            }
        }
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    fn restart_playback(&mut self) -> Task<Message> {
        let start_when_ready = self.playback.is_none() || self.playback_playing;
        self.prepare_playback(0, start_when_ready)
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    fn seek_to_sample(&mut self) -> Task<Message> {
        match self.seek_sample_query.trim().parse::<u64>() {
            Ok(target_sample) => self.prepare_playback(target_sample, self.playback_playing),
            Err(error) => {
                self.status = format!("Invalid seek sample: {error}");
                Task::none()
            }
        }
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    fn close_playback(&mut self) {
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        let state_error = self.persist_clap_plugin_states().err();
        let mut shutdown_error = None;
        if let Some(playback) = self.playback.take() {
            match playback.shutdown() {
                Ok((instruments, effects)) => {
                    self.deactivate_stopped_instruments(instruments);
                    self.deactivate_stopped_effects(effects);
                }
                Err(error) => {
                    shutdown_error = Some(error.to_string());
                    let owner_ids = self.clap_effect_owners.keys().copied().collect::<Vec<_>>();
                    if let Some(cleanup_error) = self.discard_unused_effect_owners(&owner_ids) {
                        shutdown_error = Some(match shutdown_error.take() {
                            Some(error) => format!("{error}; {cleanup_error}"),
                            None => cleanup_error,
                        });
                    }
                    let owner_ids = self
                        .clap_instrument_owners
                        .keys()
                        .copied()
                        .collect::<Vec<_>>();
                    if let Some(cleanup_error) = self.discard_unused_instrument_owners(&owner_ids) {
                        shutdown_error = Some(match shutdown_error.take() {
                            Some(error) => format!("{error}; {cleanup_error}"),
                            None => cleanup_error,
                        });
                    }
                }
            }
        }
        self.playback_playing = false;
        self.playhead_sample = 0;
        self.seek_sample_query = "0".to_owned();
        self.status = shutdown_error.map_or_else(
            || format!("{} output closed", self.playback_name()),
            |error| format!("{} shutdown failed: {error}", self.playback_name()),
        );
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        if let Some(error) = state_error {
            self.status
                .push_str(&format!("; plugin state save failed: {error}"));
        }
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    fn prepare_playback(&mut self, target_sample: u64, start_when_ready: bool) -> Task<Message> {
        if self.playback_busy || self.io_busy {
            self.status = "Wait for current operation to finish".to_owned();
            return Task::none();
        }
        let Some(path) = self.project_path.clone() else {
            self.status = "Save or open project before playback".to_owned();
            return Task::none();
        };
        self.clap_plugin_warnings.clear();
        if let Err(error) = self.persist_clap_plugin_states() {
            self.clap_plugin_warnings.push(format!(
                "Some plugin state could not be captured; previously saved state will be used ({error})"
            ));
        }
        let snapshot = self.project.snapshot();
        self.playback_busy = true;
        self.status = format!("Preparing playback at sample {target_sample}…");
        let result = Arc::new(Mutex::new(None));
        let message_result = Arc::clone(&result);
        Task::perform(
            run_blocking("aaadaw-playback-prepare", move || {
                prepare_project_playback_file(path, snapshot, target_sample)
            }),
            move |prepared| {
                *message_result
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(prepared);
                Message::PlaybackPrepared {
                    target_sample,
                    start_when_ready,
                    result: SharedPreparedPlayback(result),
                }
            },
        )
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    fn finish_playback_preparation(
        &mut self,
        target_sample: u64,
        start_when_ready: bool,
        result: SharedPreparedPlayback,
    ) {
        let start_when_ready = start_when_ready && !self.recording_cancelled_transport_start;
        self.recording_cancelled_transport_start = false;
        self.playback_busy = false;
        let result = result.0.lock().ok().and_then(|mut result| result.take());
        let mut prepared = match result {
            Some(Ok(prepared)) => prepared,
            Some(Err(error)) => {
                self.status = format!("Playback preparation failed: {error}");
                return;
            }
            None => {
                self.status = "Playback preparation result was unavailable".to_owned();
                return;
            }
        };

        let instrument_owner_ids = match self.install_track_instrument_processors(&mut prepared) {
            Ok(ids) => ids,
            Err(error) => {
                self.status = format!("Playback preparation failed: {error}");
                return;
            }
        };

        let fx_owner_ids = match self.install_track_fx_processors(&mut prepared) {
            Ok(ids) => ids,
            Err(error) => {
                drop(prepared);
                let cleanup_error = self.discard_unused_instrument_owners(&instrument_owner_ids);
                self.status = format!("Playback preparation failed: {error}");
                if let Some(cleanup_error) = cleanup_error {
                    self.status.push_str(&format!("; {cleanup_error}"));
                }
                return;
            }
        };

        if self.playback.is_some() {
            let (result, retired_instruments, retired_effects) = {
                let playback = self.playback.as_mut().expect("playback exists");
                let result = playback.replace_graph(prepared);
                playback.collect_retired_graphs();
                (
                    result,
                    playback.take_retired_instrument_processors(),
                    playback.take_retired_fx_processors(),
                )
            };
            self.deactivate_stopped_instruments(retired_instruments);
            self.deactivate_stopped_effects(retired_effects);
            match result {
                Ok(()) => {
                    self.playhead_sample = target_sample;
                    self.seek_sample_query = target_sample.to_string();
                    self.playback_graph_dirty = false;
                    self.status = format!("Queued seek to sample {target_sample}");
                }
                Err(error) => {
                    let mut cleanup_error = self.discard_unused_effect_owners(&fx_owner_ids);
                    if let Some(instrument_error) =
                        self.discard_unused_instrument_owners(&instrument_owner_ids)
                    {
                        cleanup_error = Some(match cleanup_error {
                            Some(error) => format!("{error}; {instrument_error}"),
                            None => instrument_error,
                        });
                    }
                    self.status =
                        format!("{} graph replacement failed: {error}", self.playback_name());
                    if let Some(cleanup_error) = cleanup_error {
                        self.status.push_str(&format!("; {cleanup_error}"));
                    }
                }
            }
            return;
        }

        let mut playback = match prepared.into_output(self.selected_playback_backend()) {
            Ok(playback) => playback,
            Err(error) => {
                let mut cleanup_error = self.discard_unused_effect_owners(&fx_owner_ids);
                if let Some(instrument_error) =
                    self.discard_unused_instrument_owners(&instrument_owner_ids)
                {
                    cleanup_error = Some(match cleanup_error {
                        Some(error) => format!("{error}; {instrument_error}"),
                        None => instrument_error,
                    });
                }
                self.status = format!("{} output setup failed: {error}", self.playback_name());
                if let Some(cleanup_error) = cleanup_error {
                    self.status.push_str(&format!("; {cleanup_error}"));
                }
                return;
            }
        };
        let play_error = if start_when_ready {
            playback.play().err()
        } else {
            None
        };
        self.playback = Some(playback);
        self.playback_graph_dirty = false;
        self.playback_playing = start_when_ready && play_error.is_none();
        self.playhead_sample = target_sample;
        self.seek_sample_query = target_sample.to_string();
        if let Some(error) = play_error {
            self.status = format!("{} play failed: {error}", self.playback_name());
            return;
        }
        self.status = if start_when_ready {
            "Playback started".to_owned()
        } else {
            format!("{} output ready", self.playback_name())
        };
        if !self.clap_plugin_warnings.is_empty() {
            self.status.push_str("; ");
            self.status.push_str(&self.clap_plugin_warnings.join("; "));
        }
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    fn update_playback_stats(&mut self) {
        let (retired_instruments, retired_effects) = if let Some(playback) = self.playback.as_mut()
        {
            self.playhead_sample = playback.stats().playhead_sample;
            playback.collect_retired_graphs();
            (
                playback.take_retired_instrument_processors(),
                playback.take_retired_fx_processors(),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        self.deactivate_stopped_instruments(retired_instruments);
        self.deactivate_stopped_effects(retired_effects);
    }

    fn add_track(&mut self) {
        let index = self.project.tracks().len();
        let was_empty = index == 0;
        let previous_revision = self.revision;
        self.apply_action(
            DawAction::CreateTrack {
                index,
                name: format!("Audio {}", index + 1),
            },
            "Track created",
        );
        if was_empty && self.revision != previous_revision {
            self.timeline.selected_track = self.project.tracks().first().map(|track| track.id());
        }
    }

    fn apply_edit<E: std::fmt::Display>(&mut self, action: Result<DawAction, E>, success: &str) {
        match action {
            Ok(action) => self.apply_action(action, success),
            Err(error) => self.status = format!("Edit failed: {error}"),
        }
    }

    fn apply_action(&mut self, action: DawAction, success: &str) {
        let live_mix_track = match &action {
            DawAction::SetTrackVolume { track_id, .. }
            | DawAction::SetTrackPan { track_id, .. } => Some(*track_id),
            _ => None,
        };
        self.status = match self.project.apply(action) {
            Ok(()) => {
                self.midi_note_clipboard.last_paste = None;
                self.revision = self.revision.wrapping_add(1);
                self.timeline.rebuild(&self.project);
                if let Some(track_id) = live_mix_track {
                    self.sync_track_mix_to_playback(track_id);
                }
                success.to_owned()
            }
            Err(error) => format!("Action failed: {error}"),
        };
    }

    fn sync_track_mix_to_playback(&self, track_id: TrackId) {
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        if let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            && let Some(playback) = &self.playback
        {
            let _ = playback.set_track_mix(track_id, track.volume_db(), track.pan());
        }
        #[cfg(not(any(feature = "jack-backend", feature = "pipewire-backend")))]
        let _ = track_id;
    }

    fn sync_all_track_mix_to_playback(&self) {
        for track in self.project.tracks() {
            self.sync_track_mix_to_playback(track.id());
        }
    }

    fn preview_track_mix(&mut self, track_id: TrackId, parameter: TrackMixParameter, value: f32) {
        match parameter {
            TrackMixParameter::Volume => {
                self.track_volume_edits.remove(&track_id);
            }
            TrackMixParameter::Pan => {
                self.track_pan_edits.remove(&track_id);
            }
        }
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            return;
        };
        let current_volume = track.volume_db();
        let current_pan = track.pan();
        if self
            .track_mix_gesture
            .is_some_and(|gesture| gesture.track_id != track_id || gesture.parameter != parameter)
        {
            self.cancel_track_mix_gesture();
        }
        let gesture = self.track_mix_gesture.get_or_insert(TrackMixGesture {
            track_id,
            parameter,
            before_volume_db: current_volume,
            before_pan: current_pan,
            after_volume_db: current_volume,
            after_pan: current_pan,
        });
        match parameter {
            TrackMixParameter::Volume => gesture.after_volume_db = value.clamp(-60.0, 6.0),
            TrackMixParameter::Pan => gesture.after_pan = value.clamp(-1.0, 1.0),
        }
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        if let Some(playback) = &self.playback
            && !playback.set_track_mix(track_id, gesture.after_volume_db, gesture.after_pan)
        {
            self.status = "Could not update the active track mix".to_owned();
        }
    }

    fn finish_track_mix_gesture(&mut self, track_id: TrackId, parameter: TrackMixParameter) {
        let Some(gesture) = self.track_mix_gesture.take() else {
            return;
        };
        if gesture.track_id != track_id || gesture.parameter != parameter {
            self.track_mix_gesture = Some(gesture);
            return;
        }
        let (before, after) = match parameter {
            TrackMixParameter::Volume => (gesture.before_volume_db, gesture.after_volume_db),
            TrackMixParameter::Pan => (gesture.before_pan, gesture.after_pan),
        };
        if (before - after).abs() < f32::EPSILON {
            self.sync_track_mix_to_playback(track_id);
            return;
        }
        let action = match parameter {
            TrackMixParameter::Volume => DawAction::SetTrackVolume {
                track_id,
                volume_db: gesture.after_volume_db,
            },
            TrackMixParameter::Pan => DawAction::SetTrackPan {
                track_id,
                pan: gesture.after_pan,
            },
        };
        self.apply_action(action, "Track mix changed");
    }

    fn cancel_track_mix_gesture(&mut self) {
        if let Some(_gesture) = self.track_mix_gesture.take() {
            #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
            if let Some(playback) = &self.playback {
                let _ = playback.set_track_mix(
                    _gesture.track_id,
                    _gesture.before_volume_db,
                    _gesture.before_pan,
                );
            }
        }
    }

    fn commit_track_volume_text(&mut self, track_id: TrackId) {
        let Some(text) = self.track_volume_edits.get(&track_id).cloned() else {
            return;
        };
        let Ok(value) = text.trim().parse::<f32>() else {
            self.status = "Enter a valid volume in dB".to_owned();
            return;
        };
        if !value.is_finite() {
            self.status = "Enter a finite volume in dB".to_owned();
            return;
        }
        let value = value.clamp(-60.0, 6.0);
        self.cancel_track_mix_gesture();
        self.track_volume_edits.remove(&track_id);
        if self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            .is_some_and(|track| (track.volume_db() - value).abs() < f32::EPSILON)
        {
            return;
        }
        self.apply_action(
            DawAction::SetTrackVolume {
                track_id,
                volume_db: value,
            },
            "Track volume changed",
        );
    }

    fn commit_track_pan_text(&mut self, track_id: TrackId) {
        let Some(text) = self.track_pan_edits.get(&track_id).cloned() else {
            return;
        };
        let Ok(value) = text.trim().parse::<f32>() else {
            self.status = "Enter a pan value from -1.0 to 1.0".to_owned();
            return;
        };
        if !value.is_finite() {
            self.status = "Enter a finite pan value".to_owned();
            return;
        }
        let value = value.clamp(-1.0, 1.0);
        self.cancel_track_mix_gesture();
        self.track_pan_edits.remove(&track_id);
        if self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            .is_some_and(|track| (track.pan() - value).abs() < f32::EPSILON)
        {
            return;
        }
        self.apply_action(
            DawAction::SetTrackPan {
                track_id,
                pan: value,
            },
            "Track pan changed",
        );
    }

    fn finish_item_drag(&mut self) {
        let preview = self.timeline.drag_preview();
        self.timeline.handle(timeline::TimelineEvent::EndItemDrag);
        let Some(preview) = preview else {
            return;
        };
        if !preview.valid {
            self.status = "Drop rejected: item would leave the project bounds".to_owned();
            return;
        }
        match self.item_drag_action(preview) {
            Ok(Some(action)) => self.apply_action(action, "Items moved"),
            Ok(None) => {}
            Err(error) => self.status = format!("Drop rejected: {error}"),
        }
    }

    fn playback_busy(&self) -> bool {
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        {
            self.playback_busy
        }
        #[cfg(not(any(feature = "jack-backend", feature = "pipewire-backend")))]
        {
            false
        }
    }

    fn playback_active(&self) -> bool {
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        {
            self.playback.is_some()
        }
        #[cfg(not(any(feature = "jack-backend", feature = "pipewire-backend")))]
        {
            false
        }
    }

    fn item_drag_action(
        &self,
        preview: timeline::ItemDragPreview,
    ) -> Result<Option<DawAction>, String> {
        let mut item_ids = self
            .timeline
            .selected_items
            .iter()
            .copied()
            .collect::<Vec<_>>();
        item_ids.sort_unstable_by_key(|item_id| item_id.value());
        let mut actions = Vec::with_capacity(item_ids.len() * 2);
        for item_id in item_ids {
            let (source_track_id, start_tick) = if let Some(item) = self
                .project
                .audio_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                (
                    item.track_id(),
                    self.project
                        .tick_at_sample(item.start_sample())
                        .map_err(|error| error.to_string())?,
                )
            } else if let Some(item) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                (item.track_id(), item.start_tick())
            } else {
                return Err(format!("item {} no longer exists", item_id.value()));
            };
            let source_track_index = self
                .project
                .tracks()
                .iter()
                .position(|track| track.id() == source_track_id)
                .ok_or_else(|| "source track no longer exists".to_owned())?;
            let target_track_index =
                usize::try_from(source_track_index as i128 + i128::from(preview.track_delta))
                    .map_err(|_| "target track is outside the project".to_owned())?;
            let target_track_id = self
                .project
                .tracks()
                .get(target_track_index)
                .map(|track| track.id())
                .ok_or_else(|| "target track is outside the project".to_owned())?;
            let target_start_tick = u64::try_from(i128::from(start_tick) + preview.delta_ticks)
                .map_err(|_| "target position is outside the project".to_owned())?;

            if let Some(item) = self
                .project
                .audio_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                let target_sample = self
                    .project
                    .sample_at_tick(target_start_tick)
                    .map_err(|error| error.to_string())?;
                if target_sample != item.start_sample() {
                    actions.push(DawAction::EditAudioItem {
                        item_id,
                        media_ref: item.media_ref().to_owned(),
                        start_sample: target_sample,
                        source_offset_samples: item.source_offset_samples(),
                        length_samples: item.length_samples(),
                    });
                }
            } else if let Some(item) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
                && target_start_tick != item.start_tick()
            {
                actions.push(DawAction::EditMidiItem {
                    item_id,
                    start_tick: target_start_tick,
                    length_ticks: item.length_ticks(),
                });
            }

            if target_track_id != source_track_id {
                actions.push(DawAction::MoveItemToTrack {
                    item_id,
                    track_id: target_track_id,
                });
            }
        }
        if actions.is_empty() {
            return Ok(None);
        }
        Ok(Some(DawAction::BatchTransaction {
            tx_id: self.revision,
            actions,
        }))
    }

    fn delete_track(&mut self, track_id: TrackId) {
        if self
            .track_mix_gesture
            .is_some_and(|gesture| gesture.track_id == track_id)
        {
            self.cancel_track_mix_gesture();
        }
        self.track_name_edits.remove(&track_id);
        self.track_volume_edits.remove(&track_id);
        self.track_pan_edits.remove(&track_id);
        self.apply_action(DawAction::DeleteTrack { track_id }, "Track deleted");
    }

    fn commit_track_name(&mut self, track_id: TrackId) {
        let Some(name) = self.track_name_edits.get(&track_id).cloned() else {
            return;
        };
        let name = name.trim().to_owned();
        if name.is_empty() {
            self.status = "Track name must not be empty".to_owned();
            return;
        }
        let Some(current_name) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            .map(|track| track.name().to_owned())
        else {
            self.track_name_edits.remove(&track_id);
            self.status = "Track no longer exists".to_owned();
            return;
        };
        self.track_name_edits.remove(&track_id);
        if name == current_name {
            self.status = "Track name unchanged".to_owned();
            return;
        }
        self.apply_action(DawAction::SetTrackName { track_id, name }, "Track renamed");
    }

    fn move_track(&mut self, track_id: TrackId, direction: i8) {
        let Some(index) = self
            .project
            .tracks()
            .iter()
            .position(|track| track.id() == track_id)
        else {
            self.status = "Track no longer exists".to_owned();
            return;
        };
        let target_index = match direction {
            -1 => index.checked_sub(1),
            1 if index + 1 < self.project.tracks().len() => Some(index + 1),
            _ => None,
        };
        let Some(target_index) = target_index else {
            self.status = "Track is already at that end of the list".to_owned();
            return;
        };
        self.apply_action(
            DawAction::MoveTrack {
                track_id,
                index: target_index,
            },
            "Track order changed",
        );
    }

    fn nudge_audio_item(&mut self, item_id: ItemId, direction: i8, milliseconds: u32) {
        self.audio_item_start_edits.remove(&item_id);
        let Some((media_ref, source_offset_samples, length_samples, start_sample)) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            .map(|item| {
                (
                    item.media_ref().to_owned(),
                    item.source_offset_samples(),
                    item.length_samples(),
                    item.start_sample(),
                )
            })
        else {
            self.status = "Audio item no longer exists".to_owned();
            return;
        };
        let delta = (u64::from(self.project.settings().sample_rate()) * u64::from(milliseconds)
            / 1_000)
            .max(1);
        let moved_sample = match direction {
            -1 => start_sample.checked_sub(delta),
            1 => start_sample.checked_add(delta),
            _ => None,
        };
        let Some(start_sample) = moved_sample else {
            self.status = "Audio item cannot move beyond the sample timeline".to_owned();
            return;
        };
        self.apply_action(
            DawAction::EditAudioItem {
                item_id,
                media_ref,
                start_sample,
                source_offset_samples,
                length_samples,
            },
            &format!("Audio item moved by {milliseconds} ms"),
        );
    }

    fn commit_audio_item_start_sample(&mut self, item_id: ItemId) {
        let Some(query) = self.audio_item_start_edits.get(&item_id).cloned() else {
            return;
        };
        let start_sample = match query.trim().parse::<u64>() {
            Ok(sample) => sample,
            Err(_) => {
                self.status = "Enter a non-negative sample position".to_owned();
                return;
            }
        };
        if self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            .is_some_and(|item| item.start_sample() == start_sample)
        {
            self.audio_item_start_edits.remove(&item_id);
            self.status = "Audio item position is unchanged".to_owned();
            return;
        }
        self.audio_item_start_edits.remove(&item_id);
        let action = set_audio_item_start_sample(&self.project, item_id, start_sample);
        self.apply_edit(action, "Audio item moved to exact sample position");
    }

    fn undo(&mut self) {
        if let Some(parameter_id) = self
            .fx_parameter_gesture
            .as_ref()
            .map(|gesture| gesture.parameter_id)
        {
            self.end_fx_parameter_gesture(parameter_id);
        }
        if self.fx_parameter_end_requested || self.pending_fx_parameter_sync.is_some() {
            self.status = "Waiting for the active CLAP parameter update to finish".to_owned();
            return;
        }
        self.audio_item_start_edits.clear();
        self.status = match self.project.undo() {
            Ok(true) => {
                self.midi_note_clipboard.last_paste = None;
                self.revision = self.revision.wrapping_add(1);
                self.timeline.rebuild(&self.project);
                self.sync_fx_parameter_cache_from_project();
                "Action undone".to_owned()
            }
            Ok(false) => "Nothing to undo".to_owned(),
            Err(error) => format!("Undo failed: {error}"),
        };
    }

    fn redo(&mut self) {
        if let Some(parameter_id) = self
            .fx_parameter_gesture
            .as_ref()
            .map(|gesture| gesture.parameter_id)
        {
            self.end_fx_parameter_gesture(parameter_id);
        }
        if self.fx_parameter_end_requested || self.pending_fx_parameter_sync.is_some() {
            self.status = "Waiting for the active CLAP parameter update to finish".to_owned();
            return;
        }
        self.audio_item_start_edits.clear();
        self.status = match self.project.redo() {
            Ok(true) => {
                self.midi_note_clipboard.last_paste =
                    self.midi_editor_item_id.and_then(|item_id| {
                        self.selected_clipboard_paste_start(item_id)
                            .map(|start_tick| (item_id, start_tick))
                    });
                self.revision = self.revision.wrapping_add(1);
                self.timeline.rebuild(&self.project);
                self.sync_fx_parameter_cache_from_project();
                "Action redone".to_owned()
            }
            Ok(false) => "Nothing to redo".to_owned(),
            Err(error) => format!("Redo failed: {error}"),
        };
    }

    fn selected_clipboard_paste_start(&self, item_id: ItemId) -> Option<u64> {
        if self.midi_note_clipboard.notes.is_empty() || self.midi_editor_selected_notes.is_empty() {
            return None;
        }
        let item = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)?;
        let mut selected = item
            .notes()
            .iter()
            .filter(|note| self.midi_editor_selected_notes.contains(&note.id()))
            .collect::<Vec<_>>();
        if selected.len() != self.midi_note_clipboard.notes.len() {
            return None;
        }
        selected.sort_by_key(|note| (note.tick(), note.pitch()));
        let start_tick = selected.iter().map(|note| note.tick()).min()?;
        let matches_clipboard =
            selected
                .iter()
                .zip(&self.midi_note_clipboard.notes)
                .all(|(note, copied)| {
                    note.tick() - start_tick == copied.tick
                        && note.pitch() == copied.pitch
                        && note.duration() == copied.duration
                        && note.velocity() == copied.velocity
                });
        matches_clipboard.then_some(start_tick)
    }

    fn run_action_query(&mut self) -> Task<Message> {
        if let Some(command) = commands::find(self, &self.action_query) {
            self.update(Message::ExecuteCommand(command))
        } else {
            self.status =
                "Unknown action. Search for a command or choose one from the Actions menu."
                    .to_owned();
            Task::none()
        }
    }

    fn save_shortcut_bindings(&mut self) {
        let bindings = match commands::validate_bindings(&self.shortcut_binding_edits) {
            Ok(bindings) => bindings,
            Err(error) => {
                self.status = format!("Shortcut bindings not saved: {error}");
                return;
            }
        };
        match keyboard_config::save(&bindings) {
            Ok(()) => {
                self.shortcut_capture_id = None;
                self.shortcut_defaults_restored.clear();
                *self
                    .shortcut_bindings
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = bindings.clone();
                self.shortcut_binding_edits = bindings;
                self.status = "Keyboard shortcuts saved".to_owned();
                self.shortcut_editor_feedback = "Keyboard shortcuts saved".to_owned();
            }
            Err(error) => self.status = format!("Keyboard shortcuts could not be saved: {error}"),
        }
    }

    fn reset_shortcut_bindings(&mut self) {
        match keyboard_config::reset() {
            Ok(()) => {
                self.shortcut_binding_edits.clear();
                self.shortcut_defaults_restored.clear();
                self.shortcut_capture_id = None;
                self.shortcut_bindings
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clear();
                self.status = "Keyboard shortcuts restored to defaults".to_owned();
                self.shortcut_editor_feedback =
                    "Keyboard shortcuts restored to defaults".to_owned();
            }
            Err(error) => {
                self.status = format!("Keyboard shortcuts could not be reset: {error}");
            }
        }
    }

    fn selected_track_id(&self) -> Option<TrackId> {
        self.timeline.selected_track.filter(|track_id| {
            self.project
                .tracks()
                .iter()
                .any(|track| track.id() == *track_id)
        })
    }

    fn begin_track_name_edit(&mut self, track_id: TrackId) -> Task<Message> {
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            return Task::none();
        };
        self.track_name_edits
            .insert(track_id, track.name().to_owned());
        self.timeline.selected_track = Some(track_id);
        let input_id = messages::track_name_input_id(track_id);
        Task::batch([
            iced::widget::operation::focus(input_id.clone()),
            iced::widget::operation::move_cursor_to_end(input_id),
        ])
    }

    pub(super) fn can_split_selected_items_at_cursor(&self) -> bool {
        self.selected_item_split_action(false, None)
            .is_ok_and(|action| action.is_some())
    }

    pub(super) fn can_split_selected_items_at_time_selection(&self) -> bool {
        self.selected_item_split_action(true, None)
            .is_ok_and(|action| action.is_some())
    }

    fn selected_item_split_action(
        &self,
        use_time_selection: bool,
        source_sample_rates: Option<&HashMap<String, u32>>,
    ) -> Result<Option<DawAction>, String> {
        let boundaries = if use_time_selection {
            let Some(selection) = self.timeline.time_selection else {
                return Ok(None);
            };
            vec![selection.start_tick, selection.end_tick]
        } else {
            vec![self.timeline.edit_cursor_tick]
        };
        let mut selected_item_ids = self
            .timeline
            .selected_items
            .iter()
            .copied()
            .collect::<Vec<_>>();
        selected_item_ids.sort_unstable_by_key(|item_id| item_id.value());

        let mut actions = Vec::new();
        for item_id in selected_item_ids {
            if let Some(item) = self
                .project
                .audio_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                let source_sample_rate = source_sample_rates
                    .and_then(|rates| rates.get(item.media_ref()).copied())
                    .unwrap_or_else(|| self.project.settings().sample_rate());
                split_audio_item_actions(
                    &self.project,
                    item,
                    &boundaries,
                    source_sample_rate,
                    &mut actions,
                )?;
            } else if let Some(item) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                split_midi_item_actions(item, &boundaries, &mut actions)?;
            }
        }
        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            }))
        }
    }

    fn selected_audio_source_sample_rates(
        &self,
        use_time_selection: bool,
    ) -> Result<HashMap<String, u32>, String> {
        let boundaries = if use_time_selection {
            let Some(selection) = self.timeline.time_selection else {
                return Ok(HashMap::new());
            };
            vec![selection.start_tick, selection.end_tick]
        } else {
            vec![self.timeline.edit_cursor_tick]
        };
        let mut media_refs = self
            .project
            .audio_items()
            .iter()
            .filter(|item| self.timeline.selected_items.contains(&item.id()))
            .filter(|item| {
                boundaries.iter().any(|tick| {
                    self.project.sample_at_tick(*tick).is_ok_and(|sample| {
                        item.start_sample() < sample && sample < item.end_sample()
                    })
                })
            })
            .map(|item| item.media_ref().to_owned())
            .collect::<Vec<_>>();
        media_refs.sort_unstable();
        media_refs.dedup();
        if media_refs.is_empty() {
            return Ok(HashMap::new());
        }

        let path = self
            .project_path
            .as_deref()
            .filter(|path| path.is_file())
            .ok_or_else(|| "save the project before splitting audio items".to_owned())?;
        let store = ProjectStore::open(path).map_err(|error| error.to_string())?;
        let mut rates = HashMap::with_capacity(media_refs.len());
        let mut metadata_result = Ok(());
        for media_ref in media_refs {
            match store.audio_asset_metadata(&media_ref) {
                Ok(Some(metadata)) => match metadata.sample_rate.filter(|rate| *rate > 0) {
                    Some(rate) => {
                        rates.insert(media_ref, rate);
                    }
                    None => {
                        metadata_result =
                            Err(format!("source sample rate is unavailable for {media_ref}"));
                        break;
                    }
                },
                Ok(None) => {
                    metadata_result =
                        Err(format!("source metadata is unavailable for {media_ref}"));
                    break;
                }
                Err(error) => {
                    metadata_result = Err(error.to_string());
                    break;
                }
            }
        }
        let close_result = store.close().map_err(|error| error.to_string());
        metadata_result?;
        close_result?;
        Ok(rates)
    }

    fn start_audio_waveform_scan(&mut self, reset: bool) {
        if let Some(worker) = self.audio_waveform_worker.take() {
            worker.cancel();
        }
        if reset {
            self.audio_waveforms.clear();
            self.timeline
                .set_audio_waveforms(&self.project, HashMap::new());
        }
        let Some(path) = self.project_path.as_deref().filter(|path| path.is_file()) else {
            return;
        };
        let media_refs = self
            .project
            .audio_items()
            .iter()
            .map(|item| item.media_ref().to_owned())
            .filter(|media_ref| !self.audio_waveforms.contains_key(media_ref))
            .collect::<Vec<_>>();
        if media_refs.is_empty() {
            return;
        }
        match AudioWaveformWorker::start(path.to_owned(), media_refs) {
            Ok(worker) => self.audio_waveform_worker = Some(worker),
            Err(error) => self.status = format!("Waveform scan could not start: {error}"),
        }
    }

    fn update_audio_waveforms(&mut self) {
        let Some(worker) = &self.audio_waveform_worker else {
            return;
        };
        let results = worker.results();
        let finished = worker.is_finished();
        let mut changed = false;
        for AudioWaveformResult {
            media_ref,
            waveform,
        } in results
        {
            match waveform {
                Ok(waveform) => {
                    self.audio_waveforms.insert(media_ref, waveform);
                    changed = true;
                }
                Err(error) => {
                    self.status = format!("Waveform unavailable: {error}");
                }
            }
        }
        if changed {
            self.timeline
                .set_audio_waveforms(&self.project, self.audio_waveforms.clone());
        }
        if finished {
            let worker = self
                .audio_waveform_worker
                .take()
                .expect("finished waveform worker is present");
            if let Err(error) = worker.join() {
                self.status = format!("Waveform scan failed: {error}");
            }
        }
    }

    fn split_selected_items(&mut self, use_time_selection: bool) {
        let source_sample_rates = match self.selected_audio_source_sample_rates(use_time_selection)
        {
            Ok(rates) => rates,
            Err(error) => {
                self.status = format!("Split failed: {error}");
                return;
            }
        };
        let action =
            match self.selected_item_split_action(use_time_selection, Some(&source_sample_rates)) {
                Ok(Some(action)) => action,
                Ok(None) => {
                    self.status = "No selected item crosses the split point".to_owned();
                    return;
                }
                Err(error) => {
                    self.status = format!("Split failed: {error}");
                    return;
                }
            };
        let previous_ids = self
            .project
            .audio_items()
            .iter()
            .map(|item| item.id())
            .chain(self.project.midi_items().iter().map(|item| item.id()))
            .collect::<HashSet<_>>();
        let previous_revision = self.revision;
        self.apply_action(action, "Selected items split");
        if self.revision == previous_revision {
            return;
        }
        let new_ids = self
            .project
            .audio_items()
            .iter()
            .map(|item| item.id())
            .chain(self.project.midi_items().iter().map(|item| item.id()))
            .filter(|item_id| !previous_ids.contains(item_id))
            .collect::<Vec<_>>();
        self.timeline.selected_items.extend(new_ids);
        self.audio_item_start_edits.clear();
    }

    fn delete_selected_items(&mut self) {
        let mut item_ids = self
            .timeline
            .selected_items
            .iter()
            .copied()
            .collect::<Vec<_>>();
        item_ids.sort_unstable_by_key(|item_id| item_id.value());
        if item_ids.is_empty() {
            self.status = "Select one or more items to delete".to_owned();
            return;
        }

        let mut actions = Vec::with_capacity(item_ids.len());
        for item_id in item_ids {
            if self
                .project
                .audio_items()
                .iter()
                .any(|item| item.id() == item_id)
            {
                self.audio_item_start_edits.remove(&item_id);
                actions.push(DawAction::DeleteAudioItem { item_id });
            } else if self
                .project
                .midi_items()
                .iter()
                .any(|item| item.id() == item_id)
            {
                actions.push(DawAction::DeleteMidiItem { item_id });
            }
        }
        if actions.is_empty() {
            self.status = "Selected items no longer exist".to_owned();
            return;
        }
        let action = if actions.len() == 1 {
            actions.pop().expect("single delete action is present")
        } else {
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            }
        };
        self.apply_action(action, "Selected items deleted");
    }

    fn save_project_command(&mut self) -> Task<Message> {
        if self.project_path.is_none() {
            self.pick_path(PathPickerTarget::SaveProject)
        } else {
            project_io::save_project(self, None)
        }
    }

    fn open_project_command(&mut self) -> Task<Message> {
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        if self.playback.is_some() {
            self.status = format!(
                "Close {} output before opening another project",
                self.playback_name()
            );
            return Task::none();
        }
        if self.io_busy {
            self.status = "Wait for current project operation to finish".to_owned();
            return Task::none();
        }
        if self.is_dirty() {
            self.status = "Save current project before opening another".to_owned();
            return Task::none();
        }
        self.pick_path(PathPickerTarget::OpenProject)
    }
}

fn snap_tick_up(tick: u64, grid_ticks: u64) -> u64 {
    let grid_ticks = grid_ticks.max(1);
    let quotient = tick / grid_ticks + u64::from(tick % grid_ticks != 0);
    quotient.saturating_mul(grid_ticks)
}

fn split_audio_item_actions(
    project: &Project,
    item: &AudioItem,
    boundaries: &[u64],
    source_sample_rate: u32,
    actions: &mut Vec<DawAction>,
) -> Result<(), String> {
    let mut cut_samples = boundaries
        .iter()
        .filter_map(|tick| project.sample_at_tick(*tick).ok())
        .filter(|sample| item.start_sample() < *sample && *sample < item.end_sample())
        .collect::<Vec<_>>();
    cut_samples.sort_unstable();
    cut_samples.dedup();
    if cut_samples.is_empty() {
        return Ok(());
    }

    let mut points = Vec::with_capacity(cut_samples.len() + 2);
    points.push(item.start_sample());
    points.extend(cut_samples);
    points.push(item.end_sample());
    for (index, segment) in points.windows(2).enumerate() {
        let start_sample = segment[0];
        let length_samples = segment[1] - segment[0];
        let project_sample_rate = project.settings().sample_rate();
        let source_delta = u64::try_from(
            (u128::from(start_sample - item.start_sample()) * u128::from(source_sample_rate)
                + u128::from(project_sample_rate) / 2)
                / u128::from(project_sample_rate),
        )
        .map_err(|_| "audio source offset exceeds the supported range".to_owned())?;
        let source_offset_samples = item
            .source_offset_samples()
            .checked_add(source_delta)
            .ok_or_else(|| "audio source offset exceeds the supported range".to_owned())?;
        if index == 0 {
            actions.push(DawAction::EditAudioItem {
                item_id: item.id(),
                media_ref: item.media_ref().to_owned(),
                start_sample: item.start_sample(),
                source_offset_samples,
                length_samples,
            });
        } else {
            actions.push(DawAction::InsertAudioItem {
                track_id: item.track_id(),
                media_ref: item.media_ref().to_owned(),
                start_sample,
                source_offset_samples,
                length_samples,
            });
        }
    }
    Ok(())
}

fn split_midi_item_actions(
    item: &MidiItem,
    boundaries: &[u64],
    actions: &mut Vec<DawAction>,
) -> Result<(), String> {
    let item_end = item
        .start_tick()
        .checked_add(item.length_ticks())
        .ok_or_else(|| "MIDI item end exceeds the supported range".to_owned())?;
    let mut split_ticks = boundaries
        .iter()
        .copied()
        .filter(|tick| item.start_tick() < *tick && *tick < item_end)
        .collect::<Vec<_>>();
    split_ticks.sort_unstable();
    split_ticks.dedup();
    if !split_ticks.is_empty() {
        actions.push(DawAction::SplitMidiItem {
            item_id: item.id(),
            split_ticks,
        });
    }
    Ok(())
}

fn item_drag_edit_guard_status(
    path_picker_busy: bool,
    import_busy: bool,
    asset_management_busy: bool,
    playback_busy: bool,
    playback_active: bool,
    io_busy: bool,
) -> Option<&'static str> {
    if path_picker_busy {
        Some("Wait for the file dialog to finish")
    } else if import_busy {
        Some("Wait for audio import to finish or cancel it")
    } else if asset_management_busy {
        Some("Wait for audio asset maintenance to finish or cancel it")
    } else if playback_busy {
        Some("Wait for playback preparation to finish")
    } else if playback_active {
        Some("Close audio output before editing the project")
    } else if io_busy {
        Some("Wait for current project operation to finish")
    } else {
        None
    }
}

fn project_path_from_query(query: &str) -> Option<PathBuf> {
    let query = query.trim();
    (!query.is_empty()).then(|| PathBuf::from(query))
}

fn scroll_arrangement_to(target: &'static str, offset_y: f32) -> Task<Message> {
    use iced::advanced::widget::operation::scrollable::{self, AbsoluteOffset};

    let target = iced::widget::Id::new(target);
    let offset = AbsoluteOffset {
        x: None,
        y: Some(offset_y.max(0.0)),
    };
    iced::advanced::widget::operate(scrollable::scroll_to(target, offset))
}

/// Runs blocking project storage work away from the Iced update thread.
#[allow(clippy::unused_async)]
async fn run_blocking<T: Send + 'static>(
    name: &'static str,
    operation: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(operation)
        .map_err(|error| format!("could not start worker: {error}"))?
        .join()
        .map_err(|_| format!("{name} worker panicked"))?
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
fn prepare_project_playback_file(
    path: PathBuf,
    snapshot: ProjectSnapshot,
    target_sample: u64,
) -> Result<PreparedAudioPlayback, String> {
    if !path.is_file() {
        return Err(format!("project file {} does not exist", path.display()));
    }
    let project = Project::from_snapshot(snapshot).map_err(|error| error.to_string())?;
    let store = ProjectStore::open(path).map_err(|error| error.to_string())?;
    let prepared = prepare_audio_playback_at(&project, &store, target_sample, 16_384, 8_192)
        .map_err(|error: PlaybackBuildError| error.to_string());
    let close = store.close().map_err(|error| error.to_string());
    let prepared = prepared?;
    close?;
    Ok(prepared)
}

fn runtime_keyboard_event(
    event: iced::Event,
    status: iced::event::Status,
    window_id: iced::window::Id,
) -> Option<Message> {
    matches!(event, iced::Event::Keyboard(_))
        .then_some(Message::RuntimeKeyboardEvent(event, status, window_id))
}

fn plugin_window_escape_message(
    event: &iced::Event,
    _status: iced::event::Status,
    window_id: iced::window::Id,
    fx_chain_window_id: Option<iced::window::Id>,
    plugin_picker_window_id: Option<iced::window::Id>,
) -> Option<Message> {
    let is_escape = matches!(
        event,
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            modifiers: iced::keyboard::Modifiers::NONE,
            repeat: false,
            ..
        })
    );
    if !is_escape {
        return None;
    }
    if plugin_picker_window_id == Some(window_id) {
        Some(Message::ClosePluginPicker)
    } else if fx_chain_window_id == Some(window_id) {
        Some(Message::CloseTrackFxChain)
    } else {
        None
    }
}

fn keyboard_shortcut_event(
    event: iced::Event,
    status: iced::event::Status,
    window_id: iced::window::Id,
    main_window_id: Option<iced::window::Id>,
    settings_window_id: Option<iced::window::Id>,
    shortcut_capture_id: Option<&str>,
) -> Option<Message> {
    if settings_window_id == Some(window_id)
        && let Some(action_id) = shortcut_capture_id
    {
        let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key,
            modifiers,
            repeat: false,
            ..
        }) = event
        else {
            return None;
        };
        return match key.as_ref() {
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => {
                Some(Message::CancelShortcutCapture)
            }
            iced::keyboard::Key::Named(
                iced::keyboard::key::Named::Backspace | iced::keyboard::key::Named::Delete,
            ) => Some(Message::ClearShortcutBinding(action_id.to_owned())),
            iced::keyboard::Key::Character(character) => Some(Message::ShortcutCaptureKey {
                action_id: action_id.to_owned(),
                key: character.to_owned(),
                modifiers,
            }),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Space) => {
                Some(Message::ShortcutCaptureKey {
                    action_id: action_id.to_owned(),
                    key: "Space".to_owned(),
                    modifiers,
                })
            }
            iced::keyboard::Key::Named(named) => Some(Message::ShortcutCaptureKey {
                action_id: action_id.to_owned(),
                key: format!("{named:?}"),
                modifiers,
            }),
            iced::keyboard::Key::Unidentified => Some(Message::ShortcutCaptureKey {
                action_id: action_id.to_owned(),
                key: "Unidentified".to_owned(),
                modifiers,
            }),
        };
    }
    if main_window_id != Some(window_id) {
        return None;
    }
    let is_escape = matches!(
        &event,
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            modifiers: iced::keyboard::Modifiers::NONE,
            repeat: false,
            ..
        })
    );
    if status != iced::event::Status::Ignored && !is_escape {
        return None;
    }
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat: false,
        ..
    }) = event
    else {
        return None;
    };
    match key.as_ref() {
        iced::keyboard::Key::Character(character) => {
            Some(Message::ShortcutPressed(character.to_owned(), modifiers))
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Space) => {
            Some(Message::ShortcutPressed(" ".to_owned(), modifiers))
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => Some(Message::Escape),
        _ => None,
    }
}

#[cfg(test)]
fn shortcut_message(
    key: iced::keyboard::Key<&str>,
    modifiers: iced::keyboard::Modifiers,
    bindings: &commands::ShortcutBindings,
) -> Option<Message> {
    if modifiers == iced::keyboard::Modifiers::NONE
        && key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
    {
        return Some(Message::Escape);
    }
    commands::from_shortcut(&key, modifiers, bindings).map(Message::ExecuteCommand)
}

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf())
        == std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf())
}
