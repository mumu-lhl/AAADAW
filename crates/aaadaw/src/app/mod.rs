Warning: truncated output (original token count: 85023)
Total output lines: 8371

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
    MainMenu, MainWorkspace, MenuNavigation, Message, MidiEditorLane, MobilePanel,
    PathPickerTarget, PendingProjectTransition, SettingsCategory, TimeMapTab,
};

pub(crate) fn run() -> iced::Result {
    let application = iced::daemon(App::new, App::update, view::view_for_window)
        .title(App::window_title)
        .theme(|_: &App, _| iced::Theme::Dark)
        .subscription(App::subscription);

    #[cfg(target_os = "android")]
    let application = application
        .default_font(iced::Font::with_name("Roboto"))
        .font(include_bytes!("../../assets/Roboto-Variable.ttf").as_slice());

    application.run()
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
    any(target_os = "windows", target_os = "macos", target_os = "android")
))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct CpalDeviceChoice {
    id: Option<String>,
    label: String,
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos", target_os = "android")
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
    mobile_panel: MobilePanel,
    mobile_panel_history: Vec<MobilePanel>,
    main_window_size: Option<iced::Size>,
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
    #[cfg(all(feature = "audio-device", target_os = "android"))]
    android_midi_input_ports: usize,
    #[cfg(all(feature = "audio-device", target_os = "android"))]
    android_midi_output_ports: usize,
    #[cfg(all(feature = "audio-device", target_os = "android"))]
    android_midi_feedback: String,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    cpal_output_devices: Vec<aaadaw_engine::CpalOutputDeviceInfo>,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    cpal_output_devices_loading: bool,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    cpal_output_devices_error: Option<String>,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    cpal_input_devices: Vec<aaadaw_engine::CpalInputDeviceInfo>,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    cpal_input_devices_loading: bool,
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
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
    recording_notice: Option<String>,
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
            any(target_os = "windows", target_os = "macos", target_os = "android")
        ),
        all(
            feature = "pipewire-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos", target_os = "android")
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
    input: Option<RunningAudioInput>,
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
pub(super) struct SharedRecordingInputRecovery(
    Arc<Mutex<Option<Result<RunningAudioInput, String>>>>,
);

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
impl std::fmt::Debug for SharedRecordingInputRecovery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedRecordingInputRecovery(..)")
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
    fn is_mobile_main_window(&self) -> bool {
        self.main_window_size
            .map(|size| size.width < 720.0)
            .unwrap_or(cfg!(target_os = "android"))
    }

    fn show_mobile_panel(&mut self, panel: MobilePanel) {
        if self.mobile_panel != panel {
            if panel != MobilePanel::Settings {
                self.cancel_shortcut_capture();
            }
            self.mobile_panel_history.push(self.mobile_panel);
            self.mobile_panel = panel;
        }
        self.active_menu = None;
        self.offline_jobs_panel_open = false;
    }

    fn navigate_back_mobile_panel(&mut self) {
        self.cancel_shortcut_capture();
        self.mobile_panel = self
            .mobile_panel_history
            .pop()
            .unwrap_or(MobilePanel::Editor);
        self.active_menu = None;
        self.offline_jobs_panel_open = false;
    }

    fn cancel_shortcut_capture(&mut self) -> bool {
        if self.shortcut_capture_id.take().is_some() {
            self.shortcut_editor_feedback = "Shortcut recording cancelled".to_owned();
            true
        } else {
            false
        }
    }

    fn new() -> (Self, Task<Message>) {
        let mut app = Self::default();
        match create_unsaved_project_session_dir() {
            Ok(session_dir) => app.session_media_dir = Some(session_dir),
            Err(error) => {
                app.status = format!("Temporary project media storage unavailable: {error}");
            }
        }
        let (main_window_id, main_window_task) = iced::window::open(iced::window::Settings {
            size: if cfg!(target_os = "android") {
                iced::Size::new(420.0, 800.0)
            } else {
                iced::Size::new(1280.0, 800.0)
            },
            min_size: Some(if cfg!(target_os = "android") {
                iced::Size::new(360.0, 480.0)
            } else {
                iced::Size::new(900.0, 620.0)
            }),
            exit_on_close_request: false,
            ..iced::window::Settings::default()
        });
        app.main_window_id = Some(main_window_id);
        app.main_window_size = Some(if cfg!(target_os = "android") {
            iced::Size::new(420.0, 800.0)
        } else {
            iced::Size::new(1280.0, 800.0)
        });
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
                        any(target_os = "windows", target_os = "macos", target_os = "android")
                    ),
                    all(
                        feature = "pipewire-backend",
                        feature = "cpal-backend",
                        any(target_os = "windows", target_os = "macos", target_os = "android")
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
        #[allow(unused_mut)]
        let mut default_plugin_paths = default_clap_search_paths();
        #[cfg(target_os = "android")]
        if let Some(app_data) = crate::android_platform::app_data_directory() {
            let plugin_directory = app_data.join("plugins");
            if let Err(error) = std::fs::create_dir_all(&plugin_directory) {
                app.clap_plugin_warnings
                    .push(format!("Could not create Android CLAP directory: {error}"));
            } else {
                default_plugin_paths.push(plugin_directory);
            }
        }
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
            || {
                #[cfg(all(feature = "audio-device", target_os = "android"))]
                {
                    self.settings_category == SettingsCategory::Audio
                }
                #[cfg(not(all(feature = "audio-device", target_os = "android")))]
                {
                    false
                }
            }
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
                    | Message::MobileNavigateBack
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
                self.cancel_shortcut_capture();
                self.main_workspace = workspace;
                self.mobile_panel = MobilePanel::Editor;
                self.mobile_panel_history.clear();
                self.active_menu = None;
            }
            Message::MobileNavigateBack => self.navigate_back_mobile_panel(),
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
                    #[cfg(target_os = "android")]
                    tracing::warn!(?window_id, "Android main window closed");
                    self.close_fx_editor_resources();
                    task = iced::exit();
                }
            }
            Message::WindowCloseRequested(window_id) => {
                if self.fx_chain_window_id == Some(window_id) {
                    self.close_fx_editor_resources();
                } else if self.main_window_id == Some(window_id) {
                    #[cfg(target_os = "android")]
                    tracing::warn!(?window_id, "Android main window close requested");
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
                if self.main_window_id == Some(window_id) {
                    self.main_window_size = Some(size);
                }
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
                    any(target_os = "windows", target_os = "macos", target_os = "android")
                ))]
                if category == SettingsCategory::Audio {
                    task = Task::batch([
                        self.refresh_cpal_output_devices(),
                        self.refresh_cpal_input_devices(),
                    ]);
                }
                #[cfg(all(feature = "audio-device", target_os = "android"))]
                if category == SettingsCategory::Audio {
                    self.android_midi_feedback.clear();
                    if let Err(error) = crate::android_platform::refresh_midi_devices() {
                        self.android_midi_feedback = format!("MIDI scan failed: {error}");
                    }
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
            #[cfg(all(feature = "audio-device", target_os = "android"))]
            Message::RefreshAndroidMidiDevices => {
                self.android_midi_feedback = match crate::android_platform::refresh_midi_devices() {
                    Ok(()) => "Scanning USB and paired Bluetooth MIDI devices…".to_owned(),
                    Err(error) => format!("MIDI scan failed: {error}"),
                };
            }
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos", target_os = "android")
            ))]
            Message::CpalOutputDevicesLoaded(result) => {
                self.finish_cpal_output_device_enumeration(result);
            }
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos", target_os = "android")
            ))]
            Message::RefreshCpalOutputDevices => {
                task = self.refresh_cpal_output_devices();
            }
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos", target_os = "android")
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
   …35023 tokens truncated…  .project
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

    #[cfg(all(feature = "audio-device", target_os = "android"))]
    fn selected_midi_input_track_id(&self) -> Option<TrackId> {
        let track_id = self.selected_track_id()?;
        self.project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id && track.instrument().is_some())
            .map(|track| track.id())
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
    #[cfg(target_os = "android")]
    let data_directory = crate::android_platform::app_data_directory();
    #[cfg(not(target_os = "android"))]
    let data_directory = directories::ProjectDirs::from("org", "AAADAW", "AAADAW")
        .map(|directories| directories.data_local_dir().to_path_buf());

    data_directory
        .map(|directory| directory.join("unsaved-sessions"))
        .ok_or_else(|| "application data directory is unavailable".to_owned())
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
    any(target_os = "windows", target_os = "macos", target_os = "android")
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
    any(target_os = "windows", target_os = "macos", target_os = "android")
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
    if (settings_window_id == Some(window_id)
        || (settings_window_id.is_none() && main_window_id == Some(window_id)))
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

fn mobile_back_event(
    event: &iced::Event,
    window_id: iced::window::Id,
    main_window_id: Option<iced::window::Id>,
    mobile_panel_open: bool,
) -> Option<Message> {
    if main_window_id != Some(window_id) || !mobile_panel_open {
        return None;
    }
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key:
            iced::keyboard::Key::Named(
                iced::keyboard::key::Named::BrowserBack | iced::keyboard::key::Named::GoBack,
            ),
        repeat: false,
        ..
    }) = event
    else {
        return None;
    };
    Some(Message::MobileNavigateBack)
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
