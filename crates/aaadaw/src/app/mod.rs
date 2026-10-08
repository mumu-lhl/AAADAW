use crate::timeline::{self, TimelineState};
use aaadaw_app::{
    AudioAssetManagementOperation, AudioAssetManagementWorker, AudioAssetSourceStatusEntry,
    AudioItemImportWorker, AudioWaveformResult, AudioWaveformWorker, ClapPluginScanReport,
    WavExportOptions, add_quarter_note, adjust_midi_note_pitch, adjust_midi_note_velocity,
    default_clap_search_paths, delete_midi_note, duplicate_audio_item, move_midi_item_by_beat,
    move_midi_note_by_sixteenth, quantize_midi_item_to_sixteenth, set_audio_item_start_sample,
};
#[cfg(feature = "audio-device")]
use aaadaw_app::{AudioCaptureControl, AudioRecordingWorker, RunningAudioInput};
#[cfg(feature = "audio-device")]
use aaadaw_app::{
    PlaybackBackend, PlaybackBuildError, PreparedAudioPlayback, RunningAudioPlayback,
    prepare_audio_playback_at,
};
#[cfg(feature = "audio-device")]
use aaadaw_core::ProjectSnapshot;
use aaadaw_core::{
    AudioItem, DawAction, FxParameterChange, ItemId, MeterPointSnapshot, MidiItem, Project,
    TempoCurve, TimeSignature, TrackId,
};
#[cfg(feature = "audio-device")]
use aaadaw_engine::{
    ClapEffectOwner, ClapInstrumentHelperProcess, ClapInstrumentOwner,
    ClapParameterAutomationReceiver, ClapParameterSender,
};
use aaadaw_engine::{ClapParameterInfo, ClapPluginGuiOwner};
use aaadaw_media::AudioWaveform;
use aaadaw_storage::{ArrangementViewState, ProjectSessionLock, ProjectStore};
use commands::CommandId;
use iced::Task;
use iced::widget::pane_grid::{self, Axis, Split};
use std::collections::{HashMap, HashSet, VecDeque};
#[cfg(feature = "audio-device")]
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub(super) const MIDI_EDITOR_KEY_WIDTH: f32 = 84.0;
pub(super) const MIDI_EDITOR_CONTENT_WIDTH_INSET: f32 = MIDI_EDITOR_KEY_WIDTH + 32.0;

mod action_macros;
mod audio_config;
mod audio_export;
mod clap_plugin_cache;
mod clap_plugin_config;
mod clap_plugin_settings;
#[cfg(feature = "audio-device")]
mod clap_plugin_state;
mod clap_track_fx;
mod clap_track_instrument;
mod commands;
mod config_paths;
mod keyboard_config;
mod media;
mod messages;
mod offline_job_queue;
mod project_io;
#[cfg(feature = "audio-device")]
mod recording;
mod recording_recovery;
#[cfg(test)]
mod tests;
mod view;
mod x11_plugin_editor;

pub(crate) use messages::{
    MainMenu, MainWorkspace, MenuNavigation, Message, MidiEditorLane, PathPickerTarget,
    PendingProjectTransition, SettingsCategory, TimeMapTab,
};

pub(crate) fn run() -> iced::Result {
    iced::daemon(App::new, App::update, view::view_for_window)
        .title(App::window_title)
        .theme(|_: &App, _| iced::Theme::Dark)
        .subscription(App::subscription)
        .run()
}

#[cfg(feature = "audio-device")]
#[derive(Default)]
struct ClapEffectOwners(HashMap<u64, ClapEffectOwner>);

#[cfg(feature = "audio-device")]
#[derive(Default)]
struct ClapInstrumentOwners(HashMap<u64, ClapInstrumentOwner>);

#[cfg(feature = "audio-device")]
#[derive(Default)]
struct ClapInstrumentHelperOwners(HashMap<u64, ClapInstrumentHelperProcess>);

#[cfg(feature = "audio-device")]
impl Deref for ClapEffectOwners {
    type Target = HashMap<u64, ClapEffectOwner>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "audio-device")]
impl DerefMut for ClapEffectOwners {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(feature = "audio-device")]
impl Drop for ClapEffectOwners {
    fn drop(&mut self) {
        for owner in self.0.values_mut() {
            let _ = owner.try_deactivate_unused();
        }
    }
}

#[cfg(feature = "audio-device")]
impl Deref for ClapInstrumentOwners {
    type Target = HashMap<u64, ClapInstrumentOwner>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "audio-device")]
impl DerefMut for ClapInstrumentOwners {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(feature = "audio-device")]
impl Drop for ClapInstrumentOwners {
    fn drop(&mut self) {
        for owner in self.0.values_mut() {
            let _ = owner.try_deactivate_unused();
        }
    }
}

#[cfg(feature = "audio-device")]
impl Deref for ClapInstrumentHelperOwners {
    type Target = HashMap<u64, ClapInstrumentHelperProcess>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "audio-device")]
impl DerefMut for ClapInstrumentHelperOwners {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(feature = "audio-device")]
impl Drop for ClapInstrumentHelperOwners {
    fn drop(&mut self) {
        for owner in self.0.values_mut() {
            let _ = owner.shutdown();
            let _ = owner.take_saved_state();
        }
    }
}

#[derive(Default)]
struct MidiNoteClipboard {
    notes: Vec<aaadaw_core::MidiNoteData>,
    span_ticks: u64,
    default_paste_tick: Option<u64>,
    last_paste: Option<(ItemId, u64)>,
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos")
))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct CpalDeviceChoice {
    id: Option<String>,
    label: String,
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos")
))]
impl std::fmt::Display for CpalDeviceChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct StereoPeakHold {
    levels: [f32; 2],
    clipped: [bool; 2],
}

impl StereoPeakHold {
    #[cfg(feature = "audio-device")]
    fn observe(&mut self, peaks: [f32; 2]) {
        for (channel, peak) in peaks.into_iter().enumerate() {
            let peak = if peak.is_finite() { peak.max(0.0) } else { 0.0 };
            self.levels[channel] = self.levels[channel].max(peak);
            self.clipped[channel] |= peak >= 1.0;
        }
    }
}

struct UnsavedSessionMedia {
    directory: PathBuf,
    lock: Option<ProjectSessionLock>,
}

impl UnsavedSessionMedia {
    fn store_path(&self) -> PathBuf {
        self.directory.join("session.aaadaw")
    }
}

impl Drop for UnsavedSessionMedia {
    fn drop(&mut self) {
        drop(self.lock.take());
        if let Err(error) = std::fs::remove_dir_all(&self.directory)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(path = %self.directory.display(), %error, "could not remove unsaved session media");
        }
    }
}

#[derive(Default)]
struct App {
    project: Project,
    action_query: String,
    action_macros: Vec<action_macros::ActionMacro>,
    action_macro_name: String,
    action_macro_step: Option<String>,
    action_macro_steps: Vec<String>,
    action_macro_editing_id: Option<u64>,
    action_macro_feedback: String,
    action_macro_config_error: Option<String>,
    shortcut_bindings: Arc<std::sync::RwLock<commands::ShortcutBindings>>,
    shortcut_binding_edits: commands::ShortcutBindings,
    shortcut_defaults_restored: HashSet<String>,
    media_panel_dock: MediaPanelDock,
    main_workspace: MainWorkspace,
    project_path_query: String,
    project_path: Option<PathBuf>,
    session_media_dir: Option<UnsavedSessionMedia>,
    retired_unsaved_sessions: Vec<UnsavedSessionMedia>,
    unsaved_session_recovery_candidates: Vec<PathBuf>,
    unsaved_session_recovery_busy: bool,
    unsaved_session_snapshot_at: Option<Instant>,
    unsaved_session_snapshot_busy: bool,
    unsaved_session_snapshot_revision: u64,
    pending_project_transition: Option<PendingProjectTransition>,
    project_lock: Option<ProjectSessionLock>,
    track_name_edits: HashMap<TrackId, String>,
    midi_item_name_edits: HashMap<ItemId, String>,
    midi_item_name_errors: HashMap<ItemId, String>,
    track_volume_edits: HashMap<TrackId, String>,
    track_pan_edits: HashMap<TrackId, String>,
    active_track_draft: Option<(TrackId, TrackDraftField)>,
    track_draft_errors: HashMap<(TrackId, TrackDraftField), String>,
    track_mix_gesture: Option<TrackMixGesture>,
    track_mix_commit_at: Option<Instant>,
    track_peak_levels: HashMap<TrackId, [f32; 2]>,
    master_peak_level: [f32; 2],
    track_peak_holds: HashMap<TrackId, StereoPeakHold>,
    input_peak_level: [f32; 2],
    input_peak_hold: StereoPeakHold,
    master_peak_hold: StereoPeakHold,
    master_guard_ticks_remaining: u8,
    audio_item_start_edits: HashMap<ItemId, String>,
    active_menu: Option<MainMenu>,
    menu_selected_command: Option<CommandId>,
    action_menu_scroll_offset: f32,
    keyboard_modifiers: iced::keyboard::Modifiers,
    main_window_id: Option<iced::window::Id>,
    settings_window_id: Option<iced::window::Id>,
    render_window_id: Option<iced::window::Id>,
    tempo_map_window_id: Option<iced::window::Id>,
    time_map_tab: TimeMapTab,
    tempo_map_edits: Vec<TempoMapEdit>,
    tempo_map_feedback: String,
    meter_map_edits: Vec<MeterMapEdit>,
    meter_map_feedback: String,
    fx_chain_window_id: Option<iced::window::Id>,
    fx_chain_track_id: Option<TrackId>,
    fx_chain_selected_index: Option<usize>,
    fx_chain_drag_index: Option<usize>,
    fx_chain_drag_target_index: Option<usize>,
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
    midi_expression_context_menu_epoch: u64,
    midi_editor_item_id: Option<ItemId>,
    midi_editor_feedback: Option<String>,
    midi_editor_selected_notes: HashSet<aaadaw_core::NoteId>,
    midi_editor_lane: MidiEditorLane,
    midi_editor_origin_tick: u64,
    midi_editor_edit_cursor_tick: Option<u64>,
    midi_editor_high_pitch: u8,
    midi_editor_pitch_rows: u8,
    midi_editor_pitch_row_height: f32,
    midi_editor_pixels_per_beat: f32,
    midi_editor_follow_playhead: bool,
    midi_editor_window_size: iced::Size,
    midi_note_clipboard: MidiNoteClipboard,
    plugin_picker_window_id: Option<iced::window::Id>,
    plugin_picker_track_id: Option<TrackId>,
    plugin_picker_instrument_track_id: Option<TrackId>,
    plugin_picker_search: String,
    shortcut_capture_id: Option<String>,
    shortcut_editor_feedback: String,
    settings_category: SettingsCategory,
    audio_settings: audio_config::AudioSettings,
    audio_recording_offset_query: Option<String>,
    audio_settings_feedback: String,
    #[cfg(feature = "audio-device")]
    last_audio_backend_failure: Option<(PlaybackBackend, String)>,
    #[cfg(all(target_os = "linux", feature = "audio-device"))]
    linux_audio_route_report: Option<(
        PlaybackBackend,
        Result<aaadaw_engine::AudioRouteSnapshot, String>,
    )>,
    #[cfg(all(target_os = "linux", feature = "audio-device"))]
    linux_audio_routes_busy: bool,
    #[cfg(all(target_os = "linux", feature = "audio-device"))]
    linux_audio_routes_generation: u64,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    cpal_output_devices: Vec<aaadaw_engine::CpalOutputDeviceInfo>,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    cpal_output_devices_loading: bool,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    cpal_output_devices_error: Option<String>,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    cpal_input_devices: Vec<aaadaw_engine::CpalInputDeviceInfo>,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    cpal_input_devices_loading: bool,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    cpal_input_devices_error: Option<String>,
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
    offline_render_busy: bool,
    offline_render_is_freeze: bool,
    active_offline_job: Option<offline_job_queue::QueuedOfflineJob<audio_export::OfflineRenderJob>>,
    offline_job_queue: offline_job_queue::OfflineJobQueue<audio_export::OfflineRenderJob>,
    offline_job_history: VecDeque<String>,
    offline_jobs_panel_open: bool,
    transport_details_open: bool,
    wav_export_options: WavExportOptions,
    offline_render_cancel: Option<Arc<AtomicBool>>,
    offline_render_progress: Option<Arc<Mutex<(u64, u64)>>>,
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
    #[cfg(feature = "audio-device")]
    recording: Option<ActiveRecording>,
    #[cfg(feature = "audio-device")]
    pending_recording: Option<ActiveRecording>,
    #[cfg(feature = "audio-device")]
    recording_starting: bool,
    #[cfg(feature = "audio-device")]
    recording_cancel_requested: bool,
    #[cfg(feature = "audio-device")]
    recording_stopping: bool,
    #[cfg(feature = "audio-device")]
    recording_cancelled_transport_start: bool,
    #[cfg(feature = "audio-device")]
    recording_start_sample: u64,
    #[cfg(feature = "audio-device")]
    recording_tracks: Vec<TrackId>,
    #[cfg(feature = "audio-device")]
    standby_monitor_track: Option<TrackId>,
    #[cfg(feature = "audio-device")]
    standby_monitor_starting: bool,
    #[cfg(feature = "audio-device")]
    standby_monitor_generation: u64,
    record_import_tracks: Option<RecordImportTarget>,
    status: String,
    #[cfg(feature = "audio-device")]
    playback: Option<RunningAudioPlayback>,
    #[cfg(feature = "audio-device")]
    playback_graph_dirty: bool,
    #[cfg(feature = "audio-device")]
    playback_position_dirty: bool,
    #[cfg(feature = "audio-device")]
    playback_busy: bool,
    #[cfg(feature = "audio-device")]
    playback_playing: bool,
    #[cfg(feature = "audio-device")]
    playback_paused: bool,
    #[cfg(feature = "audio-device")]
    playback_start_sample: u64,
    #[cfg(feature = "audio-device")]
    playhead_sample: u64,
    #[cfg(feature = "audio-device")]
    seek_sample_query: String,
    #[cfg(any(
        all(feature = "jack-backend", feature = "pipewire-backend"),
        all(
            feature = "jack-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        ),
        all(
            feature = "pipewire-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        )
    ))]
    playback_backend: PlaybackBackend,
    #[cfg(feature = "audio-device")]
    clap_effect_owners: ClapEffectOwners,
    #[cfg(feature = "audio-device")]
    clap_effect_parameter_senders: HashMap<u64, ClapParameterSender>,
    #[cfg(feature = "audio-device")]
    clap_effect_automation_receivers: HashMap<u64, ClapParameterAutomationReceiver>,
    #[cfg(feature = "audio-device")]
    clap_effect_targets: HashMap<u64, (TrackId, usize, String)>,
    #[cfg(feature = "audio-device")]
    clap_effect_parameter_targets: HashMap<u64, (TrackId, usize)>,
    #[cfg(feature = "audio-device")]
    fx_automation_write_target: Option<(TrackId, usize, u32)>,
    #[cfg(feature = "audio-device")]
    fx_automation_instance_id: Option<u64>,
    #[cfg(feature = "audio-device")]
    fx_automation_disarm_command_queued: bool,
    #[cfg(feature = "audio-device")]
    fx_automation_disarming: bool,
    #[cfg(feature = "audio-device")]
    fx_automation_finish_requested: bool,
    #[cfg(feature = "audio-device")]
    fx_automation_capture: Vec<aaadaw_core::FxParameterAutomationPoint>,
    #[cfg(feature = "audio-device")]
    fx_automation_capture_overflowed: bool,
    #[cfg(feature = "audio-device")]
    fx_automation_parameter_change: Option<FxAutomationParameterChange>,
    #[cfg(feature = "audio-device")]
    pending_fx_automation_history: Option<FxAutomationHistoryAction>,
    pending_fx_parameter_sync: Option<FxParameterChange>,
    #[cfg(feature = "audio-device")]
    clap_effect_state_overrides: HashSet<(TrackId, usize, String)>,
    #[cfg(feature = "audio-device")]
    clap_instrument_owners: ClapInstrumentOwners,
    #[cfg(feature = "audio-device")]
    clap_instrument_helper_owners: ClapInstrumentHelperOwners,
    #[cfg(feature = "audio-device")]
    clap_instrument_targets: HashMap<u64, TrackId>,
    #[cfg(feature = "audio-device")]
    clap_instrument_helper_targets: HashMap<u64, (TrackId, String)>,
    #[cfg(feature = "audio-device")]
    clap_plugin_warnings: Vec<String>,
}

#[derive(Clone, Debug)]
struct TempoMapEdit {
    original_tick: Option<u64>,
    tick: String,
    bpm: String,
    curve: TempoCurve,
}

#[derive(Clone, Debug)]
struct MeterMapEdit {
    original_tick: Option<u64>,
    tick: String,
    numerator: String,
    denominator: String,
}

#[cfg(feature = "audio-device")]
fn action_rebuilds_playback_graph(action: &DawAction) -> bool {
    match action {
        DawAction::SetTrackFxChain { .. }
        | DawAction::SetTempo { .. }
        | DawAction::DeleteTempoPoint { .. }
        | DawAction::SetTempoCurve { .. }
        | DawAction::SetTrackFxParameterAutomation { .. }
        | DawAction::FreezeTrack { .. }
        | DawAction::UnfreezeTrack { .. } => true,
        DawAction::BatchTransaction { actions, .. } => {
            actions.iter().any(action_rebuilds_playback_graph)
        }
        _ => false,
    }
}

#[cfg(feature = "audio-device")]
fn fx_parameter_automation_schedule(
    project: &Project,
) -> Vec<(
    TrackId,
    usize,
    String,
    Vec<aaadaw_core::FxParameterAutomationLane>,
)> {
    project
        .tracks()
        .iter()
        .flat_map(|track| {
            track
                .fx_chain()
                .iter()
                .enumerate()
                .filter(|(_, plugin)| !plugin.parameter_automation().is_empty())
                .map(|(chain_index, plugin)| {
                    (
                        track.id(),
                        chain_index,
                        plugin.plugin_id().to_owned(),
                        plugin.parameter_automation().to_vec(),
                    )
                })
        })
        .collect()
}

#[derive(Debug)]
struct FxParameterGesture {
    track_id: TrackId,
    chain_index: usize,
    parameter_id: u32,
    before: f64,
    after: f64,
    before_state: Option<Vec<u8>>,
    #[cfg(feature = "audio-device")]
    write_armed: bool,
}

#[cfg(feature = "audio-device")]
#[derive(Debug)]
struct FxAutomationParameterChange {
    track_id: TrackId,
    chain_index: usize,
    parameter_id: u32,
    before: f64,
    after: f64,
    before_state: Option<Vec<u8>>,
    after_state: Option<Vec<u8>>,
}

#[cfg(feature = "audio-device")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FxAutomationHistoryAction {
    Undo,
    Redo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrackMixParameter {
    Volume,
    Pan,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum TrackDraftField {
    Name,
    Volume,
    Pan,
}

impl TrackDraftField {
    fn label(self) -> &'static str {
        match self {
            Self::Name => "track name",
            Self::Volume => "track volume",
            Self::Pan => "track pan",
        }
    }
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

#[cfg(feature = "audio-device")]
struct ActiveRecording {
    input: RunningAudioInput,
    writer: AudioRecordingWorker,
    control: AudioCaptureControl,
    placement_correction: audio_config::RecordingPlacementCorrection,
    capture_timeline_anchor: Option<aaadaw_app::CaptureTimelineAnchor>,
    recovery_manifest_path: PathBuf,
}

struct RecordImportTarget {
    track_ids: Vec<TrackId>,
    recreated_track_ids: Vec<TrackId>,
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

#[cfg(feature = "audio-device")]
#[derive(Clone)]
pub(super) struct SharedRecordingStart(Arc<Mutex<Option<Result<ActiveRecording, String>>>>);

#[cfg(feature = "audio-device")]
#[derive(Clone)]
pub(super) struct SharedStandbyInput(
    Arc<Mutex<Option<Result<aaadaw_app::StandbyAudioInput, String>>>>,
);

#[cfg(feature = "audio-device")]
impl std::fmt::Debug for SharedStandbyInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedStandbyInput(..)")
    }
}

#[cfg(feature = "audio-device")]
impl std::fmt::Debug for SharedRecordingStart {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedRecordingStart(..)")
    }
}

#[cfg(feature = "audio-device")]
type RecordingPositionSavedResult = Arc<Mutex<Option<Result<(ActiveRecording, u64), String>>>>;

#[cfg(feature = "audio-device")]
#[derive(Clone)]
pub(super) struct SharedRecordingPositionSaved(RecordingPositionSavedResult);

#[cfg(feature = "audio-device")]
impl std::fmt::Debug for SharedRecordingPositionSaved {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedRecordingPositionSaved(..)")
    }
}

#[cfg(feature = "audio-device")]
type RecordingClockAnchorReadyResult = Arc<Mutex<Option<Result<(ActiveRecording, u64), String>>>>;

#[cfg(feature = "audio-device")]
#[derive(Clone)]
pub(super) struct SharedRecordingClockAnchorReady(RecordingClockAnchorReadyResult);

#[cfg(feature = "audio-device")]
impl std::fmt::Debug for SharedRecordingClockAnchorReady {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedRecordingClockAnchorReady(..)")
    }
}

#[cfg(feature = "audio-device")]
type RecordingStopResult = Result<(Vec<PathBuf>, PathBuf, u64), String>;

#[cfg(feature = "audio-device")]
#[derive(Clone)]
pub(super) struct SharedRecordingStop(Arc<Mutex<Option<RecordingStopResult>>>);

#[cfg(feature = "audio-device")]
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

#[cfg(feature = "audio-device")]
#[derive(Clone)]
pub(crate) struct SharedPreparedPlayback(Arc<Mutex<Option<Result<PreparedAudioPlayback, String>>>>);

#[cfg(feature = "audio-device")]
impl std::fmt::Debug for SharedPreparedPlayback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedPreparedPlayback(..)")
    }
}

fn duplicate_item_actions(
    project: &Project,
    item_ids: &[ItemId],
) -> Result<Vec<DawAction>, String> {
    if item_ids.len() == 1 {
        let item_id = item_ids[0];
        if project
            .audio_items()
            .iter()
            .any(|item| item.id() == item_id)
        {
            return duplicate_audio_item(project, item_id)
                .map(|action| vec![action])
                .map_err(|error| error.to_string());
        }
        if project.midi_items().iter().any(|item| item.id() == item_id) {
            return Ok(vec![DawAction::DuplicateMidiItem { item_id }]);
        }
        return Err("selected item no longer exists".to_owned());
    }

    if item_ids.iter().all(|item_id| {
        project
            .audio_items()
            .iter()
            .any(|item| item.id() == *item_id)
    }) {
        let mut audio_ranges = Vec::with_capacity(item_ids.len());
        for item_id in item_ids {
            let item = project
                .audio_items()
                .iter()
                .find(|item| item.id() == *item_id)
                .ok_or_else(|| "selected item no longer exists".to_owned())?;
            let end_sample = item
                .start_sample()
                .checked_add(item.length_samples())
                .ok_or_else(|| "audio item end exceeds the sample timeline".to_owned())?;
            audio_ranges.push((item, item.start_sample(), end_sample));
        }
        let group_start = audio_ranges
            .iter()
            .map(|(item, _, _)| item.start_sample())
            .min()
            .ok_or_else(|| "select one or more items to duplicate".to_owned())?;
        let group_end = audio_ranges
            .iter()
            .map(|(_, _, end_sample)| *end_sample)
            .max()
            .ok_or_else(|| "select one or more items to duplicate".to_owned())?;
        let offset = group_end - group_start;
        return audio_ranges
            .into_iter()
            .map(|(item, _, _)| {
                let duplicate_start = item
                    .start_sample()
                    .checked_add(offset)
                    .ok_or_else(|| "duplicate exceeds the sample timeline".to_owned())?;
                duplicate_start
                    .checked_add(item.length_samples())
                    .ok_or_else(|| "duplicate exceeds the sample timeline".to_owned())?;
                Ok(DawAction::InsertAudioItem {
                    track_id: item.track_id(),
                    media_ref: item.media_ref().to_owned(),
                    start_sample: duplicate_start,
                    source_offset_samples: item.source_offset_samples(),
                    length_samples: item.length_samples(),
                })
            })
            .collect();
    }

    let mut selected_ranges = Vec::with_capacity(item_ids.len());
    for item_id in item_ids {
        if let Some(item) = project
            .audio_items()
            .iter()
            .find(|item| item.id() == *item_id)
        {
            let end_sample = item
                .start_sample()
                .checked_add(item.length_samples())
                .ok_or_else(|| "audio item end exceeds the sample timeline".to_owned())?;
            let start_tick = project
                .tick_at_sample(item.start_sample())
                .map_err(|error| error.to_string())?;
            let end_tick = project
                .tick_at_sample(end_sample)
                .map_err(|error| error.to_string())?;
            selected_ranges.push((*item_id, start_tick, end_tick));
        } else if let Some(item) = project
            .midi_items()
            .iter()
            .find(|item| item.id() == *item_id)
        {
            let end_tick = item
                .start_tick()
                .checked_add(item.length_ticks())
                .ok_or_else(|| "MIDI item end exceeds the project timeline".to_owned())?;
            selected_ranges.push((*item_id, item.start_tick(), end_tick));
        } else {
            return Err("selected item no longer exists".to_owned());
        }
    }

    let group_start = selected_ranges
        .iter()
        .map(|(_, start_tick, _)| *start_tick)
        .min()
        .ok_or_else(|| "select one or more items to duplicate".to_owned())?;
    let group_end = selected_ranges
        .iter()
        .map(|(_, _, end_tick)| *end_tick)
        .max()
        .ok_or_else(|| "select one or more items to duplicate".to_owned())?;
    let offset = group_end.saturating_sub(group_start);
    let mut actions = Vec::with_capacity(selected_ranges.len());
    for (item_id, start_tick, _) in selected_ranges {
        let duplicate_start_tick = start_tick
            .checked_add(offset)
            .ok_or_else(|| "duplicate exceeds the project timeline".to_owned())?;
        if let Some(item) = project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
        {
            let start_sample = project
                .sample_at_tick(duplicate_start_tick)
                .map_err(|error| error.to_string())?;
            start_sample
                .checked_add(item.length_samples())
                .ok_or_else(|| "duplicate exceeds the sample timeline".to_owned())?;
            actions.push(DawAction::InsertAudioItem {
                track_id: item.track_id(),
                media_ref: item.media_ref().to_owned(),
                start_sample,
                source_offset_samples: item.source_offset_samples(),
                length_samples: item.length_samples(),
            });
        } else {
            actions.push(DawAction::DuplicateMidiItemAt {
                item_id,
                start_tick: duplicate_start_tick,
            });
        }
    }
    Ok(actions)
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let mut app = Self::default();
        match create_unsaved_project_session_dir() {
            Ok(session_dir) => app.session_media_dir = Some(session_dir),
            Err(error) => {
                app.status = format!("Temporary project media storage unavailable: {error}");
            }
        }
        let (main_window_id, main_window_task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(1280.0, 800.0),
            min_size: Some(iced::Size::new(900.0, 620.0)),
            exit_on_close_request: false,
            ..iced::window::Settings::default()
        });
        app.main_window_id = Some(main_window_id);
        let main_window_task = main_window_task.discard();
        match action_macros::load().and_then(commands::validate_action_macros) {
            Ok(macros) => app.action_macros = macros,
            Err(error) => {
                app.action_macro_feedback = format!("Action macros could not be loaded: {error}");
                app.action_macro_config_error = Some(error);
            }
        }
        match keyboard_config::load() {
            Ok(bindings) => {
                if let Ok(bindings) =
                    commands::validate_bindings_with_macros(&bindings, &app.action_macros)
                {
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
        match audio_config::load() {
            Ok(settings) => {
                #[cfg(any(
                    all(feature = "jack-backend", feature = "pipewire-backend"),
                    all(
                        feature = "jack-backend",
                        feature = "cpal-backend",
                        any(target_os = "windows", target_os = "macos")
                    ),
                    all(
                        feature = "pipewire-backend",
                        feature = "cpal-backend",
                        any(target_os = "windows", target_os = "macos")
                    )
                ))]
                app.restore_playback_backend(settings.playback_backend);
                app.audio_settings = settings;
            }
            Err(error) => {
                app.audio_settings_feedback =
                    format!("Audio config unavailable; using defaults ({error})");
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
        let session_recovery_task = match unsaved_sessions_root() {
            Ok(session_root) => {
                let active_session_dir = app
                    .session_media_dir
                    .as_ref()
                    .map_or_else(PathBuf::new, |session| session.directory.clone());
                Task::perform(
                    run_blocking("aaadaw-unsaved-session-scan", move || {
                        project_io::scan_unsaved_session_recoveries(
                            session_root,
                            active_session_dir,
                        )
                    }),
                    |result| match result {
                        Ok(candidates) => Message::UnsavedSessionRecoveryScanned(Ok(candidates)),
                        Err(error) => Message::UnsavedSessionRecoveryScanned(Err(error)),
                    },
                )
            }
            Err(error) => {
                tracing::warn!(%error, "could not locate unsaved session recovery directory");
                Task::none()
            }
        };
        let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
            return (
                app,
                Task::batch([main_window_task, plugin_scan_task, session_recovery_task]),
            );
        };
        app.project_path_query = path.to_string_lossy().into_owned();
        app.io_busy = true;
        app.status = format!("Opening {}…", path.display());
        let message_path = path.clone();
        let task = Task::perform(
            run_blocking("aaadaw-project-open", move || {
                project_io::load_project_session(path)
            }),
            move |result| Message::ProjectLoaded(message_path, Arc::new(Mutex::new(Some(result)))),
        );
        (
            app,
            Task::batch([
                main_window_task,
                plugin_scan_task,
                session_recovery_task,
                task,
            ]),
        )
    }

    pub(super) fn media_store_path(&self) -> Option<PathBuf> {
        self.project_path.clone().or_else(|| {
            self.session_media_dir
                .as_ref()
                .map(UnsavedSessionMedia::store_path)
        })
    }

    fn window_title(&self, window_id: iced::window::Id) -> String {
        if self.settings_window_id == Some(window_id) {
            "AAADAW Settings".to_owned()
        } else if self.render_window_id == Some(window_id) {
            "Render project to WAV".to_owned()
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
        #[cfg(feature = "audio-device")]
        let playback_active = self.playback.is_some();
        #[cfg(not(feature = "audio-device"))]
        let playback_active = false;
        #[cfg(feature = "audio-device")]
        let fx_automation_finishing =
            self.fx_automation_disarming || self.fx_automation_finish_requested;
        #[cfg(not(feature = "audio-device"))]
        let fx_automation_finishing = false;
        #[cfg(feature = "audio-device")]
        let recording_active =
            self.recording.is_some() || self.recording_starting || self.recording_stopping;
        #[cfg(not(feature = "audio-device"))]
        let recording_active = false;

        let background_tick_interval = if self.track_mix_gesture.is_some() {
            Duration::from_millis(30)
        } else {
            Duration::from_millis(100)
        };
        let background_ticks = if self.import_busy
            || playback_active
            || recording_active
            || fx_automation_finishing
            || self.offline_render_busy
            || !self.offline_job_queue.is_empty()
            || self.audio_asset_management_busy
            || self.audio_waveform_worker.is_some()
            || self.track_mix_gesture.is_some()
            || self.unsaved_session_snapshot_at.is_some()
        {
            iced::time::every(background_tick_interval).map(|_| Message::BackgroundTick)
        } else {
            iced::Subscription::none()
        };
        let meter_ticks = if playback_active {
            iced::time::every(Duration::from_millis(33)).map(|_| Message::MeterTick)
        } else {
            iced::Subscription::none()
        };
        iced::Subscription::batch([
            iced::event::listen_with(runtime_keyboard_event),
            iced::event::listen_with(midi_expression_context_menu_event),
            iced::event::listen_with(fx_chain_plugin_drag_event),
            iced::window::close_events().map(Message::WindowClosed),
            iced::window::close_requests().map(Message::WindowCloseRequested),
            iced::window::resize_events()
                .map(|(window_id, size)| Message::WindowResized(window_id, size)),
            background_ticks,
            meter_ticks,
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let revision_before_message = self.revision;
        #[cfg(feature = "audio-device")]
        let standby_input_completion = matches!(
            &message,
            Message::StandbyInputStarted(_, _, _)
                | Message::StandbyInputClosed
                | Message::RecordingInputDiscarded
        );
        #[cfg(not(feature = "audio-device"))]
        let standby_input_completion = false;
        let completes_pending_mix_reset = match &message {
            Message::ResetTrackVolumeByDoubleClick(track_id) => {
                self.track_mix_commit_is_pending(*track_id, TrackMixParameter::Volume)
            }
            Message::ResetTrackPanByDoubleClick(track_id) => {
                self.track_mix_commit_is_pending(*track_id, TrackMixParameter::Pan)
            }
            _ => false,
        };
        if self.track_mix_commit_at.is_some()
            && !matches!(&message, Message::BackgroundTick | Message::MeterTick)
            && !completes_pending_mix_reset
        {
            self.commit_track_mix_gesture();
        }
        let preserve_context_targets = matches!(
            &message,
            Message::RuntimeKeyboardEvent(..)
                | Message::ShortcutPressed(..)
                | Message::MenuKeyboard(_)
                | Message::ToggleTransportDetails
                | Message::BackgroundTick
                | Message::MeterTick
                | Message::AudioImportStarted(_)
                | Message::AudioImportFinished(_)
                | Message::AudioAssetManagementStarted(_)
                | Message::AudioAssetManagementFinished(_)
        ) || {
            #[cfg(feature = "audio-device")]
            {
                matches!(
                    &message,
                    Message::PlaybackPrepared { .. }
                        | Message::RecordingStarted(_)
                        | Message::StandbyInputStarted(_, _, _)
                        | Message::StandbyInputClosed
                        | Message::RecordingInputDiscarded
                        | Message::RecordingPositionSaved(_)
                        | Message::RecordingClockAnchorReady(_)
                        | Message::RecordingStopped(_)
                )
            }
            #[cfg(not(feature = "audio-device"))]
            {
                false
            }
        };
        let preserve_menu_state = preserve_context_targets
            || matches!(
                &message,
                // These global pointer listeners run for ordinary menu clicks too; they clean up
                // unrelated MIDI/FX gestures and do not mean the pointer left the menu.
                Message::DismissMidiExpressionContextMenus(_)
                    | Message::FinishFxChainPluginDrag
                    | Message::ActionQueryChanged(_)
                    | Message::ToggleTransportDetails
                    | Message::ActionMenuScrolled(_)
                    | Message::RunActionQuery
            );
        if !preserve_context_targets
            && !matches!(
                &message,
                Message::Timeline(
                    timeline::TimelineEvent::OpenTrackContextMenu(_)
                        | timeline::TimelineEvent::ToggleTrackContextMenu(_)
                ) | Message::Escape
            )
        {
            self.timeline.context_track = None;
        }
        if !preserve_context_targets
            && !matches!(
                &message,
                Message::Timeline(timeline::TimelineEvent::OpenItemContextMenu { .. })
                    | Message::Escape
            )
        {
            self.timeline.context_item = None;
            self.timeline.context_item_position = None;
        }
        if !preserve_context_targets
            && !matches!(
                &message,
                Message::Timeline(
                    timeline::TimelineEvent::OpenVolumeAutomationPointMenu { .. }
                        | timeline::TimelineEvent::OpenFxAutomationPointMenu { .. }
                ) | Message::Escape
            )
        {
            self.timeline.context_automation_point = None;
            self.timeline.context_automation_position = None;
        }
        if !preserve_menu_state
            && !matches!(
                &message,
                Message::ToggleMainMenu(_)
                    | Message::ToggleOfflineJobsPanel
                    | Message::DismissMainMenu
                    | Message::Escape
            )
        {
            self.active_menu = None;
        }
        let menu_ui_message = matches!(
            &message,
            Message::ActionQueryChanged(_)
                | Message::ActionMenuScrolled(_)
                | Message::MenuKeyboard(
                    MenuNavigation::Open
                        | MenuNavigation::NextMenu
                        | MenuNavigation::PreviousMenu
                        | MenuNavigation::NextCommand
                        | MenuNavigation::PreviousCommand
                )
        ) || matches!(
            &message,
            Message::RuntimeKeyboardEvent(_, _, window_id)
                if self.main_window_id == Some(*window_id)
        );
        let window_safe_message = menu_ui_message
            || matches!(
                &message,
                Message::OpenSettings
                    | Message::OpenClapPluginSettings
                    | Message::OpenRenderWindow
                    | Message::ShowMainWorkspace(_)
                    | Message::OpenTempoMap
                    | Message::OpenMeterMap
                    | Message::SelectTimeMapTab(_)
                    | Message::ApplyTempoMap
                    | Message::AddTempoPoint
                    | Message::DeleteTempoPoint(_)
                    | Message::TempoPointTickChanged(_, _)
                    | Message::TempoPointBpmChanged(_, _)
                    | Message::CycleTempoCurve(_)
                    | Message::AddMeterPoint
                    | Message::DeleteMeterPoint(_)
                    | Message::MeterPointTickChanged(_, _)
                    | Message::MeterPointNumeratorChanged(_, _)
                    | Message::MeterPointDenominatorChanged(_, _)
                    | Message::ApplyMeterMap
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
                    | Message::ExecuteCommand(commands::CommandId::ExportWav)
                    | Message::ToggleMediaBrowserPanel
                    | Message::ExecuteCommand(commands::CommandId::ToggleMediaBrowserPanel)
                    | Message::ToggleOfflineJobsPanel
                    | Message::ToggleTransportDetails
                    | Message::ExecuteCommand(commands::CommandId::ToggleOfflineJobsPanel)
                    | Message::WindowClosed(_)
                    | Message::WindowCloseRequested(_)
                    | Message::StartShortcutCapture(_)
                    | Message::ClearShortcutBinding(_)
                    | Message::RestoreShortcutDefault(_)
                    | Message::SelectSettingsCategory(_)
                    | Message::RecordingOffsetTextChanged(_)
                    | Message::ApplyRecordingOffset
                    | Message::CancelShortcutCapture
                    | Message::ShortcutCaptureKey { .. }
                    | Message::SaveShortcutBindings
                    | Message::ResetShortcutBindings
                    | Message::RemoveClapPluginPath(_)
                    | Message::RescanClapPlugins
                    | Message::ClapPluginsScanned(_)
                    | Message::PickPath(PathPickerTarget::AddClapPluginPath)
                    | Message::CancelOfflineRender
                    | Message::OfflineRenderFinished(_)
                    | Message::FreezeTrackFinished(_)
                    | Message::RemoveQueuedOfflineJob(_)
            );
        let offline_queue_submission = self.offline_render_busy
            && matches!(
                &message,
                Message::FreezeTrack(_)
                    | Message::PickPath(PathPickerTarget::ExportWav)
                    | Message::PathPicked(PathPickerTarget::ExportWav, _)
                    | Message::RemoveQueuedOfflineJob(_)
            );
        let allowed_during_io = standby_input_completion
            || window_safe_message
            || offline_queue_submission
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
                    | Message::CancelOfflineRender
                    | Message::OfflineRenderFinished(_)
                    | Message::FreezeTrackFinished(_)
                    | Message::AudioItemRelinked(..)
                    | Message::BackgroundTick
                    | Message::MeterTick
            );
        if matches!(
            &message,
            Message::Timeline(
                timeline::TimelineEvent::BeginItemDrag { .. }
                    | timeline::TimelineEvent::UpdateItemDrag { .. }
                    | timeline::TimelineEvent::EndItemDrag
                    | timeline::TimelineEvent::BeginItemTrim { .. }
                    | timeline::TimelineEvent::UpdateItemTrim { .. }
                    | timeline::TimelineEvent::EndItemTrim
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
            self.timeline
                .handle(timeline::TimelineEvent::CancelItemTrim);
            self.status = status.to_owned();
            return Task::none();
        }
        if self.path_picker_busy
            && !window_safe_message
            && !standby_input_completion
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
                    | Message::MeterTick
            )
        {
            self.status = "Wait for the file dialog to finish".to_owned();
            return Task::none();
        }
        if self.recording_recovery_busy
            && !standby_input_completion
            && !menu_ui_message
            && !matches!(
                &message,
                Message::RecordingRecoveryPrepared(_)
                    | Message::RecordingRecoveryDiscarded(..)
                    | Message::RecordingRecoveryCleaned(_)
                    | Message::BackgroundTick
                    | Message::MeterTick
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
            && !standby_input_completion
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
                    | Message::MeterTick
            )
        {
            self.status = "Wait for audio import to finish or cancel it".to_owned();
            return Task::none();
        }
        if self.audio_asset_management_busy
            && !window_safe_message
            && !standby_input_completion
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
                    | Message::MeterTick
            )
        {
            self.status = "Wait for audio asset maintenance to finish or cancel it".to_owned();
            return Task::none();
        }
        #[cfg(feature = "audio-device")]
        {
            if (self.recording.is_some() || self.recording_starting || self.recording_stopping)
                && !window_safe_message
                && !matches!(
                    &message,
                    Message::StopPlayback
                        | Message::StopRecording
                        | Message::ToggleInputMonitor(_)
                        | Message::RecordingStarted(_)
                        | Message::StandbyInputStarted(_, _, _)
                        | Message::StandbyInputClosed
                        | Message::RecordingInputDiscarded
                        | Message::RecordingPositionSaved(_)
                        | Message::RecordingClockAnchorReady(_)
                        | Message::RecordingStopped(_)
                        | Message::BackgroundTick
                        | Message::MeterTick
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
                && !standby_input_completion
                && !matches!(
                    &message,
                    Message::PlaybackPrepared { .. }
                        | Message::StopRecording
                        | Message::RecordingStarted(_)
                        | Message::RecordingPositionSaved(_)
                        | Message::RecordingClockAnchorReady(_)
                        | Message::RecordingStopped(_)
                        | Message::ToggleMainMenu(_)
                        | Message::DismissMainMenu
                        | Message::Escape
                        | Message::ToggleMediaBrowserPanel
                        | Message::ToggleTransportDetails
                        | Message::BackgroundTick
                        | Message::MeterTick
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
            if self.playback_active()
                && (unsupported_playback_history_edit
                    || matches!(
                        &message,
                        Message::AddTrack
                            | Message::AddBusTrack
                            | Message::SetTrackOutput(..)
                            | Message::AddMidiItem
                            | Message::AddMidiNote(_)
                            | Message::AddMidiNoteAt(..)
                            | Message::EditMidiNotes(..)
                            | Message::DeleteMidiNotes(..)
                            | Message::SetMidiControllers(..)
                            | Message::DeleteMidiItem(_)
                            | Message::MidiItemNameChanged(..)
                            | Message::CommitMidiItemName(_)
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
                            | Message::WindowResized(..)
                            | Message::NudgeAudioItem(..)
                            | Message::BeginAudioItemStartSampleEdit(_)
                            | Message::AudioItemStartSampleChanged(..)
                            | Message::CommitAudioItemStartSample(_)
                            | Message::CancelAudioItemStartSampleEdit(_)
                            | Message::DeleteAudioItem(_)
                            | Message::DeleteSelectedItems
                            | Message::DuplicateSelectedItems
                            | Message::DuplicateAudioItem(_)
                            | Message::DuplicateMidiItem(_)
                            | Message::SplitSelectedItemsAtCursor
                            | Message::SplitSelectedItemsAtTimeSelection
                            | Message::RunActionQuery
                            | Message::ToggleTransportDetails
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
                self.offline_jobs_panel_open = false;
                if self.active_menu == Some(menu) {
                    self.active_menu = None;
                    self.menu_selected_command = None;
                } else {
                    task = self.open_main_menu(menu);
                }
            }
            Message::MenuKeyboard(navigation) => {
                task = self.navigate_main_menu(navigation);
            }
            Message::ShowMainWorkspace(workspace) => {
                self.main_workspace = workspace;
                self.active_menu = None;
            }
            Message::OpenSettings => task = self.open_settings(),
            Message::OpenClapPluginSettings => {
                self.settings_category = SettingsCategory::ClapPlugins;
                task = self.open_settings();
            }
            Message::OpenRenderWindow => task = self.open_render_window(),
            Message::OpenTempoMap => task = self.open_tempo_map(TimeMapTab::Tempo),
            Message::OpenMeterMap => task = self.open_tempo_map(TimeMapTab::Meter),
            Message::SelectTimeMapTab(tab) => self.time_map_tab = tab,
            Message::AddTempoPoint => self.add_tempo_point_edit(),
            Message::DeleteTempoPoint(index) => {
                if index < self.tempo_map_edits.len()
                    && self.tempo_map_edits[index].original_tick != Some(0)
                {
                    self.tempo_map_edits.remove(index);
                }
            }
            Message::TempoPointTickChanged(index, value) => {
                if let Some(point) = self.tempo_map_edits.get_mut(index) {
                    point.tick = value;
                }
            }
            Message::TempoPointBpmChanged(index, value) => {
                if let Some(point) = self.tempo_map_edits.get_mut(index) {
                    point.bpm = value;
                }
            }
            Message::CycleTempoCurve(index) => {
                let has_next = index + 1 < self.tempo_map_edits.len();
                if has_next && let Some(point) = self.tempo_map_edits.get_mut(index) {
                    point.curve = match point.curve {
                        TempoCurve::Step => TempoCurve::Linear,
                        TempoCurve::Linear => TempoCurve::Logarithmic,
                        TempoCurve::Logarithmic => TempoCurve::Bézier,
                        TempoCurve::Bézier => TempoCurve::Step,
                    };
                }
            }
            Message::ApplyTempoMap => task = self.apply_tempo_map_edits(),
            Message::AddMeterPoint => self.add_meter_point_edit(),
            Message::DeleteMeterPoint(index) => {
                if index < self.meter_map_edits.len()
                    && self.meter_map_edits[index].original_tick != Some(0)
                {
                    self.meter_map_edits.remove(index);
                }
            }
            Message::MeterPointTickChanged(index, value) => {
                if let Some(point) = self.meter_map_edits.get_mut(index) {
                    point.tick = value;
                }
            }
            Message::MeterPointNumeratorChanged(index, value) => {
                if let Some(point) = self.meter_map_edits.get_mut(index) {
                    point.numerator = value;
                }
            }
            Message::MeterPointDenominatorChanged(index, value) => {
                if let Some(point) = self.meter_map_edits.get_mut(index) {
                    point.denominator = value;
                }
            }
            Message::ApplyMeterMap => self.apply_meter_map_edits(),
            Message::WindowClosed(window_id) => {
                if self.settings_window_id == Some(window_id) {
                    self.settings_window_id = None;
                    self.shortcut_capture_id = None;
                    self.shortcut_editor_feedback.clear();
                } else if self.render_window_id == Some(window_id) {
                    self.render_window_id = None;
                } else if self.tempo_map_window_id == Some(window_id) {
                    self.tempo_map_window_id = None;
                    self.tempo_map_edits.clear();
                    self.meter_map_edits.clear();
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
                    self.release_midi_preview();
                    self.midi_editor_window_id = None;
                    self.midi_editor_item_id = None;
                    self.midi_editor_feedback = None;
                    self.midi_editor_selected_notes.clear();
                } else if self.main_window_id == Some(window_id) {
                    self.close_fx_editor_resources();
                    task = iced::exit();
                }
            }
            Message::WindowCloseRequested(window_id) => {
                if self.fx_chain_window_id == Some(window_id) {
                    self.close_fx_editor_resources();
                } else if self.main_window_id == Some(window_id) {
                    self.release_midi_preview();
                    task = self.begin_project_transition(
                        PendingProjectTransition::CloseMainWindow(window_id),
                    );
                }
            }
            Message::OpenTrackFxChain(track_id) => task = self.open_track_fx_chain(track_id),
            Message::OpenMidiEditor(item_id) => task = self.open_midi_editor(item_id),
            Message::CloseMidiEditor => {
                self.release_midi_preview();
                if let Some(window_id) = self.midi_editor_window_id.take() {
                    self.midi_editor_item_id = None;
                    self.midi_editor_feedback = None;
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_edit_cursor_tick = None;
                    task = iced::window::close(window_id);
                }
            }
            Message::SelectMidiEditorLane(lane) => {
                self.release_midi_preview();
                self.midi_editor_lane = lane;
            }
            Message::PreviewMidiNote(track_id, pitch) => {
                #[cfg(feature = "audio-device")]
                if self.midi_editor_window_id.is_some()
                    && !self.playback_playing
                    && let Some(playback) = &self.playback
                {
                    let _ = playback.preview_midi_note(track_id, pitch);
                }
                #[cfg(not(feature = "audio-device"))]
                let _ = (track_id, pitch);
            }
            Message::ReleaseMidiPreview => self.release_midi_preview(),
            Message::MidiEditorFeedback(feedback) => {
                self.midi_editor_feedback = Some(feedback);
            }
            Message::SelectMidiNotes(note_ids) => {
                self.midi_editor_feedback = None;
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
                        let copy_grid = if self.timeline.snap_enabled {
                            self.timeline
                                .snap_grid
                                .tick_interval(self.project.settings().ppq())
                                .unwrap_or(1)
                        } else {
                            1
                        };
                        let default_paste_tick =
                            self.midi_editor_edit_cursor_tick.unwrap_or_else(|| {
                                snap_tick_up(
                                    first_tick
                                        .saturating_sub(item.source_offset_ticks())
                                        .saturating_add(self.midi_note_clipboard.span_ticks),
                                    copy_grid,
                                )
                            });
                        self.midi_note_clipboard.default_paste_tick =
                            Some(default_paste_tick.min(item.length_ticks()));
                        if self.midi_editor_edit_cursor_tick.is_none() {
                            self.midi_editor_edit_cursor_tick =
                                self.midi_note_clipboard.default_paste_tick;
                        }
                        self.midi_note_clipboard.last_paste = None;
                        let feedback =
                            format!("Copied {} MIDI notes", self.midi_note_clipboard.notes.len());
                        self.status = feedback.clone();
                        self.midi_editor_feedback = Some(feedback);
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
                let grid = self.midi_editor_snap_interval();
                let source_start_tick = item.source_offset_ticks();
                let item_end_tick = source_start_tick.saturating_add(item.length_ticks());
                let item_length = item.length_ticks();
                let tick = self
                    .midi_note_clipboard
                    .last_paste
                    .filter(|(last_item, _)| *last_item == item_id)
                    .map_or_else(
                        || self.midi_editor_paste_target_tick(Some(item_id)),
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
                        tick: source_start_tick
                            .saturating_add(tick)
                            .saturating_add(note.tick),
                        ..*note
                    })
                    .collect::<Vec<_>>();
                if notes.iter().any(|note| {
                    note.tick < source_start_tick
                        || note.tick.saturating_add(note.duration) > item_end_tick
                }) {
                    let feedback = "Paste rejected: notes would extend beyond the MIDI item";
                    self.status = feedback.to_owned();
                    self.midi_editor_feedback = Some(feedback.to_owned());
                } else {
                    self.midi_editor_feedback = None;
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
                        self.midi_editor_edit_cursor_tick = Some(
                            snap_tick_up(
                                tick.saturating_add(self.midi_note_clipboard.span_ticks),
                                grid,
                            )
                            .min(item_length),
                        );
                    }
                }
            }
            Message::DuplicateMidiNotes(item_id) => self.duplicate_selected_midi_notes(item_id),
            Message::SetPianoRollCursor(item_id, tick) => {
                if self.midi_editor_item_id == Some(item_id)
                    && let Some(item) = self
                        .project
                        .midi_items()
                        .iter()
                        .find(|item| item.id() == item_id)
                {
                    self.midi_editor_edit_cursor_tick = Some(tick.min(item.length_ticks()));
                    self.midi_note_clipboard.last_paste = None;
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_feedback = None;
                }
            }
            Message::AddMidiNoteAt(item_id, data) => {
                self.midi_editor_feedback = None;
                self.apply_action(
                    DawAction::AddMidiNotes {
                        item_id,
                        notes: vec![data],
                    },
                    "MIDI note added",
                );
            }
            Message::CopyDragMidiNotes(item_id, notes) => {
                let Some(item) = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                else {
                    return task;
                };
                if notes.is_empty() {
                    return task;
                }
                let old_ids = item
                    .notes()
                    .iter()
                    .map(|note| note.id())
                    .collect::<HashSet<_>>();
                let revision = self.revision;
                self.midi_editor_feedback = None;
                self.apply_action(
                    DawAction::AddMidiNotes { item_id, notes },
                    "MIDI notes copied",
                );
                if self.revision != revision
                    && let Some(item) = self
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
            }
            Message::EditMidiNotes(item_id, edits) => {
                self.midi_editor_feedback = None;
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
                    self.midi_editor_feedback = None;
                    self.apply_action(
                        DawAction::DeleteMidiNotes { item_id, note_ids },
                        "MIDI notes deleted",
                    );
                    self.midi_editor_selected_notes.clear();
                }
            }
            Message::SetMidiControllers(item_id, controllers) => {
                self.midi_editor_feedback = None;
                self.apply_action(
                    DawAction::SetMidiControllers {
                        item_id,
                        controllers,
                    },
                    "MIDI controller lane edited",
                );
            }
            Message::SetMidiPitchBends(item_id, pitch_bends) => {
                self.midi_editor_feedback = None;
                self.apply_action(
                    DawAction::SetMidiPitchBends {
                        item_id,
                        pitch_bends,
                    },
                    "MIDI pitch-bend lane edited",
                );
            }
            Message::PianoRollPan(beats) => {
                let delta = i128::from(beats) * i128::from(self.project.settings().ppq());
                self.midi_editor_origin_tick = (i128::from(self.midi_editor_origin_tick) + delta)
                    .clamp(0, i128::from(u64::MAX))
                    as u64;
            }
            Message::PianoRollPanPixels(delta) if delta.is_finite() => {
                let ticks = (f64::from(delta) / f64::from(self.midi_editor_pixels_per_beat)
                    * f64::from(self.project.settings().ppq()))
                .round() as i128;
                self.midi_editor_origin_tick = i128::from(self.midi_editor_origin_tick)
                    .saturating_add(ticks)
                    .clamp(0, i128::from(u64::MAX))
                    as u64;
            }
            Message::PianoRollPanPixels(_) => {}
            Message::PianoRollZoom(factor) if factor.is_finite() && factor > 0.0 => {
                self.midi_editor_pixels_per_beat =
                    (self.midi_editor_pixels_per_beat * factor).clamp(1.0, 300.0);
            }
            Message::PianoRollZoom(_) => {}
            Message::PianoRollZoomAt(factor, anchor_x)
                if factor.is_finite() && factor > 0.0 && anchor_x.is_finite() =>
            {
                let ppq = f64::from(self.project.settings().ppq());
                let old_scale = f64::from(self.midi_editor_pixels_per_beat);
                let anchor_tick =
                    self.midi_editor_origin_tick as f64 + f64::from(anchor_x) / old_scale * ppq;
                let new_scale = (self.midi_editor_pixels_per_beat * factor).clamp(1.0, 300.0);
                let new_origin = anchor_tick - f64::from(anchor_x) / f64::from(new_scale) * ppq;
                self.midi_editor_origin_tick =
                    new_origin.round().clamp(0.0, u64::MAX as f64) as u64;
                self.midi_editor_pixels_per_beat = new_scale;
            }
            Message::PianoRollZoomAt(_, _) => {}
            Message::PianoRollPitchScroll(delta) => {
                self.midi_editor_high_pitch = (i16::from(self.midi_editor_high_pitch)
                    + i16::from(delta))
                .clamp(35, 127) as u8;
            }
            Message::FitPianoRollToNotes(item_id) => {
                self.fit_midi_editor_to_item(item_id);
            }
            Message::TogglePianoRollFollowPlayhead => {
                self.midi_editor_follow_playhead = !self.midi_editor_follow_playhead;
                self.midi_editor_feedback = Some(if self.midi_editor_follow_playhead {
                    "Follow playhead on".to_owned()
                } else {
                    "Follow playhead off".to_owned()
                });
            }
            Message::OpenTrackInstrumentPicker(track_id) => {
                task = self.open_track_instrument_picker(track_id)
            }
            Message::SetTrackInstrumentGui(track_id, open) => {
                task = self.request_track_instrument_gui(track_id, open)
            }
            Message::TrackInstrumentNativeParent(track_id, open, parent) => {
                self.set_track_instrument_gui(track_id, open, parent)
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
            Message::BeginFxChainPluginDrag(index) => {
                self.fx_chain_drag_index = Some(index);
                self.fx_chain_drag_target_index = Some(index);
            }
            Message::HoverFxChainPluginDragTarget(index) => {
                if self.fx_chain_drag_index.is_some() {
                    self.fx_chain_drag_target_index = Some(index);
                }
            }
            Message::LeaveFxChainPluginDragTarget(index) => {
                if self.fx_chain_drag_target_index == Some(index) {
                    self.fx_chain_drag_target_index = None;
                }
            }
            Message::FinishFxChainPluginDrag => {
                let from = self.fx_chain_drag_index.take();
                let to = self.fx_chain_drag_target_index.take();
                if let Some((from, to)) = from.zip(to) {
                    task = self.reorder_fx_chain_plugin(from, to);
                }
            }
            Message::ReorderFxChainPlugin { from, to } => {
                task = self.reorder_fx_chain_plugin(from, to)
            }
            Message::FxParameterChanged(id, value) => self.change_fx_parameter(id, value),
            Message::CancelFxParameterGesture(id) => self.cancel_fx_parameter_gesture(id),
            Message::FxAutomationWriteToggled(id) => {
                #[cfg(feature = "audio-device")]
                self.toggle_fx_automation_write(id);
                #[cfg(not(feature = "audio-device"))]
                let _ = id;
            }
            Message::FxAutomationLaneToggled {
                parameter_id: id,
                name,
                min_value,
                max_value,
                stepped,
            } => {
                if let (Some(track_id), Some(chain_index)) =
                    (self.fx_chain_track_id, self.fx_chain_selected_index)
                {
                    let value_range = if stepped {
                        (min_value.ceil(), max_value.floor())
                    } else {
                        (min_value, max_value)
                    };
                    self.handle_timeline_view_event(timeline::TimelineEvent::ToggleFxAutomation {
                        track_id,
                        chain_index,
                        parameter_id: id,
                        name,
                        value_range,
                        stepped,
                    });
                }
            }
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
            Message::WindowResized(window_id, size) => {
                if self.midi_editor_window_id == Some(window_id) {
                    self.midi_editor_window_size = size;
                }
                if self.fx_chain_window_id == Some(window_id) {
                    self.fx_chain_window_size = size;
                    self.resize_fx_editor_host();
                }
            }
            Message::StartShortcutCapture(action_id) => {
                self.shortcut_capture_id = Some(action_id);
                self.shortcut_editor_feedback =
                    "Press a shortcut; Backspace clears it; Escape cancels".to_owned();
            }
            Message::ClearShortcutBinding(action_id) => self.clear_shortcut_binding(action_id),
            Message::RestoreShortcutDefault(action_id) => self.restore_shortcut_default(action_id),
            Message::SelectSettingsCategory(category) => {
                self.settings_category = category;
                if self.shortcut_capture_id.take().is_some() {
                    self.shortcut_editor_feedback = "Shortcut recording cancelled".to_owned();
                }
                #[cfg(all(
                    feature = "cpal-backend",
                    any(target_os = "windows", target_os = "macos")
                ))]
                if category == SettingsCategory::Audio {
                    task = Task::batch([
                        self.refresh_cpal_output_devices(),
                        self.refresh_cpal_input_devices(),
                    ]);
                }
            }
            Message::SetMasterOutputCeilingDbfs(ceiling_dbfs) => {
                self.set_master_output_ceiling_dbfs(ceiling_dbfs);
            }
            Message::SetWavSampleFormat(format) => {
                self.wav_export_options.sample_format = format;
                if !format.is_integer() {
                    self.wav_export_options.dither = false;
                }
            }
            Message::SetWavDither(enabled) => {
                self.wav_export_options.dither =
                    enabled && self.wav_export_options.sample_format.is_integer();
            }
            Message::RecordingOffsetTextChanged(value) => {
                self.audio_recording_offset_query = Some(value);
            }
            Message::ApplyRecordingOffset => self.apply_recording_offset(),
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))]
            Message::CpalOutputDevicesLoaded(result) => {
                self.finish_cpal_output_device_enumeration(result);
            }
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))]
            Message::RefreshCpalOutputDevices => {
                task = self.refresh_cpal_output_devices();
            }
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))]
            Message::SelectCpalOutputDevice(device_id) => {
                let settings = audio_config::AudioSettings {
                    cpal_output_device_id: device_id.clone(),
                    ..self.audio_settings.clone()
                };
                self.audio_settings = settings.clone();
                match audio_config::save(&settings) {
                    Ok(()) => {
                        let target = device_id.as_deref().unwrap_or("System default");
                        self.audio_settings_feedback = if self.playback.is_some() {
                            format!(
                                "{target} selected for the next output; close and reopen playback to apply"
                            )
                        } else {
                            format!("{target} selected for system audio playback")
                        };
                    }
                    Err(error) => {
                        self.audio_settings_feedback = format!(
                            "System audio output selection is active for this session but could not be saved: {error}"
                        );
                    }
                }
            }
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))]
            Message::CpalInputDevicesLoaded(result) => {
                self.finish_cpal_input_device_enumeration(result);
            }
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))]
            Message::RefreshCpalInputDevices => {
                task = self.refresh_cpal_input_devices();
            }
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))]
            Message::SelectCpalInputDevice(device_id) => {
                let input_changed = self.audio_settings.cpal_input_device_id != device_id;
                let cpal_is_selected = self.selected_playback_backend() == PlaybackBackend::Cpal;
                let input_monitor_was_open = self.standby_monitor_starting
                    || self
                        .playback
                        .as_ref()
                        .is_some_and(aaadaw_app::RunningAudioPlayback::has_standby_input);
                let close_standby = if input_changed && cpal_is_selected {
                    self.release_standby_input()
                } else {
                    Task::none()
                };
                let settings = audio_config::AudioSettings {
                    cpal_input_device_id: device_id.clone(),
                    ..self.audio_settings.clone()
                };
                self.audio_settings = settings.clone();
                match audio_config::save(&settings) {
                    Ok(()) => {
                        let target = device_id.as_deref().unwrap_or("System default");
                        self.audio_settings_feedback = if self.recording.is_some()
                            || self.recording_starting
                        {
                            format!(
                                "{target} selected for the next input connection; the current take keeps its existing input"
                            )
                        } else {
                            format!("{target} selected for system audio recording")
                        };
                        if input_monitor_was_open && cpal_is_selected && input_changed {
                            self.audio_settings_feedback.push_str(
                                "; input monitoring stopped, re-enable it to use the new endpoint",
                            );
                        }
                    }
                    Err(error) => {
                        self.audio_settings_feedback = format!(
                            "System audio input selection is active for this session but could not be saved: {error}"
                        );
                    }
                }
                task = close_standby;
            }
            Message::RemoveClapPluginPath(path) => {
                task = self.remove_clap_plugin_path(path);
            }
            Message::RescanClapPlugins => task = self.start_clap_plugin_scan(true),
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
                if let iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) =
                    &event
                {
                    self.keyboard_modifiers = *modifiers;
                }
                let message = menu_navigation_event(
                    &event,
                    status,
                    window_id,
                    self.main_window_id,
                    self.active_menu,
                )
                .or_else(|| {
                    plugin_window_escape_message(
                        &event,
                        status,
                        window_id,
                        self.fx_chain_window_id,
                        self.plugin_picker_window_id,
                    )
                })
                .or_else(|| {
                    midi_editor_shortcut_event(
                        event.clone(),
                        status,
                        window_id,
                        self.midi_editor_window_id,
                    )
                })
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
            Message::DismissMidiExpressionContextMenus(window_id) => {
                if self.midi_editor_window_id == Some(window_id) {
                    self.midi_expression_context_menu_epoch =
                        self.midi_expression_context_menu_epoch.wrapping_add(1);
                }
            }
            Message::DismissMainMenu => {
                self.active_menu = None;
                self.offline_jobs_panel_open = false;
            }
            Message::ToggleOfflineJobsPanel => {
                self.offline_jobs_panel_open = !self.offline_jobs_panel_open;
            }
            Message::ToggleTransportDetails => {
                self.transport_details_open = !self.transport_details_open;
            }
            Message::Escape => {
                if self.pending_project_transition.is_some() {
                    self.pending_project_transition = None;
                } else if self.offline_jobs_panel_open {
                    self.offline_jobs_panel_open = false;
                } else if self.active_menu.take().is_some() {
                    self.menu_selected_command = None;
                } else if !self.cancel_active_track_draft() {
                    let dismissed_item_menu = self.timeline.context_item.take().is_some();
                    let dismissed_track_menu = self.timeline.context_track.take().is_some();
                    let dismissed_automation_menu =
                        self.timeline.context_automation_point.take().is_some();
                    if dismissed_item_menu || dismissed_track_menu || dismissed_automation_menu {
                        self.timeline.context_item_position = None;
                        self.timeline.context_automation_position = None;
                    } else {
                        self.timeline
                            .handle(timeline::TimelineEvent::ClearTimeSelection);
                    }
                }
            }
            Message::NewProject => {
                task = self.begin_project_transition(PendingProjectTransition::NewProject)
            }
            Message::Timeline(timeline::TimelineEvent::EndItemDrag) => self.finish_item_drag(),
            Message::Timeline(timeline::TimelineEvent::EndItemTrim) => self.finish_item_trim(),
            Message::Timeline(timeline::TimelineEvent::CancelItemDrag) => {
                self.timeline
                    .handle(timeline::TimelineEvent::CancelItemDrag);
                self.status = "Item drag cancelled".to_owned();
            }
            Message::Timeline(timeline::TimelineEvent::CancelItemTrim) => {
                self.timeline
                    .handle(timeline::TimelineEvent::CancelItemTrim);
                self.status = "Audio item trim cancelled".to_owned();
            }
            Message::Timeline(timeline::TimelineEvent::InsertVolumeAutomationAt {
                track_index,
                tick,
                gain_db,
            }) => {
                if self.volume_automation_edit_busy() {
                    self.status =
                        "Stop playback or recording before editing volume automation".to_owned();
                } else if let (Some(track), Ok(sample)) = (
                    self.project.tracks().get(track_index),
                    self.project.sample_at_tick(tick),
                ) {
                    let track_id = track.id();
                    let mut points = track.volume_automation().to_vec();
                    if let Some(point) = aaadaw_core::VolumeAutomationPoint::new(sample, gain_db) {
                        let point_index =
                            match points.binary_search_by_key(&sample, |point| point.sample()) {
                                Ok(index) => {
                                    points[index] = point;
                                    index
                                }
                                Err(index) => {
                                    points.insert(index, point);
                                    index
                                }
                            };
                        self.timeline.volume_automation_tracks.insert(track_id);
                        let revision = self.revision;
                        self.apply_action(
                            DawAction::SetTrackVolumeAutomation { track_id, points },
                            "Automation point added",
                        );
                        if self.revision != revision {
                            self.timeline.handle(
                                timeline::TimelineEvent::SelectVolumeAutomationPoint {
                                    track_id,
                                    index: point_index,
                                },
                            );
                        }
                    }
                }
            }
            Message::Timeline(timeline::TimelineEvent::SetVolumeAutomation(track_id, points)) => {
                if self.volume_automation_edit_busy() {
                    self.status =
                        "Stop playback or recording before editing volume automation".to_owned();
                } else {
                    self.apply_action(
                        DawAction::SetTrackVolumeAutomation { track_id, points },
                        "Volume automation edited",
                    );
                }
            }
            Message::Timeline(timeline::TimelineEvent::SetFxAutomation {
                track_id,
                chain_index,
                parameter_id,
                points,
                selected_point,
            }) => {
                if self.project_graph_edit_busy() {
                    self.status =
                        "Stop playback or recording before editing FX automation".to_owned();
                } else {
                    let revision = self.revision;
                    self.apply_action(
                        DawAction::SetTrackFxParameterAutomation {
                            track_id,
                            chain_index,
                            parameter_id,
                            points,
                        },
                        "FX automation edited",
                    );
                    if self.revision != revision
                        && let Some(index) = selected_point
                    {
                        self.timeline
                            .handle(timeline::TimelineEvent::SelectFxAutomationPoint {
                                track_id,
                                chain_index,
                                parameter_id,
                                index,
                            });
                    }
                }
            }
            Message::Timeline(timeline::TimelineEvent::ClearSelectedFxAutomationPoint) => {
                self.timeline
                    .handle(timeline::TimelineEvent::ClearSelectedFxAutomationPoint);
            }
            Message::Timeline(timeline::TimelineEvent::DeleteFxAutomationPoint {
                track_id,
                chain_index,
                parameter_id,
                index,
            }) => {
                self.timeline
                    .handle(timeline::TimelineEvent::ClearSelectedFxAutomationPoint);
                if self.project_graph_edit_busy() {
                    self.status =
                        "Stop playback or recording before editing FX automation".to_owned();
                } else if let Some(plugin) = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                    .and_then(|track| track.fx_chain().get(chain_index))
                    && let Some(lane) = plugin.parameter_automation_for(parameter_id)
                {
                    let mut points = lane.points().to_vec();
                    if index < points.len() {
                        points.remove(index);
                        self.apply_action(
                            DawAction::SetTrackFxParameterAutomation {
                                track_id,
                                chain_index,
                                parameter_id,
                                points,
                            },
                            "FX automation point deleted",
                        );
                    }
                }
            }
            Message::Timeline(timeline::TimelineEvent::DeleteVolumeAutomationPoint {
                track_id,
                index,
            }) => {
                self.timeline
                    .handle(timeline::TimelineEvent::ClearSelectedVolumeAutomationPoint);
                if self.volume_automation_edit_busy() {
                    self.status =
                        "Stop playback or recording before editing volume automation".to_owned();
                } else if let Some(track) = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                {
                    let mut points = track.volume_automation().to_vec();
                    if index < points.len() {
                        points.remove(index);
                        self.apply_action(
                            DawAction::SetTrackVolumeAutomation { track_id, points },
                            "Automation point deleted",
                        );
                    }
                }
            }
            Message::Timeline(event) => self.handle_timeline_view_event(event),
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
            #[cfg(feature = "audio-device")]
            Message::TogglePlayback => {
                if self.playback_playing {
                    self.pause_playback();
                } else {
                    task = self.start_playback();
                }
            }
            Message::AddTrack => {
                self.active_menu = None;
                self.add_track();
            }
            Message::AddBusTrack => {
                if self.project_graph_edit_busy() {
                    self.status =
                        "Stop playback or recording before changing track routing".to_owned();
                } else {
                    self.active_menu = None;
                    self.add_bus_track();
                }
            }
            Message::SetTrackOutput(track_id, output_track) => {
                if self.project_graph_edit_busy() {
                    self.status =
                        "Stop playback or recording before changing track routing".to_owned();
                } else {
                    self.apply_action(
                        DawAction::SetTrackOutput {
                            track_id,
                            output_track,
                        },
                        "Track output changed",
                    );
                }
            }
            Message::AddMidiItem => {
                self.add_midi_item();
            }
            Message::AddMidiNote(item_id) => {
                let action = add_quarter_note(&self.project, item_id);
                self.apply_edit(action, "C4 MIDI note added");
            }
            Message::DeleteMidiItem(item_id) => {
                self.midi_item_name_edits.remove(&item_id);
                self.midi_item_name_errors.remove(&item_id);
                self.apply_action(DawAction::DeleteMidiItem { item_id }, "MIDI item deleted");
            }
            Message::MidiItemNameChanged(item_id, name) => {
                self.midi_item_name_errors.remove(&item_id);
                self.midi_item_name_edits.insert(item_id, name);
            }
            Message::CommitMidiItemName(item_id) => self.commit_midi_item_name(item_id),
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
                self.begin_track_draft(track_id, TrackDraftField::Name);
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
                #[cfg(feature = "audio-device")]
                let was_armed = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                    .is_some_and(|track| track.is_record_armed());
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
                #[cfg(feature = "audio-device")]
                {
                    if was_armed {
                        if self.standby_monitor_track == Some(track_id) {
                            self.standby_monitor_track = None;
                            self.standby_monitor_generation =
                                self.standby_monitor_generation.wrapping_add(1);
                        }
                        if let Some(playback) = self.playback.as_ref() {
                            playback.set_track_input_monitor(track_id, false);
                            if !playback.has_enabled_input_monitor() {
                                task = self.release_standby_input();
                            }
                        }
                    }
                }
            }
            #[cfg(feature = "audio-device")]
            Message::ToggleInputMonitor(track_id) => {
                task = self.toggle_input_monitor(track_id);
            }
            Message::PreviewTrackVolume(track_id, volume_db) => {
                self.preview_track_mix(track_id, TrackMixParameter::Volume, volume_db);
            }
            Message::CancelTrackMixGesture => self.cancel_track_mix_gesture(),
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
                self.begin_track_draft(track_id, TrackDraftField::Volume);
            }
            Message::TrackPanTextChanged(track_id, value) => {
                self.track_pan_edits.insert(track_id, value);
                self.begin_track_draft(track_id, TrackDraftField::Pan);
            }
            Message::CommitTrackVolumeText(track_id) => {
                self.commit_track_volume_text(track_id);
            }
            Message::CommitTrackPanText(track_id) => {
                self.commit_track_pan_text(track_id);
            }
            Message::ResetTrackVolume(track_id) => self.reset_track_volume(track_id, false),
            Message::ResetTrackVolumeByDoubleClick(track_id) => {
                self.reset_track_volume(track_id, true);
            }
            Message::ResetTrackPan(track_id) => self.reset_track_pan(track_id, false),
            Message::ResetTrackPanByDoubleClick(track_id) => {
                self.reset_track_pan(track_id, true);
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
            Message::DuplicateSelectedItems => self.duplicate_selected_items(),
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
                let tempo_before = self.project.tempo_points().collect::<Vec<_>>();
                let meter_before = self.project.time_signature_map();
                self.undo();
                let tempo_changed = self.project.tempo_points().ne(tempo_before);
                let meter_changed = self.project.time_signature_map() != meter_before;
                if meter_changed && self.tempo_map_window_id.is_some() {
                    self.refresh_meter_map_edits();
                }
                if tempo_changed {
                    if self.tempo_map_window_id.is_some() {
                        self.refresh_tempo_map_edits();
                    }
                    #[cfg(feature = "audio-device")]
                    if self.playback.is_some() {
                        task = self.prepare_playback(self.playhead_sample, self.playback_playing);
                    }
                }
                self.sync_all_track_mix_to_playback();
            }
            Message::Redo => {
                self.active_menu = None;
                let tempo_before = self.project.tempo_points().collect::<Vec<_>>();
                let meter_before = self.project.time_signature_map();
                self.redo();
                let tempo_changed = self.project.tempo_points().ne(tempo_before);
                let meter_changed = self.project.time_signature_map() != meter_before;
                if meter_changed && self.tempo_map_window_id.is_some() {
                    self.refresh_meter_map_edits();
                }
                if tempo_changed {
                    if self.tempo_map_window_id.is_some() {
                        self.refresh_tempo_map_edits();
                    }
                    #[cfg(feature = "audio-device")]
                    if self.playback.is_some() {
                        task = self.prepare_playback(self.playhead_sample, self.playback_playing);
                    }
                }
                self.sync_all_track_mix_to_playback();
            }
            Message::ActionQueryChanged(query) => {
                self.action_query = query;
                if self.active_menu == Some(MainMenu::Actions) {
                    self.menu_selected_command =
                        commands::matching_actions_menu(self, &self.action_query)
                            .into_iter()
                            .find(|entry| entry.enabled)
                            .map(|entry| entry.id);
                    self.action_menu_scroll_offset = 0.0;
                    task = scroll_widget_to("aaadaw-actions-menu-results", 0.0);
                }
            }
            Message::ActionMenuScrolled(offset) => {
                self.action_menu_scroll_offset = offset.max(0.0);
            }
            Message::ActionMacroNameChanged(name) => self.action_macro_name = name,
            Message::ActionMacroStepSelected(step) => self.action_macro_step = Some(step),
            Message::AddActionMacroStep => self.add_action_macro_step(),
            Message::RemoveActionMacroStep(index) => {
                if index < self.action_macro_steps.len() {
                    self.action_macro_steps.remove(index);
                }
            }
            Message::MoveActionMacroStep(index, direction) => {
                let destination = index.saturating_add_signed(direction);
                if index < self.action_macro_steps.len()
                    && destination < self.action_macro_steps.len()
                {
                    self.action_macro_steps.swap(index, destination);
                }
            }
            Message::NewActionMacro => {
                self.action_macro_editing_id = None;
                self.action_macro_name.clear();
                self.action_macro_steps.clear();
                self.action_macro_step = commands::macro_step_choices()
                    .first()
                    .map(|choice| choice.id.clone());
                self.action_macro_feedback.clear();
            }
            Message::EditActionMacro(id) => self.edit_action_macro(id),
            Message::SaveActionMacro => self.save_action_macro(),
            Message::DeleteActionMacro(id) => self.delete_action_macro(id),
            Message::ShortcutPressed(key, modifiers) => {
                if self.pending_project_transition.is_some() {
                    return Task::none();
                }
                let key = match key.as_str() {
                    " " => iced::keyboard::Key::Named(iced::keyboard::key::Named::Space),
                    "Delete" => iced::keyboard::Key::Named(iced::keyboard::key::Named::Delete),
                    character => iced::keyboard::Key::Character(character),
                };
                let shortcut = {
                    let bindings = self
                        .shortcut_bindings
                        .read()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    commands::from_shortcut(&key, modifiers, &bindings, &self.action_macros)
                        .map(Message::ExecuteCommand)
                };
                if let Some(message) = shortcut {
                    task = self.update(message);
                }
            }
            Message::SaveShortcutBindings => self.save_shortcut_bindings(),
            Message::ResetShortcutBindings => self.reset_shortcut_bindings(),
            Message::RunActionQuery => task = self.run_action_query(),
            Message::ExecuteCommand(command) => {
                if self.pending_project_transition.is_none() {
                    task = commands::dispatch(self, command);
                }
            }
            Message::ToggleMediaBrowserPanel => self.media_panel_dock.toggle(),
            Message::MediaPanelResized(split, ratio) => {
                self.media_panel_dock.resize(split, ratio);
            }
            Message::PickPath(target) => task = self.pick_path(target),
            Message::PathPicked(target, result) => task = self.path_picked(target, result),
            Message::CancelOfflineRender => self.cancel_offline_render(),
            Message::RemoveQueuedOfflineJob(id) => self.remove_queued_offline_job(id),
            Message::OfflineRenderFinished(result) => {
                task = self.finish_offline_render(result);
            }
            Message::FreezeTrack(track_id) => task = self.start_freeze_track(track_id),
            Message::FreezeTrackFinished(result) => task = self.finish_freeze_track(result),
            Message::UnfreezeTrack(track_id) => {
                self.apply_action(DawAction::UnfreezeTrack { track_id }, "Track unfrozen")
            }
            Message::OpenProject => task = self.open_project_command(),
            Message::SaveProject => {
                task = self.save_project_command();
            }
            Message::SaveBeforeProjectTransition => {
                task = self.save_before_project_transition();
            }
            Message::DiscardProjectChanges => {
                task = self.discard_and_continue_project_transition();
            }
            Message::CancelProjectTransition => {
                self.pending_project_transition = None;
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
                self.update_offline_render_progress();
                let offline_queue_task = self.resume_offline_job_queue();
                let unsaved_snapshot_task = if self
                    .unsaved_session_snapshot_at
                    .is_some_and(|deadline| Instant::now() >= deadline)
                {
                    self.save_unsaved_session_snapshot()
                } else {
                    Task::none()
                };
                if self
                    .track_mix_commit_at
                    .is_some_and(|deadline| Instant::now() >= deadline)
                {
                    self.commit_track_mix_gesture();
                }
                #[cfg(feature = "audio-device")]
                {
                    self.poll_fx_automation_capture();
                    if self.fx_parameter_end_requested
                        && let Some(gesture) = self.fx_parameter_gesture.as_ref()
                    {
                        self.end_fx_parameter_gesture(gesture.parameter_id);
                    }
                    if self.fx_automation_finish_requested && self.fx_parameter_gesture.is_none() {
                        self.fx_automation_finish_requested = false;
                        self.finish_fx_automation_write();
                    }
                    if self.fx_automation_write_target.is_none() {
                        match self.pending_fx_automation_history.take() {
                            Some(FxAutomationHistoryAction::Undo) => self.undo(),
                            Some(FxAutomationHistoryAction::Redo) => self.redo(),
                            None => {}
                        }
                    }
                    if let Some(change) = self.pending_fx_parameter_sync {
                        self.sync_fx_parameter_change(change);
                    }
                    self.update_playback_stats();
                    self.follow_midi_editor_playhead();
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
                #[cfg(not(feature = "audio-device"))]
                {
                    self.update_audio_waveforms();
                    task = Task::batch([
                        self.update_audio_import(),
                        self.update_audio_asset_management(),
                    ]);
                }
                task = Task::batch([offline_queue_task, unsaved_snapshot_task, task]);
            }
            Message::MeterTick => {
                #[cfg(feature = "audio-device")]
                self.update_track_peak_levels();
            }
            Message::ClearTrackMeter(track_id) => {
                self.track_peak_levels.remove(&track_id);
                self.track_peak_holds.remove(&track_id);
            }
            Message::ClearMasterMeter => {
                self.master_peak_level = [0.0; 2];
                self.master_peak_hold = StereoPeakHold::default();
                #[cfg(feature = "audio-device")]
                if let Some(playback) = self.playback.as_ref() {
                    playback.reset_master_output_meter();
                }
            }
            Message::ClearInputMeter => {
                self.input_peak_level = [0.0; 2];
                self.input_peak_hold = StereoPeakHold::default();
                #[cfg(feature = "audio-device")]
                if let Some(recording) = self.recording.as_ref() {
                    let _ = recording.control.take_input_peak();
                }
                #[cfg(feature = "audio-device")]
                if let Some(playback) = self.playback.as_ref() {
                    let _ = playback.take_standby_input_peak();
                }
            }
            Message::ProjectLoaded(path, result) => {
                self.io_busy = false;
                let result = result.lock().ok().and_then(|mut result| result.take());
                match result {
                    Some(Ok((project, arrangement_view_state, project_lock))) => {
                        self.pending_project_transition = None;
                        self.project_lock = Some(project_lock);
                        self.project = project;
                        self.refresh_tempo_map_edits();
                        self.refresh_meter_map_edits();
                        self.tempo_map_feedback.clear();
                        self.meter_map_feedback.clear();
                        self.midi_note_clipboard.last_paste = None;
                        self.timeline
                            .replace_project(&self.project, arrangement_view_state.as_ref());
                        self.timeline.selected_item = None;
                        self.timeline.selected_track = None;
                        self.timeline.selected_tracks.clear();
                        self.timeline.time_selection = None;
                        self.timeline.origin_tick = 0;
                        self.timeline.edit_cursor_tick = 0;
                        self.timeline.vertical_scroll = 0.0;
                        task = Task::batch([
                            scroll_arrangement_to(timeline::TCP_SCROLL_ID, 0.0),
                            scroll_arrangement_to(timeline::TIMELINE_SCROLL_ID, 0.0),
                        ]);
                        self.clear_track_draft_state();
                        self.track_mix_gesture = None;
                        self.track_mix_commit_at = None;
                        self.audio_item_start_edits.clear();
                        self.audio_asset_source_statuses.clear();
                        self.project_path_query = path.to_string_lossy().into_owned();
                        self.project_path = Some(path.clone());
                        self.retire_unsaved_session_media();
                        self.unsaved_session_snapshot_revision = 0;
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
                    Some(Err(error)) => {
                        self.pending_project_transition = None;
                        self.project_path_query = self
                            .project_path
                            .as_ref()
                            .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
                        tracing::error!(error = %error, "project open failed");
                        self.status = format!("Open failed: {error}");
                    }
                    None => {
                        self.pending_project_transition = None;
                        self.project_path_query = self
                            .project_path
                            .as_ref()
                            .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
                        self.status = "Project open result was unavailable".to_owned();
                    }
                }
            }
            Message::ProjectSaved(path, revision, result, plugin_state_warning, shared_lock) => {
                self.io_busy = false;
                let mut continue_transition = false;
                match result {
                    Ok(()) => {
                        if let Some(lock) = shared_lock.take() {
                            self.project_lock = Some(lock);
                        }
                        self.project_path_query = path.to_string_lossy().into_owned();
                        self.project_path = Some(path.clone());
                        self.retire_unsaved_session_media();
                        self.unsaved_session_snapshot_revision = 0;
                        self.saved_revision = revision;
                        continue_transition =
                            self.revision == revision && self.pending_project_transition.is_some();
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
                    Err(error) => {
                        tracing::error!(error = %error, "project save failed");
                        self.status = format!("Save failed: {error}");
                    }
                }
                if continue_transition {
                    task = Task::batch([task, self.continue_pending_project_transition()]);
                }
            }
            Message::UnsavedSessionRecoveryScanned(result) => match result {
                Ok(candidates) => self.unsaved_session_recovery_candidates = candidates,
                Err(error) => {
                    tracing::warn!(%error, "unsaved session recovery scan failed");
                    self.status = format!("Unsaved session scan failed: {error}");
                }
            },
            Message::RecoverUnsavedSession(path) => {
                if self.project_path.is_some() || self.is_dirty() {
                    self.status = "Save or discard the current project before recovering a session"
                        .to_owned();
                } else if self.unsaved_session_recovery_busy {
                    self.status = "Wait for unsaved session recovery to finish".to_owned();
                } else {
                    self.unsaved_session_recovery_busy = true;
                    let message_path = path.clone();
                    task = Task::perform(
                        run_blocking("aaadaw-unsaved-session-recover", move || {
                            project_io::load_project_session(path)
                        }),
                        move |result| {
                            Message::UnsavedSessionRecovered(
                                message_path,
                                Arc::new(Mutex::new(Some(result))),
                            )
                        },
                    );
                }
            }
            Message::DiscardUnsavedSession(path) => {
                if self.unsaved_session_recovery_busy {
                    self.status = "Wait for unsaved session recovery to finish".to_owned();
                } else {
                    self.unsaved_session_recovery_busy = true;
                    let message_path = path.clone();
                    task = Task::perform(
                        run_blocking("aaadaw-unsaved-session-discard", move || {
                            project_io::discard_unsaved_session(path)
                        }),
                        move |result| Message::UnsavedSessionDiscarded(message_path, result),
                    );
                }
            }
            Message::UnsavedSessionRecovered(path, result) => {
                self.unsaved_session_recovery_busy = false;
                let result = result.lock().ok().and_then(|mut result| result.take());
                match result {
                    Some(Ok((project, arrangement_view_state, lock))) => {
                        self.retire_unsaved_session_media();
                        let Some(directory) = path.parent().map(PathBuf::from) else {
                            self.status = "Recovered session path is invalid".to_owned();
                            return Task::none();
                        };
                        self.session_media_dir = Some(UnsavedSessionMedia {
                            directory,
                            lock: Some(lock),
                        });
                        self.project = project;
                        self.project_path = None;
                        self.project_path_query.clear();
                        self.project_lock = None;
                        self.project_generation = self.project_generation.wrapping_add(1);
                        self.revision = 1;
                        self.saved_revision = 0;
                        self.unsaved_session_snapshot_revision = self.revision;
                        self.unsaved_session_recovery_candidates
                            .retain(|candidate| candidate != &path);
                        self.timeline
                            .replace_project(&self.project, arrangement_view_state.as_ref());
                        self.clear_track_draft_state();
                        self.start_audio_waveform_scan(true);
                        self.status = "Unsaved project session recovered".to_owned();
                    }
                    Some(Err(error)) => {
                        tracing::error!(path = %path.display(), %error, "unsaved session recovery failed");
                        self.status = format!("Session recovery failed: {error}");
                    }
                    None => self.status = "Session recovery result was unavailable".to_owned(),
                }
            }
            Message::UnsavedSessionDiscarded(path, result) => {
                self.unsaved_session_recovery_busy = false;
                match result {
                    Ok(()) => {
                        self.unsaved_session_recovery_candidates
                            .retain(|candidate| candidate != &path);
                        self.status = "Unsaved session discarded".to_owned();
                    }
                    Err(error) => {
                        self.status = format!("Session discard failed: {error}");
                    }
                }
            }
            Message::UnsavedSessionSnapshotSaved(generation, revision, result) => {
                self.unsaved_session_snapshot_busy = false;
                if generation == self.project_generation {
                    match result {
                        Ok(()) => {
                            self.unsaved_session_snapshot_revision =
                                self.unsaved_session_snapshot_revision.max(revision);
                        }
                        Err(error) => {
                            tracing::error!(%error, "unsaved session snapshot save failed");
                            self.status = format!("Session recovery snapshot failed: {error}");
                        }
                    }
                    if self.project_path.is_none()
                        && self.session_media_dir.is_some()
                        && self.revision > revision
                    {
                        self.unsaved_session_snapshot_at =
                            Some(Instant::now() + Duration::from_millis(300));
                    }
                }
                self.retired_unsaved_sessions.clear();
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
                            tracing::error!(error = %error, "recording recovery scan failed");
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
                    Err(error) => {
                        tracing::error!(error = %error, "recording recovery discard failed");
                        self.status = format!("Recording discard failed: {error}");
                    }
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
                    tracing::error!(error = %error, "recording recovery cleanup failed");
                    self.status = format!(
                        "Project saved, but recording source cleanup failed; saving again will retry: {error}"
                    );
                }
            },
            #[cfg(feature = "audio-device")]
            Message::StartPlayback => task = self.start_playback(),
            #[cfg(feature = "audio-device")]
            Message::StopPlayback => {
                self.release_midi_preview();
                self.finish_fx_automation_write();
                if self.recording.is_some() {
                    task = self.stop_recording();
                } else {
                    task = self.stop_transport_to_start();
                }
            }
            #[cfg(feature = "audio-device")]
            Message::PanicMidi => self.panic_midi(),
            #[cfg(feature = "audio-device")]
            Message::StartRecording => task = self.start_recording(),
            #[cfg(feature = "audio-device")]
            Message::StopRecording => task = self.stop_recording(),
            #[cfg(feature = "audio-device")]
            Message::RecordingStarted(result) => task = self.finish_recording_start(result),
            #[cfg(feature = "audio-device")]
            Message::StandbyInputStarted(track_id, generation, result) => {
                task = self.finish_standby_input_start(track_id, generation, result);
            }
            #[cfg(feature = "audio-device")]
            Message::StandbyInputClosed | Message::RecordingInputDiscarded => {
                self.standby_monitor_starting = false;
                if let Some(track_id) = self.standby_monitor_track {
                    task = self.start_standby_input(track_id);
                }
            }
            #[cfg(feature = "audio-device")]
            Message::RecordingPositionSaved(result) => {
                task = self.finish_recording_position_saved(result);
            }
            #[cfg(feature = "audio-device")]
            Message::RecordingClockAnchorReady(result) => {
                task = self.finish_recording_clock_anchor_ready(result);
            }
            #[cfg(feature = "audio-device")]
            Message::RecordingStopped(result) => task = self.finish_recording_stop(result),
            #[cfg(feature = "audio-device")]
            Message::RestartPlayback => task = self.restart_playback(),
            #[cfg(feature = "audio-device")]
            Message::SeekSampleChanged(sample) => self.seek_sample_query = sample,
            #[cfg(feature = "audio-device")]
            Message::SeekToItem(sample) => {
                self.seek_sample_query = sample.to_string();
                task = self.prepare_playback(sample, self.playback_playing);
            }
            #[cfg(feature = "audio-device")]
            Message::SeekToSample => task = self.seek_to_sample(),
            #[cfg(feature = "audio-device")]
            Message::ClosePlayback => task = self.close_playback(),
            #[cfg(feature = "audio-device")]
            Message::PlaybackPrepared {
                target_sample,
                start_when_ready,
                result,
            } => {
                self.finish_playback_preparation(target_sample, start_when_ready, result);
                if self.pending_recording.is_some() {
                    task = self.begin_pending_recording();
                } else if self.recording_starting && self.recording.is_none() {
                    if self.recording_cancel_requested || self.playback.is_none() {
                        self.recording_starting = false;
                        self.recording_cancel_requested = false;
                        self.recording_cancelled_transport_start = false;
                        if self.playback.is_none() {
                            self.status =
                                "Recording could not start because audio output is unavailable"
                                    .to_owned();
                        } else {
                            self.status = "Recording setup cancelled".to_owned();
                        }
                    } else {
                        self.recording_starting = false;
                        task = self.start_recording();
                    }
                }
                if let Some(track_id) = self.standby_monitor_track {
                    if self.playback.is_some() && !self.recording_starting {
                        let playback = self.playback.as_ref().expect("playback exists");
                        if playback.has_standby_input() {
                            if playback.set_track_input_monitor(track_id, true) {
                                self.standby_monitor_track = None;
                                self.status = "Input monitoring enabled for armed track".to_owned();
                            } else {
                                self.standby_monitor_track = None;
                                self.status = "Input monitor route is unavailable".to_owned();
                            }
                        } else {
                            task = self.start_standby_input(track_id);
                        }
                    } else if self.playback.is_none() {
                        self.standby_monitor_track = None;
                        self.status =
                            "Standby input could not start because audio output is unavailable"
                                .to_owned();
                    }
                }
            }
            #[cfg(any(
                all(feature = "jack-backend", feature = "pipewire-backend"),
                all(
                    feature = "jack-backend",
                    feature = "cpal-backend",
                    any(target_os = "windows", target_os = "macos")
                ),
                all(
                    feature = "pipewire-backend",
                    feature = "cpal-backend",
                    any(target_os = "windows", target_os = "macos")
                )
            ))]
            Message::SelectPlaybackBackend(backend) => {
                if self.playback.is_some() {
                    self.status = "Close the current output before switching backends".to_owned();
                } else {
                    self.playback_backend = backend;
                    let settings = audio_config::AudioSettings {
                        playback_backend: Some(Self::playback_backend_setting(backend)),
                        ..self.audio_settings.clone()
                    };
                    match audio_config::save(&settings) {
                        Ok(()) => {
                            self.status = format!("{} selected for playback", backend.name());
                            self.last_audio_backend_failure = None;
                            self.audio_settings_feedback =
                                format!("{} selected for playback", backend.name());
                        }
                        Err(error) => {
                            self.status = format!(
                                "{} selected for this session but could not be saved: {error}",
                                backend.name()
                            );
                            self.audio_settings_feedback = self.status.clone();
                        }
                    }
                    self.audio_settings = settings;
                }
            }
            #[cfg(all(target_os = "linux", feature = "audio-device"))]
            Message::RefreshLinuxAudioRoutes => task = self.refresh_linux_audio_routes(),
            #[cfg(all(target_os = "linux", feature = "audio-device"))]
            Message::LinuxAudioRoutesRefreshed(generation, backend, result) => {
                if generation == self.linux_audio_routes_generation {
                    self.linux_audio_routes_busy = false;
                    self.linux_audio_route_report = Some((backend, result));
                }
            }
            #[cfg(all(target_os = "linux", feature = "audio-device"))]
            Message::ReconnectLinuxAudioBackend => {
                task = self.reconnect_linux_audio_backend();
            }
        }
        if self.revision > revision_before_message
            && self.project_path.is_none()
            && self.session_media_dir.is_some()
            && self.revision > self.unsaved_session_snapshot_revision
        {
            self.unsaved_session_snapshot_at = Some(Instant::now() + Duration::from_millis(500));
        }
        task
    }

    fn save_unsaved_session_snapshot(&mut self) -> Task<Message> {
        if self.unsaved_session_snapshot_busy {
            self.unsaved_session_snapshot_at = Some(Instant::now() + Duration::from_millis(300));
            return Task::none();
        }
        let Some(path) = self
            .session_media_dir
            .as_ref()
            .map(UnsavedSessionMedia::store_path)
        else {
            self.unsaved_session_snapshot_at = None;
            return Task::none();
        };
        let generation = self.project_generation;
        let revision = self.revision;
        let snapshot = self.project.snapshot();
        let view_state = self.timeline.arrangement_view_state(&self.project);
        self.unsaved_session_snapshot_at = None;
        self.unsaved_session_snapshot_busy = true;
        Task::perform(
            run_blocking("aaadaw-unsaved-session-snapshot", move || {
                project_io::save_unsaved_session_snapshot(path, snapshot, view_state)
            }),
            move |result| Message::UnsavedSessionSnapshotSaved(generation, revision, result),
        )
    }

    fn retire_unsaved_session_media(&mut self) {
        self.unsaved_session_snapshot_at = None;
        if let Some(session) = self.session_media_dir.take()
            && self.unsaved_session_snapshot_busy
        {
            self.retired_unsaved_sessions.push(session);
        }
    }

    fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
            || !self.track_name_edits.is_empty()
            || !self.midi_item_name_edits.is_empty()
            || !self.track_volume_edits.is_empty()
            || !self.track_pan_edits.is_empty()
    }

    fn handle_timeline_view_event(&mut self, event: timeline::TimelineEvent) {
        #[cfg(feature = "audio-device")]
        let edit_cursor_tick = match &event {
            timeline::TimelineEvent::SelectEmpty(tick)
            | timeline::TimelineEvent::SetEditCursor(tick) => Some(*tick),
            _ => None,
        };
        let previous_view_state = matches!(
            &event,
            timeline::TimelineEvent::ToggleVolumeAutomation(_)
                | timeline::TimelineEvent::ToggleFxAutomation { .. }
                | timeline::TimelineEvent::ResizeFxAutomationLane { .. }
        )
        .then(|| self.timeline.arrangement_view_state(&self.project));
        let item_trim_preview_changed = matches!(
            &event,
            timeline::TimelineEvent::BeginItemTrim { .. }
                | timeline::TimelineEvent::UpdateItemTrim { .. }
        );
        self.timeline.handle(event);
        if item_trim_preview_changed && let Some(preview) = self.timeline.item_trim_preview() {
            let valid = self.item_trim_preview_is_valid(preview);
            self.timeline.set_item_trim_valid(valid);
        }
        #[cfg(feature = "audio-device")]
        if let Some(tick) = edit_cursor_tick
            && !self.playback_playing
            && !self.playback_paused
            && !self.playback_busy
            && let Ok(sample) = self.project.sample_at_tick(tick)
        {
            self.playhead_sample = sample;
            self.playback_start_sample = sample;
            self.seek_sample_query = sample.to_string();
            self.playback_position_dirty = self.playback.is_some();
        }
        if previous_view_state
            .is_some_and(|previous| previous != self.timeline.arrangement_view_state(&self.project))
        {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn new_project(&mut self) {
        self.release_midi_preview();
        if self.io_busy
            || self.import_busy
            || self.audio_asset_management_busy
            || self.path_picker_busy
        {
            self.status = "Wait for the current project operation to finish".to_owned();
            return;
        }
        #[cfg(feature = "audio-device")]
        if self.playback.is_some() {
            self.status = format!(
                "Close {} output before creating a new project",
                self.playback_name()
            );
            return;
        }

        self.project = Project::new();
        self.refresh_tempo_map_edits();
        self.refresh_meter_map_edits();
        self.tempo_map_feedback.clear();
        self.meter_map_feedback.clear();
        self.project_generation = self.project_generation.wrapping_add(1);
        self.recording_recovery_candidates.clear();
        self.recording_recovery_scanning = false;
        self.midi_note_clipboard.last_paste = None;
        self.project_path = None;
        self.retire_unsaved_session_media();
        self.unsaved_session_snapshot_revision = 0;
        let session_media_error = match create_unsaved_project_session_dir() {
            Ok(session_dir) => {
                self.session_media_dir = Some(session_dir);
                None
            }
            Err(error) => {
                self.session_media_dir = None;
                Some(error)
            }
        };
        self.project_lock = None;
        self.project_path_query.clear();
        self.revision = 0;
        self.saved_revision = 0;
        self.timeline.replace_project(&self.project, None);
        self.timeline.selected_track = None;
        self.timeline.selected_tracks.clear();
        self.timeline.selected_item = None;
        self.timeline.selected_items.clear();
        self.timeline.time_selection = None;
        self.timeline.origin_tick = 0;
        self.timeline.edit_cursor_tick = 0;
        self.timeline.vertical_scroll = 0.0;
        self.clear_track_draft_state();
        self.track_mix_gesture = None;
        self.track_mix_commit_at = None;
        self.audio_item_start_edits.clear();
        self.audio_waveforms.clear();
        self.audio_asset_source_statuses.clear();
        self.status = new_project_status(session_media_error.as_deref());
    }

    fn begin_project_transition(&mut self, transition: PendingProjectTransition) -> Task<Message> {
        if self.pending_project_transition.is_some() {
            return Task::none();
        }
        if self.io_busy
            || self.unsaved_session_snapshot_busy
            || self.path_picker_busy
            || self.import_busy
            || self.audio_asset_management_busy
            || self.playback_busy()
        {
            self.status = "Wait for the current project operation to finish".to_owned();
            return Task::none();
        }
        #[cfg(feature = "audio-device")]
        if matches!(
            transition,
            PendingProjectTransition::NewProject | PendingProjectTransition::OpenProject
        ) && self.playback.is_some()
        {
            self.status = format!(
                "Close {} output before changing projects",
                self.playback_name()
            );
            return Task::none();
        }
        self.pending_project_transition = Some(transition);
        if self.is_dirty() {
            self.status = "Save changes to the current project?".to_owned();
            Task::none()
        } else {
            self.continue_pending_project_transition()
        }
    }

    fn save_before_project_transition(&mut self) -> Task<Message> {
        if self.pending_project_transition.is_none() {
            return Task::none();
        }
        self.save_project_command()
    }

    fn discard_and_continue_project_transition(&mut self) -> Task<Message> {
        match self.pending_project_transition {
            Some(PendingProjectTransition::NewProject) => {
                self.pending_project_transition = None;
                self.saved_revision = self.revision;
                self.new_project();
                Task::none()
            }
            Some(PendingProjectTransition::OpenProject) => {
                self.pick_path(PathPickerTarget::OpenProject)
            }
            Some(PendingProjectTransition::CloseMainWindow(window_id)) => {
                self.pending_project_transition = None;
                self.close_main_window(window_id)
            }
            None => Task::none(),
        }
    }

    fn continue_pending_project_transition(&mut self) -> Task<Message> {
        match self.pending_project_transition {
            Some(PendingProjectTransition::NewProject) => {
                self.pending_project_transition = None;
                self.new_project();
                Task::none()
            }
            Some(PendingProjectTransition::OpenProject) => {
                self.pick_path(PathPickerTarget::OpenProject)
            }
            Some(PendingProjectTransition::CloseMainWindow(window_id)) => {
                self.pending_project_transition = None;
                self.close_main_window(window_id)
            }
            None => Task::none(),
        }
    }

    fn close_main_window(&mut self, window_id: iced::window::Id) -> Task<Message> {
        self.close_fx_editor_resources();
        iced::window::close(window_id)
    }

    fn open_settings(&mut self) -> Task<Message> {
        if let Some(window_id) = self.settings_window_id {
            let focus = iced::window::gain_focus(window_id);
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))]
            return Task::batch([
                focus,
                self.refresh_cpal_output_devices(),
                self.refresh_cpal_input_devices(),
            ]);
            #[cfg(not(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            )))]
            return focus;
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(760.0, 620.0),
            min_size: Some(iced::Size::new(640.0, 460.0)),
            ..iced::window::Settings::default()
        });
        self.settings_window_id = Some(window_id);
        #[cfg(all(
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        ))]
        return Task::batch([
            task.discard(),
            self.refresh_cpal_output_devices(),
            self.refresh_cpal_input_devices(),
        ]);
        #[cfg(not(all(
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        )))]
        task.discard()
    }

    fn open_render_window(&mut self) -> Task<Message> {
        if let Some(window_id) = self.render_window_id {
            return iced::window::gain_focus(window_id);
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(540.0, 480.0),
            min_size: Some(iced::Size::new(420.0, 360.0)),
            ..iced::window::Settings::default()
        });
        self.render_window_id = Some(window_id);
        task.discard()
    }

    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    fn refresh_cpal_output_devices(&mut self) -> Task<Message> {
        start_cpal_device_enumeration(
            &mut self.cpal_output_devices_loading,
            &mut self.cpal_output_devices_error,
            "aaadaw-cpal-device-list",
            || aaadaw_engine::enumerate_cpal_output_devices().map_err(|error| error.to_string()),
            Message::CpalOutputDevicesLoaded,
        )
    }

    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    fn finish_cpal_output_device_enumeration(
        &mut self,
        result: Result<Vec<aaadaw_engine::CpalOutputDeviceInfo>, String>,
    ) {
        finish_cpal_device_enumeration(
            &mut self.cpal_output_devices,
            &mut self.cpal_output_devices_loading,
            &mut self.cpal_output_devices_error,
            &mut self.audio_settings_feedback,
            "output",
            result,
        );
    }

    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    fn refresh_cpal_input_devices(&mut self) -> Task<Message> {
        start_cpal_device_enumeration(
            &mut self.cpal_input_devices_loading,
            &mut self.cpal_input_devices_error,
            "aaadaw-cpal-input-device-list",
            || aaadaw_engine::enumerate_cpal_input_devices().map_err(|error| error.to_string()),
            Message::CpalInputDevicesLoaded,
        )
    }

    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    fn finish_cpal_input_device_enumeration(
        &mut self,
        result: Result<Vec<aaadaw_engine::CpalInputDeviceInfo>, String>,
    ) {
        finish_cpal_device_enumeration(
            &mut self.cpal_input_devices,
            &mut self.cpal_input_devices_loading,
            &mut self.cpal_input_devices_error,
            &mut self.audio_settings_feedback,
            "input",
            result,
        );
    }

    fn open_tempo_map(&mut self, tab: TimeMapTab) -> Task<Message> {
        self.time_map_tab = tab;
        if let Some(window_id) = self.tempo_map_window_id {
            return iced::window::gain_focus(window_id);
        }
        self.refresh_tempo_map_edits();
        self.refresh_meter_map_edits();
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(660.0, 420.0),
            min_size: Some(iced::Size::new(560.0, 320.0)),
            ..iced::window::Settings::default()
        });
        self.tempo_map_window_id = Some(window_id);
        task.discard()
    }

    fn refresh_tempo_map_edits(&mut self) {
        self.tempo_map_edits = self
            .project
            .tempo_points()
            .map(|(tick, bpm, curve)| TempoMapEdit {
                original_tick: Some(tick),
                tick: tick.to_string(),
                bpm: bpm.to_string(),
                curve,
            })
            .collect();
    }

    fn refresh_meter_map_edits(&mut self) {
        self.meter_map_edits = self
            .project
            .time_signature_points()
            .map(|(tick, signature)| MeterMapEdit {
                original_tick: Some(tick),
                tick: tick.to_string(),
                numerator: signature.numerator().to_string(),
                denominator: signature.denominator().to_string(),
            })
            .collect();
    }

    fn add_tempo_point_edit(&mut self) {
        let tick = self.timeline.edit_cursor_tick;
        if self
            .tempo_map_edits
            .iter()
            .any(|point| point.tick.parse::<u64>().ok() == Some(tick))
        {
            self.tempo_map_feedback = "A tempo point already exists at the edit cursor".to_owned();
            return;
        }
        let bpm = self.project.tempo_at_tick(tick);
        let point = TempoMapEdit {
            original_tick: None,
            tick: tick.to_string(),
            bpm: bpm.to_string(),
            curve: TempoCurve::Step,
        };
        let index = self
            .tempo_map_edits
            .partition_point(|existing| existing.tick.parse::<u64>().unwrap_or(u64::MAX) < tick);
        self.tempo_map_edits.insert(index, point);
        self.tempo_map_feedback.clear();
    }

    fn apply_tempo_map_edits(&mut self) -> Task<Message> {
        let mut desired = Vec::with_capacity(self.tempo_map_edits.len());
        for point in &self.tempo_map_edits {
            let (Ok(tick), Ok(bpm)) = (
                point.tick.trim().parse::<u64>(),
                point.bpm.trim().parse::<f64>(),
            ) else {
                self.tempo_map_feedback = "Enter a whole project tick and a numeric BPM".to_owned();
                return Task::none();
            };
            if !bpm.is_finite() || bpm <= 0.0 {
                self.tempo_map_feedback = "BPM must be finite and greater than zero".to_owned();
                return Task::none();
            }
            desired.push((tick, bpm, point.curve, point.original_tick));
        }
        desired.sort_by_key(|point| point.0);
        if desired.first().map(|point| point.0) != Some(0) {
            self.tempo_map_feedback = "The initial tempo point must stay at tick zero".to_owned();
            return Task::none();
        }
        if desired.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            self.tempo_map_feedback = "Tempo point positions must be unique".to_owned();
            return Task::none();
        }

        let current = self.project.tempo_points().collect::<Vec<_>>();
        let desired_ticks = desired.iter().map(|point| point.0).collect::<HashSet<_>>();
        let mut actions = current
            .iter()
            .filter(|(tick, _, _)| *tick != 0 && !desired_ticks.contains(tick))
            .map(|(start_tick, _, _)| DawAction::DeleteTempoPoint {
                start_tick: *start_tick,
            })
            .collect::<Vec<_>>();
        for (tick, bpm, curve, _) in &desired {
            if current
                .iter()
                .find(|(current_tick, _, _)| current_tick == tick)
                .is_none_or(|(_, current_bpm, _)| current_bpm != bpm)
            {
                actions.push(DawAction::SetTempo {
                    start_tick: *tick,
                    bpm: *bpm,
                });
            }
            let final_point = desired.last().is_some_and(|last| last.0 == *tick);
            let desired_curve = if final_point {
                TempoCurve::Step
            } else {
                *curve
            };
            if current
                .iter()
                .find(|(current_tick, _, _)| current_tick == tick)
                .is_none_or(|(_, _, current_curve)| current_curve != &desired_curve)
            {
                actions.push(DawAction::SetTempoCurve {
                    start_tick: *tick,
                    curve: desired_curve,
                });
            }
        }
        if actions.is_empty() {
            self.tempo_map_feedback = "Tempo map is unchanged".to_owned();
            return Task::none();
        }
        let previous_revision = self.revision;
        self.apply_action(
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            },
            "Tempo map edited",
        );
        if self.revision != previous_revision {
            self.refresh_tempo_map_edits();
            self.tempo_map_feedback = "Tempo map applied as one undoable edit".to_owned();
            #[cfg(feature = "audio-device")]
            if self.playback.is_some() {
                return self.prepare_playback(self.playhead_sample, self.playback_playing);
            }
        } else {
            self.tempo_map_feedback = self.status.clone();
        }
        Task::none()
    }

    fn add_meter_point_edit(&mut self) {
        let tick = self.timeline.edit_cursor_tick;
        if self
            .meter_map_edits
            .iter()
            .any(|point| point.tick.parse::<u64>().ok() == Some(tick))
        {
            self.meter_map_feedback =
                "A time-signature point already exists at the edit cursor".to_owned();
            return;
        }
        let signature = self.project.time_signature_at_tick(tick);
        let point = MeterMapEdit {
            original_tick: None,
            tick: tick.to_string(),
            numerator: signature.numerator().to_string(),
            denominator: signature.denominator().to_string(),
        };
        let index = self
            .meter_map_edits
            .partition_point(|existing| existing.tick.parse::<u64>().unwrap_or(u64::MAX) < tick);
        self.meter_map_edits.insert(index, point);
        self.meter_map_feedback.clear();
    }

    fn apply_meter_map_edits(&mut self) {
        let mut desired = Vec::with_capacity(self.meter_map_edits.len());
        for point in &self.meter_map_edits {
            let (Ok(start_tick), Ok(numerator), Ok(denominator)) = (
                point.tick.trim().parse::<u64>(),
                point.numerator.trim().parse::<u32>(),
                point.denominator.trim().parse::<u32>(),
            ) else {
                self.meter_map_feedback =
                    "Enter a whole project tick, numerator, and denominator".to_owned();
                return;
            };
            let signature = match TimeSignature::new(numerator, denominator) {
                Ok(signature) => signature,
                Err(error) => {
                    self.meter_map_feedback = error.to_string();
                    return;
                }
            };
            desired.push(MeterPointSnapshot {
                start_tick,
                numerator: signature.numerator(),
                denominator: signature.denominator(),
            });
        }
        desired.sort_by_key(|point| point.start_tick);
        if desired.first().map(|point| point.start_tick) != Some(0) {
            self.meter_map_feedback =
                "The initial time-signature point must stay at tick zero".to_owned();
            return;
        }
        if desired
            .windows(2)
            .any(|pair| pair[0].start_tick == pair[1].start_tick)
        {
            self.meter_map_feedback = "Time-signature point positions must be unique".to_owned();
            return;
        }
        if desired == self.project.time_signature_map() {
            self.meter_map_feedback = "Time-signature map is unchanged".to_owned();
            return;
        }

        let previous_revision = self.revision;
        self.apply_action(
            DawAction::SetTimeSignatureMap { points: desired },
            "Time-signature map edited",
        );
        if self.revision != previous_revision {
            self.refresh_meter_map_edits();
            self.meter_map_feedback = "Time-signature map applied as one undoable edit".to_owned();
        } else {
            self.meter_map_feedback = self.status.clone();
        }
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
            let item_changed = self.midi_editor_item_id != Some(item_id);
            if item_changed {
                self.release_midi_preview();
                self.midi_editor_origin_tick = 0;
                self.midi_editor_edit_cursor_tick = None;
            }
            self.midi_editor_item_id = Some(item_id);
            if item_changed {
                self.fit_midi_editor_to_item(item_id);
            }
            self.midi_editor_feedback = None;
            self.midi_editor_selected_notes.clear();
            return iced::window::gain_focus(window_id);
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(1000.0, 620.0),
            min_size: Some(iced::Size::new(720.0, 420.0)),
            ..iced::window::Settings::default()
        });
        self.midi_editor_window_id = Some(window_id);
        self.midi_editor_window_size = iced::Size::new(1000.0, 620.0);
        self.midi_editor_item_id = Some(item_id);
        self.midi_editor_feedback = None;
        self.midi_editor_selected_notes.clear();
        self.midi_editor_origin_tick = 0;
        self.midi_editor_edit_cursor_tick = None;
        self.midi_editor_high_pitch = 84;
        self.midi_editor_pitch_rows = 36;
        self.midi_editor_pitch_row_height = 18.0;
        self.midi_editor_pixels_per_beat = 96.0;
        self.fit_midi_editor_to_item(item_id);
        task.discard()
    }

    fn fit_midi_editor_to_item(&mut self, item_id: ItemId) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let ppq = u64::from(self.project.settings().ppq());
        let source_start_tick = item.source_offset_ticks();
        let source_end_tick = source_start_tick.saturating_add(item.length_ticks());
        let visible_notes = item
            .notes()
            .iter()
            .filter(|note| (source_start_tick..source_end_tick).contains(&note.tick()))
            .collect::<Vec<_>>();
        let first_tick = visible_notes
            .iter()
            .map(|note| note.tick().saturating_sub(source_start_tick))
            .min()
            .unwrap_or(0);
        let last_tick = visible_notes
            .iter()
            .map(|note| {
                note.tick()
                    .saturating_add(note.duration())
                    .min(source_end_tick)
                    .saturating_sub(source_start_tick)
            })
            .max()
            .unwrap_or(ppq.saturating_mul(4));
        let margin = ppq / 4;
        let origin_tick = first_tick.saturating_sub(margin);
        let visible_ticks = last_tick
            .saturating_add(margin)
            .saturating_sub(origin_tick)
            .max(ppq);
        let content_width =
            (self.midi_editor_window_size.width - MIDI_EDITOR_CONTENT_WIDTH_INSET).max(120.0);
        let beats = visible_ticks as f32 / ppq.max(1) as f32;
        self.midi_editor_origin_tick = origin_tick;
        self.midi_editor_pixels_per_beat = (content_width / beats).clamp(1.0, 300.0);
        let highest_pitch = visible_notes.iter().map(|note| note.pitch()).max();
        let lowest_pitch = visible_notes.iter().map(|note| note.pitch()).min();
        if let (Some(lowest_pitch), Some(highest_pitch)) = (lowest_pitch, highest_pitch) {
            let high_pitch = highest_pitch.saturating_add(2).min(127);
            let low_pitch = lowest_pitch.saturating_sub(2);
            let pitch_rows = high_pitch
                .saturating_sub(low_pitch)
                .saturating_add(1)
                .max(1);
            let available_height = (self.midi_editor_window_size.height - 260.0).max(80.0);
            self.midi_editor_high_pitch = high_pitch;
            self.midi_editor_pitch_rows = pitch_rows;
            self.midi_editor_pitch_row_height =
                (available_height / f32::from(pitch_rows)).clamp(2.0, 18.0);
        } else {
            self.midi_editor_high_pitch = 84;
            self.midi_editor_pitch_rows = 36;
            self.midi_editor_pitch_row_height = 18.0;
        }
    }

    fn midi_editor_playhead_tick(&self, item: &aaadaw_core::MidiItem) -> Option<u64> {
        #[cfg(feature = "audio-device")]
        {
            self.project
                .tick_at_sample(self.playhead_sample)
                .ok()
                .and_then(|tick| tick.checked_sub(item.start_tick()))
        }
        #[cfg(not(feature = "audio-device"))]
        {
            let _ = item;
            None
        }
    }

    #[cfg(feature = "audio-device")]
    fn follow_midi_editor_playhead(&mut self) {
        if !self.midi_editor_follow_playhead || !self.playback_playing {
            return;
        }
        let Some(item_id) = self.midi_editor_item_id else {
            return;
        };
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let Some(playhead_tick) = self.midi_editor_playhead_tick(item) else {
            return;
        };
        let visible_pixels =
            (self.midi_editor_window_size.width - MIDI_EDITOR_CONTENT_WIDTH_INSET).max(120.0);
        let visible_ticks = (visible_pixels / self.midi_editor_pixels_per_beat
            * self.project.settings().ppq() as f32) as u64;
        self.midi_editor_origin_tick =
            playhead_tick.saturating_sub(visible_ticks.saturating_mul(3) / 4);
    }

    fn clear_shortcut_binding(&mut self, action_id: String) {
        let mut candidate = self.shortcut_binding_edits.clone();
        candidate.insert(action_id.clone(), String::new());
        match commands::validate_bindings_with_macros(&candidate, &self.action_macros) {
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
        match commands::validate_bindings_with_macros(&candidate, &self.action_macros) {
            Ok(bindings) => {
                self.shortcut_binding_edits = bindings;
                self.shortcut_defaults_restored.insert(action_id.clone());
                self.shortcut_capture_id = None;
                self.shortcut_editor_feedback = format!(
                    "{} restored to its default. Save to apply this change.",
                    commands::label_for_id(self, &action_id).unwrap_or(action_id)
                );
            }
            Err(error) => {
                self.shortcut_editor_feedback = format!(
                    "Default shortcut could not be restored: {}",
                    commands::friendly_shortcut_error(&error, &self.action_macros)
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
        match commands::validate_bindings_with_macros(&candidate, &self.action_macros) {
            Ok(bindings) => {
                if let Some(action_id) = self.shortcut_capture_id.take() {
                    self.shortcut_defaults_restored.remove(&action_id);
                }
                self.shortcut_binding_edits = bindings;
                self.shortcut_editor_feedback = format!(
                    "Recorded {}. Save to apply this change.",
                    commands::format_shortcut_label(&binding)
                );
            }
            Err(error) => {
                let error = commands::friendly_shortcut_error(&error, &self.action_macros);
                self.shortcut_editor_feedback = format!(
                    "{}; press another key or Escape",
                    commands::format_shortcut_label(&error)
                );
            }
        }
    }

    #[cfg(feature = "audio-device")]
    fn selected_playback_backend(&self) -> PlaybackBackend {
        #[cfg(any(
            all(feature = "jack-backend", feature = "pipewire-backend"),
            all(
                feature = "jack-backend",
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ),
            all(
                feature = "pipewire-backend",
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            )
        ))]
        {
            self.playback_backend
        }
        #[cfg(all(
            feature = "jack-backend",
            not(feature = "pipewire-backend"),
            not(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))
        ))]
        {
            PlaybackBackend::Jack
        }
        #[cfg(all(
            feature = "pipewire-backend",
            not(feature = "jack-backend"),
            not(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))
        ))]
        {
            PlaybackBackend::PipeWire
        }
        #[cfg(all(
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos"),
            not(feature = "jack-backend"),
            not(feature = "pipewire-backend")
        ))]
        {
            PlaybackBackend::Cpal
        }
        #[cfg(all(
            not(feature = "jack-backend"),
            not(feature = "pipewire-backend"),
            not(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))
        ))]
        {
            PlaybackBackend::Unavailable
        }
    }

    #[cfg(any(
        all(feature = "jack-backend", feature = "pipewire-backend"),
        all(
            feature = "jack-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        ),
        all(
            feature = "pipewire-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        )
    ))]
    fn playback_backend_setting(backend: PlaybackBackend) -> audio_config::PlaybackBackendSetting {
        match backend {
            #[cfg(feature = "jack-backend")]
            PlaybackBackend::Jack => audio_config::PlaybackBackendSetting::Jack,
            #[cfg(feature = "pipewire-backend")]
            PlaybackBackend::PipeWire => audio_config::PlaybackBackendSetting::PipeWire,
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))]
            PlaybackBackend::Cpal => audio_config::PlaybackBackendSetting::Cpal,
        }
    }

    #[cfg(any(
        all(feature = "jack-backend", feature = "pipewire-backend"),
        all(
            feature = "jack-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        ),
        all(
            feature = "pipewire-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        )
    ))]
    fn restore_playback_backend(&mut self, setting: Option<audio_config::PlaybackBackendSetting>) {
        let Some(setting) = setting else {
            return;
        };
        self.playback_backend = match setting {
            #[cfg(feature = "jack-backend")]
            audio_config::PlaybackBackendSetting::Jack => PlaybackBackend::Jack,
            #[cfg(feature = "pipewire-backend")]
            audio_config::PlaybackBackendSetting::PipeWire => PlaybackBackend::PipeWire,
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos")
            ))]
            audio_config::PlaybackBackendSetting::Cpal => PlaybackBackend::Cpal,
            _ => self.playback_backend,
        };
    }

    #[cfg(feature = "audio-device")]
    fn playback_name(&self) -> &'static str {
        self.selected_playback_backend().name()
    }

    #[cfg(all(target_os = "linux", feature = "audio-device"))]
    fn refresh_linux_audio_routes(&mut self) -> Task<Message> {
        if self.linux_audio_routes_busy {
            self.status = "Audio route inspection is already running".to_owned();
            return Task::none();
        }
        let Some(playback) = self.playback.as_ref() else {
            let backend = self.selected_playback_backend();
            self.linux_audio_route_report = Some((
                backend,
                Err("Open playback to inspect AAADAW's current output route".to_owned()),
            ));
            return Task::none();
        };
        let backend = playback.backend();
        #[cfg(feature = "jack-backend")]
        let client_name = playback.jack_client_name().map(str::to_owned);
        #[cfg(not(feature = "jack-backend"))]
        let client_name = None;
        #[cfg(feature = "pipewire-backend")]
        let node_id = playback.pipewire_node_id();
        #[cfg(not(feature = "pipewire-backend"))]
        let node_id = None;
        #[cfg(feature = "jack-backend")]
        let input_client_name = self
            .recording
            .as_ref()
            .and_then(|recording| recording.input.jack_client_name())
            .map(str::to_owned);
        #[cfg(not(feature = "jack-backend"))]
        let input_client_name = None;
        #[cfg(feature = "pipewire-backend")]
        let input_node_id = self
            .recording
            .as_ref()
            .and_then(|recording| recording.input.pipewire_node_id());
        #[cfg(not(feature = "pipewire-backend"))]
        let input_node_id = None;
        let generation = self.linux_audio_routes_generation.wrapping_add(1);
        self.linux_audio_routes_generation = generation;
        self.linux_audio_routes_busy = true;
        let message_backend = backend;
        Task::perform(
            run_blocking("aaadaw-linux-audio-routes", move || {
                inspect_linux_audio_routes(
                    backend,
                    client_name,
                    node_id,
                    input_client_name,
                    input_node_id,
                )
            }),
            move |result| Message::LinuxAudioRoutesRefreshed(generation, message_backend, result),
        )
    }

    #[cfg(all(target_os = "linux", feature = "audio-device"))]
    fn reconnect_linux_audio_backend(&mut self) -> Task<Message> {
        if self.playback_playing || self.playback_paused {
            self.status = "Stop playback before reconnecting the audio backend".to_owned();
            return Task::none();
        }
        if self.playback_busy || self.io_busy {
            self.status = "Wait for current audio or project operation to finish".to_owned();
            return Task::none();
        }
        if self.recording.is_some() || self.recording_starting || self.recording_stopping {
            self.status = "Stop recording before reconnecting the audio backend".to_owned();
            return Task::none();
        }
        let target_sample = self.playhead_sample;
        let close_task = self.close_playback();
        self.linux_audio_routes_generation = self.linux_audio_routes_generation.wrapping_add(1);
        self.linux_audio_routes_busy = false;
        self.linux_audio_route_report = None;
        let prepare_task = self.prepare_playback(target_sample, false);
        Task::batch([close_task, prepare_task])
    }

    fn release_midi_preview(&self) {
        #[cfg(feature = "audio-device")]
        if let Some(playback) = &self.playback {
            playback.release_midi_preview();
        }
    }

    #[cfg(feature = "audio-device")]
    fn start_playback(&mut self) -> Task<Message> {
        self.release_midi_preview();
        self.recording_cancelled_transport_start = false;
        if self.playback.is_some() && !self.playback_playing && !self.playback_paused {
            self.playback_start_sample = self.playhead_sample;
        }
        if self.playback.is_some() && !self.playback_graph_dirty {
            self.clap_plugin_warnings.clear();
            if let Err(error) = self.persist_clap_plugin_states() {
                self.clap_plugin_warnings.push(format!(
                    "Some plugin state could not be captured; previously saved state was kept ({error})"
                ));
            }
        }
        if self.playback.is_some() && (self.playback_graph_dirty || self.playback_position_dirty) {
            return self.prepare_playback(self.playhead_sample, true);
        }
        if let Some(playback) = self.playback.as_mut() {
            return match playback.play() {
                Ok(()) => {
                    self.playback_playing = true;
                    self.playback_paused = false;
                    self.status = "Playback started".to_owned();
                    if !self.clap_plugin_warnings.is_empty() {
                        self.status.push_str("; ");
                        self.status.push_str(&self.clap_plugin_warnings.join("; "));
                    }
                    Task::none()
                }
                Err(error) => {
                    tracing::error!(backend = self.playback_name(), error = %error, "playback start failed");
                    self.status = format!("{} play failed: {error}", self.playback_name());
                    Task::none()
                }
            };
        }
        self.prepare_playback(self.playhead_sample, true)
    }

    #[cfg(feature = "audio-device")]
    fn pause_playback(&mut self) {
        self.release_midi_preview();
        let Some(playback) = self.playback.as_mut() else {
            self.status = format!("{} output is not open", self.playback_name());
            return;
        };
        match playback.stop() {
            Ok(()) => {
                self.playback_playing = false;
                self.playback_paused = true;
                self.playhead_sample = playback.stats().playhead_sample;
                self.status = "Playback paused".to_owned();
            }
            Err(error) => {
                tracing::error!(backend = self.playback_name(), error = %error, "playback stop failed");
                self.status = format!("{} stop failed: {error}", self.playback_name());
            }
        }
    }

    #[cfg(feature = "audio-device")]
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
                tracing::error!(backend = self.playback_name(), error = %error, "MIDI panic failed");
                self.status = format!("{} MIDI Panic failed: {error}", self.playback_name())
            }
        }
    }

    #[cfg(feature = "audio-device")]
    fn restart_playback(&mut self) -> Task<Message> {
        let can_prepare = !self.playback_busy && !self.io_busy && self.media_store_path().is_some();
        let task = self.prepare_playback(0, true);
        if can_prepare {
            self.playback_start_sample = 0;
            self.playback_paused = false;
        }
        task
    }

    #[cfg(feature = "audio-device")]
    fn stop_transport_to_start(&mut self) -> Task<Message> {
        if self.playback.is_none() {
            self.status = format!("{} output is not open", self.playback_name());
            return Task::none();
        }
        let was_playing = self.playback_playing;
        if was_playing {
            self.pause_playback();
        }
        self.playback_paused = false;
        let start_sample = self.playback_start_sample;
        if was_playing || self.playhead_sample != start_sample {
            self.prepare_playback(start_sample, false)
        } else {
            self.status = "Playback stopped".to_owned();
            Task::none()
        }
    }

    #[cfg(feature = "audio-device")]
    fn seek_to_sample(&mut self) -> Task<Message> {
        match self.seek_sample_query.trim().parse::<u64>() {
            Ok(target_sample) => {
                if !self.playback_playing {
                    self.playback_start_sample = target_sample;
                }
                self.prepare_playback(target_sample, self.playback_playing)
            }
            Err(error) => {
                self.status = format!("Invalid seek sample: {error}");
                Task::none()
            }
        }
    }

    #[cfg(feature = "audio-device")]
    fn close_playback(&mut self) -> Task<Message> {
        self.release_midi_preview();
        self.standby_monitor_track = None;
        self.standby_monitor_generation = self.standby_monitor_generation.wrapping_add(1);
        self.reset_track_meters();
        #[cfg(feature = "audio-device")]
        let state_error = self.persist_clap_plugin_states().err();
        let mut shutdown_error = None;
        let mut standby_input = None;
        if let Some(mut playback) = self.playback.take() {
            playback.disable_input_monitoring();
            standby_input = playback.take_standby_input();
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
        let helper_ids = self
            .clap_instrument_helper_owners
            .keys()
            .copied()
            .collect::<Vec<_>>();
        if let Some(cleanup_error) = self.discard_unused_instrument_owners(&helper_ids) {
            shutdown_error = Some(match shutdown_error.take() {
                Some(error) => format!("{error}; {cleanup_error}"),
                None => cleanup_error,
            });
        }
        self.playback_playing = false;
        self.playback_paused = false;
        self.playback_position_dirty = false;
        self.playhead_sample = 0;
        self.playback_start_sample = 0;
        self.seek_sample_query = "0".to_owned();
        self.status = shutdown_error.map_or_else(
            || format!("{} output closed", self.playback_name()),
            |error| format!("{} shutdown failed: {error}", self.playback_name()),
        );
        #[cfg(feature = "audio-device")]
        if let Some(error) = state_error {
            self.status
                .push_str(&format!("; plugin state save failed: {error}"));
        }
        standby_input.map_or_else(Task::none, |standby_input| {
            self.close_standby_input_async(standby_input)
        })
    }

    #[cfg(feature = "audio-device")]
    fn prepare_playback(&mut self, target_sample: u64, start_when_ready: bool) -> Task<Message> {
        if self.playback_busy || self.io_busy {
            self.status = "Wait for current operation to finish".to_owned();
            return Task::none();
        }
        let Some(path) = self.media_store_path() else {
            self.status = "Temporary project media storage is unavailable".to_owned();
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

    #[cfg(feature = "audio-device")]
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
                tracing::error!(error = %error, "playback preparation failed");
                self.status = format!("Playback preparation failed: {error}");
                return;
            }
            None => {
                self.status = "Playback preparation result was unavailable".to_owned();
                return;
            }
        };
        prepared.set_master_output_ceiling_dbfs(self.audio_settings.master_output_ceiling);

        let instrument_owner_ids = match self.install_track_instrument_processors(&mut prepared) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::error!(error = %error, "playback instrument preparation failed");
                self.status = format!("Playback preparation failed: {error}");
                return;
            }
        };

        let fx_owner_ids = match self.install_track_fx_processors(&mut prepared) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::error!(error = %error, "playback effect preparation failed");
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
            self.release_midi_preview();
            let (result, play_result, retired_instruments, retired_effects) = {
                let playback = self.playback.as_mut().expect("playback exists");
                let result = playback.replace_graph(prepared);
                let play_result = if result.is_ok() && start_when_ready {
                    playback.play()
                } else {
                    Ok(())
                };
                playback.collect_retired_graphs();
                (
                    result,
                    play_result,
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
                    self.playback_position_dirty = false;
                    self.reset_track_meters();
                    if let Err(error) = play_result {
                        self.playback_playing = false;
                        let backend = self.playback_name().to_owned();
                        self.last_audio_backend_failure =
                            Some((self.selected_playback_backend(), error.to_string()));
                        self.status = format!("{backend} play failed: {error}");
                        return;
                    }
                    self.playback_playing = start_when_ready;
                    self.playback_paused = !start_when_ready && self.playback_paused;
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

        #[cfg(all(
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        ))]
        let output_result = prepared.into_output(
            self.selected_playback_backend(),
            self.audio_settings.cpal_output_device_id.as_deref(),
        );
        #[cfg(not(all(
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        )))]
        let output_result = prepared.into_output(self.selected_playback_backend(), None);
        let mut playback = match output_result {
            Ok(playback) => playback,
            Err(error) => {
                tracing::error!(backend = self.playback_name(), error = %error, "audio output setup failed");
                self.last_audio_backend_failure =
                    Some((self.selected_playback_backend(), error.to_string()));
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
        let retired_helper_ids = self
            .clap_instrument_helper_owners
            .keys()
            .copied()
            .filter(|id| !instrument_owner_ids.contains(id))
            .collect::<Vec<_>>();
        if let Some(error) = self.discard_unused_instrument_owners(&retired_helper_ids) {
            self.clap_plugin_warnings.push(error);
        }
        self.reset_track_meters();
        self.playback_graph_dirty = false;
        self.playback_position_dirty = false;
        self.playback_playing = start_when_ready && play_error.is_none();
        self.playback_paused = false;
        if start_when_ready {
            self.playback_start_sample = target_sample;
        }
        self.playhead_sample = target_sample;
        self.seek_sample_query = target_sample.to_string();
        if let Some(error) = play_error {
            tracing::error!(backend = self.playback_name(), error = %error, "playback start failed");
            self.last_audio_backend_failure =
                Some((self.selected_playback_backend(), error.to_string()));
            self.status = format!("{} play failed: {error}", self.playback_name());
            return;
        }
        self.last_audio_backend_failure = None;
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

    #[cfg(feature = "audio-device")]
    fn update_playback_stats(&mut self) {
        let (retired_instruments, retired_effects, output_device_lost) =
            if let Some(playback) = self.playback.as_mut() {
                let stats = playback.stats();
                self.playhead_sample = stats.playhead_sample;
                playback.collect_retired_graphs();
                (
                    playback.take_retired_instrument_processors(),
                    playback.take_retired_fx_processors(),
                    stats.output_device_lost,
                )
            } else {
                (Vec::new(), Vec::new(), false)
            };
        self.deactivate_stopped_instruments(retired_instruments);
        self.deactivate_stopped_effects(retired_effects);
        let mut new_helper_failures = Vec::new();
        for (instance_id, owner) in self.clap_instrument_helper_owners.iter_mut() {
            let _ = owner.try_wait();
            owner.is_stalled(Duration::from_secs(5));
            let faulted = owner.region().is_faulted();
            let underruns = owner.region().underrun_count();
            if (faulted || underruns > 0)
                && !self
                    .clap_plugin_warnings
                    .iter()
                    .any(|warning| warning.contains(&format!("helper {instance_id}")))
            {
                let (track_id, plugin_id) = self
                    .clap_instrument_helper_targets
                    .get(instance_id)
                    .map(|(track_id, plugin_id)| (format!("{track_id:?}"), plugin_id.as_str()))
                    .unwrap_or_else(|| ("unknown track".to_owned(), "unknown plugin"));
                new_helper_failures.push(if faulted {
                    format!(
                        "Isolated CLAP instrument {plugin_id} on {track_id} failed (helper {instance_id}, fault {}); that track is silent",
                        owner.region().fault_code()
                    )
                } else {
                    format!(
                        "Isolated CLAP instrument {plugin_id} on {track_id} missed {underruns} audio blocks (helper {instance_id}); late blocks are silent"
                    )
                });
            }
            let gui_status = owner.gui_status();
            if matches!(gui_status, 2 | 3)
                && !self
                    .clap_plugin_warnings
                    .iter()
                    .any(|warning| warning.contains(&format!("editor (helper {instance_id})")))
            {
                let (track_id, plugin_id) = self
                    .clap_instrument_helper_targets
                    .get(instance_id)
                    .map(|(track_id, plugin_id)| (format!("{track_id:?}"), plugin_id.as_str()))
                    .unwrap_or_else(|| ("unknown track".to_owned(), "unknown plugin"));
                new_helper_failures.push(if gui_status == 2 {
                    format!("CLAP instrument {plugin_id} on {track_id} does not support a native editor (helper {instance_id})")
                } else {
                    format!("CLAP instrument {plugin_id} on {track_id} could not open its native editor (helper {instance_id})")
                });
            }
        }
        if self.playback.is_none() {
            let retired_ids = self
                .clap_instrument_helper_targets
                .iter()
                .filter_map(|(instance_id, (track_id, plugin_id))| {
                    let still_assigned = self
                        .project
                        .tracks()
                        .iter()
                        .find(|track| track.id() == *track_id)
                        .and_then(|track| track.instrument())
                        .is_some_and(|instrument| instrument.plugin_id() == plugin_id);
                    (!still_assigned).then_some(*instance_id)
                })
                .collect::<Vec<_>>();
            if let Some(error) = self.discard_unused_instrument_owners(&retired_ids) {
                self.clap_plugin_warnings.push(error);
            }
        }
        if !new_helper_failures.is_empty() {
            self.clap_plugin_warnings
                .extend(new_helper_failures.iter().cloned());
            self.status.push_str("; ");
            self.status.push_str(&new_helper_failures.join("; "));
        }
        if output_device_lost {
            self.handle_playback_device_lost();
        }
    }

    #[cfg(feature = "audio-device")]
    fn handle_playback_device_lost(&mut self) {
        tracing::error!(
            backend = self.playback_name(),
            "playback output device was lost"
        );
        self.playback_playing = false;
        self.playback_paused = false;
        self.reset_track_meters();
        self.status = "System audio output device unavailable; playback stopped. Close playback and reopen it after selecting an available device".to_owned();
    }

    #[cfg(feature = "audio-device")]
    fn update_track_peak_levels(&mut self) {
        let meter_active =
            self.playback.as_ref().is_some_and(|playback| {
                self.playback_playing || playback.has_enabled_input_monitor()
            }) || self.recording.is_some();
        if !meter_active {
            self.reset_track_meters();
            return;
        }

        let observed_input = self
            .recording
            .as_ref()
            .map(|recording| recording.control.take_input_peak())
            .or_else(|| {
                self.playback
                    .as_ref()
                    .and_then(|playback| playback.take_standby_input_peak())
            })
            .unwrap_or([0.0; 2]);
        for (channel, observed) in observed_input.into_iter().enumerate() {
            self.input_peak_level[channel] = observed.max(self.input_peak_level[channel] * 0.96);
        }
        self.input_peak_hold.observe(observed_input);

        if let Some(playback) = self.playback.as_ref() {
            let observed = playback.take_master_output_peak();
            for (level, peak) in self.master_peak_level.iter_mut().zip(observed) {
                *level = peak.max(*level * 0.96);
            }
            self.master_peak_hold.observe(observed);
            if playback.take_master_guard_active() {
                self.master_guard_ticks_remaining = 15;
            } else {
                self.master_guard_ticks_remaining =
                    self.master_guard_ticks_remaining.saturating_sub(1);
            }
        }

        for track in self.project.tracks() {
            let levels = self.track_peak_levels.entry(track.id()).or_default();
            let observed = self
                .playback
                .as_ref()
                .and_then(|playback| playback.take_track_peak(track.id()))
                .unwrap_or([0.0; 2]);
            for channel in 0..2 {
                levels[channel] = observed[channel].max(levels[channel] * 0.96);
            }
            self.track_peak_holds
                .entry(track.id())
                .or_default()
                .observe(observed);
        }
        self.track_peak_levels.retain(|track_id, _| {
            self.project
                .tracks()
                .iter()
                .any(|track| track.id() == *track_id)
        });
        self.track_peak_holds.retain(|track_id, _| {
            self.project
                .tracks()
                .iter()
                .any(|track| track.id() == *track_id)
        });
    }

    #[cfg(feature = "audio-device")]
    fn reset_track_meters(&mut self) {
        self.track_peak_levels.clear();
        self.input_peak_level = [0.0; 2];
        self.master_peak_level = [0.0; 2];
        self.master_guard_ticks_remaining = 0;
        #[cfg(feature = "audio-device")]
        if let Some(playback) = self.playback.as_ref() {
            playback.reset_track_peaks();
            playback.reset_master_output_meter();
        }
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
        if was_empty
            && self.revision != previous_revision
            && let Some(track_id) = self.project.tracks().first().map(|track| track.id())
        {
            self.timeline.select_track_only(track_id);
        }
    }

    fn add_midi_item(&mut self) {
        let Some(track_id) = self.selected_track_id() else {
            self.status = if self.project.tracks().is_empty() {
                "Edit failed: add a track before creating a MIDI item".to_owned()
            } else {
                "Select a regular track before creating a MIDI item".to_owned()
            };
            return;
        };
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            return;
        };
        if track.is_bus() {
            self.status = "Select a regular track before creating a MIDI item".to_owned();
            return;
        }

        let start_tick = self.timeline.edit_cursor_tick;
        let length_ticks = self
            .timeline
            .time_selection
            .map_or(u64::from(self.project.settings().ppq()) * 4, |selection| {
                selection.end_tick - selection.start_tick
            });
        self.apply_action(
            DawAction::InsertMidiItem {
                track_id,
                start_tick,
                length_ticks,
            },
            "MIDI item created",
        );
    }

    fn add_bus_track(&mut self) {
        let index = self.project.tracks().len();
        let bus_number = self
            .project
            .tracks()
            .iter()
            .filter(|track| track.is_bus())
            .count()
            + 1;
        let track_id = self.project.tracks().get(index).map(|track| track.id());
        let previous_revision = self.revision;
        self.apply_action(
            DawAction::CreateBusTrack {
                index,
                name: format!("Bus {bus_number}"),
            },
            "Bus track created",
        );
        if self.revision != previous_revision
            && let Some(track_id) = self
                .project
                .tracks()
                .last()
                .map(|track| track.id())
                .or(track_id)
        {
            self.timeline.select_track_only(track_id);
        }
    }

    fn apply_edit<E: std::fmt::Display>(&mut self, action: Result<DawAction, E>, success: &str) {
        match action {
            Ok(action) => self.apply_action(action, success),
            Err(error) => self.status = format!("Edit failed: {error}"),
        }
    }

    fn apply_action(&mut self, action: DawAction, success: &str) {
        #[cfg(feature = "audio-device")]
        let rebuild_playback_graph = action_rebuilds_playback_graph(&action);
        let live_mix_track = match &action {
            DawAction::SetTrackVolume { track_id, .. }
            | DawAction::SetTrackPan { track_id, .. } => Some(*track_id),
            _ => None,
        };
        let live_mute_solo_track = match &action {
            DawAction::SetTrackMute { track_id, .. } | DawAction::SetTrackSolo { track_id, .. } => {
                Some(*track_id)
            }
            _ => None,
        };
        self.status = match self.project.apply(action) {
            Ok(()) => {
                self.midi_note_clipboard.last_paste = None;
                self.revision = self.revision.wrapping_add(1);
                self.timeline.rebuild(&self.project);
                let live_midi_item_ids = self
                    .project
                    .midi_items()
                    .iter()
                    .map(|item| item.id())
                    .collect::<HashSet<_>>();
                self.midi_item_name_edits
                    .retain(|item_id, _| live_midi_item_ids.contains(item_id));
                self.midi_item_name_errors
                    .retain(|item_id, _| live_midi_item_ids.contains(item_id));
                #[cfg(feature = "audio-device")]
                if rebuild_playback_graph {
                    self.playback_graph_dirty = true;
                }
                if let Some(track_id) = live_mix_track {
                    self.sync_track_mix_to_playback(track_id);
                }
                if let Some(track_id) = live_mute_solo_track {
                    self.sync_track_mute_solo_to_playback(track_id);
                }
                success.to_owned()
            }
            Err(error) => format!("Action failed: {error}"),
        };
    }

    fn sync_track_mix_to_playback(&self, track_id: TrackId) {
        #[cfg(feature = "audio-device")]
        if let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            && let Some(playback) = &self.playback
        {
            let _ = playback.set_track_mix(track_id, track.volume_db(), track.pan());
        }
        #[cfg(not(feature = "audio-device"))]
        let _ = track_id;
    }

    fn sync_track_mute_solo_to_playback(&self, track_id: TrackId) {
        #[cfg(feature = "audio-device")]
        if let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            && let Some(playback) = &self.playback
        {
            let _ = playback.set_track_mute_solo(track_id, track.is_muted(), track.is_solo());
        }
        #[cfg(not(feature = "audio-device"))]
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
                let discarded_draft = self.track_volume_edits.remove(&track_id).is_some();
                self.clear_track_draft(track_id, TrackDraftField::Volume);
                if discarded_draft {
                    self.status = "Track volume draft discarded by slider adjustment".to_owned();
                }
            }
            TrackMixParameter::Pan => {
                let discarded_draft = self.track_pan_edits.remove(&track_id).is_some();
                self.clear_track_draft(track_id, TrackDraftField::Pan);
                if discarded_draft {
                    self.status = "Track pan draft discarded by slider adjustment".to_owned();
                }
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
        #[cfg(feature = "audio-device")]
        if let Some(playback) = &self.playback
            && !playback.set_track_mix(track_id, gesture.after_volume_db, gesture.after_pan)
        {
            self.status = "Could not update the active track mix".to_owned();
        }
    }

    fn finish_track_mix_gesture(&mut self, track_id: TrackId, parameter: TrackMixParameter) {
        let Some(gesture) = self.track_mix_gesture else {
            return;
        };
        if gesture.track_id != track_id || gesture.parameter != parameter {
            return;
        }
        let (before, after) = match parameter {
            TrackMixParameter::Volume => (gesture.before_volume_db, gesture.after_volume_db),
            TrackMixParameter::Pan => (gesture.before_pan, gesture.after_pan),
        };
        if (before - after).abs() < f32::EPSILON {
            self.track_mix_gesture = None;
            self.track_mix_commit_at = None;
            self.sync_track_mix_to_playback(track_id);
            return;
        }
        self.track_mix_commit_at = Some(Instant::now() + Duration::from_millis(350));
    }

    fn commit_track_mix_gesture(&mut self) {
        self.track_mix_commit_at = None;
        let Some(gesture) = self.track_mix_gesture.take() else {
            return;
        };
        let (before, after) = match gesture.parameter {
            TrackMixParameter::Volume => (gesture.before_volume_db, gesture.after_volume_db),
            TrackMixParameter::Pan => (gesture.before_pan, gesture.after_pan),
        };
        if (before - after).abs() < f32::EPSILON {
            self.sync_track_mix_to_playback(gesture.track_id);
            return;
        }
        let action = match gesture.parameter {
            TrackMixParameter::Volume => DawAction::SetTrackVolume {
                track_id: gesture.track_id,
                volume_db: gesture.after_volume_db,
            },
            TrackMixParameter::Pan => DawAction::SetTrackPan {
                track_id: gesture.track_id,
                pan: gesture.after_pan,
            },
        };
        self.apply_action(action, "Track mix changed");
    }

    fn track_mix_commit_is_pending(&self, track_id: TrackId, parameter: TrackMixParameter) -> bool {
        self.track_mix_commit_at.is_some()
            && self.track_mix_gesture.is_some_and(|gesture| {
                gesture.track_id == track_id && gesture.parameter == parameter
            })
    }

    fn clear_track_mix_for_reset(
        &mut self,
        track_id: TrackId,
        parameter: TrackMixParameter,
        double_click: bool,
    ) {
        if double_click && self.track_mix_commit_is_pending(track_id, parameter) {
            self.track_mix_gesture = None;
            self.track_mix_commit_at = None;
        } else {
            self.cancel_track_mix_gesture();
        }
    }

    fn cancel_track_mix_gesture(&mut self) {
        self.track_mix_commit_at = None;
        if let Some(_gesture) = self.track_mix_gesture.take() {
            #[cfg(feature = "audio-device")]
            if let Some(playback) = &self.playback {
                let _ = playback.set_track_mix(
                    _gesture.track_id,
                    _gesture.before_volume_db,
                    _gesture.before_pan,
                );
            }
        }
    }

    fn begin_track_draft(&mut self, track_id: TrackId, field: TrackDraftField) {
        self.active_track_draft = Some((track_id, field));
        self.track_draft_errors.remove(&(track_id, field));
        self.status = "Track field edit pending: Enter applies, Escape discards; blur keeps the draft and Save commits valid edits".to_owned();
    }

    fn clear_track_draft(&mut self, track_id: TrackId, field: TrackDraftField) {
        self.track_draft_errors.remove(&(track_id, field));
        if self.active_track_draft == Some((track_id, field)) {
            self.active_track_draft = None;
        }
    }

    fn cancel_active_track_draft(&mut self) -> bool {
        let Some((track_id, field)) = self.active_track_draft.take() else {
            return false;
        };
        match field {
            TrackDraftField::Name => {
                self.track_name_edits.remove(&track_id);
            }
            TrackDraftField::Volume => {
                self.track_volume_edits.remove(&track_id);
            }
            TrackDraftField::Pan => {
                self.track_pan_edits.remove(&track_id);
            }
        }
        self.track_draft_errors.remove(&(track_id, field));
        self.status = "Track field draft discarded".to_owned();
        true
    }

    fn clear_track_draft_state(&mut self) {
        self.track_name_edits.clear();
        self.midi_item_name_edits.clear();
        self.midi_item_name_errors.clear();
        self.track_volume_edits.clear();
        self.track_pan_edits.clear();
        self.active_track_draft = None;
        self.track_draft_errors.clear();
    }

    fn clear_track_drafts(&mut self, track_id: TrackId) {
        self.track_name_edits.remove(&track_id);
        self.track_volume_edits.remove(&track_id);
        self.track_pan_edits.remove(&track_id);
        self.track_draft_errors
            .retain(|(edited_track_id, _), _| *edited_track_id != track_id);
        if self
            .active_track_draft
            .is_some_and(|(edited_track_id, _)| edited_track_id == track_id)
        {
            self.active_track_draft = None;
        }
    }

    pub(super) fn commit_pending_track_drafts(&mut self) -> bool {
        let mut actions = Vec::new();
        let mut first_error = None;

        for track in self.project.tracks() {
            let track_id = track.id();
            if let Some(text) = self.track_name_edits.get(&track_id) {
                match parse_track_name_draft(text) {
                    Ok(name) if name != track.name() => {
                        actions.push(DawAction::SetTrackName { track_id, name });
                    }
                    Ok(_) => {}
                    Err(error) => {
                        first_error = Some((track_id, TrackDraftField::Name, error));
                        break;
                    }
                }
            }
            if let Some(text) = self.track_volume_edits.get(&track_id) {
                match parse_track_volume_draft(text) {
                    Ok(volume_db) if (volume_db - track.volume_db()).abs() >= f32::EPSILON => {
                        actions.push(DawAction::SetTrackVolume {
                            track_id,
                            volume_db,
                        });
                    }
                    Ok(_) => {}
                    Err(error) => {
                        first_error = Some((track_id, TrackDraftField::Volume, error));
                        break;
                    }
                }
            }
            if let Some(text) = self.track_pan_edits.get(&track_id) {
                match parse_track_pan_draft(text) {
                    Ok(pan) if (pan - track.pan()).abs() >= f32::EPSILON => {
                        actions.push(DawAction::SetTrackPan { track_id, pan });
                    }
                    Ok(_) => {}
                    Err(error) => {
                        first_error = Some((track_id, TrackDraftField::Pan, error));
                        break;
                    }
                }
            }
        }

        let mut first_midi_item_error = None;
        for item in self.project.midi_items() {
            let Some(text) = self.midi_item_name_edits.get(&item.id()) else {
                continue;
            };
            match parse_midi_item_name_draft(text) {
                Ok(name) if name != item.name() => {
                    actions.push(DawAction::SetMidiItemName {
                        item_id: item.id(),
                        name,
                    });
                }
                Ok(_) => {}
                Err(error) => {
                    first_midi_item_error = Some((item.id(), error));
                    break;
                }
            }
        }

        if let Some((item_id, error)) = first_midi_item_error {
            self.midi_item_name_errors.insert(item_id, error.to_owned());
            self.status =
                format!("Cannot save MIDI item name: {error}; correct it or clear the draft");
            return false;
        }

        if let Some((track_id, field, error)) = first_error {
            self.track_draft_errors
                .insert((track_id, field), error.to_owned());
            let track_name = self
                .project
                .tracks()
                .iter()
                .find(|track| track.id() == track_id)
                .map_or("track", |track| track.name());
            self.status = format!(
                "Cannot save: {track_name} {} — {error}; correct it or press Escape to discard",
                field.label()
            );
            return false;
        }

        if !actions.is_empty() {
            let previous_revision = self.revision;
            let action = if actions.len() == 1 {
                actions.pop().expect("one pending track edit action exists")
            } else {
                DawAction::BatchTransaction {
                    tx_id: self.revision,
                    actions,
                }
            };
            self.apply_action(action, "Track field edits applied");
            if self.revision == previous_revision {
                return false;
            }
        }
        self.clear_track_draft_state();
        true
    }

    fn commit_midi_item_name(&mut self, item_id: ItemId) {
        let Some(text) = self.midi_item_name_edits.get(&item_id).cloned() else {
            return;
        };
        let name = match parse_midi_item_name_draft(&text) {
            Ok(name) => name,
            Err(error) => {
                self.midi_item_name_errors.insert(item_id, error.to_owned());
                self.status = error.to_owned();
                return;
            }
        };
        let Some(current_name) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
            .map(|item| item.name().to_owned())
        else {
            self.midi_item_name_edits.remove(&item_id);
            self.midi_item_name_errors.remove(&item_id);
            self.status = "MIDI item no longer exists".to_owned();
            return;
        };
        self.midi_item_name_edits.remove(&item_id);
        self.midi_item_name_errors.remove(&item_id);
        if name == current_name {
            self.status = "MIDI item name unchanged".to_owned();
            return;
        }
        self.apply_action(
            DawAction::SetMidiItemName { item_id, name },
            "MIDI item renamed",
        );
    }

    fn commit_track_volume_text(&mut self, track_id: TrackId) {
        let Some(text) = self.track_volume_edits.get(&track_id).cloned() else {
            return;
        };
        let value = match parse_track_volume_draft(&text) {
            Ok(value) => value,
            Err(error) => {
                self.track_draft_errors
                    .insert((track_id, TrackDraftField::Volume), error.to_owned());
                self.status = error.to_owned();
                return;
            }
        };
        self.cancel_track_mix_gesture();
        self.track_volume_edits.remove(&track_id);
        self.clear_track_draft(track_id, TrackDraftField::Volume);
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
        let value = match parse_track_pan_draft(&text) {
            Ok(value) => value,
            Err(error) => {
                self.track_draft_errors
                    .insert((track_id, TrackDraftField::Pan), error.to_owned());
                self.status = error.to_owned();
                return;
            }
        };
        self.cancel_track_mix_gesture();
        self.track_pan_edits.remove(&track_id);
        self.clear_track_draft(track_id, TrackDraftField::Pan);
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

    fn reset_track_volume(&mut self, track_id: TrackId, double_click: bool) {
        self.clear_track_mix_for_reset(track_id, TrackMixParameter::Volume, double_click);
        let discarded_draft = self.track_volume_edits.remove(&track_id).is_some();
        self.clear_track_draft(track_id, TrackDraftField::Volume);
        if discarded_draft {
            self.status = "Track volume draft discarded by reset".to_owned();
        }
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
        } else {
            self.sync_track_mix_to_playback(track_id);
        }
    }

    fn reset_track_pan(&mut self, track_id: TrackId, double_click: bool) {
        self.clear_track_mix_for_reset(track_id, TrackMixParameter::Pan, double_click);
        let discarded_draft = self.track_pan_edits.remove(&track_id).is_some();
        self.clear_track_draft(track_id, TrackDraftField::Pan);
        if discarded_draft {
            self.status = "Track pan draft discarded by reset".to_owned();
        }
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
        } else {
            self.sync_track_mix_to_playback(track_id);
        }
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

    fn finish_item_trim(&mut self) {
        let preview = self.timeline.item_trim_preview();
        self.timeline.handle(timeline::TimelineEvent::EndItemTrim);
        let Some(preview) = preview else {
            return;
        };
        match self.item_trim_action(preview) {
            Ok(Some((action, status))) => self.apply_action(action, status),
            Ok(None) => {}
            Err(error) => self.status = format!("Trim rejected: {error}"),
        }
    }

    fn item_trim_preview_is_valid(&self, preview: timeline::ItemTrimPreview) -> bool {
        if self
            .project
            .midi_items()
            .iter()
            .any(|item| item.id() == preview.item_id)
        {
            let item = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == preview.item_id)
                .expect("MIDI item was just found");
            return self.midi_item_trim_bounds(item, preview).is_ok();
        }
        let Some(item) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == preview.item_id)
        else {
            return false;
        };
        self.audio_item_trim_bounds(item, preview).is_ok()
    }

    fn item_trim_action(
        &self,
        preview: timeline::ItemTrimPreview,
    ) -> Result<Option<(DawAction, &'static str)>, String> {
        if let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == preview.item_id)
        {
            let (start_tick, length_ticks, source_offset_ticks) =
                self.midi_item_trim_bounds(item, preview)?;
            if item.start_tick() == start_tick
                && item.length_ticks() == length_ticks
                && item.source_offset_ticks() == source_offset_ticks
            {
                return Ok(None);
            }
            let action = if preview.edge == timeline::ItemTrimEdge::Start {
                DawAction::TrimMidiItemStart {
                    item_id: item.id(),
                    start_tick,
                    length_ticks,
                    source_offset_ticks,
                }
            } else {
                DawAction::EditMidiItem {
                    item_id: item.id(),
                    start_tick,
                    length_ticks,
                }
            };
            return Ok(Some((action, "MIDI item trimmed")));
        }
        let item = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == preview.item_id)
            .ok_or_else(|| format!("item {} no longer exists", preview.item_id.value()))?;
        let Some((start_sample, source_offset_samples, length_samples)) =
            self.audio_item_trim_bounds(item, preview)?
        else {
            return Ok(None);
        };
        Ok(Some((
            DawAction::EditAudioItem {
                item_id: preview.item_id,
                media_ref: item.media_ref().to_owned(),
                start_sample,
                source_offset_samples,
                length_samples,
            },
            "Audio item trimmed",
        )))
    }

    fn audio_item_trim_bounds(
        &self,
        item: &aaadaw_core::AudioItem,
        preview: timeline::ItemTrimPreview,
    ) -> Result<Option<(u64, u64, u64)>, String> {
        let old_start = item.start_sample();
        let old_end = old_start
            .checked_add(item.length_samples())
            .ok_or_else(|| "audio item range overflows sample time".to_owned())?;
        let start_sample = self
            .project
            .sample_at_tick(preview.start_tick)
            .map_err(|error| error.to_string())?;
        let end_sample = self
            .project
            .sample_at_tick(preview.end_tick)
            .map_err(|error| error.to_string())?;
        if start_sample >= end_sample {
            return Err("Audio Items must remain at least one sample long".to_owned());
        }
        if start_sample == old_start && end_sample == old_end {
            return Ok(None);
        }
        let waveform = self.audio_waveforms.get(item.media_ref()).ok_or_else(|| {
            "Audio source bounds are unavailable until waveform scanning finishes".to_owned()
        })?;
        let project_rate = u64::from(self.project.settings().sample_rate());
        let source_rate = u64::from(waveform.sample_rate());
        let start_delta = i128::from(start_sample) - i128::from(old_start);
        let source_start_delta = scale_project_samples_to_source_frames_toward_zero(
            start_delta,
            source_rate,
            project_rate,
        )?;
        let source_offset_samples = i128::from(item.source_offset_samples())
            .checked_add(source_start_delta)
            .and_then(|offset| u64::try_from(offset).ok())
            .ok_or_else(|| "Audio trim exceeds the start of the source".to_owned())?;
        let source_length = scale_project_samples_to_source_frames_ceil(
            i128::from(end_sample - start_sample),
            source_rate,
            project_rate,
        )?;
        let source_end = i128::from(source_offset_samples)
            .checked_add(source_length)
            .and_then(|end| u64::try_from(end).ok())
            .ok_or_else(|| "Audio trim exceeds the end of the source".to_owned())?;
        if source_offset_samples >= source_end || source_end > waveform.frame_count() {
            return Err("Audio trim must stay within the source media".to_owned());
        }
        let length_samples = end_sample - start_sample;
        Ok(Some((start_sample, source_offset_samples, length_samples)))
    }

    fn midi_item_trim_bounds(
        &self,
        item: &aaadaw_core::MidiItem,
        preview: timeline::ItemTrimPreview,
    ) -> Result<(u64, u64, u64), String> {
        let length_ticks = preview
            .end_tick
            .checked_sub(preview.start_tick)
            .filter(|length| *length > 0)
            .ok_or_else(|| "MIDI Items must remain at least one tick long".to_owned())?;
        let source_offset_ticks = if preview.edge == timeline::ItemTrimEdge::Start {
            let delta = i128::from(preview.start_tick) - i128::from(item.start_tick());
            u64::try_from(i128::from(item.source_offset_ticks()) + delta)
                .map_err(|_| "MIDI trim cannot extend before the source content".to_owned())?
        } else {
            item.source_offset_ticks()
        };
        source_offset_ticks
            .checked_add(length_ticks)
            .ok_or_else(|| "MIDI trim exceeds the source content range".to_owned())?;
        Ok((preview.start_tick, length_ticks, source_offset_ticks))
    }

    fn playback_busy(&self) -> bool {
        #[cfg(feature = "audio-device")]
        {
            self.playback_busy
        }
        #[cfg(not(feature = "audio-device"))]
        {
            false
        }
    }

    fn playback_active(&self) -> bool {
        #[cfg(feature = "audio-device")]
        {
            playback_prevents_project_edits(self.playback.is_some(), self.playback_playing)
        }
        #[cfg(not(feature = "audio-device"))]
        {
            false
        }
    }

    fn volume_automation_edit_busy(&self) -> bool {
        self.project_graph_edit_busy()
    }

    fn project_graph_edit_busy(&self) -> bool {
        #[cfg(feature = "audio-device")]
        let recording =
            self.recording.is_some() || self.recording_starting || self.recording_stopping;
        #[cfg(not(feature = "audio-device"))]
        let recording = false;
        self.playback_busy() || self.playback_active() || recording
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

            if preview.copy {
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
                    actions.push(DawAction::InsertAudioItem {
                        track_id: target_track_id,
                        media_ref: item.media_ref().to_owned(),
                        start_sample: target_sample,
                        source_offset_samples: item.source_offset_samples(),
                        length_samples: item.length_samples(),
                    });
                } else {
                    actions.push(DawAction::DuplicateMidiItemToTrack {
                        item_id,
                        track_id: target_track_id,
                        start_tick: target_start_tick,
                    });
                }
                continue;
            }

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
        self.clear_track_drafts(track_id);
        self.apply_action(DawAction::DeleteTrack { track_id }, "Track deleted");
    }

    fn commit_track_name(&mut self, track_id: TrackId) {
        let Some(name) = self.track_name_edits.get(&track_id).cloned() else {
            return;
        };
        let name = match parse_track_name_draft(&name) {
            Ok(name) => name,
            Err(error) => {
                self.track_draft_errors
                    .insert((track_id, TrackDraftField::Name), error.to_owned());
                self.status = error.to_owned();
                return;
            }
        };
        let Some(current_name) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            .map(|track| track.name().to_owned())
        else {
            self.track_name_edits.remove(&track_id);
            self.clear_track_draft(track_id, TrackDraftField::Name);
            self.status = "Track no longer exists".to_owned();
            return;
        };
        self.track_name_edits.remove(&track_id);
        self.clear_track_draft(track_id, TrackDraftField::Name);
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
        let selected_fx_instance_id = self.selected_fx_chain_instance_id();
        #[cfg(feature = "audio-device")]
        let tempo_before = self.project.tempo_points().collect::<Vec<_>>();
        #[cfg(feature = "audio-device")]
        let fx_automation_before = fx_parameter_automation_schedule(&self.project);
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
        #[cfg(feature = "audio-device")]
        if self.fx_automation_write_target.is_some() {
            self.pending_fx_automation_history = Some(FxAutomationHistoryAction::Undo);
            self.finish_fx_automation_write();
            if self.fx_automation_write_target.is_some() {
                self.status = "Waiting for the FX automation take to finish before undo".to_owned();
                return;
            }
            self.pending_fx_automation_history = None;
        }
        self.audio_item_start_edits.clear();
        self.status = match self.project.undo() {
            Ok(true) => {
                self.restore_fx_chain_selection(selected_fx_instance_id);
                self.midi_note_clipboard.last_paste = None;
                self.revision = self.revision.wrapping_add(1);
                self.timeline.rebuild(&self.project);
                #[cfg(feature = "audio-device")]
                {
                    self.playback_graph_dirty = true;
                }
                #[cfg(feature = "audio-device")]
                if self.project.tempo_points().ne(tempo_before) {
                    self.playback_graph_dirty = true;
                }
                #[cfg(feature = "audio-device")]
                if fx_automation_before != fx_parameter_automation_schedule(&self.project) {
                    self.playback_graph_dirty = true;
                }
                self.sync_fx_parameter_cache_from_project();
                "Action undone".to_owned()
            }
            Ok(false) => "Nothing to undo".to_owned(),
            Err(error) => format!("Undo failed: {error}"),
        };
    }

    fn redo(&mut self) {
        let selected_fx_instance_id = self.selected_fx_chain_instance_id();
        #[cfg(feature = "audio-device")]
        let tempo_before = self.project.tempo_points().collect::<Vec<_>>();
        #[cfg(feature = "audio-device")]
        let fx_automation_before = fx_parameter_automation_schedule(&self.project);
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
        #[cfg(feature = "audio-device")]
        if self.fx_automation_write_target.is_some() {
            self.pending_fx_automation_history = Some(FxAutomationHistoryAction::Redo);
            self.finish_fx_automation_write();
            if self.fx_automation_write_target.is_some() {
                self.status = "Waiting for the FX automation take to finish before redo".to_owned();
                return;
            }
            self.pending_fx_automation_history = None;
        }
        self.audio_item_start_edits.clear();
        self.status = match self.project.redo() {
            Ok(true) => {
                self.restore_fx_chain_selection(selected_fx_instance_id);
                self.midi_note_clipboard.last_paste =
                    self.midi_editor_item_id.and_then(|item_id| {
                        self.selected_clipboard_paste_start(item_id)
                            .map(|start_tick| (item_id, start_tick))
                    });
                self.revision = self.revision.wrapping_add(1);
                self.timeline.rebuild(&self.project);
                #[cfg(feature = "audio-device")]
                {
                    self.playback_graph_dirty = true;
                }
                #[cfg(feature = "audio-device")]
                if self.project.tempo_points().ne(tempo_before) {
                    self.playback_graph_dirty = true;
                }
                #[cfg(feature = "audio-device")]
                if fx_automation_before != fx_parameter_automation_schedule(&self.project) {
                    self.playback_graph_dirty = true;
                }
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
        matches_clipboard.then_some(start_tick.saturating_sub(item.source_offset_ticks()))
    }

    fn duplicate_selected_midi_notes(&mut self, item_id: ItemId) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let source_start_tick = item.source_offset_ticks();
        let item_end_tick = source_start_tick.saturating_add(item.length_ticks());
        let item_length = item.length_ticks();
        let selected = item
            .notes()
            .iter()
            .filter(|note| self.midi_editor_selected_notes.contains(&note.id()))
            .map(|note| {
                (
                    note.id(),
                    aaadaw_core::MidiNoteData {
                        pitch: note.pitch(),
                        tick: note.tick(),
                        duration: note.duration(),
                        velocity: note.velocity(),
                    },
                )
            })
            .collect::<Vec<_>>();
        let Some(source_start) = selected.iter().map(|(_, note)| note.tick).min() else {
            return;
        };
        let source_end = selected
            .iter()
            .map(|(_, note)| note.tick.saturating_add(note.duration))
            .max()
            .unwrap_or(source_start);
        let span = source_end.saturating_sub(source_start);
        let grid = self.midi_editor_snap_interval();
        let target = snap_tick_up(source_end, grid);
        let duplicates = selected
            .iter()
            .map(|(_, note)| aaadaw_core::MidiNoteData {
                tick: target.saturating_add(note.tick.saturating_sub(source_start)),
                ..*note
            })
            .collect::<Vec<_>>();
        if duplicates.iter().any(|note| {
            note.tick < source_start_tick || note.tick.saturating_add(note.duration) > item_end_tick
        }) {
            let feedback = "Duplicate rejected: notes would extend beyond the MIDI item";
            self.status = feedback.to_owned();
            self.midi_editor_feedback = Some(feedback.to_owned());
            return;
        }

        let old_ids = item
            .notes()
            .iter()
            .map(|note| note.id())
            .collect::<HashSet<_>>();
        let revision = self.revision;
        self.midi_editor_feedback = None;
        self.apply_action(
            DawAction::AddMidiNotes {
                item_id,
                notes: duplicates,
            },
            "MIDI notes duplicated",
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
            self.midi_note_clipboard.last_paste = None;
            self.midi_editor_edit_cursor_tick = Some(
                snap_tick_up(target.saturating_add(span), grid)
                    .saturating_sub(source_start_tick)
                    .min(item_length),
            );
        }
    }

    fn midi_editor_snap_interval(&self) -> u64 {
        if self.timeline.snap_enabled {
            self.timeline
                .snap_grid
                .tick_interval(self.project.settings().ppq())
                .unwrap_or(1)
        } else {
            1
        }
    }

    fn midi_editor_paste_target_tick(&self, item_id: Option<ItemId>) -> u64 {
        let grid = self.midi_editor_snap_interval();
        if let Some((_, previous)) = self
            .midi_note_clipboard
            .last_paste
            .filter(|(last_item, _)| Some(*last_item) == item_id)
        {
            return snap_tick_up(
                previous.saturating_add(self.midi_note_clipboard.span_ticks),
                grid,
            );
        }
        self.midi_editor_edit_cursor_tick
            .or(self.midi_note_clipboard.default_paste_tick)
            .map_or(0, |tick| tick.saturating_add(grid / 2) / grid * grid)
    }

    fn open_main_menu(&mut self, menu: MainMenu) -> Task<Message> {
        self.active_menu = Some(menu);
        self.menu_selected_command = self.menu_command_ids(menu).first().copied();
        self.action_menu_scroll_offset = 0.0;
        if menu == MainMenu::Actions {
            Task::batch([
                iced::widget::operation::focus(messages::action_search_input_id()),
                scroll_widget_to("aaadaw-actions-menu-results", 0.0),
            ])
        } else {
            Task::none()
        }
    }

    fn menu_command_ids(&self, menu: MainMenu) -> Vec<CommandId> {
        let entries = if menu == MainMenu::Actions {
            commands::matching_actions_menu(self, &self.action_query)
        } else {
            commands::for_menu(self, menu)
        };
        entries
            .into_iter()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.id)
            .collect()
    }

    fn navigate_main_menu(&mut self, navigation: MenuNavigation) -> Task<Message> {
        const MENUS: [MainMenu; 7] = [
            MainMenu::File,
            MainMenu::Edit,
            MainMenu::View,
            MainMenu::Insert,
            MainMenu::Item,
            MainMenu::Track,
            MainMenu::Actions,
        ];
        match navigation {
            MenuNavigation::Open => {
                if self.active_menu.is_some() {
                    self.active_menu = None;
                    self.menu_selected_command = None;
                    Task::none()
                } else {
                    self.open_main_menu(MENUS[0])
                }
            }
            MenuNavigation::NextMenu | MenuNavigation::PreviousMenu => {
                let direction = if navigation == MenuNavigation::NextMenu {
                    1_isize
                } else {
                    -1_isize
                };
                let current = self
                    .active_menu
                    .and_then(|active| MENUS.iter().position(|menu| *menu == active))
                    .unwrap_or(0);
                let next = if direction > 0 {
                    (current + 1) % MENUS.len()
                } else {
                    (current + MENUS.len() - 1) % MENUS.len()
                };
                self.open_main_menu(MENUS[next])
            }
            MenuNavigation::NextCommand | MenuNavigation::PreviousCommand => {
                let Some(menu) = self.active_menu else {
                    return Task::none();
                };
                let commands = self.menu_command_ids(menu);
                if commands.is_empty() {
                    self.menu_selected_command = None;
                    return Task::none();
                }
                let direction = if navigation == MenuNavigation::NextCommand {
                    1_isize
                } else {
                    -1_isize
                };
                let current = self
                    .menu_selected_command
                    .and_then(|selected| commands.iter().position(|command| *command == selected));
                let next = current.map_or_else(
                    || if direction > 0 { 0 } else { commands.len() - 1 },
                    |index| {
                        if direction > 0 {
                            (index + 1) % commands.len()
                        } else {
                            (index + commands.len() - 1) % commands.len()
                        }
                    },
                );
                self.menu_selected_command = Some(commands[next]);
                if menu == MainMenu::Actions {
                    scroll_actions_menu_selection(self, commands[next])
                } else {
                    Task::none()
                }
            }
            MenuNavigation::Activate => {
                if let Some(command) = self
                    .menu_selected_command
                    .filter(|command| commands::is_enabled(self, *command))
                {
                    self.update(Message::ExecuteCommand(command))
                } else if self.active_menu == Some(MainMenu::Actions) {
                    self.run_action_query()
                } else {
                    Task::none()
                }
            }
        }
    }

    fn run_action_query(&mut self) -> Task<Message> {
        let selected = (self.active_menu == Some(MainMenu::Actions))
            .then_some(self.menu_selected_command)
            .flatten();
        if let Some(command) = selected
            .filter(|command| commands::is_enabled(self, *command))
            .or_else(|| commands::find(self, &self.action_query))
        {
            self.update(Message::ExecuteCommand(command))
        } else {
            self.status =
                "Unknown action. Search for a command or choose one from the Actions menu."
                    .to_owned();
            Task::none()
        }
    }

    fn add_action_macro_step(&mut self) {
        if self.action_macro_steps.len() >= action_macros::MAX_MACRO_STEPS {
            self.action_macro_feedback = format!(
                "A macro can contain at most {} steps",
                action_macros::MAX_MACRO_STEPS
            );
            return;
        }
        let Some(step) = self.action_macro_step.clone() else {
            self.action_macro_feedback = "Choose a supported action first".to_owned();
            return;
        };
        if !commands::macro_step_ids().contains(&step) {
            self.action_macro_feedback = "That action cannot be used in a macro".to_owned();
            return;
        }
        self.action_macro_steps.push(step);
        self.action_macro_feedback.clear();
    }

    fn edit_action_macro(&mut self, id: u64) {
        if let Some(action_macro) = self
            .action_macros
            .iter()
            .find(|action_macro| action_macro.id == id)
        {
            self.action_macro_editing_id = Some(id);
            self.action_macro_name = action_macro.name.clone();
            self.action_macro_steps = action_macro.steps.clone();
            self.action_macro_step = commands::macro_step_choices()
                .first()
                .map(|choice| choice.id.clone());
            self.action_macro_feedback.clear();
        }
    }

    fn save_action_macro(&mut self) {
        if self.action_macro_config_error.is_some() {
            self.action_macro_feedback =
                "Fix the action macro config file before saving".to_owned();
            return;
        }
        let id = match self.action_macro_editing_id {
            Some(id) => id,
            None => match self
                .action_macros
                .iter()
                .map(|action_macro| action_macro.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
            {
                Some(id) => id,
                None => {
                    self.action_macro_feedback = "No more macro IDs are available".to_owned();
                    return;
                }
            },
        };
        let action_macro = action_macros::ActionMacro {
            id,
            name: self.action_macro_name.trim().to_owned(),
            steps: self.action_macro_steps.clone(),
        };
        let mut candidate = self.action_macros.clone();
        if let Some(index) = candidate.iter().position(|existing| existing.id == id) {
            candidate[index] = action_macro;
        } else {
            candidate.push(action_macro);
        }
        let candidate = match commands::validate_action_macros(candidate) {
            Ok(candidate) => candidate,
            Err(error) => {
                self.action_macro_feedback = format!("Macro was not saved: {error}");
                return;
            }
        };
        match action_macros::save(&candidate) {
            Ok(()) => {
                self.action_macros = candidate;
                self.action_macro_editing_id = Some(id);
                self.action_macro_feedback = "Macro saved".to_owned();
                self.status = "Action macro saved".to_owned();
            }
            Err(error) => {
                self.action_macro_feedback = format!("Macro could not be saved: {error}");
            }
        }
    }

    fn delete_action_macro(&mut self, id: u64) {
        if self.action_macro_config_error.is_some() {
            self.action_macro_feedback =
                "Fix the action macro config file before editing".to_owned();
            return;
        }
        if !self
            .action_macros
            .iter()
            .any(|action_macro| action_macro.id == id)
        {
            return;
        }
        let mut candidate = self.action_macros.clone();
        candidate.retain(|action_macro| action_macro.id != id);
        let mut saved_bindings = match keyboard_config::load() {
            Ok(bindings) => bindings,
            Err(error) => {
                self.action_macro_feedback =
                    format!("Shortcut config could not be loaded: {error}");
                return;
            }
        };
        saved_bindings.remove(&commands::macro_id(id));
        let saved_bindings =
            match commands::validate_bindings_with_macros(&saved_bindings, &candidate) {
                Ok(bindings) => bindings,
                Err(error) => {
                    self.action_macro_feedback = format!("Macro could not be deleted: {error}");
                    return;
                }
            };
        let mut staged_bindings = self.shortcut_binding_edits.clone();
        staged_bindings.remove(&commands::macro_id(id));
        let staged_bindings =
            match commands::validate_bindings_with_macros(&staged_bindings, &candidate) {
                Ok(bindings) => bindings,
                Err(error) => {
                    self.action_macro_feedback = format!("Macro could not be deleted: {error}");
                    return;
                }
            };
        if let Err(error) = action_macros::save(&candidate) {
            self.action_macro_feedback = format!("Macro could not be deleted: {error}");
            return;
        }
        if let Err(error) = keyboard_config::save(&saved_bindings) {
            let rollback = action_macros::save(&self.action_macros);
            self.action_macro_feedback = match rollback {
                Ok(()) => format!("Macro could not be deleted: shortcut save failed: {error}"),
                Err(rollback_error) => format!(
                    "Macro deletion partially failed: shortcut save failed: {error}; macro rollback failed: {rollback_error}"
                ),
            };
            return;
        }
        let mut active_bindings = self
            .shortcut_bindings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        active_bindings.remove(&commands::macro_id(id));
        self.action_macros = candidate;
        self.shortcut_binding_edits = staged_bindings;
        self.shortcut_defaults_restored
            .remove(&commands::macro_id(id));
        *self
            .shortcut_bindings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = active_bindings;
        if self.action_macro_editing_id == Some(id) {
            self.action_macro_editing_id = None;
            self.action_macro_name.clear();
            self.action_macro_steps.clear();
        }
        self.action_macro_feedback = "Macro deleted".to_owned();
        self.status = "Action macro deleted".to_owned();
    }

    fn save_shortcut_bindings(&mut self) {
        let bindings = match commands::validate_bindings_with_macros(
            &self.shortcut_binding_edits,
            &self.action_macros,
        ) {
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

    fn set_master_output_ceiling_dbfs(&mut self, ceiling: aaadaw_engine::MasterOutputCeiling) {
        let settings = audio_config::AudioSettings {
            master_output_ceiling: ceiling,
            ..self.audio_settings.clone()
        };
        #[cfg(feature = "audio-device")]
        if let Some(playback) = &self.playback {
            playback.set_master_output_ceiling_dbfs(ceiling);
        }
        self.audio_settings = settings.clone();
        match audio_config::save(&settings) {
            Ok(()) => {
                self.audio_settings_feedback =
                    format!("Master sample-peak ceiling set to {ceiling}");
            }
            Err(error) => {
                self.audio_settings_feedback =
                    format!("Ceiling is active for this session but could not be saved: {error}");
            }
        }
    }

    fn apply_recording_offset(&mut self) {
        let value = self
            .audio_recording_offset_query
            .as_deref()
            .unwrap_or("0.000");
        let Some(offset_us) = audio_config::parse_recording_offset_ms(value) else {
            self.audio_settings_feedback =
                "Enter a recording offset from -5000 to 5000 ms, with up to 3 decimal places"
                    .to_owned();
            return;
        };
        let settings = audio_config::AudioSettings {
            recording_offset_us: offset_us,
            ..self.audio_settings.clone()
        };
        self.audio_settings = settings.clone();
        self.audio_recording_offset_query =
            Some(audio_config::format_recording_offset_ms(offset_us));
        match audio_config::save(&settings) {
            Ok(()) => {
                self.audio_settings_feedback = format!(
                    "Recording placement offset set to {} ms; positive values move items later",
                    audio_config::format_recording_offset_ms(offset_us)
                );
            }
            Err(error) => {
                self.audio_settings_feedback = format!(
                    "Recording placement offset is active for this session but could not be saved: {error}"
                );
            }
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
        self.begin_track_draft(track_id, TrackDraftField::Name);
        self.timeline.select_track_only(track_id);
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
            .media_store_path()
            .filter(|path| path.is_file())
            .ok_or_else(|| "audio media store is unavailable for splitting".to_owned())?;
        let store = ProjectStore::open(&path).map_err(|error| error.to_string())?;
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
        let Some(path) = self.media_store_path().filter(|path| path.is_file()) else {
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
            cache_hit: _,
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

    fn duplicate_selected_items(&mut self) {
        let mut item_ids = self
            .timeline
            .selected_items
            .iter()
            .copied()
            .collect::<Vec<_>>();
        item_ids.sort_unstable_by_key(|item_id| item_id.value());
        if item_ids.is_empty() {
            self.status = "Select one or more items to duplicate".to_owned();
            return;
        }

        let mut actions = match duplicate_item_actions(&self.project, &item_ids) {
            Ok(actions) => actions,
            Err(error) => {
                self.status = format!("Could not duplicate selected items: {error}");
                return;
            }
        };
        if actions.is_empty() {
            self.status = "Selected items no longer exist".to_owned();
            return;
        }
        let action = if actions.len() == 1 {
            actions.pop().expect("single duplicate action is present")
        } else {
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            }
        };
        self.apply_action(action, "Selected items duplicated");
    }

    fn save_project_command(&mut self) -> Task<Message> {
        if self.project_path.is_none() {
            self.pick_path(PathPickerTarget::SaveProject)
        } else {
            project_io::save_project(self, None)
        }
    }

    fn open_project_command(&mut self) -> Task<Message> {
        self.begin_project_transition(PendingProjectTransition::OpenProject)
    }
}

fn parse_track_name_draft(text: &str) -> Result<String, &'static str> {
    let name = text.trim();
    if name.is_empty() {
        Err("Track name must not be empty")
    } else {
        Ok(name.to_owned())
    }
}

fn parse_midi_item_name_draft(text: &str) -> Result<String, &'static str> {
    let name = text.trim();
    if name.is_empty() {
        Err("MIDI item name must not be empty")
    } else if name.chars().count() > 128 {
        Err("MIDI item name must be 128 characters or fewer")
    } else {
        Ok(name.to_owned())
    }
}

fn parse_track_volume_draft(text: &str) -> Result<f32, &'static str> {
    let value = text
        .trim()
        .parse::<f32>()
        .map_err(|_| "Enter a valid volume in dB")?;
    if !value.is_finite() {
        return Err("Enter a finite volume in dB");
    }
    Ok(value.clamp(-60.0, 6.0))
}

#[cfg(all(target_os = "linux", feature = "audio-device"))]
#[allow(unused_mut, unused_variables)]
fn inspect_linux_audio_routes(
    backend: PlaybackBackend,
    client_name: Option<String>,
    node_id: Option<u32>,
    input_client_name: Option<String>,
    input_node_id: Option<u32>,
) -> Result<aaadaw_engine::AudioRouteSnapshot, String> {
    let mut snapshot = match backend {
        #[cfg(feature = "jack-backend")]
        PlaybackBackend::Jack => {
            let client_name = client_name.ok_or_else(|| {
                "The active JACK output client could not be identified".to_owned()
            })?;
            aaadaw_engine::inspect_jack_output_routes(&client_name)?
        }
        #[cfg(feature = "pipewire-backend")]
        PlaybackBackend::PipeWire => {
            let node_id = node_id.ok_or_else(|| {
                "The active PipeWire output stream could not be identified".to_owned()
            })?;
            aaadaw_engine::inspect_pipewire_output_routes(node_id)?
        }
        _ => {
            return Err(format!(
                "{} route diagnostics are unavailable in this Linux build",
                backend.name()
            ));
        }
    };
    #[cfg(feature = "jack-backend")]
    if let Some(client_name) = input_client_name {
        snapshot.input_routes =
            aaadaw_engine::inspect_jack_input_routes(&client_name)?.input_routes;
    }
    #[cfg(feature = "pipewire-backend")]
    if let Some(node_id) = input_node_id {
        snapshot.input_routes = aaadaw_engine::inspect_pipewire_input_routes(node_id)?.input_routes;
    }
    Ok(snapshot)
}

fn parse_track_pan_draft(text: &str) -> Result<f32, &'static str> {
    let value = text
        .trim()
        .parse::<f32>()
        .map_err(|_| "Enter a pan value from -1.0 to 1.0")?;
    if !value.is_finite() {
        return Err("Enter a finite pan value");
    }
    Ok(value.clamp(-1.0, 1.0))
}

fn snap_tick_up(tick: u64, grid_ticks: u64) -> u64 {
    let grid_ticks = grid_ticks.max(1);
    let quotient = tick / grid_ticks + u64::from(!tick.is_multiple_of(grid_ticks));
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

#[cfg(any(feature = "audio-device", test))]
fn playback_prevents_project_edits(playback_open: bool, transport_playing: bool) -> bool {
    playback_open && transport_playing
}

fn project_path_from_query(query: &str) -> Option<PathBuf> {
    let query = query.trim();
    (!query.is_empty()).then(|| PathBuf::from(query))
}

fn create_unsaved_project_session_dir() -> Result<UnsavedSessionMedia, String> {
    create_unsaved_project_session_dir_in(&unsaved_sessions_root()?)
}

fn unsaved_sessions_root() -> Result<PathBuf, String> {
    let directories = directories::ProjectDirs::from("org", "AAADAW", "AAADAW")
        .ok_or_else(|| "application data directory is unavailable".to_owned())?;
    Ok(directories.data_local_dir().join("unsaved-sessions"))
}

fn create_unsaved_project_session_dir_in(
    session_root: &std::path::Path,
) -> Result<UnsavedSessionMedia, String> {
    std::fs::create_dir_all(session_root).map_err(|error| error.to_string())?;
    let session_dir = tempfile::Builder::new()
        .prefix("session-")
        .tempdir_in(session_root)
        .map_err(|error| error.to_string())?;
    let store_path = session_dir.path().join("session.aaadaw");
    let mut store = ProjectStore::open(&store_path).map_err(|error| error.to_string())?;
    store
        .save_with_arrangement_view_state(&Project::new(), &ArrangementViewState::default())
        .map_err(|error| error.to_string())?;
    store.close().map_err(|error| error.to_string())?;
    let lock = ProjectSessionLock::acquire(&store_path).map_err(|error| error.to_string())?;
    Ok(UnsavedSessionMedia {
        directory: session_dir.keep(),
        lock: Some(lock),
    })
}

fn new_project_status(session_media_error: Option<&str>) -> String {
    session_media_error.map_or_else(
        || "New project created".to_owned(),
        |error| format!("New project created, but temporary media storage is unavailable: {error}"),
    )
}

fn scroll_arrangement_to(target: &'static str, offset_y: f32) -> Task<Message> {
    scroll_widget_to(target, offset_y)
}

fn scroll_widget_to(target: &'static str, offset_y: f32) -> Task<Message> {
    use iced::advanced::widget::operation::scrollable::{self, AbsoluteOffset};

    let target = iced::widget::Id::new(target);
    let offset = AbsoluteOffset {
        x: None,
        y: Some(offset_y.max(0.0)),
    };
    iced::advanced::widget::operate(scrollable::scroll_to(target, offset))
}

fn scale_project_samples_to_source_frames_toward_zero(
    project_samples: i128,
    source_rate: u64,
    project_rate: u64,
) -> Result<i128, String> {
    if source_rate == 0 || project_rate == 0 {
        return Err("Audio sample rates must be positive".to_owned());
    }
    let numerator = project_samples
        .checked_mul(i128::from(source_rate))
        .ok_or_else(|| "Audio trim exceeds the supported time range".to_owned())?;
    Ok(numerator / i128::from(project_rate))
}

fn scale_project_samples_to_source_frames_ceil(
    project_samples: i128,
    source_rate: u64,
    project_rate: u64,
) -> Result<i128, String> {
    let numerator = project_samples
        .checked_mul(i128::from(source_rate))
        .ok_or_else(|| "Audio trim exceeds the supported time range".to_owned())?;
    let denominator = i128::from(project_rate);
    let remainder = numerator.rem_euclid(denominator);
    numerator
        .div_euclid(denominator)
        .checked_add(i128::from(remainder > 0))
        .ok_or_else(|| "Audio trim exceeds the supported time range".to_owned())
}

fn scroll_actions_menu_selection(app: &mut App, selected: CommandId) -> Task<Message> {
    let entries = commands::matching_actions_menu(app, &app.action_query);
    let Some(offset_y) =
        actions_menu_selection_scroll_offset(&entries, selected, app.action_menu_scroll_offset)
    else {
        return Task::none();
    };
    app.action_menu_scroll_offset = offset_y;
    scroll_widget_to("aaadaw-actions-menu-results", offset_y)
}

fn actions_menu_selection_scroll_offset(
    entries: &[commands::CommandEntry],
    selected: CommandId,
    current_offset: f32,
) -> Option<f32> {
    const VIEWPORT_HEIGHT: f32 = 284.0;
    const ROW_HEIGHT: f32 = 28.0;
    const CATEGORY_HEIGHT: f32 = 16.0;
    const SEPARATOR_HEIGHT: f32 = 5.0;
    const ROW_GAP: f32 = 4.0;

    let mut y = 0.0;
    let mut category = None;
    let mut selected_bounds = None;
    for entry in entries {
        if category != Some(entry.category) {
            category = Some(entry.category);
            y += CATEGORY_HEIGHT + ROW_GAP;
        }
        if entry.separator_before {
            y += SEPARATOR_HEIGHT + ROW_GAP;
        }
        if entry.id == selected {
            selected_bounds = Some((y, y + ROW_HEIGHT));
        }
        y += ROW_HEIGHT + ROW_GAP;
    }
    let content_height = y;
    let (row_top, row_bottom) = selected_bounds?;
    let max_offset = (content_height - VIEWPORT_HEIGHT).max(0.0);
    let current_offset = current_offset.clamp(0.0, max_offset);
    let next_offset = if row_top < current_offset {
        row_top
    } else if row_bottom > current_offset + VIEWPORT_HEIGHT {
        row_bottom - VIEWPORT_HEIGHT
    } else {
        return None;
    }
    .clamp(0.0, max_offset);
    (next_offset != current_offset).then_some(next_offset)
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

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos")
))]
fn start_cpal_device_enumeration<T: Send + 'static>(
    loading: &mut bool,
    error: &mut Option<String>,
    worker_name: &'static str,
    enumerate: impl FnOnce() -> Result<Vec<T>, String> + Send + 'static,
    message: fn(Result<Vec<T>, String>) -> Message,
) -> Task<Message> {
    if *loading {
        return Task::none();
    }
    *loading = true;
    *error = None;
    Task::perform(run_blocking(worker_name, enumerate), message)
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos")
))]
fn finish_cpal_device_enumeration<T>(
    devices: &mut Vec<T>,
    loading: &mut bool,
    error: &mut Option<String>,
    feedback: &mut String,
    direction: &str,
    result: Result<Vec<T>, String>,
) {
    *loading = false;
    match result {
        Ok(enumerated) => {
            *devices = enumerated;
            *error = None;
        }
        Err(enumeration_error) => {
            devices.clear();
            *error = Some(enumeration_error.clone());
            *feedback = format!(
                "System audio {direction} devices could not be listed: {enumeration_error}"
            );
        }
    }
}

#[cfg(feature = "audio-device")]
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

fn menu_navigation_event(
    event: &iced::Event,
    status: iced::event::Status,
    window_id: iced::window::Id,
    main_window_id: Option<iced::window::Id>,
    active_menu: Option<MainMenu>,
) -> Option<Message> {
    if main_window_id != Some(window_id) {
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
    let navigation = match key.as_ref() {
        iced::keyboard::Key::Named(iced::keyboard::key::Named::F10)
            if status == iced::event::Status::Ignored =>
        {
            MenuNavigation::Open
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) if active_menu.is_some() => {
            return Some(Message::Escape);
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft)
            if active_menu.is_some()
                && (status == iced::event::Status::Ignored || modifiers.alt()) =>
        {
            MenuNavigation::PreviousMenu
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowRight)
            if active_menu.is_some()
                && (status == iced::event::Status::Ignored || modifiers.alt()) =>
        {
            MenuNavigation::NextMenu
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp)
            if active_menu.is_some()
                && (status == iced::event::Status::Ignored
                    || active_menu == Some(MainMenu::Actions)) =>
        {
            MenuNavigation::PreviousCommand
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown)
            if active_menu.is_some()
                && (status == iced::event::Status::Ignored
                    || active_menu == Some(MainMenu::Actions)) =>
        {
            MenuNavigation::NextCommand
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter)
            if active_menu.is_some()
                && (status == iced::event::Status::Ignored
                    || active_menu == Some(MainMenu::Actions)) =>
        {
            return Some(if active_menu == Some(MainMenu::Actions) {
                Message::RunActionQuery
            } else {
                Message::MenuKeyboard(MenuNavigation::Activate)
            });
        }
        _ => return None,
    };
    Some(Message::MenuKeyboard(navigation))
}

fn runtime_keyboard_event(
    event: iced::Event,
    status: iced::event::Status,
    window_id: iced::window::Id,
) -> Option<Message> {
    matches!(event, iced::Event::Keyboard(_))
        .then_some(Message::RuntimeKeyboardEvent(event, status, window_id))
}

fn midi_expression_context_menu_event(
    event: iced::Event,
    _status: iced::event::Status,
    window_id: iced::window::Id,
) -> Option<Message> {
    matches!(
        event,
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left))
    )
    .then_some(Message::DismissMidiExpressionContextMenus(window_id))
}

fn fx_chain_plugin_drag_event(
    event: iced::Event,
    _status: iced::event::Status,
    _window_id: iced::window::Id,
) -> Option<Message> {
    matches!(
        event,
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Left
        ))
    )
    .then_some(Message::FinishFxChainPluginDrag)
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
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Backspace) => {
                Some(Message::ClearShortcutBinding(action_id.to_owned()))
            }
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Delete) => {
                Some(Message::ShortcutCaptureKey {
                    action_id: action_id.to_owned(),
                    key: "Delete".to_owned(),
                    modifiers,
                })
            }
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
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Delete) => {
            Some(Message::ShortcutPressed("Delete".to_owned(), modifiers))
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Backspace) => {
            Some(Message::ShortcutPressed("Delete".to_owned(), modifiers))
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => Some(Message::Escape),
        _ => None,
    }
}

fn midi_editor_shortcut_event(
    event: iced::Event,
    status: iced::event::Status,
    window_id: iced::window::Id,
    midi_editor_window_id: Option<iced::window::Id>,
) -> Option<Message> {
    if midi_editor_window_id != Some(window_id) {
        return None;
    }
    if matches!(
        &event,
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            repeat: false,
            ..
        })
    ) {
        return None;
    }
    keyboard_shortcut_event(event, status, window_id, Some(window_id), None, None)
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
    commands::from_shortcut(&key, modifiers, bindings, &[]).map(Message::ExecuteCommand)
}

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf())
        == std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf())
}
