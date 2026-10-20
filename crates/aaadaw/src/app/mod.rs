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
    prepare_audio_playback_at_with_loop_range,
};
#[cfg(feature = "audio-device")]
use aaadaw_core::ProjectSnapshot;
use aaadaw_core::{
    AudioItem, DawAction, FxParameterChange, GridFraction, ItemId, MarkerId, MeterPointSnapshot,
    MidiChannelPressureData, MidiControllerData, MidiFileImportOptions, MidiFileProjectImport,
    MidiItem, MidiItemPosition, MidiNoteData, MidiOpaqueEventData, MidiOpaqueEventKind,
    MidiPitchBendData, MidiPolyAftertouchData, MidiProgramChangeData, NoteId, Project,
    RazorEditArea, RegionId, TempoCurve, TimeSignature, TrackId,
};
#[cfg(feature = "audio-device")]
use aaadaw_engine::{
    ClapEffectOwner, ClapInstrumentHelperProcess, ClapInstrumentOwner,
    ClapParameterAutomationReceiver, ClapParameterSender, TransportLoopRange,
};
use aaadaw_engine::{ClapParameterInfo, ClapPluginGuiOwner};
use aaadaw_media::AudioWaveform;
use aaadaw_storage::{ArrangementViewState, ProjectSessionLock, ProjectStore};
use audio_export::{RenderBounds, RenderSource};
use commands::CommandId;
use iced::Task;
use iced::widget::pane_grid::{self, Axis, Split};
use std::collections::{HashMap, HashSet, VecDeque};
#[cfg(feature = "audio-device")]
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) const MIDI_EDITOR_KEY_WIDTH: f32 = 84.0;
pub(super) const MIDI_EDITOR_CONTENT_WIDTH_INSET: f32 = MIDI_EDITOR_KEY_WIDTH + 32.0;

fn midi_editor_grid_fraction(grid: timeline::SnapGrid) -> GridFraction {
    let (numerator, denominator) = match grid {
        timeline::SnapGrid::Whole => (1, 1),
        timeline::SnapGrid::Half => (1, 2),
        timeline::SnapGrid::Quarter => (1, 4),
        timeline::SnapGrid::Eighth => (1, 8),
        timeline::SnapGrid::Sixteenth => (1, 16),
        timeline::SnapGrid::ThirtySecond => (1, 32),
        timeline::SnapGrid::DottedHalf => (3, 4),
        timeline::SnapGrid::DottedQuarter => (3, 8),
        timeline::SnapGrid::DottedEighth => (3, 16),
        timeline::SnapGrid::DottedSixteenth => (3, 32),
        timeline::SnapGrid::HalfTriplet => (1, 3),
        timeline::SnapGrid::QuarterTriplet => (1, 6),
        timeline::SnapGrid::EighthTriplet => (1, 12),
        timeline::SnapGrid::SixteenthTriplet => (1, 24),
        timeline::SnapGrid::ThirtySecondTriplet => (1, 48),
    };
    GridFraction::new(numerator, denominator).expect("snap grid maps to a positive fraction")
}

fn midi_editor_measure_length_ticks(ppq: u32, meter: TimeSignature) -> u64 {
    let beat_ticks = u64::from(ppq)
        .saturating_mul(4)
        .checked_div(u64::from(meter.denominator()))
        .unwrap_or(0)
        .max(1);
    beat_ticks
        .saturating_mul(u64::from(meter.numerator()))
        .max(1)
}

fn midi_editor_filter_position_value(tick: u64, measure_ticks: u64) -> u32 {
    let position = tick % measure_ticks.max(1);
    (u128::from(position) * u128::from(MIDI_EDITOR_FILTER_POSITION_SCALE)
        / u128::from(measure_ticks.max(1))) as u32
}

fn midi_editor_filter_repeat_position_value(
    tick: u64,
    ppq: u32,
    measure_ticks: u64,
    repeat_every: MidiEditorFilterRepeat,
) -> u32 {
    let measure_ticks = measure_ticks.max(1);
    let period_ticks = repeat_every.period_ticks(ppq, measure_ticks);
    let position = tick % period_ticks;
    (u128::from(position) * u128::from(MIDI_EDITOR_FILTER_POSITION_SCALE)
        / u128::from(measure_ticks))
    .min(u128::from(MIDI_EDITOR_FILTER_POSITION_SCALE)) as u32
}

fn midi_editor_filter_position_high_value(tick: u64, measure_ticks: u64) -> u32 {
    let measure_ticks = measure_ticks.max(1);
    let end_tick = (tick % measure_ticks).saturating_add(1).min(measure_ticks);
    let scaled = u128::from(end_tick) * u128::from(MIDI_EDITOR_FILTER_POSITION_SCALE);
    let denominator = u128::from(measure_ticks);
    scaled
        .div_ceil(denominator)
        .min(u128::from(MIDI_EDITOR_FILTER_POSITION_SCALE)) as u32
}

fn parse_midi_editor_filter_position(value: &str) -> Option<u32> {
    let value = value.trim().parse::<f64>().ok()?;
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return None;
    }
    Some((value * f64::from(MIDI_EDITOR_FILTER_POSITION_SCALE)).round() as u32)
}

fn parse_midi_editor_filter_value(value: &str) -> Option<u8> {
    let value = value.trim().parse::<u8>().ok()?;
    (value <= 127).then_some(value)
}

fn parse_midi_editor_filter_pitch_bend_value(value: &str) -> Option<i16> {
    let value = value.trim().parse::<i16>().ok()?;
    (-8_192..=8_191).contains(&value).then_some(value)
}

fn format_midi_editor_filter_position(value: u32) -> String {
    format!(
        "{:.8}",
        f64::from(value.min(MIDI_EDITOR_FILTER_POSITION_SCALE))
            / f64::from(MIDI_EDITOR_FILTER_POSITION_SCALE)
    )
}

fn parse_midi_editor_filter_length(value: &str, ppq: u32) -> Option<u64> {
    let value = value.trim();
    if let Some(ticks) = value.strip_suffix('t') {
        return ticks.trim().parse().ok();
    }
    let Some((numerator, denominator)) = value.split_once('/') else {
        return value.parse().ok();
    };
    let numerator = numerator.trim().parse::<u64>().ok()?;
    let denominator = denominator.trim().parse::<u64>().ok()?;
    if denominator == 0 {
        return None;
    }
    u64::from(ppq)
        .checked_mul(4)?
        .checked_mul(numerator)?
        .checked_div(denominator)
}

fn format_midi_editor_filter_length(ticks: u64, ppq: u32) -> String {
    let whole_note_ticks = u64::from(ppq).saturating_mul(4);
    for denominator in (0..=12).map(|power| 1_u64 << power) {
        for numerator in 1..=32_u64 {
            if whole_note_ticks.saturating_mul(numerator) % denominator == 0
                && whole_note_ticks.saturating_mul(numerator) / denominator == ticks
            {
                let divisor = gcd_u64(numerator, denominator);
                return format!("{}/{}", numerator / divisor, denominator / divisor);
            }
        }
    }
    format!("{ticks}t")
}

fn gcd_u64(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left.max(1)
}

fn midi_editor_note_filter_event(
    note: &aaadaw_core::MidiNote,
    ppq: u32,
    measure_ticks: u64,
    repeat_every: MidiEditorFilterRepeat,
) -> MidiEditorNoteFilterEvent {
    MidiEditorNoteFilterEvent {
        pitch: note.pitch(),
        velocity: note.velocity(),
        position_in_measure: midi_editor_filter_repeat_position_value(
            note.tick(),
            ppq,
            measure_ticks,
            repeat_every,
        ),
        length_ticks: note.duration(),
    }
}

fn next_midi_humanize_seed() -> u64 {
    static SEED_COUNTER: AtomicU64 = AtomicU64::new(0);
    let time_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64);
    time_seed ^ SEED_COUNTER.fetch_add(1, Ordering::Relaxed).rotate_left(17)
}

mod action_list;
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
mod desktop_layout;
mod fader;
mod fader_data;
mod grid_settings;
mod item_nudge;
mod item_properties;
mod keyboard_config;
mod loop_time_preferences;
mod marker_manager;
mod marker_properties;
mod media;
mod messages;
mod midi_export;
mod midi_humanize_preferences;
mod midi_quantize_preferences;
mod offline_job_queue;
mod project_defaults;
mod project_io;
mod project_settings;
mod project_templates;
mod recent_projects;
#[cfg(feature = "audio-device")]
mod recording;
mod recording_recovery;
mod region_properties;
mod region_render_matrix;
mod render_wildcard_preferences;
mod routing;
mod routing_matrix;
mod shortcut;
mod shortcut_map;
#[cfg(test)]
mod tests;
mod view;
mod x11_plugin_editor;

use messages::MIDI_EDITOR_FILTER_POSITION_SCALE;
pub(crate) use messages::{
    MainMenu, MainWorkspace, MenuNavigation, Message, MidiEditorChannelSelection,
    MidiEditorControllerFilter, MidiEditorCursorBoundary, MidiEditorEventKind,
    MidiEditorEventListCursor, MidiEditorEventListCursorDirection, MidiEditorEventTypeFilter,
    MidiEditorFilterRepeat, MidiEditorFilterSelectionOperation, MidiEditorFilterState,
    MidiEditorLane, MidiEditorMode, MidiEditorNoteFilter, MidiEditorNoteFilterEvent,
    MidiEditorPitchBendFilter, MidiEditorPitchFilterValue, MidiEditorPositionFilter,
    MidiEditorProgramChangeFilter, MidiEditorTool, MidiHumanizeScope, MidiQuantizeOperation,
    MidiQuantizeScope, MidiTransposeScope, MobilePanel, PathPickerTarget, PendingProjectTransition,
    SettingsCategory, TimeMapTab,
};

#[derive(Clone)]
enum AppHistoryEntry {
    Project,
    ProjectWithInlineMidiSelection(InlineMidiSelectionHistory),
    ProjectWithItemSelection(ItemSelectionHistory),
    InlineMidiSelection(InlineMidiSelectionHistory),
    ItemSelection {
        label: &'static str,
        before: Box<timeline::ItemSelectionState>,
        after: Box<timeline::ItemSelectionState>,
        project_history_cursor: usize,
    },
}

#[derive(Clone)]
struct ItemSelectionHistory {
    before: Box<timeline::ItemSelectionState>,
    after: Box<timeline::ItemSelectionState>,
    project_history_cursor_before: usize,
    project_history_cursor_after: usize,
}

fn rebase_audio_take_source_offset_samples(
    source_offset_samples: i64,
    target_start_sample: u64,
    donor_start_sample: u64,
    source_sample_rate: u32,
    project_sample_rate: u32,
    playback_rate: f64,
) -> Option<i64> {
    if source_sample_rate == 0
        || project_sample_rate == 0
        || !playback_rate.is_finite()
        || playback_rate <= 0.0
    {
        return None;
    }
    let project_delta_samples = i128::from(target_start_sample) - i128::from(donor_start_sample);
    let source_delta_samples = project_delta_samples as f64 * f64::from(source_sample_rate)
        / f64::from(project_sample_rate)
        * playback_rate;
    if !source_delta_samples.is_finite() {
        return None;
    }
    let source_delta_samples = source_delta_samples.round();
    if source_delta_samples < i64::MIN as f64 || source_delta_samples >= -(i64::MIN as f64) {
        return None;
    }
    source_offset_samples.checked_add(source_delta_samples as i64)
}

#[derive(Clone)]
struct InlineMidiSelectionHistory {
    item_id: ItemId,
    before: HashSet<aaadaw_core::NoteId>,
    after: HashSet<aaadaw_core::NoteId>,
    editor_before: Option<MidiEditorSelectionState>,
    editor_after: Option<MidiEditorSelectionState>,
    project_history_cursor: usize,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct MidiEditorSelectionState {
    notes: HashSet<NoteId>,
    controllers: HashSet<MidiControllerData>,
    pitch_bends: HashSet<MidiPitchBendData>,
    program_changes: HashSet<MidiProgramChangeData>,
    poly_aftertouches: HashSet<MidiPolyAftertouchData>,
    channel_pressures: HashSet<MidiChannelPressureData>,
    opaque_events: HashSet<u32>,
    event_list_cursor: Option<MidiEditorEventListCursor>,
    event_list_anchor: Option<MidiEditorEventListCursor>,
}

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

#[derive(Clone, Debug, PartialEq)]
struct AudioItemClipboard {
    project_generation: u64,
    project_sample_rate: u32,
    span_samples: u64,
    items: Vec<AudioItemClipboardEntry>,
}

#[derive(Clone, Debug, PartialEq)]
struct AudioTakeClipboard {
    project_generation: u64,
    takes: Vec<aaadaw_core::AudioItemTakeSnapshot>,
}

#[derive(Clone, Debug, PartialEq)]
struct AudioItemClipboardEntry {
    media_ref: String,
    name: String,
    source_offset_samples: u64,
    source_section_start_samples: u64,
    source_section_length_samples: u64,
    length_samples: u64,
    volume: f32,
    take_volume: f32,
    take_pan: f32,
    take_playback_rate: f64,
    muted: bool,
    phase_inverted: bool,
    track_offset: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellProfile {
    Desktop,
    Touch,
}

impl Default for ShellProfile {
    fn default() -> Self {
        if cfg!(target_os = "android") {
            Self::Touch
        } else {
            Self::Desktop
        }
    }
}

type TrackRazorAreas = Vec<(TrackId, Vec<RazorEditArea>)>;

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
    desktop_layout_save_at: Option<Instant>,
    loop_time_preferences_save_at: Option<Instant>,
    midi_humanize_preferences_save_at: Option<Instant>,
    midi_humanize_preferences: midi_humanize_preferences::MidiHumanizePreferences,
    midi_quantize_preferences_save_at: Option<Instant>,
    midi_quantize_preferences: midi_quantize_preferences::MidiQuantizePreferences,
    midi_transpose_scope: MidiTransposeScope,
    main_workspace: MainWorkspace,
    shell_profile: ShellProfile,
    mobile_panel: MobilePanel,
    mobile_panel_history: Vec<MobilePanel>,
    main_window_size: Option<iced::Size>,
    android_app_unfocused: bool,
    project_path_query: String,
    project_path: Option<PathBuf>,
    recent_project_paths: Vec<PathBuf>,
    recent_projects_menu_open: bool,
    project_template_paths: Vec<PathBuf>,
    project_templates_menu_open: bool,
    pending_project_template_overwrite: Option<PathBuf>,
    pending_empty_midi_export: Option<midi_export::PendingEmptyMidiExport>,
    pending_project_path_to_open: Option<PathBuf>,
    pending_project_template_source: Option<PathBuf>,
    pending_project_template_working_copy: Option<project_templates::WorkingCopy>,
    project_template_source: Option<PathBuf>,
    session_media_dir: Option<UnsavedSessionMedia>,
    retired_unsaved_sessions: Vec<UnsavedSessionMedia>,
    unsaved_session_recovery_candidates: Vec<PathBuf>,
    unsaved_session_recovery_busy: bool,
    unsaved_session_snapshot_at: Option<Instant>,
    unsaved_session_snapshot_busy: bool,
    unsaved_session_snapshot_revision: u64,
    pending_project_transition: Option<PendingProjectTransition>,
    pending_focused_track_deletion: Option<Vec<TrackId>>,
    project_lock: Option<ProjectSessionLock>,
    project_template_working_copy: Option<project_templates::WorkingCopy>,
    track_name_edits: HashMap<TrackId, String>,
    midi_item_name_edits: HashMap<ItemId, String>,
    midi_item_name_errors: HashMap<ItemId, String>,
    track_volume_edits: HashMap<TrackId, String>,
    track_pan_edits: HashMap<TrackId, String>,
    active_track_draft: Option<(TrackId, TrackDraftField)>,
    track_draft_errors: HashMap<(TrackId, TrackDraftField), String>,
    track_mix_gesture: Option<TrackMixGesture>,
    master_mix_gesture: Option<(TrackMixParameter, aaadaw_core::MasterMix)>,
    master_mix_commit_at: Option<Instant>,
    track_mix_commit_at: Option<Instant>,
    track_peak_levels: HashMap<TrackId, [f32; 2]>,
    master_peak_level: [f32; 2],
    track_peak_holds: HashMap<TrackId, StereoPeakHold>,
    input_peak_level: [f32; 2],
    input_peak_hold: StereoPeakHold,
    master_peak_hold: StereoPeakHold,
    master_guard_ticks_remaining: u8,
    audio_item_start_edits: HashMap<ItemId, String>,
    item_properties: Option<item_properties::ItemProperties>,
    midi_item_properties: Option<item_properties::MidiItemProperties>,
    batch_item_properties: Option<item_properties::BatchItemProperties>,
    item_properties_window_id: Option<iced::window::Id>,
    item_properties_generation: u64,
    item_properties_unit: item_properties::TimeUnit,
    item_nudge_dialog: item_nudge::Dialog,
    item_nudge_window_id: Option<iced::window::Id>,
    marker_properties: Option<marker_properties::MarkerProperties>,
    marker_properties_window_id: Option<iced::window::Id>,
    region_properties: Option<region_properties::RegionProperties>,
    region_properties_window_id: Option<iced::window::Id>,
    marker_manager: Option<marker_manager::MarkerManager>,
    marker_manager_window_id: Option<iced::window::Id>,
    region_render_matrix_window_id: Option<iced::window::Id>,
    routing_matrix_window_id: Option<iced::window::Id>,
    selected_render_markers: HashSet<MarkerId>,
    selected_render_regions: HashSet<RegionId>,
    active_menu: Option<MainMenu>,
    menu_selected_command: Option<CommandId>,
    action_menu_scroll_offset: f32,
    keyboard_modifiers: iced::keyboard::Modifiers,
    main_window_id: Option<iced::window::Id>,
    settings_window_id: Option<iced::window::Id>,
    about_window_id: Option<iced::window::Id>,
    project_settings_window_id: Option<iced::window::Id>,
    project_settings: Option<project_settings::Draft>,
    snap_grid_settings_window_id: Option<iced::window::Id>,
    routing_window_id: Option<iced::window::Id>,
    routing_track_id: Option<TrackId>,
    routing_destination_filter: Option<TrackId>,
    routing_send_drafts: HashMap<aaadaw_core::SendId, routing::SendDraft>,
    action_list_window_id: Option<iced::window::Id>,
    action_input_window_id: Option<iced::window::Id>,
    action_list: action_list::ActionListState,
    render_window_id: Option<iced::window::Id>,
    midi_export_window_id: Option<iced::window::Id>,
    midi_export_dialog: Option<midi_export::MidiExportDialogState>,
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
    midi_editor_selected_controllers: HashSet<MidiControllerData>,
    midi_editor_selected_pitch_bends: HashSet<MidiPitchBendData>,
    midi_editor_selected_program_changes: HashSet<MidiProgramChangeData>,
    midi_editor_selected_poly_aftertouches: HashSet<MidiPolyAftertouchData>,
    midi_editor_selected_channel_pressures: HashSet<MidiChannelPressureData>,
    midi_editor_selected_opaque_events: HashSet<u32>,
    midi_editor_event_list_cursor: Option<MidiEditorEventListCursor>,
    midi_editor_event_list_anchor: Option<MidiEditorEventListCursor>,
    midi_quantize_dialog: Option<MidiQuantizeDialogState>,
    midi_humanize_dialog: Option<MidiHumanizeDialogState>,
    midi_transpose_dialog: Option<MidiTransposeDialogState>,
    midi_event_properties_dialog: Option<MidiEventPropertiesDialogState>,
    midi_event_properties_controller_options: MidiEventPropertiesControllerOptions,
    midi_editor_lane: MidiEditorLane,
    midi_editor_tool: MidiEditorTool,
    midi_editor_mode: MidiEditorMode,
    midi_editor_channel_selection: MidiEditorChannelSelection,
    midi_editor_event_type_filter: MidiEditorEventTypeFilter,
    midi_editor_draw_channel: u8,
    midi_editor_filter_enabled: bool,
    midi_editor_event_filter_enabled: bool,
    midi_editor_event_list_filter_active: bool,
    midi_editor_filter_channels: u16,
    midi_editor_note_filter: MidiEditorNoteFilter,
    midi_editor_controller_filter: MidiEditorControllerFilter,
    midi_editor_pitch_bend_filter: MidiEditorPitchBendFilter,
    midi_editor_program_change_filter: MidiEditorProgramChangeFilter,
    midi_editor_position_filter: MidiEditorPositionFilter,
    midi_editor_filter_position_low: String,
    midi_editor_filter_position_high: String,
    midi_editor_filter_note_length_low: String,
    midi_editor_filter_note_length_high: String,
    midi_editor_filter_value_low: String,
    midi_editor_filter_value_high: String,
    midi_editor_filter_channels_initialized: bool,
    midi_editor_filter_inverted: bool,
    midi_editor_filter_dialog_open: bool,
    midi_editor_origin_tick: u64,
    midi_editor_edit_cursor_tick: Option<u64>,
    midi_editor_high_pitch: u8,
    midi_editor_pitch_rows: u8,
    midi_editor_pitch_row_height: f32,
    midi_editor_pixels_per_beat: f32,
    midi_editor_follow_playhead: bool,
    midi_editor_window_size: iced::Size,
    midi_note_clipboard: MidiNoteClipboard,
    audio_item_clipboard: Option<AudioItemClipboard>,
    audio_take_clipboard: Option<AudioTakeClipboard>,
    plugin_picker_window_id: Option<iced::window::Id>,
    plugin_picker_track_id: Option<TrackId>,
    plugin_picker_instrument_track_id: Option<TrackId>,
    plugin_picker_search: String,
    shortcut_capture_id: Option<String>,
    shortcut_capture_append: bool,
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
    repeat_enabled: bool,
    #[cfg(feature = "audio-device")]
    repeat_reconfiguration_previous: Option<(bool, bool)>,
    wav_export_options: WavExportOptions,
    render_dialog: audio_export::RenderDialogState,
    offline_render_cancel: Option<Arc<AtomicBool>>,
    offline_render_progress: Option<Arc<Mutex<(u64, u64)>>>,
    audio_waveforms: HashMap<String, Arc<AudioWaveform>>,
    audio_waveform_worker: Option<AudioWaveformWorker>,
    relink_source_path_query: String,
    revision: u64,
    saved_revision: u64,
    app_undo_history: Vec<AppHistoryEntry>,
    app_redo_history: Vec<AppHistoryEntry>,
    project_generation: u64,
    io_busy: bool,
    audio_file_path_query: String,
    import_busy: bool,
    pending_midi_file_import: Option<MidiFileProjectImport>,
    midi_file_import_options: MidiFileImportOptions,
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

#[derive(Debug, Clone, Copy)]
struct MidiQuantizeDialogState {
    item_id: ItemId,
    scope: MidiQuantizeScope,
    operation: MidiQuantizeOperation,
    strength_percent: u8,
    bypass: bool,
    fix_overlaps: bool,
}

#[derive(Debug, Clone, Copy)]
struct MidiHumanizeDialogState {
    item_id: ItemId,
    scope: MidiHumanizeScope,
    timing_amount_percent: u8,
    velocity_amount_percent: u8,
    timing_bias_percent: i16,
    seed: u64,
}

#[derive(Debug, Clone)]
struct MidiTransposeDialogState {
    item_id: ItemId,
    scope: MidiTransposeScope,
    semitones: String,
}

#[derive(Debug, Clone)]
struct MidiEventPropertiesDialogState {
    item_id: ItemId,
    target: MidiEventPropertiesTarget,
    position: String,
    position_mixed: bool,
    length: String,
    length_mixed: bool,
    channel: String,
    channel_mixed: bool,
    parameter: String,
    parameter_mixed: bool,
    value: String,
    value_mixed: bool,
    off_value: String,
    off_value_mixed: bool,
    bank_msb: String,
    bank_msb_mixed: bool,
    bank_msb_dirty: bool,
    bank_lsb: String,
    bank_lsb_mixed: bool,
    bank_lsb_dirty: bool,
    opaque_type: Option<MidiOpaqueEventPropertiesType>,
    opaque_type_dirty: bool,
    opaque_payload: String,
    opaque_payload_dirty: bool,
    opaque_multiline: bool,
    opaque_editor: iced::widget::text_editor::Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MidiEventPropertiesTarget {
    Notes(Vec<NoteId>),
    Controllers(Vec<MidiControllerData>),
    PitchBends(Vec<MidiPitchBendData>),
    ProgramChanges(Vec<MidiProgramChangeData>),
    PolyAftertouches(Vec<MidiPolyAftertouchData>),
    ChannelPressures(Vec<MidiChannelPressureData>),
    Opaque(u32, MidiOpaqueEventData),
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MidiOpaqueEventPropertiesType {
    SysEx,
    RawMeta,
    TextEvent,
    CopyrightNotice,
    TrackName,
    InstrumentName,
    Lyrics,
    Marker,
    Cue,
    ProgramName,
    DeviceName,
}

impl MidiOpaqueEventPropertiesType {
    pub(crate) const META_OPTIONS: [Self; 9] = [
        Self::TextEvent,
        Self::CopyrightNotice,
        Self::TrackName,
        Self::InstrumentName,
        Self::Lyrics,
        Self::Marker,
        Self::Cue,
        Self::ProgramName,
        Self::DeviceName,
    ];

    fn meta_kind(self) -> Option<u8> {
        Some(match self {
            Self::TextEvent => 0x01,
            Self::CopyrightNotice => 0x02,
            Self::TrackName => 0x03,
            Self::InstrumentName => 0x04,
            Self::Lyrics => 0x05,
            Self::Marker => 0x06,
            Self::Cue => 0x07,
            Self::ProgramName => 0x08,
            Self::DeviceName => 0x09,
            Self::SysEx | Self::RawMeta => return None,
        })
    }

    fn from_meta_kind(kind: u8) -> Option<Self> {
        Some(
            Self::META_OPTIONS
                .into_iter()
                .find(|event_type| event_type.meta_kind() == Some(kind))
                .unwrap_or(Self::RawMeta),
        )
    }
}

impl std::fmt::Display for MidiOpaqueEventPropertiesType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::SysEx => "SysEx",
            Self::RawMeta => "Raw Meta (hex)",
            Self::TextEvent => "Text event",
            Self::CopyrightNotice => "Copyright notice",
            Self::TrackName => "Track name",
            Self::InstrumentName => "Instrument name",
            Self::Lyrics => "Lyrics",
            Self::Marker => "Marker",
            Self::Cue => "Cue",
            Self::ProgramName => "Program name",
            Self::DeviceName => "Device name",
        })
    }
}

fn midi_opaque_event_properties_type_and_payload(
    kind: &MidiOpaqueEventKind,
) -> Option<(MidiOpaqueEventPropertiesType, String)> {
    match kind {
        MidiOpaqueEventKind::Meta { kind, data } => {
            let event_type = MidiOpaqueEventPropertiesType::from_meta_kind(*kind)?;
            let payload = if midi_opaque_event_properties_payload_is_hex(*kind, data) {
                format_midi_event_hex_bytes(data)
            } else {
                String::from_utf8_lossy(data)
                    .trim_end_matches('\0')
                    .to_owned()
            };
            Some((event_type, payload))
        }
        MidiOpaqueEventKind::SystemExclusive { status, data } => {
            let editable_data = if *status == 0xF0 && data.last() == Some(&0xF7) {
                &data[..data.len().saturating_sub(1)]
            } else {
                data.as_slice()
            };
            Some((
                MidiOpaqueEventPropertiesType::SysEx,
                editable_data
                    .iter()
                    .map(|byte| format!("{byte:02X}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            ))
        }
    }
}

fn midi_opaque_event_properties_payload_is_hex(kind: u8, data: &[u8]) -> bool {
    match MidiOpaqueEventPropertiesType::from_meta_kind(kind) {
        Some(MidiOpaqueEventPropertiesType::RawMeta) => true,
        Some(_) => std::str::from_utf8(data).is_err(),
        None => false,
    }
}

fn midi_opaque_event_properties_dialog_payload_is_hex(
    dialog: &MidiEventPropertiesDialogState,
) -> bool {
    matches!(
        &dialog.target,
        MidiEventPropertiesTarget::Opaque(
            _,
            MidiOpaqueEventData {
                kind: MidiOpaqueEventKind::Meta { kind, data },
                ..
            }
        ) if midi_opaque_event_properties_payload_is_hex(*kind, data)
    )
}

fn parse_midi_event_hex_bytes(value: &str) -> Option<Vec<u8>> {
    value
        .split_whitespace()
        .map(|token| {
            (token.len() == 2)
                .then(|| u8::from_str_radix(token, 16).ok())
                .flatten()
        })
        .collect()
}

fn midi_sysex_file_bytes(status: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(payload.len().saturating_add(2));
    bytes.push(status);
    bytes.extend_from_slice(payload);
    if status == 0xF0 && bytes.last() != Some(&0xF7) {
        bytes.push(0xF7);
    }
    bytes
}

fn midi_sysex_payload_from_file(status: u8, mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.first() == Some(&status) {
        bytes.remove(0);
    }
    if status == 0xF0 && bytes.last() == Some(&0xF7) {
        bytes.pop();
    }
    bytes
}

fn load_midi_sysex_file(path: &Path, status: u8) -> Result<String, String> {
    std::fs::read(path)
        .map(|bytes| format_midi_event_hex_bytes(&midi_sysex_payload_from_file(status, bytes)))
        .map_err(|error| format!("Could not load SysEx data: {error}"))
}

fn save_midi_sysex_file(path: &Path, status: u8, payload: &[u8]) -> Result<(), String> {
    std::fs::write(path, midi_sysex_file_bytes(status, payload))
        .map_err(|error| format!("Could not save SysEx data: {error}"))
}

fn format_midi_event_hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MidiEventPropertiesOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MidiEventPropertiesRelativeValue {
    operator: MidiEventPropertiesOperator,
    operand: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MidiEventPropertiesEdit {
    Preserve,
    Absolute(i128),
    Relative(MidiEventPropertiesRelativeValue),
}

#[derive(Debug, Clone)]
struct MidiEventPropertiesControllerOptions(iced::widget::combo_box::State<String>);

impl MidiEventPropertiesControllerOptions {
    fn reset(&mut self, selection: Option<String>) {
        let options = view::midi_event_controller_property_options();
        self.0 = iced::widget::combo_box::State::with_selection(options, selection.as_ref());
    }
}

impl Default for MidiEventPropertiesControllerOptions {
    fn default() -> Self {
        Self(iced::widget::combo_box::State::new(
            view::midi_event_controller_property_options(),
        ))
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemSnapDirection {
    Left,
    Right,
    Nearest,
}

fn swing_grid_tick_candidates(
    project: &Project,
    approximate_tick: f64,
    grid_ticks: u64,
    swing_amount_percent: i16,
) -> Result<Vec<f64>, String> {
    if !approximate_tick.is_finite() || grid_ticks == 0 {
        return Err("the current snap grid is not available".to_owned());
    }
    let grid_ticks_f64 = grid_ticks as f64;
    let base_index = (approximate_tick / grid_ticks_f64).floor().max(0.0) as u64;
    let first_index = base_index.saturating_sub(2);
    let last_index = base_index.saturating_add(3);
    let swing_offset = grid_ticks_f64 * f64::from(swing_amount_percent.clamp(-100, 100)) / 200.0;
    let mut candidates = Vec::with_capacity(6);
    for index in first_index..=last_index {
        let straight_tick = index as f64 * grid_ticks_f64;
        let tick = if index % 2 == 1 {
            straight_tick + swing_offset
        } else {
            straight_tick
        };
        if tick < 0.0 {
            continue;
        }
        if tick > 0.0 && tick.fract().abs() < 0.000_001 {
            let rounded_tick = tick.round();
            if rounded_tick <= u64::MAX as f64
                && let Ok(position) = project.musical_position_at_tick(rounded_tick as u64)
                && position.beat() == 1
                && position.tick_in_beat() == 0
            {
                // REAPER's item snap actions leave non-zero measure starts out of
                // their target set. The project start itself remains a valid point.
                continue;
            }
        }
        candidates.push(tick);
    }
    candidates.sort_by(f64::total_cmp);
    candidates.dedup_by(|left, right| (*left - *right).abs() < 0.000_001);
    Ok(candidates)
}

fn snap_from_grid_candidates(
    candidates: &[f64],
    target: f64,
    direction: ItemSnapDirection,
) -> Result<f64, String> {
    let lower = candidates
        .iter()
        .copied()
        .rfind(|candidate| *candidate <= target);
    let upper = candidates
        .iter()
        .copied()
        .find(|candidate| *candidate >= target);
    match (lower, upper) {
        (Some(_lower), Some(upper)) if (upper - target).abs() < 0.000_001 => Ok(upper),
        (Some(lower), Some(_upper)) if (target - lower).abs() < 0.000_001 => Ok(lower),
        (Some(lower), Some(upper)) => Ok(match direction {
            ItemSnapDirection::Left => lower,
            ItemSnapDirection::Right => upper,
            ItemSnapDirection::Nearest if target - lower <= upper - target => lower,
            ItemSnapDirection::Nearest => upper,
        }),
        (Some(lower), None) => Ok(lower),
        (None, Some(upper)) => Ok(upper),
        (None, None) => Err("could not locate a snap point".to_owned()),
    }
}

fn midi_item_swing_snap_target(
    project: &Project,
    start_position: MidiItemPosition,
    grid_ticks: u64,
    direction: ItemSnapDirection,
    swing_amount_percent: i16,
) -> Result<MidiItemPosition, String> {
    let candidates =
        swing_grid_position_candidates(project, start_position, grid_ticks, swing_amount_percent)?;
    let lower = candidates
        .iter()
        .copied()
        .rfind(|candidate| *candidate <= start_position);
    let upper = candidates
        .iter()
        .copied()
        .find(|candidate| *candidate >= start_position);
    match (lower, upper) {
        (Some(_), Some(upper)) if upper == start_position => Ok(upper),
        (Some(lower), Some(_)) if lower == start_position => Ok(lower),
        (Some(lower), Some(upper)) => Ok(match direction {
            ItemSnapDirection::Left => lower,
            ItemSnapDirection::Right => upper,
            ItemSnapDirection::Nearest
                if start_position.total_subticks() - lower.total_subticks()
                    <= upper.total_subticks() - start_position.total_subticks() =>
            {
                lower
            }
            ItemSnapDirection::Nearest => upper,
        }),
        (Some(lower), None) => Ok(lower),
        (None, Some(upper)) => Ok(upper),
        (None, None) => Err("could not locate a snap point".to_owned()),
    }
}

fn swing_grid_position_candidates(
    project: &Project,
    approximate_position: MidiItemPosition,
    grid_ticks: u64,
    swing_amount_percent: i16,
) -> Result<Vec<MidiItemPosition>, String> {
    if grid_ticks == 0 {
        return Err("the current snap grid is not available".to_owned());
    }
    let base_index = approximate_position.tick() / grid_ticks;
    let first_index = base_index.saturating_sub(2);
    let last_index = base_index.saturating_add(3);
    let swing_amount = i128::from(swing_amount_percent.clamp(-100, 100));
    let grid_subticks = i128::from(grid_ticks)
        .checked_mul(i128::from(aaadaw_core::MIDI_ITEM_SUBTICKS_PER_TICK))
        .ok_or_else(|| "the snap grid is outside the supported timeline".to_owned())?;
    let mut candidates = Vec::with_capacity(6);
    for index in first_index..=last_index {
        let straight = i128::from(index)
            .checked_mul(grid_subticks)
            .ok_or_else(|| "the snap grid is outside the supported timeline".to_owned())?;
        let swing = if index % 2 == 1 {
            i128::from(grid_ticks)
                .checked_mul(swing_amount)
                .ok_or_else(|| "the swing offset is outside the supported timeline".to_owned())?
        } else {
            0
        };
        let total_subticks = straight
            .checked_add(swing)
            .ok_or_else(|| "the swing point is outside the supported timeline".to_owned())?;
        if total_subticks < 0 {
            continue;
        }
        let Some(position) = MidiItemPosition::from_total_subticks(total_subticks as u128) else {
            continue;
        };
        if position.subtick() == 0
            && position.tick() > 0
            && let Ok(musical_position) = project.musical_position_at_tick(position.tick())
            && musical_position.beat() == 1
            && musical_position.tick_in_beat() == 0
        {
            // REAPER's item-snap actions omit non-zero measure starts from their target set.
            continue;
        }
        candidates.push(position);
    }
    candidates.sort_unstable();
    candidates.dedup();
    Ok(candidates)
}

fn audio_item_snap_target(
    project: &Project,
    start_sample: u64,
    grid_ticks: u64,
    direction: ItemSnapDirection,
    swing_amount_percent: i16,
) -> Result<u64, String> {
    let approximate_tick = project
        .tick_at_sample_position(start_sample as f64)
        .map_err(|error| error.to_string())?;
    let candidates =
        swing_grid_tick_candidates(project, approximate_tick, grid_ticks, swing_amount_percent)?;
    let candidate_samples = candidates
        .into_iter()
        .map(|tick| {
            project
                .sample_at_tick_position(tick)
                .map(|sample| sample.round())
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let target = snap_from_grid_candidates(&candidate_samples, start_sample as f64, direction)?;
    if target < 0.0 || target > u64::MAX as f64 {
        return Err("the snap point is outside the timeline".to_owned());
    }
    Ok(target.round() as u64)
}

fn action_updates_item_mix(action: &DawAction) -> bool {
    match action {
        DawAction::SetAudioItemFades { .. }
        | DawAction::SetAudioItemVolume { .. }
        | DawAction::SetAudioItemMute { .. }
        | DawAction::SetAudioItemPhase { .. }
        | DawAction::SetAudioItemTakeMix { .. } => true,
        DawAction::BatchTransaction { actions, .. } => actions.iter().any(action_updates_item_mix),
        _ => false,
    }
}

fn parse_midi_event_bbt(
    value: &str,
    ppq: u64,
    meter: TimeSignature,
    is_length: bool,
) -> Option<u64> {
    let mut components = value.trim().split('.');
    let bar = components.next()?.parse::<u64>().ok()?;
    let beat = components.next()?.parse::<u64>().ok()?;
    let hundredths = components.next()?.parse::<u64>().ok()?;
    if components.next().is_some() || ppq == 0 || hundredths > 99 {
        return None;
    }
    let beat_ticks = ppq
        .checked_mul(4)?
        .checked_div(u64::from(meter.denominator()))?
        .max(1);
    let measure_ticks = beat_ticks.checked_mul(u64::from(meter.numerator()))?;
    let (bars, beats) = if is_length {
        if beat >= u64::from(meter.numerator()) {
            return None;
        }
        (bar, beat)
    } else {
        if bar == 0 || beat == 0 || beat > u64::from(meter.numerator()) {
            return None;
        }
        (bar - 1, beat - 1)
    };
    let subticks = beat_ticks
        .checked_mul(hundredths)?
        .checked_add(50)?
        .checked_div(100)?;
    bars.checked_mul(measure_ticks)?
        .checked_add(beats.checked_mul(beat_ticks)?)?
        .checked_add(subticks)
}

fn parse_midi_event_property_edit(
    value: &str,
    mixed: bool,
    parse_absolute: impl FnOnce(&str) -> Option<i128>,
    parse_relative: impl FnOnce(&str) -> Option<MidiEventPropertiesRelativeValue>,
) -> Option<MidiEventPropertiesEdit> {
    let value = value.trim();
    if value.is_empty() {
        return mixed.then_some(MidiEventPropertiesEdit::Preserve);
    }
    if matches!(value.chars().next(), Some('+' | '-' | '*' | '/')) {
        return parse_relative(value).map(MidiEventPropertiesEdit::Relative);
    }
    parse_absolute(value).map(MidiEventPropertiesEdit::Absolute)
}

fn parse_midi_event_relative_numeric(value: &str) -> Option<MidiEventPropertiesRelativeValue> {
    let mut characters = value.trim().chars();
    let operator = match characters.next()? {
        '+' => MidiEventPropertiesOperator::Add,
        '-' => MidiEventPropertiesOperator::Subtract,
        '*' => MidiEventPropertiesOperator::Multiply,
        '/' => MidiEventPropertiesOperator::Divide,
        _ => return None,
    };
    let operand = characters.as_str().parse::<f64>().ok()?;
    if !operand.is_finite() || (operator == MidiEventPropertiesOperator::Divide && operand == 0.0) {
        return None;
    }
    Some(MidiEventPropertiesRelativeValue { operator, operand })
}

fn parse_midi_event_time_edit(
    value: &str,
    mixed: bool,
    ppq: u64,
    meter: TimeSignature,
    is_length: bool,
) -> Option<MidiEventPropertiesEdit> {
    parse_midi_event_property_edit(
        value,
        mixed,
        |value| parse_midi_event_bbt(value, ppq, meter, is_length).map(i128::from),
        |value| {
            let value = value.trim();
            let mut characters = value.chars();
            let operator = match characters.next()? {
                '+' => MidiEventPropertiesOperator::Add,
                '-' => MidiEventPropertiesOperator::Subtract,
                '*' => MidiEventPropertiesOperator::Multiply,
                '/' => MidiEventPropertiesOperator::Divide,
                _ => return None,
            };
            let operand = characters.as_str();
            let operand = if operand.contains('.')
                && matches!(
                    operator,
                    MidiEventPropertiesOperator::Add | MidiEventPropertiesOperator::Subtract
                ) {
                parse_midi_event_bbt(operand, ppq, meter, true)? as f64
            } else {
                operand.parse::<f64>().ok()?
            };
            if !operand.is_finite()
                || (operator == MidiEventPropertiesOperator::Divide && operand == 0.0)
            {
                return None;
            }
            Some(MidiEventPropertiesRelativeValue { operator, operand })
        },
    )
}

fn parse_midi_event_pitch_edit(value: &str, mixed: bool) -> Option<MidiEventPropertiesEdit> {
    parse_midi_event_property_edit(
        value,
        mixed,
        |value| parse_midi_event_pitch(value).map(i128::from),
        |value| {
            let value = value.trim();
            let first = value.chars().next()?;
            if matches!(first, '+' | '-') {
                let operand = &value[first.len_utf8()..];
                if let Some((octaves, semitones)) = operand.split_once(['o', 'O']) {
                    let octaves = octaves.parse::<i128>().ok()?;
                    let semitones = if semitones.is_empty() {
                        0
                    } else {
                        semitones.parse::<i128>().ok()?
                    };
                    let magnitude = octaves.checked_mul(12)?.checked_add(semitones)?;
                    let (operator, magnitude) = if first == '+' {
                        (MidiEventPropertiesOperator::Add, magnitude)
                    } else {
                        (MidiEventPropertiesOperator::Subtract, magnitude)
                    };
                    return Some(MidiEventPropertiesRelativeValue {
                        operator,
                        operand: magnitude as f64,
                    });
                }
            }
            parse_midi_event_relative_numeric(value)
        },
    )
}

fn apply_midi_event_properties_edit(
    original: i128,
    edit: MidiEventPropertiesEdit,
    minimum: i128,
    maximum: i128,
) -> Option<i128> {
    let value = match edit {
        MidiEventPropertiesEdit::Preserve => original,
        MidiEventPropertiesEdit::Absolute(value) => value,
        MidiEventPropertiesEdit::Relative(relative) => {
            let original = original as f64;
            let result = match relative.operator {
                MidiEventPropertiesOperator::Add => original + relative.operand,
                MidiEventPropertiesOperator::Subtract => original - relative.operand,
                MidiEventPropertiesOperator::Multiply => original * relative.operand,
                MidiEventPropertiesOperator::Divide => original / relative.operand,
            };
            if !result.is_finite() || result < minimum as f64 || result > maximum as f64 {
                return None;
            }
            result.round() as i128
        }
    };
    (minimum..=maximum).contains(&value).then_some(value)
}

fn midi_event_pitch_label(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
    ];
    format!(
        "{pitch} {}{}",
        NAMES[usize::from(pitch % 12)],
        i16::from(pitch) / 12 - 1
    )
}

fn parse_midi_event_pitch(value: &str) -> Option<u8> {
    if let Ok(number) = value.split_whitespace().next()?.parse::<u8>() {
        return (number <= 127).then_some(number);
    }
    let mut chars = value.trim().chars();
    let pitch_class = match chars.next()?.to_ascii_uppercase() {
        'C' => 0_i16,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let mut semitone = pitch_class;
    let next = chars.next()?;
    let octave = if matches!(next, '#' | 'b' | '♯' | '♭') {
        semitone += match next {
            '#' | '♯' => 1,
            'b' | '♭' => -1,
            _ => unreachable!(),
        };
        chars.as_str().parse::<i16>().ok()?
    } else {
        let octave = next.to_string();
        let rest = chars.as_str();
        if rest.is_empty() {
            octave.parse::<i16>().ok()?
        } else {
            format!("{octave}{rest}").parse::<i16>().ok()?
        }
    };
    let midi_pitch = (octave + 1).checked_mul(12)?.checked_add(semitone)?;
    u8::try_from(midi_pitch).ok().filter(|pitch| *pitch <= 127)
}

fn parse_midi_event_controller(value: &str) -> Option<u8> {
    value
        .split_whitespace()
        .next()?
        .parse::<u8>()
        .ok()
        .filter(|controller| *controller <= 127)
}

fn common_midi_event_property(values: impl IntoIterator<Item = String>) -> (String, bool) {
    let mut values = values.into_iter();
    let Some(first) = values.next() else {
        return (String::new(), false);
    };
    if values.any(|value| value != first) {
        (String::new(), true)
    } else {
        (first, false)
    }
}

fn effective_midi_bank_value(
    item: &aaadaw_core::MidiItem,
    channel: u8,
    tick: u64,
    controller: u8,
) -> u8 {
    item.controllers()
        .iter()
        .filter(|event| {
            event.channel == channel && event.controller == controller && event.tick <= tick
        })
        .max_by_key(|event| event.tick)
        .map_or(0, |event| event.value)
}

fn set_midi_bank_value(
    controllers: &mut Vec<MidiControllerData>,
    channel: u8,
    tick: u64,
    controller: u8,
    value: u8,
) {
    if let Some(event) = controllers.iter_mut().find(|event| {
        event.channel == channel && event.controller == controller && event.tick == tick
    }) {
        event.value = value;
    } else {
        controllers.push(MidiControllerData {
            controller,
            channel,
            tick,
            value,
        });
    }
}

#[cfg(feature = "audio-device")]
fn action_rebuilds_playback_graph(action: &DawAction) -> bool {
    match action {
        DawAction::SetTrackOutput { .. }
        | DawAction::SetProjectSampleRate { .. }
        | DawAction::SetTrackMainSend { .. }
        | DawAction::CreateAudioSend { .. }
        | DawAction::UpdateAudioSend { .. }
        | DawAction::DeleteAudioSend { .. }
        | DawAction::SetTrackFxChain { .. }
        | DawAction::SetTempo { .. }
        | DawAction::DeleteTempoPoint { .. }
        | DawAction::SetTempoCurve { .. }
        | DawAction::SetTrackFxParameterAutomation { .. }
        | DawAction::SetAudioItemActiveTake { .. }
        | DawAction::DeleteActiveAudioItemTake { .. }
        | DawAction::DeleteActiveMidiItemTake { .. }
        | DawAction::PasteAudioItemAsTake { .. }
        | DawAction::SetMidiItemPosition { .. }
        | DawAction::EditMidiItemPlacement { .. }
        | DawAction::SetMidiItemContents { .. }
        | DawAction::SetMidiItemLoopSource { .. }
        | DawAction::SetMidiItemMute { .. }
        | DawAction::DeleteMidiItem { .. }
        | DawAction::ImplodeMidiItemsOnSameTrackIntoTakes { .. }
        | DawAction::SetMidiControllers { .. }
        | DawAction::SetMidiProgramChanges { .. }
        | DawAction::SetAudioItemTakePlaybackRate { .. }
        | DawAction::ImportMidiTrack { .. }
        | DawAction::FreezeTrack { .. }
        | DawAction::UnfreezeTrack { .. } => true,
        DawAction::BatchTransaction { actions, .. } => {
            actions.iter().any(action_rebuilds_playback_graph)
        }
        _ => false,
    }
}

#[cfg(feature = "audio-device")]
fn rescale_transport_sample(sample: u64, old_rate: u32, new_rate: u32) -> u64 {
    if old_rate == 0 || new_rate == 0 {
        return sample;
    }
    let numerator = u128::from(sample) * u128::from(new_rate);
    let denominator = u128::from(old_rate);
    let rounded = numerator / denominator + u128::from(numerator % denominator * 2 >= denominator);
    u64::try_from(rounded).unwrap_or(u64::MAX)
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

#[cfg(all(feature = "audio-device", target_os = "android"))]
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

#[cfg(all(feature = "audio-device", target_os = "android"))]
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
    Mixer,
}

struct MediaPanelDock {
    panes: Option<pane_grid::State<MainPane>>,
    split: Option<Split>,
    mixer_split: Option<Split>,
    open: bool,
    mixer_open: bool,
    main_ratio: f32,
    arrange_ratio: f32,
}

impl Default for MediaPanelDock {
    fn default() -> Self {
        Self::from_layout(desktop_layout::DesktopLayout::default())
    }
}

impl MediaPanelDock {
    fn from_layout(layout: desktop_layout::DesktopLayout) -> Self {
        let mut dock = Self {
            panes: None,
            split: None,
            mixer_split: None,
            open: layout.media_open,
            mixer_open: layout.mixer_open,
            main_ratio: layout.media_ratio,
            arrange_ratio: layout.arrange_ratio,
        };
        dock.rebuild();
        dock
    }
    fn layout(&self) -> desktop_layout::DesktopLayout {
        desktop_layout::DesktopLayout {
            media_open: self.open,
            mixer_open: self.mixer_open,
            media_ratio: self.main_ratio,
            arrange_ratio: self.arrange_ratio,
            ..Default::default()
        }
    }
    fn rebuild(&mut self) {
        self.split = None;
        self.mixer_split = None;
        if !self.open && !self.mixer_open {
            self.panes = None;
            return;
        }
        let (mut panes, arrangement) = pane_grid::State::new(MainPane::Arrangement);
        if self.open {
            let (_, split) = panes
                .split(Axis::Vertical, arrangement, MainPane::MediaBrowser)
                .expect("arrangement exists");
            panes.resize(split, self.main_ratio);
            self.split = Some(split);
        }
        if self.mixer_open {
            let (_, split) = panes
                .split(Axis::Horizontal, arrangement, MainPane::Mixer)
                .expect("arrangement exists");
            panes.resize(split, self.arrange_ratio);
            self.mixer_split = Some(split);
        }
        self.panes = Some(panes);
    }
    fn toggle(&mut self) {
        self.open = !self.open;
        self.rebuild();
    }
    fn toggle_mixer(&mut self) {
        self.mixer_open = !self.mixer_open;
        self.rebuild();
    }
    fn resize(&mut self, split: Split, ratio: f32) {
        if !ratio.is_finite() {
            return;
        }
        let ratio = if self.split == Some(split) {
            self.main_ratio = ratio.clamp(0.55, 0.86);
            self.main_ratio
        } else if self.mixer_split == Some(split) {
            self.arrange_ratio = ratio.clamp(0.20, 0.90);
            self.arrange_ratio
        } else {
            return;
        };
        if let Some(panes) = &mut self.panes {
            panes.resize(split, ratio);
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
    fn visible_midi_note_ids(&self) -> HashSet<aaadaw_core::NoteId> {
        let Some(item_id) = self.midi_editor_item_id else {
            return HashSet::new();
        };
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return HashSet::new();
        };
        let measure_ticks = midi_editor_measure_length_ticks(
            self.project.settings().ppq(),
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let filter = MidiEditorFilterState {
            channel_selection: self.midi_editor_channel_selection,
            channel_filter_enabled: self.midi_editor_filter_enabled,
            event_filter_enabled: self.midi_editor_event_filter_enabled,
            channels: self.midi_editor_filter_channels,
            inverted: self.midi_editor_filter_inverted,
            event_type: self.midi_editor_event_type_filter,
            note: self.midi_editor_note_filter,
            controller: self.midi_editor_controller_filter,
            pitch_bend: self.midi_editor_pitch_bend_filter,
            program_change: self.midi_editor_program_change_filter,
            position: self.midi_editor_position_filter,
        };
        item.notes()
            .iter()
            .filter(|note| {
                filter.note_visible(
                    note.channel(),
                    midi_editor_note_filter_event(
                        note,
                        self.project.settings().ppq(),
                        measure_ticks,
                        self.midi_editor_position_filter.repeat_every,
                    ),
                )
            })
            .map(|note| note.id())
            .collect()
    }

    fn remap_selected_controller_events(
        selected: &HashSet<MidiControllerData>,
        after: &[MidiControllerData],
    ) -> HashSet<MidiControllerData> {
        let mut remaining = after.to_vec();
        let mut remapped = HashSet::new();
        for event in selected {
            if remaining.contains(event) {
                remapped.insert(*event);
                remaining.retain(|candidate| candidate != event);
            } else if let Some((index, candidate)) = remaining
                .iter()
                .enumerate()
                .filter(|(_, candidate)| {
                    candidate.channel == event.channel
                        && candidate.controller == event.controller
                        && candidate.value == event.value
                })
                .min_by_key(|(_, candidate)| candidate.tick.abs_diff(event.tick))
            {
                remapped.insert(*candidate);
                remaining.remove(index);
            } else if let Some((index, candidate)) = remaining
                .iter()
                .enumerate()
                .filter(|(_, candidate)| {
                    candidate.channel == event.channel && candidate.controller == event.controller
                })
                .min_by_key(|(_, candidate)| candidate.tick.abs_diff(event.tick))
            {
                remapped.insert(*candidate);
                remaining.remove(index);
            }
        }
        remapped
    }

    fn remap_selected_pitch_bend_events(
        selected: &HashSet<MidiPitchBendData>,
        after: &[MidiPitchBendData],
    ) -> HashSet<MidiPitchBendData> {
        let mut remaining = after.to_vec();
        let mut remapped = HashSet::new();
        for event in selected {
            if remaining.contains(event) {
                remapped.insert(*event);
                remaining.retain(|candidate| candidate != event);
            } else if let Some((index, candidate)) = remaining
                .iter()
                .enumerate()
                .filter(|(_, candidate)| {
                    candidate.channel == event.channel && candidate.value == event.value
                })
                .min_by_key(|(_, candidate)| candidate.tick.abs_diff(event.tick))
            {
                remapped.insert(*candidate);
                remaining.remove(index);
            } else if let Some((index, candidate)) = remaining
                .iter()
                .enumerate()
                .filter(|(_, candidate)| candidate.channel == event.channel)
                .min_by_key(|(_, candidate)| candidate.tick.abs_diff(event.tick))
            {
                remapped.insert(*candidate);
                remaining.remove(index);
            }
        }
        remapped
    }

    fn remap_selected_midi_point_events<T: Copy + Eq + std::hash::Hash>(
        selected: &HashSet<T>,
        after: &[T],
        same_payload: impl Fn(&T, &T) -> bool,
        same_identity: impl Fn(&T, &T) -> bool,
        tick: impl Fn(&T) -> u64,
    ) -> HashSet<T> {
        let mut remaining = after.to_vec();
        let mut remapped = HashSet::new();
        for event in selected {
            if remaining.contains(event) {
                remapped.insert(*event);
                remaining.retain(|candidate| candidate != event);
            } else if let Some((index, candidate)) = remaining
                .iter()
                .enumerate()
                .filter(|(_, candidate)| same_payload(event, candidate))
                .min_by_key(|(_, candidate)| tick(candidate).abs_diff(tick(event)))
            {
                remapped.insert(*candidate);
                remaining.remove(index);
            } else if let Some((index, candidate)) = remaining
                .iter()
                .enumerate()
                .filter(|(_, candidate)| same_identity(event, candidate))
                .min_by_key(|(_, candidate)| tick(candidate).abs_diff(tick(event)))
            {
                remapped.insert(*candidate);
                remaining.remove(index);
            }
        }
        remapped
    }

    fn remap_selected_midi_opaque_events(
        selected: &[MidiOpaqueEventData],
        after: &[MidiOpaqueEventData],
    ) -> HashSet<u32> {
        let mut remaining = after
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, event)| (index as u32, event))
            .collect::<Vec<_>>();
        let mut remapped = HashSet::new();
        for event in selected {
            let position = remaining
                .iter()
                .position(|(_, candidate)| candidate == event)
                .or_else(|| {
                    remaining
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, candidate))| {
                            candidate.order == event.order && candidate.kind == event.kind
                        })
                        .min_by_key(|(_, (_, candidate))| candidate.tick.abs_diff(event.tick))
                        .map(|(position, _)| position)
                });
            if let Some(position) = position {
                let (index, _) = remaining.remove(position);
                remapped.insert(index);
            }
        }
        remapped
    }

    fn selected_midi_opaque_event_data(&self) -> Vec<MidiOpaqueEventData> {
        let Some(item_id) = self.midi_editor_item_id else {
            return Vec::new();
        };
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return Vec::new();
        };
        self.midi_editor_selected_opaque_events
            .iter()
            .filter_map(|index| item.opaque_events().get(*index as usize).cloned())
            .collect()
    }

    fn midi_event_list_cursor_is_selected(&self, cursor: MidiEditorEventListCursor) -> bool {
        match cursor {
            MidiEditorEventListCursor::Note(note_id) => {
                self.midi_editor_selected_notes.contains(&note_id)
            }
            MidiEditorEventListCursor::Controller(event) => {
                self.midi_editor_selected_controllers.contains(&event)
            }
            MidiEditorEventListCursor::BankSelect(msb, lsb) => {
                self.midi_editor_selected_controllers.contains(&msb)
                    || self.midi_editor_selected_controllers.contains(&lsb)
            }
            MidiEditorEventListCursor::PitchBend(event) => {
                self.midi_editor_selected_pitch_bends.contains(&event)
            }
            MidiEditorEventListCursor::ProgramChange(event) => {
                self.midi_editor_selected_program_changes.contains(&event)
            }
            MidiEditorEventListCursor::PolyAftertouch(event) => {
                self.midi_editor_selected_poly_aftertouches.contains(&event)
            }
            MidiEditorEventListCursor::ChannelPressure(event) => {
                self.midi_editor_selected_channel_pressures.contains(&event)
            }
            MidiEditorEventListCursor::Opaque(index) => {
                self.midi_editor_selected_opaque_events.contains(&index)
            }
            MidiEditorEventListCursor::End => false,
        }
    }

    fn move_midi_editor_event_list_cursor(
        &mut self,
        direction: MidiEditorEventListCursorDirection,
        preserve_selection: bool,
        extend_selection: bool,
    ) {
        if self.midi_editor_mode != MidiEditorMode::EventList {
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
            self.midi_editor_event_list_cursor = None;
            self.midi_editor_event_list_anchor = None;
            return;
        };
        let candidates = view::midi_event_list_cursors(self, item);
        if candidates.is_empty() {
            self.midi_editor_event_list_cursor = None;
            self.midi_editor_event_list_anchor = None;
            return;
        }
        let cursor = self
            .midi_editor_event_list_cursor
            .filter(|cursor| candidates.contains(cursor))
            .or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .find(|cursor| self.midi_event_list_cursor_is_selected(*cursor))
            });
        let current_index =
            cursor.and_then(|cursor| candidates.iter().position(|candidate| *candidate == cursor));
        let has_multiselection = self.midi_editor_selected_notes.len()
            + self.midi_editor_selected_controllers.len()
            + self.midi_editor_selected_pitch_bends.len()
            + self.midi_editor_selected_program_changes.len()
            + self.midi_editor_selected_poly_aftertouches.len()
            + self.midi_editor_selected_channel_pressures.len()
            + self.midi_editor_selected_opaque_events.len()
            > 1;
        let collapse_at_cursor = !preserve_selection && !extend_selection && has_multiselection;
        let default_index = match direction {
            MidiEditorEventListCursorDirection::Previous => candidates.len() - 1,
            MidiEditorEventListCursorDirection::Next => 0,
        };
        let next_index = if collapse_at_cursor {
            current_index.unwrap_or(default_index)
        } else {
            match (current_index, direction) {
                (None, _) => default_index,
                (Some(index), MidiEditorEventListCursorDirection::Previous) => {
                    index.saturating_sub(1)
                }
                (Some(index), MidiEditorEventListCursorDirection::Next) => {
                    (index + 1).min(candidates.len() - 1)
                }
            }
        };
        let cursor = candidates[next_index];
        self.midi_editor_event_list_cursor = Some(cursor);
        if !preserve_selection {
            self.midi_editor_selected_notes.clear();
            self.midi_editor_selected_controllers.clear();
            self.midi_editor_selected_pitch_bends.clear();
            self.midi_editor_selected_program_changes.clear();
            self.midi_editor_selected_poly_aftertouches.clear();
            self.midi_editor_selected_channel_pressures.clear();
            self.midi_editor_selected_opaque_events.clear();
            if extend_selection {
                let anchor_index = self
                    .midi_editor_event_list_anchor
                    .filter(|anchor| candidates.contains(anchor))
                    .and_then(|anchor| candidates.iter().position(|candidate| *candidate == anchor))
                    .or(current_index)
                    .unwrap_or(next_index);
                let (start, end) = if anchor_index <= next_index {
                    (anchor_index, next_index)
                } else {
                    (next_index, anchor_index)
                };
                for range_cursor in candidates[start..=end].iter().copied() {
                    match range_cursor {
                        MidiEditorEventListCursor::Note(note_id) => {
                            self.midi_editor_selected_notes.insert(note_id);
                        }
                        MidiEditorEventListCursor::Controller(event) => {
                            self.midi_editor_selected_controllers.insert(event);
                        }
                        MidiEditorEventListCursor::BankSelect(msb, lsb) => {
                            self.midi_editor_selected_controllers.insert(msb);
                            self.midi_editor_selected_controllers.insert(lsb);
                        }
                        MidiEditorEventListCursor::PitchBend(event) => {
                            self.midi_editor_selected_pitch_bends.insert(event);
                        }
                        MidiEditorEventListCursor::ProgramChange(event) => {
                            self.midi_editor_selected_program_changes.insert(event);
                        }
                        MidiEditorEventListCursor::PolyAftertouch(event) => {
                            self.midi_editor_selected_poly_aftertouches.insert(event);
                        }
                        MidiEditorEventListCursor::ChannelPressure(event) => {
                            self.midi_editor_selected_channel_pressures.insert(event);
                        }
                        MidiEditorEventListCursor::Opaque(index) => {
                            self.midi_editor_selected_opaque_events.insert(index);
                        }
                        MidiEditorEventListCursor::End => {}
                    }
                }
            } else {
                match cursor {
                    MidiEditorEventListCursor::Note(note_id) => {
                        self.midi_editor_selected_notes.insert(note_id);
                    }
                    MidiEditorEventListCursor::Controller(event) => {
                        self.midi_editor_selected_controllers.insert(event);
                    }
                    MidiEditorEventListCursor::BankSelect(msb, lsb) => {
                        self.midi_editor_selected_controllers.insert(msb);
                        self.midi_editor_selected_controllers.insert(lsb);
                    }
                    MidiEditorEventListCursor::PitchBend(event) => {
                        self.midi_editor_selected_pitch_bends.insert(event);
                    }
                    MidiEditorEventListCursor::ProgramChange(event) => {
                        self.midi_editor_selected_program_changes.insert(event);
                    }
                    MidiEditorEventListCursor::PolyAftertouch(event) => {
                        self.midi_editor_selected_poly_aftertouches.insert(event);
                    }
                    MidiEditorEventListCursor::ChannelPressure(event) => {
                        self.midi_editor_selected_channel_pressures.insert(event);
                    }
                    MidiEditorEventListCursor::Opaque(index) => {
                        self.midi_editor_selected_opaque_events.insert(index);
                    }
                    MidiEditorEventListCursor::End => {}
                }
            }
        }
        if !preserve_selection && !extend_selection {
            self.midi_editor_event_list_anchor = Some(cursor);
        } else if extend_selection
            && self
                .midi_editor_event_list_anchor
                .is_none_or(|anchor| !candidates.contains(&anchor))
        {
            self.midi_editor_event_list_anchor = Some(
                current_index
                    .map(|index| candidates[index])
                    .unwrap_or(cursor),
            );
        }
    }

    fn is_mobile_main_window(&self) -> bool {
        self.shell_profile == ShellProfile::Touch
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
        if self.mobile_panel == MobilePanel::MidiEditor {
            self.release_midi_preview();
            self.midi_editor_window_id = None;
            self.midi_editor_item_id = None;
            self.midi_editor_feedback = None;
            self.midi_editor_selected_notes.clear();
            self.midi_editor_selected_controllers.clear();
            self.midi_editor_selected_pitch_bends.clear();
            self.midi_editor_selected_program_changes.clear();
            self.midi_editor_selected_opaque_events.clear();
            self.midi_editor_event_list_cursor = None;
            self.midi_editor_event_list_anchor = None;
            self.midi_editor_edit_cursor_tick = None;
        }
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
        match recent_projects::load() {
            Ok(paths) => app.recent_project_paths = paths,
            Err(error) => tracing::warn!(%error, "recent-project history could not be loaded"),
        }
        match project_templates::load() {
            Ok(paths) => app.project_template_paths = paths,
            Err(error) => tracing::warn!(%error, "project-template list could not be loaded"),
        }
        match project_defaults::load().and_then(|defaults| defaults.create_project()) {
            Ok(project) => app.project = project,
            Err(error) => {
                app.status = format!("Default project settings unavailable: {error}");
            }
        }
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
        if !app.is_mobile_main_window() {
            match desktop_layout::load() {
                Ok(layout) => app.media_panel_dock = MediaPanelDock::from_layout(layout),
                Err(error) => app.status = format!("Desktop layout unavailable: {error}"),
            }
        }
        match loop_time_preferences::load() {
            Ok(preferences) => {
                app.timeline.loop_points_linked = preferences.linked;
            }
            Err(error) => {
                app.status = format!("Loop/time selection preferences unavailable: {error}");
            }
        }
        match midi_humanize_preferences::load() {
            Ok(preferences) => app.midi_humanize_preferences = preferences,
            Err(error) => {
                app.status = format!("MIDI Humanize preferences unavailable: {error}");
            }
        }
        match midi_quantize_preferences::load() {
            Ok(preferences) => app.midi_quantize_preferences = preferences,
            Err(error) => {
                app.status = format!("MIDI Quantize preferences unavailable: {error}");
            }
        }
        match render_wildcard_preferences::load() {
            Ok(preferences) => {
                app.render_dialog.allow_slash_in_wildcard_substitutions =
                    preferences.allow_slash_in_substitutions;
            }
            Err(error) => {
                app.status = format!("Render wildcard preferences unavailable: {error}");
            }
        }
        match item_properties::load_unit() {
            Ok(unit) => app.item_properties_unit = unit,
            Err(error) => {
                app.status = format!("Item Properties configuration unavailable: {error}")
            }
        }
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

    pub(super) fn project_display_title(&self) -> String {
        let project_name = self
            .project_path
            .as_ref()
            .and_then(|path| path.file_name())
            .map_or_else(
                || "New project".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            );
        format!("{project_name}{}", if self.is_dirty() { "  *" } else { "" })
    }

    fn window_title(&self, window_id: iced::window::Id) -> String {
        if self.item_properties_window_id == Some(window_id) {
            "Media Item Properties".to_owned()
        } else if self.item_nudge_window_id == Some(window_id) {
            "Nudge/Set Items".to_owned()
        } else if self.marker_properties_window_id == Some(window_id) {
            "Edit Marker".to_owned()
        } else if self.region_properties_window_id == Some(window_id) {
            "Edit Region".to_owned()
        } else if self.marker_manager_window_id == Some(window_id) {
            "Region/Marker Manager".to_owned()
        } else if self.region_render_matrix_window_id == Some(window_id) {
            "Region Render Matrix".to_owned()
        } else if self.routing_matrix_window_id == Some(window_id) {
            "Routing Matrix".to_owned()
        } else if self.routing_window_id == Some(window_id) {
            self.routing_track_id
                .and_then(|id| self.project.tracks().iter().find(|track| track.id() == id))
                .map_or_else(
                    || "Track routing".to_owned(),
                    |track| format!("Routing for {}", track.name()),
                )
        } else if self.action_input_window_id == Some(window_id) {
            "Keyboard input".to_owned()
        } else if self.action_list_window_id == Some(window_id) {
            "Actions".to_owned()
        } else if self.project_settings_window_id == Some(window_id) {
            "Project Settings".to_owned()
        } else if self.snap_grid_settings_window_id == Some(window_id) {
            "Snap/Grid Settings".to_owned()
        } else if self.settings_window_id == Some(window_id) {
            "AAADAW Settings".to_owned()
        } else if self.about_window_id == Some(window_id) {
            "About AAADAW".to_owned()
        } else if self.render_window_id == Some(window_id) {
            "Render project to WAV".to_owned()
        } else if self.midi_export_window_id == Some(window_id) {
            "Export project MIDI".to_owned()
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
        } else if self.main_window_id == Some(window_id) {
            format!("{} — AAADAW", self.project_display_title())
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
        let app_can_poll = !cfg!(target_os = "android") || !self.android_app_unfocused;
        let background_ticks = if app_can_poll
            && (self.import_busy
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
                || self.master_mix_gesture.is_some()
                || self.desktop_layout_save_at.is_some()
                || self.loop_time_preferences_save_at.is_some()
                || self.midi_humanize_preferences_save_at.is_some()
                || self.midi_quantize_preferences_save_at.is_some()
                || self.unsaved_session_snapshot_at.is_some())
        {
            iced::time::every(background_tick_interval).map(|_| Message::BackgroundTick)
        } else {
            iced::Subscription::none()
        };
        let meter_ticks = if app_can_poll && playback_active {
            iced::time::every(Duration::from_millis(33)).map(|_| Message::MeterTick)
        } else {
            iced::Subscription::none()
        };
        iced::Subscription::batch([
            iced::event::listen_with(runtime_keyboard_event),
            iced::event::listen_with(android_app_focus_event),
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

    fn commit_midi_humanize_dialog(&mut self) {
        let Some(dialog) = self.midi_humanize_dialog.take() else {
            return;
        };
        self.midi_humanize_preferences = midi_humanize_preferences::MidiHumanizePreferences {
            all_notes: dialog.scope == MidiHumanizeScope::AllNotes,
            timing_amount_percent: dialog.timing_amount_percent,
            velocity_amount_percent: dialog.velocity_amount_percent,
            timing_bias_percent: dialog.timing_bias_percent,
            ..Default::default()
        };
        self.schedule_midi_humanize_preferences_save();
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == dialog.item_id)
        else {
            return;
        };
        let note_ids: Vec<aaadaw_core::NoteId> = match dialog.scope {
            MidiHumanizeScope::AllNotes => item.notes().iter().map(|note| note.id()).collect(),
            MidiHumanizeScope::SelectedNotes => {
                self.midi_editor_selected_notes.iter().copied().collect()
            }
        };
        if note_ids.is_empty() {
            return;
        }
        self.apply_action(
            DawAction::HumanizeMidiNotes {
                item_id: dialog.item_id,
                note_ids,
                grid: midi_editor_grid_fraction(self.timeline.midi_editor_snap_grid),
                timing_amount_percent: dialog.timing_amount_percent,
                velocity_amount_percent: dialog.velocity_amount_percent,
                timing_bias_percent: dialog.timing_bias_percent,
                seed: dialog.seed,
            },
            "MIDI notes humanized",
        );
    }

    fn commit_midi_transpose_dialog(&mut self) -> Task<Message> {
        let Some(dialog) = self.midi_transpose_dialog.take() else {
            return Task::none();
        };
        let Ok(semitones) = dialog.semitones.trim().parse::<i8>() else {
            return Task::none();
        };
        if semitones == 0 {
            return Task::none();
        }
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == dialog.item_id)
        else {
            return Task::none();
        };
        let edits = item
            .notes()
            .iter()
            .filter(|note| {
                dialog.scope == MidiTransposeScope::AllNotes
                    || self.midi_editor_selected_notes.contains(&note.id())
            })
            .map(|note| {
                note.pitch().checked_add_signed(semitones).map(|pitch| {
                    (
                        note.id(),
                        MidiNoteData {
                            pitch,
                            channel: note.channel(),
                            tick: note.tick(),
                            duration: note.duration(),
                            velocity: note.velocity(),
                            off_velocity: note.off_velocity(),
                        },
                    )
                })
            })
            .collect::<Option<Vec<_>>>();
        let Some(edits) = edits else {
            return Task::none();
        };
        if edits.is_empty() {
            Task::none()
        } else {
            self.update(Message::EditMidiNotes(dialog.item_id, edits))
        }
    }

    fn midi_event_properties_state(
        &self,
        item_id: ItemId,
        target: MidiEventPropertiesTarget,
    ) -> Option<MidiEventPropertiesDialogState> {
        let item = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)?;
        let ppq = u64::from(self.project.settings().ppq());
        let meter = self.project.time_signature_at_tick(0);
        let (
            position,
            position_mixed,
            length,
            length_mixed,
            channel,
            channel_mixed,
            parameter,
            parameter_mixed,
            value,
            value_mixed,
        ) = match &target {
            MidiEventPropertiesTarget::Terminal => (
                String::new(),
                false,
                String::new(),
                false,
                String::new(),
                false,
                String::new(),
                false,
                String::new(),
                false,
            ),
            MidiEventPropertiesTarget::Notes(note_ids) => {
                if note_ids.is_empty() {
                    return None;
                }
                let notes = note_ids
                    .iter()
                    .map(|note_id| item.notes().iter().find(|note| note.id() == *note_id))
                    .collect::<Option<Vec<_>>>()?;
                let (position, position_mixed) = common_midi_event_property(
                    notes
                        .iter()
                        .map(|note| view::midi_event_list_position(note.tick(), ppq, meter)),
                );
                let (length, length_mixed) = common_midi_event_property(
                    notes
                        .iter()
                        .map(|note| view::midi_event_list_length(note.duration(), ppq, meter)),
                );
                let (channel, channel_mixed) = common_midi_event_property(
                    notes.iter().map(|note| (note.channel() + 1).to_string()),
                );
                let (parameter, parameter_mixed) = common_midi_event_property(
                    notes
                        .iter()
                        .map(|note| midi_event_pitch_label(note.pitch())),
                );
                let (value, value_mixed) = common_midi_event_property(
                    notes.iter().map(|note| note.velocity().to_string()),
                );
                (
                    position,
                    position_mixed,
                    length,
                    length_mixed,
                    channel,
                    channel_mixed,
                    parameter,
                    parameter_mixed,
                    value,
                    value_mixed,
                )
            }
            MidiEventPropertiesTarget::Controllers(events) => {
                if events.is_empty() {
                    return None;
                }
                let events = events
                    .iter()
                    .map(|target| item.controllers().iter().find(|event| *event == target))
                    .collect::<Option<Vec<_>>>()?;
                let (position, position_mixed) = common_midi_event_property(
                    events
                        .iter()
                        .map(|event| view::midi_event_list_position(event.tick, ppq, meter)),
                );
                let (channel, channel_mixed) = common_midi_event_property(
                    events.iter().map(|event| (event.channel + 1).to_string()),
                );
                let (parameter, parameter_mixed) = common_midi_event_property(
                    events
                        .iter()
                        .map(|event| view::midi_event_controller_property_label(event.controller)),
                );
                let (value, value_mixed) =
                    common_midi_event_property(events.iter().map(|event| event.value.to_string()));
                (
                    position,
                    position_mixed,
                    String::new(),
                    false,
                    channel,
                    channel_mixed,
                    parameter,
                    parameter_mixed,
                    value,
                    value_mixed,
                )
            }
            MidiEventPropertiesTarget::PitchBends(events) => {
                if events.is_empty() {
                    return None;
                }
                let events = events
                    .iter()
                    .map(|target| item.pitch_bends().iter().find(|event| *event == target))
                    .collect::<Option<Vec<_>>>()?;
                let (position, position_mixed) = common_midi_event_property(
                    events
                        .iter()
                        .map(|event| view::midi_event_list_position(event.tick, ppq, meter)),
                );
                let (channel, channel_mixed) = common_midi_event_property(
                    events.iter().map(|event| (event.channel + 1).to_string()),
                );
                let (value, value_mixed) =
                    common_midi_event_property(events.iter().map(|event| event.value.to_string()));
                (
                    position,
                    position_mixed,
                    String::new(),
                    false,
                    channel,
                    channel_mixed,
                    String::new(),
                    false,
                    value,
                    value_mixed,
                )
            }
            MidiEventPropertiesTarget::ProgramChanges(events) => {
                if events.is_empty() {
                    return None;
                }
                let events = events
                    .iter()
                    .map(|target| item.program_changes().iter().find(|event| *event == target))
                    .collect::<Option<Vec<_>>>()?;
                let (position, position_mixed) = common_midi_event_property(
                    events
                        .iter()
                        .map(|event| view::midi_event_list_position(event.tick, ppq, meter)),
                );
                let (channel, channel_mixed) = common_midi_event_property(
                    events.iter().map(|event| (event.channel + 1).to_string()),
                );
                let (value, value_mixed) = common_midi_event_property(
                    events.iter().map(|event| event.program.to_string()),
                );
                (
                    position,
                    position_mixed,
                    String::new(),
                    false,
                    channel,
                    channel_mixed,
                    String::new(),
                    false,
                    value,
                    value_mixed,
                )
            }
            MidiEventPropertiesTarget::PolyAftertouches(events) => {
                if events.is_empty() {
                    return None;
                }
                let events = events
                    .iter()
                    .map(|target| {
                        item.poly_aftertouches()
                            .iter()
                            .find(|event| *event == target)
                    })
                    .collect::<Option<Vec<_>>>()?;
                let (position, position_mixed) = common_midi_event_property(
                    events
                        .iter()
                        .map(|event| view::midi_event_list_position(event.tick, ppq, meter)),
                );
                let (channel, channel_mixed) = common_midi_event_property(
                    events.iter().map(|event| (event.channel + 1).to_string()),
                );
                let (parameter, parameter_mixed) = common_midi_event_property(
                    events
                        .iter()
                        .map(|event| midi_event_pitch_label(event.note)),
                );
                let (value, value_mixed) =
                    common_midi_event_property(events.iter().map(|event| event.value.to_string()));
                (
                    position,
                    position_mixed,
                    String::new(),
                    false,
                    channel,
                    channel_mixed,
                    parameter,
                    parameter_mixed,
                    value,
                    value_mixed,
                )
            }
            MidiEventPropertiesTarget::ChannelPressures(events) => {
                if events.is_empty() {
                    return None;
                }
                let events = events
                    .iter()
                    .map(|target| {
                        item.channel_pressures()
                            .iter()
                            .find(|event| *event == target)
                    })
                    .collect::<Option<Vec<_>>>()?;
                let (position, position_mixed) = common_midi_event_property(
                    events
                        .iter()
                        .map(|event| view::midi_event_list_position(event.tick, ppq, meter)),
                );
                let (channel, channel_mixed) = common_midi_event_property(
                    events.iter().map(|event| (event.channel + 1).to_string()),
                );
                let (value, value_mixed) =
                    common_midi_event_property(events.iter().map(|event| event.value.to_string()));
                (
                    position,
                    position_mixed,
                    String::new(),
                    false,
                    channel,
                    channel_mixed,
                    String::new(),
                    false,
                    value,
                    value_mixed,
                )
            }
            MidiEventPropertiesTarget::Opaque(index, target_event) => {
                let event = item.opaque_events().get(*index as usize)?;
                if event != target_event {
                    return None;
                }
                let (position, position_mixed) =
                    common_midi_event_property([view::midi_event_list_position(
                        event.tick, ppq, meter,
                    )]);
                (
                    position,
                    position_mixed,
                    String::new(),
                    false,
                    String::new(),
                    false,
                    String::new(),
                    false,
                    String::new(),
                    false,
                )
            }
        };
        let (off_value, off_value_mixed) = match &target {
            MidiEventPropertiesTarget::Notes(note_ids) => {
                let notes = note_ids
                    .iter()
                    .map(|note_id| item.notes().iter().find(|note| note.id() == *note_id))
                    .collect::<Option<Vec<_>>>()?;
                common_midi_event_property(notes.iter().map(|note| note.off_velocity().to_string()))
            }
            _ => (String::new(), false),
        };
        let (bank_msb, bank_msb_mixed, bank_lsb, bank_lsb_mixed) = match &target {
            MidiEventPropertiesTarget::ProgramChanges(events) => {
                let (msb, msb_mixed) = common_midi_event_property(events.iter().map(|event| {
                    effective_midi_bank_value(item, event.channel, event.tick, 0).to_string()
                }));
                let (lsb, lsb_mixed) = common_midi_event_property(events.iter().map(|event| {
                    effective_midi_bank_value(item, event.channel, event.tick, 32).to_string()
                }));
                (msb, msb_mixed, lsb, lsb_mixed)
            }
            _ => (String::new(), false, String::new(), false),
        };
        let (opaque_type, opaque_payload) = match &target {
            MidiEventPropertiesTarget::Opaque(_, event) => {
                let (event_type, payload) =
                    midi_opaque_event_properties_type_and_payload(&event.kind)?;
                (Some(event_type), payload)
            }
            _ => (None, String::new()),
        };
        Some(MidiEventPropertiesDialogState {
            item_id,
            target,
            position,
            position_mixed,
            length,
            length_mixed,
            channel,
            channel_mixed,
            parameter,
            parameter_mixed,
            value,
            value_mixed,
            off_value,
            off_value_mixed,
            bank_msb,
            bank_msb_mixed,
            bank_msb_dirty: false,
            bank_lsb,
            bank_lsb_mixed,
            bank_lsb_dirty: false,
            opaque_type,
            opaque_type_dirty: false,
            opaque_editor: iced::widget::text_editor::Content::with_text(&opaque_payload),
            opaque_payload,
            opaque_payload_dirty: false,
            opaque_multiline: false,
        })
    }

    fn reset_midi_event_properties_controller_options(&mut self) {
        let selection = self
            .midi_event_properties_dialog
            .as_ref()
            .and_then(|dialog| {
                matches!(&dialog.target, MidiEventPropertiesTarget::Controllers(_))
                    .then(|| dialog.parameter.clone())
            });
        self.midi_event_properties_controller_options
            .reset(selection);
    }

    fn commit_midi_event_properties(&mut self, close_dialog: bool) -> Task<Message> {
        let Some(dialog) = self.midi_event_properties_dialog.clone() else {
            return Task::none();
        };
        if matches!(&dialog.target, MidiEventPropertiesTarget::Terminal) {
            self.midi_editor_feedback = None;
            if close_dialog {
                self.midi_event_properties_dialog = None;
            }
            return Task::none();
        }
        if matches!(&dialog.target, MidiEventPropertiesTarget::Opaque(..)) {
            return self.commit_midi_opaque_event_properties(dialog, close_dialog);
        }
        let item_id = dialog.item_id;
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            self.midi_event_properties_dialog = None;
            return Task::none();
        };
        let ppq = u64::from(self.project.settings().ppq());
        let meter = self.project.time_signature_at_tick(0);
        let Some(position_edit) =
            parse_midi_event_time_edit(&dialog.position, dialog.position_mixed, ppq, meter, false)
        else {
            self.midi_editor_feedback = Some("Position must use bar.beat.hundredths format".into());
            return Task::none();
        };
        let Some(channel_edit) = parse_midi_event_property_edit(
            &dialog.channel,
            dialog.channel_mixed,
            |value| value.parse::<i128>().ok(),
            parse_midi_event_relative_numeric,
        ) else {
            self.midi_editor_feedback = Some("Channel must be from 1 to 16".into());
            return Task::none();
        };
        match dialog.target {
            MidiEventPropertiesTarget::Notes(note_ids) => {
                let Some(duration_edit) = parse_midi_event_time_edit(
                    &dialog.length,
                    dialog.length_mixed,
                    ppq,
                    meter,
                    true,
                ) else {
                    self.midi_editor_feedback =
                        Some("Length must use bar.beat.hundredths format".into());
                    return Task::none();
                };
                let Some(pitch_edit) =
                    parse_midi_event_pitch_edit(&dialog.parameter, dialog.parameter_mixed)
                else {
                    self.midi_editor_feedback = Some("Pitch must be from 0 to 127".into());
                    return Task::none();
                };
                let Some(velocity_edit) = parse_midi_event_property_edit(
                    &dialog.value,
                    dialog.value_mixed,
                    |value| value.parse::<i128>().ok(),
                    parse_midi_event_relative_numeric,
                ) else {
                    self.midi_editor_feedback = Some("Velocity must be from 0 to 127".into());
                    return Task::none();
                };
                let Some(off_velocity_edit) = parse_midi_event_property_edit(
                    &dialog.off_value,
                    dialog.off_value_mixed,
                    |value| value.parse::<i128>().ok(),
                    parse_midi_event_relative_numeric,
                ) else {
                    self.midi_editor_feedback = Some("Off value must be from 0 to 127".into());
                    return Task::none();
                };
                let edits = note_ids
                    .iter()
                    .map(|note_id| {
                        let note = item.notes().iter().find(|note| note.id() == *note_id)?;
                        let channel = apply_midi_event_properties_edit(
                            i128::from(note.channel()) + 1,
                            channel_edit,
                            1,
                            16,
                        )? as u8
                            - 1;
                        let pitch = apply_midi_event_properties_edit(
                            i128::from(note.pitch()),
                            pitch_edit,
                            0,
                            127,
                        )? as u8;
                        let tick = apply_midi_event_properties_edit(
                            i128::from(note.tick()),
                            position_edit,
                            0,
                            i128::from(u64::MAX),
                        )? as u64;
                        let duration = apply_midi_event_properties_edit(
                            i128::from(note.duration()),
                            duration_edit,
                            1,
                            i128::from(u64::MAX),
                        )? as u64;
                        let velocity = apply_midi_event_properties_edit(
                            i128::from(note.velocity()),
                            velocity_edit,
                            0,
                            127,
                        )? as u8;
                        let off_velocity = apply_midi_event_properties_edit(
                            i128::from(note.off_velocity()),
                            off_velocity_edit,
                            0,
                            127,
                        )? as u8;
                        Some((
                            note.id(),
                            MidiNoteData {
                                channel,
                                pitch,
                                tick,
                                duration,
                                velocity,
                                off_velocity,
                            },
                        ))
                    })
                    .collect::<Option<Vec<_>>>();
                let Some(edits) = edits else {
                    self.midi_editor_feedback = Some(
                        "Position, length, channel, pitch, velocity, or off value is outside its valid range"
                            .into(),
                    );
                    return Task::none();
                };
                let unchanged = edits.iter().all(|(note_id, data)| {
                    item.notes()
                        .iter()
                        .find(|note| note.id() == *note_id)
                        .is_some_and(|note| {
                            note.channel() == data.channel
                                && note.pitch() == data.pitch
                                && note.tick() == data.tick
                                && note.duration() == data.duration
                                && note.velocity() == data.velocity
                                && note.off_velocity() == data.off_velocity
                        })
                });
                if unchanged {
                    self.midi_editor_feedback = None;
                    if close_dialog {
                        self.midi_event_properties_dialog = None;
                    }
                    return Task::none();
                }
                self.midi_editor_feedback = None;
                let task = self.update(Message::EditMidiNotes(item_id, edits));
                if self.status.starts_with("Action failed:") {
                    self.midi_editor_feedback = Some(self.status.clone());
                    return task;
                }
                if close_dialog {
                    self.midi_event_properties_dialog = None;
                } else {
                    self.midi_event_properties_dialog = self.midi_event_properties_state(
                        item_id,
                        MidiEventPropertiesTarget::Notes(note_ids),
                    );
                }
                task
            }
            MidiEventPropertiesTarget::Controllers(events) => {
                let Some(controller_edit) = parse_midi_event_property_edit(
                    &dialog.parameter,
                    dialog.parameter_mixed,
                    |value| parse_midi_event_controller(value).map(i128::from),
                    parse_midi_event_relative_numeric,
                ) else {
                    self.midi_editor_feedback = Some("Controller must be from 0 to 127".into());
                    return Task::none();
                };
                let Some(value_edit) = parse_midi_event_property_edit(
                    &dialog.value,
                    dialog.value_mixed,
                    |value| value.parse::<i128>().ok(),
                    parse_midi_event_relative_numeric,
                ) else {
                    self.midi_editor_feedback = Some("Value must be from 0 to 127".into());
                    return Task::none();
                };
                let mut controllers = item.controllers().to_vec();
                let mut updated_events = Vec::with_capacity(events.len());
                for event in &events {
                    let Some(index) = controllers.iter().position(|candidate| candidate == event)
                    else {
                        self.midi_event_properties_dialog = None;
                        return Task::none();
                    };
                    let Some(channel) = apply_midi_event_properties_edit(
                        i128::from(event.channel) + 1,
                        channel_edit,
                        1,
                        16,
                    ) else {
                        self.midi_editor_feedback = Some("Channel must be from 1 to 16".into());
                        return Task::none();
                    };
                    let Some(tick) = apply_midi_event_properties_edit(
                        i128::from(event.tick),
                        position_edit,
                        0,
                        i128::from(u64::MAX),
                    ) else {
                        self.midi_editor_feedback =
                            Some("Position is outside its valid range".into());
                        return Task::none();
                    };
                    let Some(controller) = apply_midi_event_properties_edit(
                        i128::from(event.controller),
                        controller_edit,
                        0,
                        127,
                    ) else {
                        self.midi_editor_feedback = Some("Controller must be from 0 to 127".into());
                        return Task::none();
                    };
                    let Some(value) = apply_midi_event_properties_edit(
                        i128::from(event.value),
                        value_edit,
                        0,
                        127,
                    ) else {
                        self.midi_editor_feedback = Some("Value must be from 0 to 127".into());
                        return Task::none();
                    };
                    let candidate = MidiControllerData {
                        controller: controller as u8,
                        channel: channel as u8 - 1,
                        tick: tick as u64,
                        value: value as u8,
                    };
                    controllers[index] = candidate;
                    updated_events.push(candidate);
                }
                if controllers == item.controllers() {
                    self.midi_editor_feedback = None;
                    if close_dialog {
                        self.midi_event_properties_dialog = None;
                    }
                    return Task::none();
                }
                self.midi_editor_feedback = None;
                let task = self.update(Message::SetMidiControllers(item_id, controllers));
                if self.status.starts_with("Action failed:") {
                    self.midi_editor_feedback = Some(self.status.clone());
                    return task;
                }
                self.midi_editor_selected_controllers = updated_events.iter().copied().collect();
                if close_dialog {
                    self.midi_event_properties_dialog = None;
                } else {
                    self.midi_event_properties_dialog = self.midi_event_properties_state(
                        item_id,
                        MidiEventPropertiesTarget::Controllers(updated_events),
                    );
                }
                self.reset_midi_event_properties_controller_options();
                task
            }
            MidiEventPropertiesTarget::PitchBends(events) => {
                let Some(value_edit) = parse_midi_event_property_edit(
                    &dialog.value,
                    dialog.value_mixed,
                    |value| value.parse::<i128>().ok(),
                    parse_midi_event_relative_numeric,
                ) else {
                    self.midi_editor_feedback = Some("Value must be from 0 to 16383".into());
                    return Task::none();
                };
                let mut pitch_bends = item.pitch_bends().to_vec();
                let mut updated_events = Vec::with_capacity(events.len());
                for event in &events {
                    let Some(index) = pitch_bends.iter().position(|candidate| candidate == event)
                    else {
                        self.midi_event_properties_dialog = None;
                        return Task::none();
                    };
                    let Some(channel) = apply_midi_event_properties_edit(
                        i128::from(event.channel) + 1,
                        channel_edit,
                        1,
                        16,
                    ) else {
                        self.midi_editor_feedback = Some("Channel must be from 1 to 16".into());
                        return Task::none();
                    };
                    let Some(tick) = apply_midi_event_properties_edit(
                        i128::from(event.tick),
                        position_edit,
                        0,
                        i128::from(u64::MAX),
                    ) else {
                        self.midi_editor_feedback =
                            Some("Position is outside its valid range".into());
                        return Task::none();
                    };
                    let Some(value) = apply_midi_event_properties_edit(
                        i128::from(event.value),
                        value_edit,
                        0,
                        16_383,
                    ) else {
                        self.midi_editor_feedback = Some("Value must be from 0 to 16383".into());
                        return Task::none();
                    };
                    let candidate = MidiPitchBendData {
                        channel: channel as u8 - 1,
                        tick: tick as u64,
                        value: value as u16,
                    };
                    pitch_bends[index] = candidate;
                    updated_events.push(candidate);
                }
                if pitch_bends == item.pitch_bends() {
                    self.midi_editor_feedback = None;
                    if close_dialog {
                        self.midi_event_properties_dialog = None;
                    }
                    return Task::none();
                }
                self.midi_editor_feedback = None;
                let task = self.update(Message::SetMidiPitchBends(item_id, pitch_bends));
                if self.status.starts_with("Action failed:") {
                    self.midi_editor_feedback = Some(self.status.clone());
                    return task;
                }
                self.midi_editor_selected_pitch_bends = updated_events.iter().copied().collect();
                if close_dialog {
                    self.midi_event_properties_dialog = None;
                } else {
                    self.midi_event_properties_dialog = self.midi_event_properties_state(
                        item_id,
                        MidiEventPropertiesTarget::PitchBends(updated_events),
                    );
                }
                task
            }
            MidiEventPropertiesTarget::ProgramChanges(events) => {
                let Some(value_edit) = parse_midi_event_property_edit(
                    &dialog.value,
                    dialog.value_mixed,
                    |value| value.parse::<i128>().ok(),
                    parse_midi_event_relative_numeric,
                ) else {
                    self.midi_editor_feedback = Some("Program number must be from 0 to 127".into());
                    return Task::none();
                };
                let bank_msb_edit = if dialog.bank_msb_dirty {
                    let Some(edit) = parse_midi_event_property_edit(
                        &dialog.bank_msb,
                        dialog.bank_msb_mixed,
                        |value| value.parse::<i128>().ok(),
                        parse_midi_event_relative_numeric,
                    ) else {
                        self.midi_editor_feedback = Some("Bank MSB must be from 0 to 127".into());
                        return Task::none();
                    };
                    edit
                } else {
                    MidiEventPropertiesEdit::Preserve
                };
                let bank_lsb_edit = if dialog.bank_lsb_dirty {
                    let Some(edit) = parse_midi_event_property_edit(
                        &dialog.bank_lsb,
                        dialog.bank_lsb_mixed,
                        |value| value.parse::<i128>().ok(),
                        parse_midi_event_relative_numeric,
                    ) else {
                        self.midi_editor_feedback = Some("Bank LSB must be from 0 to 127".into());
                        return Task::none();
                    };
                    edit
                } else {
                    MidiEventPropertiesEdit::Preserve
                };
                let mut program_changes = item.program_changes().to_vec();
                let mut controllers = item.controllers().to_vec();
                let mut updated_events = Vec::with_capacity(events.len());
                for event in &events {
                    let Some(index) = program_changes
                        .iter()
                        .position(|candidate| candidate == event)
                    else {
                        self.midi_event_properties_dialog = None;
                        return Task::none();
                    };
                    let Some(channel) = apply_midi_event_properties_edit(
                        i128::from(event.channel) + 1,
                        channel_edit,
                        1,
                        16,
                    ) else {
                        self.midi_editor_feedback = Some("Channel must be from 1 to 16".into());
                        return Task::none();
                    };
                    let Some(tick) = apply_midi_event_properties_edit(
                        i128::from(event.tick),
                        position_edit,
                        0,
                        i128::from(u64::MAX),
                    ) else {
                        self.midi_editor_feedback =
                            Some("Position is outside its valid range".into());
                        return Task::none();
                    };
                    let Some(program) = apply_midi_event_properties_edit(
                        i128::from(event.program),
                        value_edit,
                        0,
                        127,
                    ) else {
                        self.midi_editor_feedback =
                            Some("Program number must be from 0 to 127".into());
                        return Task::none();
                    };
                    let candidate = MidiProgramChangeData {
                        channel: channel as u8 - 1,
                        tick: tick as u64,
                        program: program as u8,
                    };
                    program_changes[index] = candidate;
                    updated_events.push(candidate);

                    if bank_msb_edit != MidiEventPropertiesEdit::Preserve {
                        let Some(value) = apply_midi_event_properties_edit(
                            i128::from(effective_midi_bank_value(
                                item,
                                event.channel,
                                event.tick,
                                0,
                            )),
                            bank_msb_edit,
                            0,
                            127,
                        ) else {
                            self.midi_editor_feedback =
                                Some("Bank MSB must be from 0 to 127".into());
                            return Task::none();
                        };
                        set_midi_bank_value(
                            &mut controllers,
                            candidate.channel,
                            candidate.tick,
                            0,
                            value as u8,
                        );
                    }
                    if bank_lsb_edit != MidiEventPropertiesEdit::Preserve {
                        let Some(value) = apply_midi_event_properties_edit(
                            i128::from(effective_midi_bank_value(
                                item,
                                event.channel,
                                event.tick,
                                32,
                            )),
                            bank_lsb_edit,
                            0,
                            127,
                        ) else {
                            self.midi_editor_feedback =
                                Some("Bank LSB must be from 0 to 127".into());
                            return Task::none();
                        };
                        set_midi_bank_value(
                            &mut controllers,
                            candidate.channel,
                            candidate.tick,
                            32,
                            value as u8,
                        );
                    }
                }
                if program_changes == item.program_changes() && controllers == item.controllers() {
                    self.midi_editor_feedback = None;
                    if close_dialog {
                        self.midi_event_properties_dialog = None;
                    }
                    return Task::none();
                }
                self.midi_editor_feedback = None;
                let previous_revision = self.revision;
                let task = if controllers == item.controllers() {
                    self.update(Message::SetMidiProgramChanges(
                        item_id,
                        program_changes.clone(),
                    ))
                } else {
                    let mut actions = vec![DawAction::SetMidiControllers {
                        item_id,
                        controllers,
                    }];
                    if program_changes != item.program_changes() {
                        actions.push(DawAction::SetMidiProgramChanges {
                            item_id,
                            program_changes,
                        });
                    }
                    self.apply_action(
                        DawAction::BatchTransaction {
                            tx_id: item_id.value(),
                            actions,
                        },
                        "MIDI bank/program select edited",
                    );
                    Task::none()
                };
                if self.status.starts_with("Action failed:") {
                    self.midi_editor_feedback = Some(self.status.clone());
                    return task;
                }
                if self.revision == previous_revision {
                    return task;
                }
                self.midi_editor_selected_program_changes =
                    updated_events.iter().copied().collect();
                if close_dialog {
                    self.midi_event_properties_dialog = None;
                } else {
                    self.midi_event_properties_dialog = self.midi_event_properties_state(
                        item_id,
                        MidiEventPropertiesTarget::ProgramChanges(updated_events),
                    );
                }
                task
            }
            MidiEventPropertiesTarget::PolyAftertouches(events) => {
                let Some(note_edit) =
                    parse_midi_event_pitch_edit(&dialog.parameter, dialog.parameter_mixed)
                else {
                    self.midi_editor_feedback = Some("Note must be from 0 to 127".into());
                    return Task::none();
                };
                let Some(value_edit) = parse_midi_event_property_edit(
                    &dialog.value,
                    dialog.value_mixed,
                    |value| value.parse::<i128>().ok(),
                    parse_midi_event_relative_numeric,
                ) else {
                    self.midi_editor_feedback = Some("Value must be from 0 to 127".into());
                    return Task::none();
                };
                let mut poly_aftertouches = item.poly_aftertouches().to_vec();
                let mut updated_events = Vec::with_capacity(events.len());
                for event in &events {
                    let Some(index) = poly_aftertouches
                        .iter()
                        .position(|candidate| candidate == event)
                    else {
                        self.midi_event_properties_dialog = None;
                        return Task::none();
                    };
                    let Some(channel) = apply_midi_event_properties_edit(
                        i128::from(event.channel) + 1,
                        channel_edit,
                        1,
                        16,
                    ) else {
                        self.midi_editor_feedback = Some("Channel must be from 1 to 16".into());
                        return Task::none();
                    };
                    let Some(tick) = apply_midi_event_properties_edit(
                        i128::from(event.tick),
                        position_edit,
                        0,
                        i128::from(u64::MAX),
                    ) else {
                        self.midi_editor_feedback =
                            Some("Position is outside its valid range".into());
                        return Task::none();
                    };
                    let Some(note) =
                        apply_midi_event_properties_edit(i128::from(event.note), note_edit, 0, 127)
                    else {
                        self.midi_editor_feedback = Some("Note must be from 0 to 127".into());
                        return Task::none();
                    };
                    let Some(value) = apply_midi_event_properties_edit(
                        i128::from(event.value),
                        value_edit,
                        0,
                        127,
                    ) else {
                        self.midi_editor_feedback = Some("Value must be from 0 to 127".into());
                        return Task::none();
                    };
                    let candidate = MidiPolyAftertouchData {
                        channel: channel as u8 - 1,
                        tick: tick as u64,
                        note: note as u8,
                        value: value as u8,
                    };
                    poly_aftertouches[index] = candidate;
                    updated_events.push(candidate);
                }
                if poly_aftertouches == item.poly_aftertouches() {
                    self.midi_editor_feedback = None;
                    if close_dialog {
                        self.midi_event_properties_dialog = None;
                    }
                    return Task::none();
                }
                self.midi_editor_feedback = None;
                let task =
                    self.update(Message::SetMidiPolyAftertouches(item_id, poly_aftertouches));
                if self.status.starts_with("Action failed:") {
                    self.midi_editor_feedback = Some(self.status.clone());
                    return task;
                }
                self.midi_editor_selected_poly_aftertouches =
                    updated_events.iter().copied().collect();
                if close_dialog {
                    self.midi_event_properties_dialog = None;
                } else {
                    self.midi_event_properties_dialog = self.midi_event_properties_state(
                        item_id,
                        MidiEventPropertiesTarget::PolyAftertouches(updated_events),
                    );
                }
                task
            }
            MidiEventPropertiesTarget::ChannelPressures(events) => {
                let Some(value_edit) = parse_midi_event_property_edit(
                    &dialog.value,
                    dialog.value_mixed,
                    |value| value.parse::<i128>().ok(),
                    parse_midi_event_relative_numeric,
                ) else {
                    self.midi_editor_feedback = Some("Value must be from 0 to 127".into());
                    return Task::none();
                };
                let mut channel_pressures = item.channel_pressures().to_vec();
                let mut updated_events = Vec::with_capacity(events.len());
                for event in &events {
                    let Some(index) = channel_pressures
                        .iter()
                        .position(|candidate| candidate == event)
                    else {
                        self.midi_event_properties_dialog = None;
                        return Task::none();
                    };
                    let Some(channel) = apply_midi_event_properties_edit(
                        i128::from(event.channel) + 1,
                        channel_edit,
                        1,
                        16,
                    ) else {
                        self.midi_editor_feedback = Some("Channel must be from 1 to 16".into());
                        return Task::none();
                    };
                    let Some(tick) = apply_midi_event_properties_edit(
                        i128::from(event.tick),
                        position_edit,
                        0,
                        i128::from(u64::MAX),
                    ) else {
                        self.midi_editor_feedback =
                            Some("Position is outside its valid range".into());
                        return Task::none();
                    };
                    let Some(value) = apply_midi_event_properties_edit(
                        i128::from(event.value),
                        value_edit,
                        0,
                        127,
                    ) else {
                        self.midi_editor_feedback = Some("Value must be from 0 to 127".into());
                        return Task::none();
                    };
                    let candidate = MidiChannelPressureData {
                        channel: channel as u8 - 1,
                        tick: tick as u64,
                        value: value as u8,
                    };
                    channel_pressures[index] = candidate;
                    updated_events.push(candidate);
                }
                if channel_pressures == item.channel_pressures() {
                    self.midi_editor_feedback = None;
                    if close_dialog {
                        self.midi_event_properties_dialog = None;
                    }
                    return Task::none();
                }
                self.midi_editor_feedback = None;
                let task =
                    self.update(Message::SetMidiChannelPressures(item_id, channel_pressures));
                if self.status.starts_with("Action failed:") {
                    self.midi_editor_feedback = Some(self.status.clone());
                    return task;
                }
                self.midi_editor_selected_channel_pressures =
                    updated_events.iter().copied().collect();
                if close_dialog {
                    self.midi_event_properties_dialog = None;
                } else {
                    self.midi_event_properties_dialog = self.midi_event_properties_state(
                        item_id,
                        MidiEventPropertiesTarget::ChannelPressures(updated_events),
                    );
                }
                task
            }
            MidiEventPropertiesTarget::Opaque(..) | MidiEventPropertiesTarget::Terminal => {
                unreachable!("handled above")
            }
        }
    }

    fn commit_midi_opaque_event_properties(
        &mut self,
        dialog: MidiEventPropertiesDialogState,
        close_dialog: bool,
    ) -> Task<Message> {
        let item_id = dialog.item_id;
        let MidiEventPropertiesTarget::Opaque(index, original) = dialog.target else {
            return Task::none();
        };
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            self.midi_event_properties_dialog = None;
            return Task::none();
        };
        if item.opaque_events().get(index as usize) != Some(&original) {
            self.midi_editor_feedback =
                Some("The MIDI event changed while its properties were open".into());
            return Task::none();
        }
        let ppq = u64::from(self.project.settings().ppq());
        let meter = self.project.time_signature_at_tick(0);
        let Some(position_edit) =
            parse_midi_event_time_edit(&dialog.position, dialog.position_mixed, ppq, meter, false)
        else {
            self.midi_editor_feedback = Some("Position must use bar.beat.hundredths format".into());
            return Task::none();
        };
        let Some(tick) = apply_midi_event_properties_edit(
            i128::from(original.tick),
            position_edit,
            0,
            i128::from(u64::MAX),
        )
        .map(|tick| tick as u64) else {
            self.midi_editor_feedback = Some("Position is outside its valid range".into());
            return Task::none();
        };
        let kind = match &original.kind {
            MidiOpaqueEventKind::Meta { kind, data } => {
                let event_type = if dialog.opaque_type_dirty {
                    dialog
                        .opaque_type
                        .and_then(MidiOpaqueEventPropertiesType::meta_kind)
                } else {
                    Some(*kind)
                };
                let Some(kind) = event_type else {
                    self.midi_editor_feedback = Some("Choose a valid Meta event type".into());
                    return Task::none();
                };
                let data = if dialog.opaque_payload_dirty {
                    if midi_opaque_event_properties_payload_is_hex(kind, data) {
                        let Some(data) = parse_midi_event_hex_bytes(&dialog.opaque_payload) else {
                            self.midi_editor_feedback = Some(
                                "Meta data must be hexadecimal bytes separated by spaces".into(),
                            );
                            return Task::none();
                        };
                        data
                    } else {
                        dialog.opaque_payload.into_bytes()
                    }
                } else {
                    data.clone()
                };
                MidiOpaqueEventKind::Meta { kind, data }
            }
            MidiOpaqueEventKind::SystemExclusive { status, data } => {
                let mut data = if dialog.opaque_payload_dirty {
                    let Some(data) = parse_midi_event_hex_bytes(&dialog.opaque_payload) else {
                        self.midi_editor_feedback =
                            Some("SysEx data must be hexadecimal bytes separated by spaces".into());
                        return Task::none();
                    };
                    data
                } else {
                    data.clone()
                };
                if *status == 0xF0 && data.last() != Some(&0xF7) {
                    data.push(0xF7);
                }
                MidiOpaqueEventKind::SystemExclusive {
                    status: *status,
                    data,
                }
            }
        };
        let updated = MidiOpaqueEventData {
            tick,
            order: original.order,
            kind,
        };
        if updated == original {
            self.midi_editor_feedback = None;
            if close_dialog {
                self.midi_event_properties_dialog = None;
            }
            return Task::none();
        }
        let mut opaque_events = item.opaque_events().to_vec();
        opaque_events[index as usize] = updated.clone();
        let revision_before = self.revision;
        self.midi_editor_feedback = None;
        self.apply_action(
            DawAction::SetMidiOpaqueEvents {
                item_id,
                events: opaque_events,
            },
            "MIDI opaque event edited",
        );
        if self.revision == revision_before {
            if self.status.starts_with("Action failed:") {
                self.midi_editor_feedback = Some(self.status.clone());
            }
            return Task::none();
        }
        if close_dialog {
            self.midi_event_properties_dialog = None;
        } else {
            self.midi_event_properties_dialog = self.midi_event_properties_state(
                item_id,
                MidiEventPropertiesTarget::Opaque(index, updated),
            );
        }
        Task::none()
    }

    fn pick_midi_opaque_sysex_file(&mut self, save: bool) -> Task<Message> {
        let Some(dialog) = self.midi_event_properties_dialog.as_ref() else {
            return Task::none();
        };
        let MidiEventPropertiesTarget::Opaque(_, event) = &dialog.target else {
            return Task::none();
        };
        let MidiOpaqueEventKind::SystemExclusive { status, .. } = &event.kind else {
            return Task::none();
        };
        let status = *status;
        if save {
            let Some(_payload) = parse_midi_event_hex_bytes(&dialog.opaque_payload) else {
                self.midi_editor_feedback =
                    Some("SysEx data must be hexadecimal bytes separated by spaces".into());
                return Task::none();
            };
        }
        self.midi_editor_feedback = None;
        self.pick_path(if save {
            PathPickerTarget::ExportMidiOpaqueSysEx { status }
        } else {
            PathPickerTarget::ImportMidiOpaqueSysEx { status }
        })
    }

    fn pick_midi_opaque_meta_text_file(&mut self) -> Task<Message> {
        if !self
            .midi_event_properties_dialog
            .as_ref()
            .is_some_and(|dialog| {
                matches!(
                    &dialog.target,
                    MidiEventPropertiesTarget::Opaque(_, event)
                        if matches!(&event.kind, MidiOpaqueEventKind::Meta { kind, .. }
                            if MidiOpaqueEventPropertiesType::from_meta_kind(*kind)
                                != Some(MidiOpaqueEventPropertiesType::RawMeta))
                )
            })
        {
            return Task::none();
        }
        self.midi_editor_feedback = None;
        self.pick_path(PathPickerTarget::ImportMidiOpaqueMetaText)
    }

    fn commit_midi_quantize_dialog(&mut self, close_dialog: bool) {
        let Some(dialog) = self.midi_quantize_dialog else {
            return;
        };
        if close_dialog {
            self.midi_quantize_dialog = None;
        }
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == dialog.item_id)
        else {
            self.midi_quantize_dialog = None;
            return;
        };
        if dialog.bypass {
            return;
        }
        let note_ids: Vec<aaadaw_core::NoteId> = match dialog.scope {
            MidiQuantizeScope::SelectedNotes => {
                self.midi_editor_selected_notes.iter().copied().collect()
            }
            MidiQuantizeScope::SelectedEvents => {
                self.midi_editor_selected_notes.iter().copied().collect()
            }
            MidiQuantizeScope::AllNotes => item.notes().iter().map(|note| note.id()).collect(),
            MidiQuantizeScope::AllEvents => Vec::new(),
        };
        if dialog.scope != MidiQuantizeScope::AllEvents
            && note_ids.is_empty()
            && (dialog.scope != MidiQuantizeScope::SelectedEvents
                || (self.midi_editor_selected_controllers.is_empty()
                    && self.midi_editor_selected_pitch_bends.is_empty()
                    && self.midi_editor_selected_program_changes.is_empty()
                    && self.midi_editor_selected_poly_aftertouches.is_empty()
                    && self.midi_editor_selected_channel_pressures.is_empty()
                    && self.midi_editor_selected_opaque_events.is_empty()))
        {
            return;
        }
        let grid = midi_editor_grid_fraction(self.timeline.midi_editor_snap_grid);
        let strength = f32::from(dialog.strength_percent) / 100.0;
        let (operation, label) = match dialog.operation {
            MidiQuantizeOperation::Position => (
                aaadaw_core::MidiNoteQuantizeOperation::Position,
                "MIDI notes quantized to editor grid",
            ),
            MidiQuantizeOperation::PositionAndNoteEnd => (
                aaadaw_core::MidiNoteQuantizeOperation::PositionAndNoteEnd,
                "MIDI note positions and ends quantized to editor grid",
            ),
            MidiQuantizeOperation::PositionAndNoteLength => (
                aaadaw_core::MidiNoteQuantizeOperation::PositionAndNoteLength,
                "MIDI note positions and lengths quantized to editor grid",
            ),
            MidiQuantizeOperation::NoteEndOnly => (
                aaadaw_core::MidiNoteQuantizeOperation::NoteEndOnly,
                "MIDI note ends quantized to editor grid",
            ),
            MidiQuantizeOperation::NoteLengthOnly => (
                aaadaw_core::MidiNoteQuantizeOperation::NoteLengthOnly,
                "MIDI note lengths quantized to editor grid",
            ),
        };
        let action = match dialog.scope {
            MidiQuantizeScope::AllEvents => DawAction::QuantizeMidiEventsWithSelection {
                item_id: dialog.item_id,
                selected_controllers: self
                    .midi_editor_selected_controllers
                    .iter()
                    .copied()
                    .collect(),
                selected_pitch_bends: self
                    .midi_editor_selected_pitch_bends
                    .iter()
                    .copied()
                    .collect(),
                selected_program_changes: self
                    .midi_editor_selected_program_changes
                    .iter()
                    .copied()
                    .collect(),
                selected_channel_pressures: self
                    .midi_editor_selected_channel_pressures
                    .iter()
                    .copied()
                    .collect(),
                grid,
                strength,
                operation,
                swing_amount_percent: self.timeline.active_snap_swing_amount_percent(),
                fix_overlaps: dialog.fix_overlaps,
            },
            MidiQuantizeScope::SelectedEvents => {
                DawAction::QuantizeSelectedMidiEventsWithOperation {
                    item_id: dialog.item_id,
                    note_ids,
                    controllers: self
                        .midi_editor_selected_controllers
                        .iter()
                        .copied()
                        .collect(),
                    pitch_bends: self
                        .midi_editor_selected_pitch_bends
                        .iter()
                        .copied()
                        .collect(),
                    program_changes: self
                        .midi_editor_selected_program_changes
                        .iter()
                        .copied()
                        .collect(),
                    poly_aftertouches: self
                        .midi_editor_selected_poly_aftertouches
                        .iter()
                        .copied()
                        .collect(),
                    channel_pressures: self
                        .midi_editor_selected_channel_pressures
                        .iter()
                        .copied()
                        .collect(),
                    opaque_events: self
                        .midi_editor_selected_opaque_events
                        .iter()
                        .filter_map(|index| item.opaque_events().get(*index as usize).cloned())
                        .collect(),
                    grid,
                    strength,
                    operation,
                    swing_amount_percent: self.timeline.active_snap_swing_amount_percent(),
                    fix_overlaps: dialog.fix_overlaps,
                }
            }
            MidiQuantizeScope::AllNotes | MidiQuantizeScope::SelectedNotes => {
                DawAction::QuantizeMidiNotesWithOperation {
                    item_id: dialog.item_id,
                    note_ids,
                    grid,
                    strength,
                    operation,
                    swing_amount_percent: self.timeline.active_snap_swing_amount_percent(),
                    fix_overlaps: dialog.fix_overlaps,
                }
            }
        };
        let label = if matches!(
            dialog.scope,
            MidiQuantizeScope::AllEvents | MidiQuantizeScope::SelectedEvents
        ) {
            "MIDI notes and events quantized to editor grid"
        } else {
            label
        };
        let selected_controllers = self.midi_editor_selected_controllers.clone();
        let selected_pitch_bends = self.midi_editor_selected_pitch_bends.clone();
        let selected_program_changes = self.midi_editor_selected_program_changes.clone();
        let selected_poly_aftertouches = self.midi_editor_selected_poly_aftertouches.clone();
        let selected_channel_pressures = self.midi_editor_selected_channel_pressures.clone();
        let selected_opaque_events = self.selected_midi_opaque_event_data();
        let previous_revision = self.revision;
        self.apply_action(action, label);
        if self.revision != previous_revision
            && let Some(item) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == dialog.item_id)
        {
            self.midi_editor_selected_controllers =
                Self::remap_selected_controller_events(&selected_controllers, item.controllers());
            self.midi_editor_selected_pitch_bends =
                Self::remap_selected_pitch_bend_events(&selected_pitch_bends, item.pitch_bends());
            self.midi_editor_selected_program_changes = Self::remap_selected_midi_point_events(
                &selected_program_changes,
                item.program_changes(),
                |left, right| left.channel == right.channel && left.program == right.program,
                |left, right| left.channel == right.channel,
                |event| event.tick,
            );
            self.midi_editor_selected_poly_aftertouches = Self::remap_selected_midi_point_events(
                &selected_poly_aftertouches,
                item.poly_aftertouches(),
                |left, right| {
                    left.channel == right.channel
                        && left.note == right.note
                        && left.value == right.value
                },
                |left, right| left.channel == right.channel && left.note == right.note,
                |event| event.tick,
            );
            self.midi_editor_selected_channel_pressures = Self::remap_selected_midi_point_events(
                &selected_channel_pressures,
                item.channel_pressures(),
                |left, right| left.channel == right.channel && left.value == right.value,
                |left, right| left.channel == right.channel,
                |event| event.tick,
            );
            self.midi_editor_selected_opaque_events = Self::remap_selected_midi_opaque_events(
                &selected_opaque_events,
                item.opaque_events(),
            );
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        if self.midi_humanize_dialog.is_some()
            && !matches!(
                &message,
                Message::AcceptMidiHumanize
                    | Message::CancelMidiHumanize
                    | Message::SetMidiHumanizeScope(_)
                    | Message::SetMidiHumanizeTiming(_)
                    | Message::SetMidiHumanizeVelocity(_)
                    | Message::SetMidiHumanizeTimingBias(_)
                    | Message::NewMidiHumanizeSeed
                    | Message::Escape
                    | Message::RuntimeKeyboardEvent(..)
                    | Message::BackgroundTick
                    | Message::MeterTick
                    | Message::WindowResized(..)
                    | Message::WindowClosed(..)
                    | Message::WindowCloseRequested(..)
                    | Message::CloseMidiEditor
            )
        {
            return Task::none();
        }
        if self.midi_quantize_dialog.is_some()
            && !matches!(
                &message,
                Message::CommitMidiQuantize
                    | Message::AcceptMidiQuantize
                    | Message::CancelMidiQuantize
                    | Message::SetMidiQuantizeScope(_)
                    | Message::SetMidiQuantizeOperation(_)
                    | Message::SetMidiQuantizeStrength(_)
                    | Message::SetMidiQuantizeBypass(_)
                    | Message::SetMidiQuantizeFixOverlaps(_)
                    | Message::Escape
                    | Message::RuntimeKeyboardEvent(..)
                    | Message::BackgroundTick
                    | Message::MeterTick
                    | Message::WindowResized(..)
                    | Message::WindowClosed(..)
                    | Message::WindowCloseRequested(..)
                    | Message::CloseMidiEditor
            )
        {
            return Task::none();
        }
        if self.midi_transpose_dialog.is_some()
            && !matches!(
                &message,
                Message::SetMidiTransposeScope(_)
                    | Message::SetMidiTransposeSemitones(_)
                    | Message::AcceptMidiTranspose
                    | Message::CancelMidiTranspose
                    | Message::Escape
                    | Message::RuntimeKeyboardEvent(..)
                    | Message::BackgroundTick
                    | Message::MeterTick
                    | Message::WindowResized(..)
                    | Message::WindowClosed(..)
                    | Message::WindowCloseRequested(..)
                    | Message::CloseMidiEditor
            )
        {
            return Task::none();
        }
        if self.midi_event_properties_dialog.is_some()
            && !matches!(
                &message,
                Message::SetMidiEventPropertiesPosition(_)
                    | Message::SetMidiEventPropertiesLength(_)
                    | Message::SetMidiEventPropertiesChannel(_)
                    | Message::SetMidiEventPropertiesParameter(_)
                    | Message::SetMidiEventPropertiesValue(_)
                    | Message::SetMidiEventPropertiesOffValue(_)
                    | Message::SetMidiEventPropertiesBankMsb(_)
                    | Message::SetMidiEventPropertiesBankLsb(_)
                    | Message::SetMidiOpaqueEventPropertiesType(_)
                    | Message::SetMidiOpaqueEventPropertiesPayload(_)
                    | Message::SetMidiOpaqueEventPropertiesMultiline(_)
                    | Message::EditMidiOpaqueEventPropertiesText(..)
                    | Message::PickMidiOpaqueSysExFile(_)
                    | Message::PathPicked(
                        PathPickerTarget::ImportMidiOpaqueSysEx { .. }
                            | PathPickerTarget::ExportMidiOpaqueSysEx { .. }
                            | PathPickerTarget::ImportMidiOpaqueMetaText,
                        _,
                    )
                    | Message::MidiOpaqueSysExFileFinished(..)
                    | Message::PickMidiOpaqueMetaTextFile
                    | Message::MidiOpaqueMetaTextFileFinished(..)
                    | Message::ApplyMidiEventProperties
                    | Message::AcceptMidiEventProperties
                    | Message::CancelMidiEventProperties
                    | Message::EditMidiNotes(..)
                    | Message::SetMidiControllers(..)
                    | Message::SetMidiPitchBends(..)
                    | Message::SetMidiProgramChanges(..)
                    | Message::SetMidiPolyAftertouches(..)
                    | Message::SetMidiChannelPressures(..)
                    | Message::Escape
                    | Message::RuntimeKeyboardEvent(..)
                    | Message::BackgroundTick
                    | Message::MeterTick
                    | Message::WindowResized(..)
                    | Message::WindowClosed(..)
                    | Message::WindowCloseRequested(..)
                    | Message::CloseMidiEditor
            )
        {
            return Task::none();
        }
        if self.action_input_window_id.is_some()
            && matches!(
                &message,
                Message::ActionListQueryChanged(_)
                    | Message::ActionListFilterSubmitted
                    | Message::ActionListSelect(_)
                    | Message::ActionListSelectBinding(_)
                    | Message::ActionListAddBinding
                    | Message::ActionListDeleteBinding
                    | Message::ActionListFindShortcut
                    | Message::ActionListRun(_)
                    | Message::ActionListOptionsToggled
                    | Message::ActionListImportShortcutMap
                    | Message::ActionListExportShortcutMap
                    | Message::ActionListImportReaperKeyMap
                    | Message::ActionListExportReaperKeyMap
                    | Message::OpenActionMacroEditor
            )
        {
            return Task::none();
        }
        if self.pending_focused_track_deletion.is_some()
            && !matches!(
                &message,
                Message::ConfirmFocusedTrackDeletion
                    | Message::CancelFocusedTrackDeletion
                    | Message::Escape
                    | Message::RuntimeKeyboardEvent(..)
                    | Message::BackgroundTick
                    | Message::MeterTick
            )
        {
            return Task::none();
        }
        if self.pending_project_template_overwrite.is_some()
            && !matches!(
                &message,
                Message::ConfirmProjectTemplateOverwrite
                    | Message::CancelProjectTemplateOverwrite
                    | Message::Escape
                    | Message::RuntimeKeyboardEvent(..)
                    | Message::BackgroundTick
                    | Message::MeterTick
            )
        {
            return Task::none();
        }
        if self.pending_midi_file_import.is_some()
            && !matches!(
                &message,
                Message::SetMidiFileImportExpandTracks(_)
                    | Message::SetMidiFileImportTempoMap(_)
                    | Message::SetMidiFileImportMarkers(_)
                    | Message::ConfirmMidiFileImport
                    | Message::CancelMidiFileImport
                    | Message::Escape
                    | Message::RuntimeKeyboardEvent(..)
                    | Message::BackgroundTick
                    | Message::MeterTick
            )
        {
            return Task::none();
        }
        if self.pending_empty_midi_export.is_some()
            && !matches!(
                &message,
                Message::ConfirmEmptyMidiExport
                    | Message::CancelEmptyMidiExport
                    | Message::Escape
                    | Message::RuntimeKeyboardEvent(..)
                    | Message::BackgroundTick
                    | Message::MeterTick
            )
        {
            return Task::none();
        }
        let revision_before_message = self.revision;
        let preserves_master_gesture = matches!(
            &message,
            Message::BackgroundTick
                | Message::MeterTick
                | Message::RuntimeKeyboardEvent(..)
                | Message::PreviewMasterVolume(_)
                | Message::PreviewMasterPan(_)
                | Message::CommitMasterMix
                | Message::CancelMasterMix
                | Message::ResetMasterVolume
                | Message::ResetMasterPan
                | Message::Escape
        );
        if !preserves_master_gesture && self.master_mix_commit_at.is_some() {
            self.commit_master_mix();
        } else if !preserves_master_gesture && self.master_mix_gesture.is_some() {
            self.cancel_master_mix();
        }
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
                | Message::MidiFileImportFinished(_)
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
                    | Message::ToggleRecentProjectsMenu
                    | Message::ToggleProjectTemplatesMenu
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
                    | Message::ToggleRecentProjectsMenu
                    | Message::ToggleProjectTemplatesMenu
                    | Message::ToggleOfflineJobsPanel
                    | Message::DismissMainMenu
                    | Message::Escape
            )
        {
            self.active_menu = None;
            self.recent_projects_menu_open = false;
            self.project_templates_menu_open = false;
        }
        let menu_ui_message = matches!(
            &message,
            Message::ActionQueryChanged(_)
                | Message::ToggleRecentProjectsMenu
                | Message::ToggleProjectTemplatesMenu
                | Message::ShortcutPressed(_)
                | Message::ActionMenuScrolled(_)
                | Message::MenuKeyboard(
                    MenuNavigation::OpenMenu(_)
                        | MenuNavigation::NextMenu
                        | MenuNavigation::PreviousMenu
                        | MenuNavigation::FirstCommand
                        | MenuNavigation::LastCommand
                        | MenuNavigation::NextCommand
                        | MenuNavigation::PreviousCommand
                )
        ) || matches!(
            &message,
            Message::RuntimeKeyboardEvent(_, _, window_id)
                if self.main_window_id == Some(*window_id)
                    || self.item_properties_window_id == Some(*window_id)
                    || self.item_nudge_window_id == Some(*window_id)
                    || self.marker_properties_window_id == Some(*window_id)
                    || self.region_properties_window_id == Some(*window_id)
                    || self.marker_manager_window_id == Some(*window_id)
                    || self.region_render_matrix_window_id == Some(*window_id)
                    || self.routing_matrix_window_id == Some(*window_id)
                    || self.routing_window_id == Some(*window_id)
                    || self.action_input_window_id == Some(*window_id)
                    || self.action_list_window_id == Some(*window_id)
                    || self.settings_window_id == Some(*window_id)
                    || self.project_settings_window_id == Some(*window_id)
                    || self.snap_grid_settings_window_id == Some(*window_id)
                    || self.midi_export_window_id == Some(*window_id)
        );
        let window_safe_message = menu_ui_message
            || matches!(
                &message,
                Message::ExecuteCommand(
                    commands::CommandId::SelectedTrack(
                        commands::TrackCommand::Routing
                            | commands::TrackCommand::CycleFolderCompact
                    ) | commands::CommandId::Track {
                        command: commands::TrackCommand::Routing
                            | commands::TrackCommand::CycleFolderCompact,
                        ..
                    } | commands::CommandId::MidiNotesUpSemitone
                        | commands::CommandId::MidiNotesDownSemitone
                        | commands::CommandId::MidiNotesUpOctave
                        | commands::CommandId::MidiNotesDownOctave
                        | commands::CommandId::MidiNotesUpSemitoneIgnoringScale
                        | commands::CommandId::MidiNotesDownSemitoneIgnoringScale
                        | commands::CommandId::MidiTransposeNotes
                        | commands::CommandId::MidiEditorPianoRoll
                        | commands::CommandId::MidiEditorEventList
                ) | Message::OpenTrackRouting(_)
                    | Message::OpenTrackRoutingTo(_, _)
                    | Message::CloseTrackRouting
                    | Message::RoutingSendDraft(..)
                    | Message::ToggleMixerPanel
                    | Message::OpenActionList
                    | Message::OpenActionMacroEditor
                    | Message::CloseActionList
                    | Message::ActionListQueryChanged(_)
                    | Message::ActionListFilterSubmitted
                    | Message::ActionListSelect(_)
                    | Message::ActionListSelectBinding(_)
                    | Message::ActionInputConfirm
                    | Message::ActionInputCancel
                    | Message::ActionInputSpecialKey
                    | Message::ActionListAddBinding
                    | Message::ActionListDeleteBinding
                    | Message::ActionListFindShortcut
                    | Message::ActionListRun(_)
                    | Message::ActionListOptionsToggled
                    | Message::ActionListImportShortcutMap
                    | Message::ActionListExportShortcutMap
                    | Message::ActionListImportReaperKeyMap
                    | Message::ActionListExportReaperKeyMap
                    | Message::ShortcutMapImported(_)
                    | Message::ShortcutMapExported(_)
                    | Message::ReaperKeyMapImported(_)
                    | Message::ReaperKeyMapExported(_)
                    | Message::OpenSettings
                    | Message::OpenAbout
                    | Message::CloseAbout
                    | Message::OpenProjectSettings
                    | Message::CloseProjectSettings
                    | Message::OpenSnapGridSettings
                    | Message::CloseSnapGridSettings
                    | Message::SnapSwingGridChanged(_)
                    | Message::SnapSwingAmountChanged(_)
                    | Message::ToggleSnapSwingGrid
                    | Message::SaveProjectSettingsAsDefault
                    | Message::ProjectFrameRateChanged(_)
                    | Message::ProjectFrameRateMenuChanged(_)
                    | Message::ProjectSettingsTabChanged(_)
                    | Message::ProjectSettingsSampleRateChanged(_)
                    | Message::ProjectSettingsForceWholeSamplesChanged(_)
                    | Message::ProjectSettingsTempoChanged(_)
                    | Message::ProjectSettingsMeterNumeratorChanged(_)
                    | Message::ProjectSettingsMeterDenominatorChanged(_)
                    | Message::ProjectSettingsStartMeasureChanged(_)
                    | Message::ProjectSettingsBpmBasisChanged(_)
                    | Message::ProjectSettingsBaseRulerLabelSpacingChanged(_)
                    | Message::ProjectSettingsStartTimeChanged(_)
                    | Message::ProjectStartTimeMenuChanged(_)
                    | Message::ProjectStartTimeCommandChosen(_)
                    | Message::ProjectNotesTitleChanged(_)
                    | Message::ProjectNotesAuthorChanged(_)
                    | Message::ProjectNotesEdited(_)
                    | Message::ProjectNotesShowOnLoadChanged(_)
                    | Message::ProjectNotesWrapChanged(_)
                    | Message::OpenClapPluginSettings
                    | Message::OpenRenderWindow
                    | Message::OpenRenderQueue
                    | Message::OpenMidiExportDialog
                    | Message::CloseMidiExportDialog
                    | Message::BrowseMidiExportDestination
                    | Message::ConfirmMidiExport
                    | Message::MidiExportDestinationChanged(_)
                    | Message::MidiExportFileTypeChanged(_)
                    | Message::MidiExportTimeScopeChanged(_)
                    | Message::MidiExportItemScopeChanged(_)
                    | Message::MidiExportPpqChanged(_)
                    | Message::MidiExportEmbedTempoChanged(_)
                    | Message::MidiExportEmbedSmpteChanged(_)
                    | Message::MidiExportMarkersChanged(_)
                    | Message::MidiExportMarkerTypeChanged(_)
                    | Message::MidiExportHashOnlyChanged(_)
                    | Message::MidiExportHiddenMarkersChanged(_)
                    | Message::ShowMainWorkspace(_)
                    | Message::MobileNavigateBack
                    | Message::ToggleItemProperties
                    | Message::OpenItemProperties
                    | Message::OpenItemNudge
                    | Message::CloseItemNudge
                    | Message::ItemNudgeModeChanged(_)
                    | Message::ItemNudgeTargetChanged(_)
                    | Message::ItemNudgeUnitChanged(_)
                    | Message::ItemNudgeNoteValueChanged(_)
                    | Message::ItemNudgeValueChanged(_)
                    | Message::ItemNudgeSnapToUnitChanged(_)
                    | Message::ItemNudgeRelativeSetChanged(_)
                    | Message::ItemNudgeGetCursor
                    | Message::ApplyItemNudgeSet
                    | Message::NudgeSelectedItems(_)
                    | Message::TransposeSelectedMidiNotes(_)
                    | Message::ItemPropertyTakeChanged(_)
                    | Message::ItemPropertiesUnitChanged(_)
                    | Message::ItemPropertyFieldChanged(..)
                    | Message::ItemPropertyNameChanged(_)
                    | Message::ItemPropertyTakeRateChanged(_)
                    | Message::ItemPropertyLoopSourceChanged(_)
                    | Message::ItemPropertyMuteChanged(_)
                    | Message::BatchItemPropertyMuteChanged(_)
                    | Message::ItemPropertyPhaseChanged(_)
                    | Message::MidiItemPropertyNameChanged(_)
                    | Message::MidiItemPropertySourceLoopLengthChanged(_)
                    | Message::CloseItemProperties
                    | Message::MarkerNameChanged(_)
                    | Message::MarkerNumberChanged(_)
                    | Message::MarkerPositionChanged(_)
                    | Message::ApplyMarkerProperties
                    | Message::CloseMarkerProperties
                    | Message::RegionNameChanged(_)
                    | Message::RegionNumberChanged(_)
                    | Message::RegionPositionChanged(_)
                    | Message::RegionLengthChanged(_)
                    | Message::ApplyRegionProperties
                    | Message::CloseRegionProperties
                    | Message::OpenMarkerManager
                    | Message::CloseMarkerManager
                    | Message::OpenRegionRenderMatrix
                    | Message::CloseRegionRenderMatrix
                    | Message::SetRegionRenderMatrixMaster(_, _)
                    | Message::SetRegionRenderMatrixTrack(_, _, _)
                    | Message::OpenRoutingMatrix
                    | Message::CloseRoutingMatrix
                    | Message::SetRoutingMatrixMainSend(_, _)
                    | Message::SetRoutingMatrixSend(_, _)
                    | Message::MarkerManagerFilterChanged(_)
                    | Message::MarkerManagerShowRegionsChanged(_)
                    | Message::MarkerManagerShowMarkersChanged(_)
                    | Message::MarkerManagerClearFilter
                    | Message::MarkerManagerSelectMarker(_, _)
                    | Message::MarkerManagerEditMarker(_)
                    | Message::MarkerManagerDeleteMarker(_)
                    | Message::MarkerManagerSelectRegion(_, _)
                    | Message::MarkerManagerEditRegion(_)
                    | Message::MarkerManagerDeleteRegion(_)
                    | Message::InsertRegionFromTimeSelection
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
                    | Message::SelectMidiEditorLane(_)
                    | Message::SelectMidiEditorTool(_)
                    | Message::SetMidiEditorMode(_)
                    | Message::AndroidAppFocusChanged(_)
                    | Message::CloseTrackFxChain
                    | Message::ClosePluginPicker
                    | Message::PluginPickerSearchChanged(_)
                    | Message::SelectFxChainPlugin(_)
                    | Message::ExecuteCommand(commands::CommandId::OpenSettings)
                    | Message::ExecuteCommand(commands::CommandId::OpenProjectSettings)
                    | Message::ExecuteCommand(commands::CommandId::OpenActionList)
                    | Message::ExecuteCommand(commands::CommandId::OpenMarkerManager)
                    | Message::ExecuteCommand(commands::CommandId::OpenRegionRenderMatrix)
                    | Message::ExecuteCommand(commands::CommandId::OpenRoutingMatrix)
                    | Message::ExecuteCommand(commands::CommandId::ExportWav)
                    | Message::ToggleMediaBrowserPanel
                    | Message::ExecuteCommand(commands::CommandId::ToggleMediaBrowserPanel)
                    | Message::ExecuteCommand(commands::CommandId::ToggleMixerPanel)
                    | Message::ToggleOfflineJobsPanel
                    | Message::ToggleTransportDetails
                    | Message::ExecuteCommand(commands::CommandId::ToggleOfflineJobsPanel)
                    | Message::WindowClosed(_)
                    | Message::WindowCloseRequested(_)
                    | Message::StartShortcutCapture(_)
                    | Message::AddShortcutBinding(_)
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
                    | Message::PickPath(
                        PathPickerTarget::ImportShortcutMap
                            | PathPickerTarget::ExportShortcutMap
                            | PathPickerTarget::ImportReaperKeyMap
                            | PathPickerTarget::ExportReaperKeyMap
                    )
                    | Message::PathPicked(
                        PathPickerTarget::ImportShortcutMap
                            | PathPickerTarget::ExportShortcutMap
                            | PathPickerTarget::ImportReaperKeyMap
                            | PathPickerTarget::ExportReaperKeyMap,
                        _,
                    )
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
                    | Message::ProjectTemplateCopied(..)
                    | Message::ProjectTemplateSaved(..)
                    | Message::ProjectMidiExported(..)
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
                timeline::TimelineEvent::PreviewItemFades { .. }
                    | timeline::TimelineEvent::CommitItemFades
                    | timeline::TimelineEvent::SetFadeCurvePreset { .. }
            ) | Message::ApplyItemProperties(_)
                | Message::ItemPropertyTakeChanged(_)
        ) && let Some(status) = item_drag_edit_guard_status(
            self.path_picker_busy,
            self.import_busy,
            self.audio_asset_management_busy,
            self.playback_busy(),
            false,
            self.io_busy,
        ) {
            self.cancel_item_fades();
            self.status = status.to_owned();
            return Task::none();
        }
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
                    | Message::MidiFileImportFinished(_)
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
            let recording_stop_command = (self.recording.is_some() || self.recording_starting)
                && matches!(
                    &message,
                    Message::TogglePlayStop
                        | Message::StopPlayback
                        | Message::ExecuteCommand(
                            commands::CommandId::TogglePlayStop | commands::CommandId::StopPlayback
                        )
                );
            if (self.recording.is_some() || self.recording_starting || self.recording_stopping)
                && !window_safe_message
                && !recording_stop_command
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
                && !recording_stop_command
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
                Message::Undo => {
                    !self.app_selection_is_next_undo() && !self.project.can_undo_track_mix()
                }
                Message::Redo => {
                    !self.app_selection_is_next_redo() && !self.project.can_redo_track_mix()
                }
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
                            | Message::TrimItemsToSelectedArea
                            | Message::EncloseMediaItemsInRazorEdit
                            | Message::DuplicateSelectedItems
                            | Message::DuplicateAudioItem(_)
                            | Message::DuplicateAudioItemTake(_)
                            | Message::DuplicateSelectedItemActiveTakes
                            | Message::DeleteSelectedItemActiveTakes
                            | Message::MoveSelectedItemActiveTakesToTop
                            | Message::ImplodeSelectedAudioItemsAsTakes
                            | Message::ImplodeSelectedMidiItemsAsTakes
                            | Message::ImplodeSelectedAudioItemsAcrossTracksAsTakes
                            | Message::ExplodeSelectedAudioItemTakesInPlace
                            | Message::CropSelectedItemsToActiveTake
                            | Message::SwitchSelectedAudioItemTakes(..)
                            | Message::SwitchAudioItemTake(..)
                            | Message::SwitchMidiItemTake(..)
                            | Message::AddEmptyTakes(..)
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
                    self.recent_projects_menu_open = false;
                    self.project_templates_menu_open = false;
                } else {
                    task = self.open_main_menu(menu);
                }
            }
            Message::ToggleRecentProjectsMenu => {
                self.project_templates_menu_open = false;
                if self.active_menu != Some(MainMenu::File) {
                    task = self.open_main_menu(MainMenu::File);
                    self.recent_projects_menu_open = true;
                } else {
                    self.recent_projects_menu_open = !self.recent_projects_menu_open;
                }
                self.menu_selected_command = Some(commands::CommandId::OpenRecentProjects);
            }
            Message::ToggleProjectTemplatesMenu => {
                self.recent_projects_menu_open = false;
                if self.active_menu != Some(MainMenu::File) {
                    task = self.open_main_menu(MainMenu::File);
                    self.project_templates_menu_open = true;
                } else {
                    self.project_templates_menu_open = !self.project_templates_menu_open;
                }
                self.menu_selected_command = Some(commands::CommandId::OpenProjectTemplates);
            }
            Message::MenuKeyboard(navigation) => {
                task = self.navigate_main_menu(navigation);
            }
            Message::ToggleMixerPanel => {
                if self.is_mobile_main_window() {
                    self.main_workspace = if self.main_workspace == MainWorkspace::Mixer {
                        MainWorkspace::Arrangement
                    } else {
                        MainWorkspace::Mixer
                    };
                } else {
                    self.media_panel_dock.toggle_mixer();
                    self.schedule_desktop_layout_save();
                }
            }
            Message::ShowMainWorkspace(workspace) => {
                self.cancel_shortcut_capture();
                self.main_workspace = workspace;
                if !self.is_mobile_main_window()
                    && workspace == MainWorkspace::Mixer
                    && !self.media_panel_dock.mixer_open
                {
                    self.media_panel_dock.toggle_mixer();
                    self.schedule_desktop_layout_save();
                }
                self.mobile_panel = MobilePanel::Editor;
                self.mobile_panel_history.clear();
                self.active_menu = None;
            }
            Message::MobileNavigateBack => self.navigate_back_mobile_panel(),
            Message::OpenSettings => task = self.open_settings(),
            Message::OpenAbout => task = self.open_about(),
            Message::CloseAbout => {
                if let Some(window_id) = self.about_window_id {
                    task = iced::window::close(window_id);
                }
            }
            Message::OpenProjectSettings => task = self.open_project_settings(),
            Message::CloseProjectSettings => task = self.close_project_settings(),
            Message::OpenSnapGridSettings => task = self.open_snap_grid_settings(),
            Message::CloseSnapGridSettings => {
                task = self.close_snap_grid_settings();
            }
            Message::SnapSwingGridChanged(enabled) => {
                self.handle_timeline_view_event(timeline::TimelineEvent::SetSnapSwingEnabled(
                    enabled,
                ));
            }
            Message::ToggleSnapSwingGrid => {
                self.handle_timeline_view_event(timeline::TimelineEvent::ToggleSnapSwing)
            }
            Message::SnapSwingAmountChanged(amount) => {
                self.handle_timeline_view_event(
                    timeline::TimelineEvent::SetSnapSwingAmountPercent(amount),
                );
            }
            Message::ApplyProjectSettings => task = self.apply_project_settings(),
            Message::SaveProjectSettingsAsDefault => task = self.save_project_settings_as_default(),
            Message::ProjectSettingsTempoChanged(value) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.tempo_bpm = value;
                }
            }
            Message::ProjectSettingsSampleRateChanged(sample_rate) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.sample_rate = sample_rate;
                }
            }
            Message::ProjectSettingsForceWholeSamplesChanged(enabled) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.force_tempo_changes_to_whole_samples = enabled;
                }
            }
            Message::ProjectSettingsMeterNumeratorChanged(value) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.meter_numerator = value;
                }
            }
            Message::ProjectSettingsMeterDenominatorChanged(value) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.meter_denominator = value;
                }
            }
            Message::ProjectSettingsStartMeasureChanged(value) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.start_measure = value;
                }
            }
            Message::ProjectSettingsBpmBasisChanged(basis) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.bpm_basis = basis;
                }
            }
            Message::ProjectSettingsBaseRulerLabelSpacingChanged(enabled) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.base_ruler_label_spacing_on_measure = enabled;
                }
            }
            Message::ProjectSettingsStartTimeChanged(value) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.project_start_time = value;
                }
            }
            Message::ProjectStartTimeMenuChanged(open) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.menu_open = open;
                }
            }
            Message::ProjectStartTimeCommandChosen(command) => {
                task = project_settings::apply_start_time_command(self, command);
            }
            Message::ProjectSettingsTabChanged(tab) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.tab = tab;
                    draft.menu_open = false;
                    draft.menu_epoch = draft.menu_epoch.wrapping_add(1);
                }
            }
            Message::ProjectNotesTitleChanged(value) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.notes.title = value;
                }
            }
            Message::ProjectNotesAuthorChanged(value) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.notes.author = value;
                }
            }
            Message::ProjectNotesShowOnLoadChanged(value) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.notes.show_on_load = value;
                }
            }
            Message::ProjectNotesWrapChanged(value) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.notes.word_wrap = value;
                }
            }
            Message::ProjectNotesEdited(action) => {
                if let Some(draft) = &mut self.project_settings {
                    let edit = action.is_edit();
                    draft.notes_editor.perform(action);
                    if edit {
                        draft.notes.text = draft.notes_editor.text();
                    }
                }
            }
            Message::ProjectFrameRateMenuChanged(open) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.menu_open = open;
                }
            }
            Message::ProjectFrameRateChanged(rate) => {
                if let Some(draft) = &mut self.project_settings {
                    draft.rate = rate;
                    draft.menu_open = false;
                }
            }
            Message::OpenClapPluginSettings => {
                self.settings_category = SettingsCategory::ClapPlugins;
                task = self.open_settings();
            }
            Message::OpenRenderWindow => task = self.open_render_window(),
            Message::OpenRenderQueue => self.offline_jobs_panel_open = true,
            Message::OpenMidiExportDialog => task = self.open_midi_export_dialog(),
            Message::CloseMidiExportDialog => task = self.close_midi_export_dialog(),
            Message::BrowseMidiExportDestination => {
                task = self.browse_midi_export_destination();
            }
            Message::ConfirmMidiExport => task = self.confirm_midi_export(),
            Message::ConfirmEmptyMidiExport => task = self.confirm_empty_midi_export(),
            Message::CancelEmptyMidiExport => self.dismiss_empty_midi_export(),
            Message::MidiExportDestinationChanged(value) => {
                if let Some(dialog) = &mut self.midi_export_dialog {
                    dialog.destination = value;
                }
            }
            Message::MidiExportFileTypeChanged(file_type) => {
                if let Some(dialog) = &mut self.midi_export_dialog {
                    dialog.file_type = file_type;
                }
            }
            Message::MidiExportTimeScopeChanged(time_scope) => {
                if let Some(dialog) = &mut self.midi_export_dialog {
                    dialog.time_scope = time_scope;
                }
            }
            Message::MidiExportItemScopeChanged(item_scope) => {
                if let Some(dialog) = &mut self.midi_export_dialog {
                    dialog.item_scope = item_scope;
                }
            }
            Message::MidiExportPpqChanged(value) => {
                if let Some(dialog) = &mut self.midi_export_dialog {
                    dialog.ppq = value;
                }
            }
            Message::MidiExportEmbedTempoChanged(enabled) => {
                if let Some(dialog) = &mut self.midi_export_dialog {
                    dialog.embed_tempo_and_time_signature = enabled;
                }
            }
            Message::MidiExportEmbedSmpteChanged(enabled) => {
                if let Some(dialog) = &mut self.midi_export_dialog {
                    dialog.embed_smpte_offset = enabled;
                }
            }
            Message::MidiExportMarkersChanged(enabled) => {
                if !self.project.markers().is_empty()
                    && let Some(dialog) = &mut self.midi_export_dialog
                {
                    dialog.export_markers = enabled;
                }
            }
            Message::MidiExportMarkerTypeChanged(marker_type) => {
                if !self.project.markers().is_empty()
                    && self
                        .midi_export_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.export_markers)
                    && let Some(dialog) = &mut self.midi_export_dialog
                {
                    dialog.marker_type = marker_type;
                }
            }
            Message::MidiExportHashOnlyChanged(enabled) => {
                if !self.project.markers().is_empty()
                    && self
                        .midi_export_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.export_markers)
                    && let Some(dialog) = &mut self.midi_export_dialog
                {
                    dialog.only_markers_starting_with_hash = enabled;
                }
            }
            Message::MidiExportHiddenMarkersChanged(enabled) => {
                if !self.project.markers().is_empty()
                    && self
                        .midi_export_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.export_markers)
                    && let Some(dialog) = &mut self.midi_export_dialog
                {
                    dialog.export_hidden_markers = enabled;
                }
            }
            Message::ProjectMidiExported(path, result) => {
                self.status = match result {
                    Ok(()) => format!("Exported project MIDI to {}", path.display()),
                    Err(error) => format!("MIDI export failed: {error}"),
                };
            }
            Message::OpenItemProperties => task = self.open_item_properties(),
            Message::OpenItemNudge => task = self.open_item_nudge(),
            Message::CloseItemNudge => task = self.close_item_nudge(),
            Message::ItemNudgeModeChanged(mode) => self.change_item_nudge_mode(mode),
            Message::ItemNudgeTargetChanged(target) => self.change_item_nudge_target(target),
            Message::ItemNudgeUnitChanged(unit) => self.change_item_nudge_unit(unit),
            Message::ItemNudgeNoteValueChanged(note_value) => {
                self.change_item_nudge_note_value(note_value)
            }
            Message::ItemNudgeValueChanged(value) => self.change_item_nudge_value(value),
            Message::ItemNudgeSnapToUnitChanged(enabled) => self.change_item_nudge_snap(enabled),
            Message::ItemNudgeRelativeSetChanged(enabled) => {
                self.change_item_nudge_relative_set(enabled)
            }
            Message::ItemNudgeGetCursor => self.item_nudge_get_cursor(),
            Message::ApplyItemNudgeSet => self.apply_item_nudge_set(),
            Message::NudgeSelectedItems(direction) => self.nudge_selected_items(direction),
            Message::ToggleItemProperties => {
                task = if self.item_properties_window_id.is_some() {
                    self.close_item_properties()
                } else {
                    self.open_item_properties()
                }
            }
            Message::CloseItemProperties => task = self.close_item_properties(),
            Message::ItemPropertyTakeChanged(take_index) => {
                self.change_item_properties_take(take_index)
            }
            Message::ItemPropertiesUnitChanged(unit) => self.change_item_properties_unit(unit),
            Message::ItemPropertyFieldChanged(field, text) => {
                if let Some(draft) = &mut self.item_properties {
                    draft.edit(field, text);
                } else if let Some(draft) = &mut self.midi_item_properties {
                    draft.edit(field, text);
                }
            }
            Message::ItemPropertyNameChanged(name) => {
                if let Some(draft) = &mut self.item_properties {
                    draft.set_name(name);
                }
            }
            Message::ItemPropertyTakeRateChanged(rate) => {
                if let Some(draft) = &mut self.item_properties {
                    draft.set_take_rate(rate);
                }
            }
            Message::ItemPropertyLoopSourceChanged(enabled) => {
                if let Some(draft) = &mut self.item_properties {
                    draft.set_loop_source(enabled);
                } else if let Some(draft) = &mut self.midi_item_properties {
                    draft.set_loop_source(enabled);
                }
            }
            Message::ItemPropertyMuteChanged(muted) => {
                if let Some(draft) = &mut self.item_properties {
                    draft.set_muted(muted);
                } else if let Some(draft) = &mut self.midi_item_properties {
                    draft.set_muted(muted);
                }
            }
            Message::BatchItemPropertyMuteChanged(muted) => {
                if let Some(draft) = &mut self.batch_item_properties {
                    draft.set_muted(muted);
                }
            }
            Message::ItemPropertyPhaseChanged(inverted) => {
                if let Some(draft) = &mut self.item_properties {
                    draft.set_phase_inverted(inverted);
                }
            }
            Message::MidiItemPropertyNameChanged(name) => {
                if let Some(draft) = &mut self.midi_item_properties {
                    draft.set_name(name);
                }
            }
            Message::MidiItemPropertySourceLoopLengthChanged(value) => {
                if let Some(draft) = &mut self.midi_item_properties {
                    draft.set_source_loop_length(value);
                }
            }
            Message::ToggleSelectedItemsMute => self.toggle_selected_items_mute(),
            Message::ApplyItemProperties(close) => task = self.apply_item_properties(close),
            Message::MarkerNameChanged(value) => {
                if let Some(draft) = &mut self.marker_properties {
                    draft.name = value;
                    draft.error = None;
                }
            }
            Message::MarkerNumberChanged(value) => {
                if let Some(draft) = &mut self.marker_properties {
                    draft.number = value;
                    draft.error = None;
                }
            }
            Message::MarkerPositionChanged(value) => {
                if let Some(draft) = &mut self.marker_properties {
                    draft.position = value;
                    draft.error = None;
                }
            }
            Message::ApplyMarkerProperties => task = self.apply_marker_properties(),
            Message::CloseMarkerProperties => task = self.close_marker_properties(),
            Message::RegionNameChanged(value) => {
                if let Some(draft) = &mut self.region_properties {
                    draft.name = value;
                    draft.error = None;
                }
            }
            Message::RegionNumberChanged(value) => {
                if let Some(draft) = &mut self.region_properties {
                    draft.number = value;
                    draft.error = None;
                }
            }
            Message::RegionPositionChanged(value) => {
                if let Some(draft) = &mut self.region_properties {
                    draft.position = value;
                    draft.error = None;
                }
            }
            Message::RegionLengthChanged(value) => {
                if let Some(draft) = &mut self.region_properties {
                    draft.length = value;
                    draft.error = None;
                }
            }
            Message::ApplyRegionProperties => task = self.apply_region_properties(),
            Message::CloseRegionProperties => task = self.close_region_properties(),
            Message::OpenMarkerManager => task = self.open_marker_manager(),
            Message::OpenRegionRenderMatrix => task = self.open_region_render_matrix(),
            Message::CloseRegionRenderMatrix => task = self.close_region_render_matrix(),
            Message::OpenRoutingMatrix => task = self.open_routing_matrix(),
            Message::CloseRoutingMatrix => task = self.close_routing_matrix(),
            Message::SetRoutingMatrixMainSend(track_id, enabled) => {
                self.set_routing_matrix_main_send(track_id, enabled)
            }
            Message::SetRoutingMatrixSend(source, destination) => {
                task = self.set_routing_matrix_send(source, destination)
            }
            Message::SetRegionRenderMatrixMaster(region_id, enabled) => {
                self.set_region_render_matrix_master(region_id, enabled)
            }
            Message::SetRegionRenderMatrixTrack(region_id, track_id, enabled) => {
                self.set_region_render_matrix_track(region_id, track_id, enabled)
            }
            Message::ToggleProjectMarkersOnRuler => task = self.toggle_project_markers_on_ruler(),
            Message::ToggleProjectRegionsOnRuler => task = self.toggle_project_regions_on_ruler(),
            Message::ToggleProjectRegionsAndMarkersOnRuler => {
                task = self.toggle_project_regions_and_markers_on_ruler()
            }
            Message::ToggleSelectedProjectMarkers => self.toggle_selected_project_markers(),
            Message::ToggleSelectedProjectRegions => self.toggle_selected_project_regions(),
            Message::ToggleSelectedProjectRegionsAndMarkers => {
                self.toggle_selected_project_regions_and_markers()
            }
            Message::CloseMarkerManager => task = self.close_marker_manager(),
            Message::MarkerManagerFilterChanged(value) => {
                if let Some(manager) = &mut self.marker_manager {
                    manager.filter = value;
                }
            }
            Message::MarkerManagerShowRegionsChanged(enabled) => {
                if let Some(manager) = &mut self.marker_manager {
                    manager.show_regions = enabled;
                }
            }
            Message::MarkerManagerShowMarkersChanged(enabled) => {
                if let Some(manager) = &mut self.marker_manager {
                    manager.show_markers = enabled;
                }
            }
            Message::MarkerManagerClearFilter => {
                if let Some(manager) = &mut self.marker_manager {
                    manager.filter.clear();
                }
            }
            Message::MarkerManagerSelectMarker(marker_id, additive) => {
                self.select_marker_from_manager(marker_id, additive)
            }
            Message::MarkerManagerSetMarkerVisibility(marker_id, visible) => {
                self.set_marker_visibility_from_manager(marker_id, visible)
            }
            Message::MarkerManagerSetRegionVisibility(region_id, visible) => {
                self.set_region_visibility_from_manager(region_id, visible)
            }
            Message::MarkerManagerEditMarker(marker_id) => {
                task = self.open_marker_properties(marker_id)
            }
            Message::MarkerManagerDeleteMarker(marker_id) => {
                self.delete_marker_from_manager(marker_id)
            }
            Message::MarkerManagerSelectRegion(region_id, additive) => {
                self.select_region_from_manager(region_id, additive)
            }
            Message::MarkerManagerEditRegion(region_id) => {
                task = self.open_region_properties(region_id)
            }
            Message::MarkerManagerDeleteRegion(region_id) => {
                self.delete_region_from_manager(region_id)
            }
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
                if self.main_window_id == Some(window_id) {
                    self.flush_loop_time_preferences();
                    self.flush_midi_humanize_preferences();
                    self.flush_midi_quantize_preferences();
                }
                if self.project_settings_window_id == Some(window_id) {
                    self.project_settings_window_id = None;
                    self.project_settings = None;
                } else if self.snap_grid_settings_window_id == Some(window_id) {
                    self.snap_grid_settings_window_id = None;
                } else if self.item_properties_window_id == Some(window_id) {
                    self.item_properties_window_id = None;
                    self.item_properties = None;
                    self.midi_item_properties = None;
                } else if self.item_nudge_window_id == Some(window_id) {
                    self.item_nudge_window_id = None;
                } else if self.marker_properties_window_id == Some(window_id) {
                    self.marker_properties_window_id = None;
                    self.marker_properties = None;
                } else if self.region_properties_window_id == Some(window_id) {
                    self.region_properties_window_id = None;
                    self.region_properties = None;
                } else if self.marker_manager_window_id == Some(window_id) {
                    self.marker_manager_window_id = None;
                    self.marker_manager = None;
                } else if self.region_render_matrix_window_id == Some(window_id) {
                    self.region_render_matrix_window_id = None;
                } else if self.routing_matrix_window_id == Some(window_id) {
                    self.routing_matrix_window_id = None;
                } else if self.routing_window_id == Some(window_id) {
                    self.routing_window_id = None;
                    self.routing_track_id = None;
                    self.routing_destination_filter = None;
                    self.routing_send_drafts.clear();
                } else if self.action_input_window_id == Some(window_id) {
                    self.action_input_window_id = None;
                    self.action_list.capture = None;
                    self.action_list.special_key_capture_pending = false;
                    self.action_list.input_action = None;
                    self.action_list.input_draft = None;
                } else if self.action_list_window_id == Some(window_id) {
                    task = self.close_action_input();
                    self.action_list_window_id = None;
                    self.action_list.capture = None;
                } else if self.settings_window_id == Some(window_id) {
                    self.settings_window_id = None;
                    self.shortcut_capture_id = None;
                    self.shortcut_editor_feedback.clear();
                } else if self.about_window_id == Some(window_id) {
                    self.about_window_id = None;
                } else if self.render_window_id == Some(window_id) {
                    self.render_window_id = None;
                } else if self.midi_export_window_id == Some(window_id) {
                    self.midi_export_window_id = None;
                    self.midi_export_dialog = None;
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
                } else if self.midi_editor_window_id == Some(window_id)
                    && self.main_window_id != Some(window_id)
                {
                    self.release_midi_preview();
                    self.midi_editor_window_id = None;
                    self.midi_editor_item_id = None;
                    self.midi_editor_feedback = None;
                    self.midi_event_properties_dialog = None;
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_selected_controllers.clear();
                    self.midi_editor_selected_pitch_bends.clear();
                    self.midi_editor_selected_program_changes.clear();
                    self.midi_editor_selected_poly_aftertouches.clear();
                    self.midi_editor_selected_channel_pressures.clear();
                    self.midi_editor_selected_opaque_events.clear();
                    self.midi_editor_event_list_cursor = None;
                    self.midi_editor_event_list_anchor = None;
                } else if self.main_window_id == Some(window_id) {
                    #[cfg(target_os = "android")]
                    tracing::warn!(?window_id, "Android main window closed");
                    self.close_fx_editor_resources();
                    task = iced::exit();
                }
            }
            Message::WindowCloseRequested(window_id) => {
                if self.main_window_id == Some(window_id) {
                    self.flush_desktop_layout();
                    self.flush_loop_time_preferences();
                    self.flush_midi_humanize_preferences();
                    self.flush_midi_quantize_preferences();
                }
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
            Message::ToggleMidiInlineEditors => {
                self.timeline.toggle_inline_midi_editors(false);
            }
            Message::CloseMidiInlineEditors => {
                self.timeline.toggle_inline_midi_editors(true);
            }
            Message::SetMidiInlineEditor(item_id, open) => {
                self.timeline.set_inline_midi_editor(item_id, open);
            }
            Message::CloseMidiEditor => {
                self.release_midi_preview();
                self.midi_quantize_dialog = None;
                self.midi_humanize_dialog = None;
                self.midi_transpose_dialog = None;
                self.midi_event_properties_dialog = None;
                if let Some(window_id) = self.midi_editor_window_id.take() {
                    self.midi_editor_item_id = None;
                    self.midi_editor_feedback = None;
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_selected_controllers.clear();
                    self.midi_editor_selected_pitch_bends.clear();
                    self.midi_editor_selected_program_changes.clear();
                    self.midi_editor_selected_poly_aftertouches.clear();
                    self.midi_editor_selected_channel_pressures.clear();
                    self.midi_editor_selected_opaque_events.clear();
                    self.midi_editor_event_list_cursor = None;
                    self.midi_editor_event_list_anchor = None;
                    self.midi_editor_edit_cursor_tick = None;
                    if self.main_window_id == Some(window_id)
                        && self.mobile_panel == MobilePanel::MidiEditor
                    {
                        self.navigate_back_mobile_panel();
                    } else {
                        task = iced::window::close(window_id);
                    }
                }
            }
            Message::SelectMidiEditorLane(lane) => {
                self.release_midi_preview();
                self.midi_editor_lane = lane;
            }
            Message::SelectMidiEditorTool(tool) => self.midi_editor_tool = tool,
            Message::SetMidiEditorMode(mode) => {
                if self.midi_editor_item_id.is_some() {
                    self.midi_editor_mode = mode;
                }
            }
            Message::MoveMidiEditorEventListCursor(
                direction,
                preserve_selection,
                extend_selection,
            ) => {
                self.move_midi_editor_event_list_cursor_with_history(
                    direction,
                    preserve_selection,
                    extend_selection,
                );
            }
            Message::MoveMidiEditorCursorToBoundary(boundary) => {
                let Some(item_id) = self.midi_editor_item_id else {
                    return task;
                };
                let Some(item) = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                else {
                    return task;
                };
                let (local_tick, project_tick) = match boundary {
                    MidiEditorCursorBoundary::FirstEvent => {
                        let Some((cursor, local_tick, project_tick)) =
                            view::midi_event_list_home_target(self, item)
                        else {
                            return task;
                        };
                        let _ = self.update(Message::SelectMidiEditorEventSelection(cursor, false));
                        (local_tick, project_tick)
                    }
                    MidiEditorCursorBoundary::End => (
                        item.length_ticks(),
                        item.start_tick().saturating_add(item.length_ticks()),
                    ),
                };
                self.midi_editor_edit_cursor_tick = Some(local_tick);
                self.midi_note_clipboard.last_paste = None;
                self.set_edit_cursor_position(timeline::EditCursorPosition::Tick(project_tick));
            }
            Message::DeleteSelectedMidiEvents(item_id) => {
                let Some(item) = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                else {
                    return task;
                };
                let note_ids = item
                    .notes()
                    .iter()
                    .filter(|note| self.midi_editor_selected_notes.contains(&note.id()))
                    .map(|note| note.id())
                    .collect::<Vec<_>>();
                let controllers = item
                    .controllers()
                    .iter()
                    .filter(|event| !self.midi_editor_selected_controllers.contains(event))
                    .copied()
                    .collect::<Vec<_>>();
                let pitch_bends = item
                    .pitch_bends()
                    .iter()
                    .filter(|event| !self.midi_editor_selected_pitch_bends.contains(event))
                    .copied()
                    .collect::<Vec<_>>();
                let program_changes = item
                    .program_changes()
                    .iter()
                    .filter(|event| !self.midi_editor_selected_program_changes.contains(event))
                    .copied()
                    .collect::<Vec<_>>();
                let poly_aftertouches = item
                    .poly_aftertouches()
                    .iter()
                    .filter(|event| !self.midi_editor_selected_poly_aftertouches.contains(event))
                    .copied()
                    .collect::<Vec<_>>();
                let channel_pressures = item
                    .channel_pressures()
                    .iter()
                    .filter(|event| !self.midi_editor_selected_channel_pressures.contains(event))
                    .copied()
                    .collect::<Vec<_>>();
                let opaque_events = item
                    .opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        !self
                            .midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.clone())
                    .collect::<Vec<_>>();
                let mut actions = Vec::new();
                if !note_ids.is_empty() {
                    actions.push(DawAction::DeleteMidiNotes { item_id, note_ids });
                }
                if controllers.len() != item.controllers().len() {
                    actions.push(DawAction::SetMidiControllers {
                        item_id,
                        controllers,
                    });
                }
                if pitch_bends.len() != item.pitch_bends().len() {
                    actions.push(DawAction::SetMidiPitchBends {
                        item_id,
                        pitch_bends,
                    });
                }
                if program_changes.len() != item.program_changes().len() {
                    actions.push(DawAction::SetMidiProgramChanges {
                        item_id,
                        program_changes,
                    });
                }
                if poly_aftertouches.len() != item.poly_aftertouches().len() {
                    actions.push(DawAction::SetMidiPolyAftertouches {
                        item_id,
                        poly_aftertouches,
                    });
                }
                if channel_pressures.len() != item.channel_pressures().len() {
                    actions.push(DawAction::SetMidiChannelPressures {
                        item_id,
                        channel_pressures,
                    });
                }
                if opaque_events.len() != item.opaque_events().len() {
                    actions.push(DawAction::SetMidiOpaqueEvents {
                        item_id,
                        events: opaque_events,
                    });
                }
                if !actions.is_empty() {
                    self.apply_action(
                        DawAction::BatchTransaction {
                            tx_id: self.revision,
                            actions,
                        },
                        "MIDI events deleted",
                    );
                }
                self.midi_editor_selected_notes.clear();
                self.midi_editor_selected_controllers.clear();
                self.midi_editor_selected_pitch_bends.clear();
                self.midi_editor_selected_program_changes.clear();
                self.midi_editor_selected_poly_aftertouches.clear();
                self.midi_editor_selected_channel_pressures.clear();
                self.midi_editor_selected_opaque_events.clear();
                self.midi_editor_event_list_cursor = None;
                self.midi_editor_event_list_anchor = None;
            }
            Message::OpenMidiEventProperties(item_id) => {
                let target = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .and_then(|item| {
                        if !self.midi_editor_selected_notes.is_empty()
                            && self.midi_editor_selected_controllers.is_empty()
                            && self.midi_editor_selected_pitch_bends.is_empty()
                            && self.midi_editor_selected_program_changes.is_empty()
                            && self.midi_editor_selected_poly_aftertouches.is_empty()
                            && self.midi_editor_selected_channel_pressures.is_empty()
                            && self.midi_editor_selected_opaque_events.is_empty()
                        {
                            Some(MidiEventPropertiesTarget::Notes(
                                item.notes()
                                    .iter()
                                    .filter(|note| {
                                        self.midi_editor_selected_notes.contains(&note.id())
                                    })
                                    .map(|note| note.id())
                                    .collect(),
                            ))
                        } else if !self.midi_editor_selected_controllers.is_empty()
                            && self.midi_editor_selected_notes.is_empty()
                            && self.midi_editor_selected_pitch_bends.is_empty()
                            && self.midi_editor_selected_program_changes.is_empty()
                            && self.midi_editor_selected_poly_aftertouches.is_empty()
                            && self.midi_editor_selected_channel_pressures.is_empty()
                            && self.midi_editor_selected_opaque_events.is_empty()
                        {
                            Some(MidiEventPropertiesTarget::Controllers(
                                item.controllers()
                                    .iter()
                                    .filter(|event| {
                                        self.midi_editor_selected_controllers.contains(event)
                                    })
                                    .copied()
                                    .collect(),
                            ))
                        } else if !self.midi_editor_selected_pitch_bends.is_empty()
                            && self.midi_editor_selected_notes.is_empty()
                            && self.midi_editor_selected_controllers.is_empty()
                            && self.midi_editor_selected_program_changes.is_empty()
                            && self.midi_editor_selected_poly_aftertouches.is_empty()
                            && self.midi_editor_selected_channel_pressures.is_empty()
                            && self.midi_editor_selected_opaque_events.is_empty()
                        {
                            Some(MidiEventPropertiesTarget::PitchBends(
                                item.pitch_bends()
                                    .iter()
                                    .filter(|event| {
                                        self.midi_editor_selected_pitch_bends.contains(event)
                                    })
                                    .copied()
                                    .collect(),
                            ))
                        } else if !self.midi_editor_selected_program_changes.is_empty()
                            && self.midi_editor_selected_notes.is_empty()
                            && self.midi_editor_selected_controllers.is_empty()
                            && self.midi_editor_selected_pitch_bends.is_empty()
                            && self.midi_editor_selected_poly_aftertouches.is_empty()
                            && self.midi_editor_selected_channel_pressures.is_empty()
                            && self.midi_editor_selected_opaque_events.is_empty()
                        {
                            Some(MidiEventPropertiesTarget::ProgramChanges(
                                item.program_changes()
                                    .iter()
                                    .filter(|event| {
                                        self.midi_editor_selected_program_changes.contains(event)
                                    })
                                    .copied()
                                    .collect(),
                            ))
                        } else if !self.midi_editor_selected_poly_aftertouches.is_empty()
                            && self.midi_editor_selected_notes.is_empty()
                            && self.midi_editor_selected_controllers.is_empty()
                            && self.midi_editor_selected_pitch_bends.is_empty()
                            && self.midi_editor_selected_program_changes.is_empty()
                            && self.midi_editor_selected_channel_pressures.is_empty()
                            && self.midi_editor_selected_opaque_events.is_empty()
                        {
                            Some(MidiEventPropertiesTarget::PolyAftertouches(
                                item.poly_aftertouches()
                                    .iter()
                                    .filter(|event| {
                                        self.midi_editor_selected_poly_aftertouches.contains(event)
                                    })
                                    .copied()
                                    .collect(),
                            ))
                        } else if !self.midi_editor_selected_channel_pressures.is_empty()
                            && self.midi_editor_selected_notes.is_empty()
                            && self.midi_editor_selected_controllers.is_empty()
                            && self.midi_editor_selected_pitch_bends.is_empty()
                            && self.midi_editor_selected_program_changes.is_empty()
                            && self.midi_editor_selected_poly_aftertouches.is_empty()
                            && self.midi_editor_selected_opaque_events.is_empty()
                        {
                            Some(MidiEventPropertiesTarget::ChannelPressures(
                                item.channel_pressures()
                                    .iter()
                                    .filter(|event| {
                                        self.midi_editor_selected_channel_pressures.contains(event)
                                    })
                                    .copied()
                                    .collect(),
                            ))
                        } else if self.midi_editor_selected_opaque_events.len() == 1
                            && self.midi_editor_selected_notes.is_empty()
                            && self.midi_editor_selected_controllers.is_empty()
                            && self.midi_editor_selected_pitch_bends.is_empty()
                            && self.midi_editor_selected_program_changes.is_empty()
                            && self.midi_editor_selected_poly_aftertouches.is_empty()
                            && self.midi_editor_selected_channel_pressures.is_empty()
                        {
                            let index = self
                                .midi_editor_selected_opaque_events
                                .iter()
                                .next()
                                .copied()?;
                            let event = item.opaque_events().get(index as usize)?.clone();
                            midi_opaque_event_properties_type_and_payload(&event.kind)?;
                            Some(MidiEventPropertiesTarget::Opaque(index, event))
                        } else if (self.midi_editor_event_list_cursor
                            == Some(MidiEditorEventListCursor::End)
                            || view::midi_event_list_cursors(self, item).as_slice()
                                == [MidiEditorEventListCursor::End])
                            && self.midi_editor_selected_notes.is_empty()
                            && self.midi_editor_selected_controllers.is_empty()
                            && self.midi_editor_selected_pitch_bends.is_empty()
                            && self.midi_editor_selected_program_changes.is_empty()
                            && self.midi_editor_selected_poly_aftertouches.is_empty()
                            && self.midi_editor_selected_channel_pressures.is_empty()
                            && self.midi_editor_selected_opaque_events.is_empty()
                        {
                            Some(MidiEventPropertiesTarget::Terminal)
                        } else {
                            None
                        }
                    });
                if self.midi_editor_item_id == Some(item_id)
                    && let Some(target) = target
                {
                    self.midi_editor_feedback = None;
                    self.midi_quantize_dialog = None;
                    self.midi_humanize_dialog = None;
                    self.midi_transpose_dialog = None;
                    if matches!(&target, MidiEventPropertiesTarget::Terminal) {
                        self.midi_editor_event_list_cursor = Some(MidiEditorEventListCursor::End);
                        self.midi_editor_event_list_anchor = Some(MidiEditorEventListCursor::End);
                    }
                    self.midi_event_properties_dialog =
                        self.midi_event_properties_state(item_id, target);
                    self.reset_midi_event_properties_controller_options();
                }
            }
            Message::SetMidiEventPropertiesPosition(value) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut() {
                    dialog.position = value;
                    dialog.position_mixed = false;
                }
            }
            Message::SetMidiEventPropertiesLength(value) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut() {
                    dialog.length = value;
                    dialog.length_mixed = false;
                }
            }
            Message::SetMidiEventPropertiesChannel(value) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut() {
                    dialog.channel = value;
                    dialog.channel_mixed = false;
                }
            }
            Message::SetMidiEventPropertiesParameter(value) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut() {
                    dialog.parameter = value;
                    dialog.parameter_mixed = false;
                }
            }
            Message::SetMidiEventPropertiesValue(value) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut() {
                    dialog.value = value;
                    dialog.value_mixed = false;
                }
            }
            Message::SetMidiEventPropertiesOffValue(value) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut() {
                    dialog.off_value = value;
                    dialog.off_value_mixed = false;
                }
            }
            Message::SetMidiEventPropertiesBankMsb(value) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut() {
                    dialog.bank_msb = value;
                    dialog.bank_msb_mixed = false;
                    dialog.bank_msb_dirty = true;
                }
            }
            Message::SetMidiEventPropertiesBankLsb(value) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut() {
                    dialog.bank_lsb = value;
                    dialog.bank_lsb_mixed = false;
                    dialog.bank_lsb_dirty = true;
                }
            }
            Message::SetMidiOpaqueEventPropertiesType(event_type) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut()
                    && dialog.opaque_type.is_some()
                    && event_type.meta_kind().is_some()
                {
                    dialog.opaque_type = Some(event_type);
                    dialog.opaque_type_dirty = true;
                }
            }
            Message::SetMidiOpaqueEventPropertiesPayload(value) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut()
                    && dialog.opaque_type.is_some()
                {
                    dialog.opaque_payload = value;
                    dialog.opaque_editor =
                        iced::widget::text_editor::Content::with_text(&dialog.opaque_payload);
                    dialog.opaque_payload_dirty = true;
                }
            }
            Message::SetMidiOpaqueEventPropertiesMultiline(multiline) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut()
                    && dialog.opaque_type.is_some_and(|event_type| {
                        event_type != MidiOpaqueEventPropertiesType::SysEx
                            && event_type != MidiOpaqueEventPropertiesType::RawMeta
                    })
                {
                    if multiline {
                        dialog.opaque_editor =
                            iced::widget::text_editor::Content::with_text(&dialog.opaque_payload);
                    } else {
                        dialog.opaque_payload = dialog.opaque_editor.text();
                    }
                    dialog.opaque_multiline = multiline;
                }
            }
            Message::EditMidiOpaqueEventPropertiesText(action) => {
                if let Some(dialog) = self.midi_event_properties_dialog.as_mut()
                    && dialog.opaque_type.is_some_and(|event_type| {
                        dialog.opaque_multiline
                            || event_type == MidiOpaqueEventPropertiesType::SysEx
                            || event_type == MidiOpaqueEventPropertiesType::RawMeta
                            || midi_opaque_event_properties_dialog_payload_is_hex(dialog)
                    })
                {
                    dialog.opaque_editor.perform(action);
                    dialog.opaque_payload = dialog.opaque_editor.text();
                    dialog.opaque_payload_dirty = true;
                }
            }
            Message::PickMidiOpaqueSysExFile(save) => {
                task = self.pick_midi_opaque_sysex_file(save);
            }
            Message::MidiOpaqueSysExFileFinished(save, result) => {
                self.path_picker_busy = false;
                match result {
                    Ok(Some(_payload)) if save => {
                        self.midi_editor_feedback = Some("SysEx data saved".to_owned());
                    }
                    Ok(Some(payload)) => {
                        if let Some(dialog) = self.midi_event_properties_dialog.as_mut()
                            && dialog.opaque_type == Some(MidiOpaqueEventPropertiesType::SysEx)
                        {
                            dialog.opaque_payload = payload;
                            dialog.opaque_editor = iced::widget::text_editor::Content::with_text(
                                &dialog.opaque_payload,
                            );
                            dialog.opaque_payload_dirty = true;
                            self.midi_editor_feedback = None;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => self.midi_editor_feedback = Some(error),
                }
            }
            Message::PickMidiOpaqueMetaTextFile => {
                task = self.pick_midi_opaque_meta_text_file();
            }
            Message::MidiOpaqueMetaTextFileFinished(result) => {
                self.path_picker_busy = false;
                match result {
                    Ok(Some(payload)) => {
                        if let Some(dialog) = self.midi_event_properties_dialog.as_mut()
                            && dialog.opaque_type.is_some_and(|event_type| {
                                event_type != MidiOpaqueEventPropertiesType::SysEx
                                    && event_type != MidiOpaqueEventPropertiesType::RawMeta
                            })
                        {
                            dialog.opaque_multiline =
                                payload.contains('\n') || payload.contains('\r');
                            dialog.opaque_payload = payload;
                            dialog.opaque_editor = iced::widget::text_editor::Content::with_text(
                                &dialog.opaque_payload,
                            );
                            dialog.opaque_payload_dirty = true;
                            self.midi_editor_feedback = None;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => self.midi_editor_feedback = Some(error),
                }
            }
            Message::ApplyMidiEventProperties => {
                task = self.commit_midi_event_properties(false);
            }
            Message::AcceptMidiEventProperties => {
                task = self.commit_midi_event_properties(true);
            }
            Message::CancelMidiEventProperties => {
                self.midi_event_properties_dialog = None;
                self.midi_editor_feedback = None;
            }
            Message::SelectMidiEditorChannel(selection) => {
                self.midi_editor_channel_selection = selection;
                match selection {
                    MidiEditorChannelSelection::AllChannels => self.midi_editor_draw_channel = 0,
                    MidiEditorChannelSelection::Channel(channel) => {
                        let channel = channel.min(15);
                        self.midi_editor_channel_selection =
                            MidiEditorChannelSelection::Channel(channel);
                        self.midi_editor_draw_channel = channel;
                    }
                    MidiEditorChannelSelection::Multichannel => {
                        if self.midi_editor_filter_channels == 0 {
                            self.midi_editor_filter_channels =
                                1_u16 << self.midi_editor_draw_channel.min(15);
                        }
                        self.midi_editor_filter_channels_initialized = true;
                        self.midi_editor_filter_dialog_open = true;
                    }
                }
            }
            Message::SetMidiEditorDrawChannel(channel) => {
                self.midi_editor_draw_channel = channel.min(15);
            }
            Message::ToggleMidiEditorChannelFilter => {
                self.midi_editor_filter_enabled = !self.midi_editor_filter_enabled;
            }
            Message::ToggleMidiEditorEventFilter => {
                self.midi_editor_event_filter_enabled = !self.midi_editor_event_filter_enabled;
                if self.midi_editor_event_filter_enabled {
                    self.midi_editor_event_list_filter_active = true;
                }
            }
            Message::ToggleMidiEditorEventListFilter => {
                self.midi_editor_event_list_filter_active =
                    !self.midi_editor_event_list_filter_active;
                if !self.midi_editor_event_list_filter_active {
                    self.midi_editor_event_filter_enabled = false;
                }
            }
            Message::SetMidiEditorFilterNotePitchEnabled(enabled) => {
                self.midi_editor_note_filter.pitch_enabled = enabled;
            }
            Message::SetMidiEditorFilterNotePitchLow(value) => {
                self.midi_editor_note_filter.pitch_range_initialized = true;
                self.midi_editor_note_filter.pitch_low =
                    value.min(127).min(self.midi_editor_note_filter.pitch_high);
            }
            Message::SetMidiEditorFilterNotePitchHigh(value) => {
                self.midi_editor_note_filter.pitch_range_initialized = true;
                self.midi_editor_note_filter.pitch_high =
                    value.min(127).max(self.midi_editor_note_filter.pitch_low);
            }
            Message::SetMidiEditorFilterPitchBendEnabled(enabled) => {
                self.midi_editor_pitch_bend_filter.enabled = enabled;
            }
            Message::SetMidiEditorFilterProgramEnabled(enabled) => {
                self.midi_editor_program_change_filter.enabled = enabled;
            }
            Message::SetMidiEditorFilterProgramLow(value) => {
                self.midi_editor_program_change_filter.range_initialized = true;
                self.midi_editor_program_change_filter.low = value
                    .min(127)
                    .min(self.midi_editor_program_change_filter.high);
            }
            Message::SetMidiEditorFilterProgramHigh(value) => {
                self.midi_editor_program_change_filter.range_initialized = true;
                self.midi_editor_program_change_filter.high = value
                    .min(127)
                    .max(self.midi_editor_program_change_filter.low);
            }
            Message::SetMidiEditorFilterNoteVelocityEnabled(enabled) => {
                self.midi_editor_note_filter.velocity_enabled = enabled;
            }
            Message::SetMidiEditorFilterNoteVelocityLow(value) => {
                self.midi_editor_note_filter.velocity_range_initialized = true;
                self.midi_editor_note_filter.velocity_low = value
                    .min(127)
                    .min(self.midi_editor_note_filter.velocity_high);
            }
            Message::SetMidiEditorFilterNoteVelocityHigh(value) => {
                self.midi_editor_note_filter.velocity_range_initialized = true;
                self.midi_editor_note_filter.velocity_high = value
                    .min(127)
                    .max(self.midi_editor_note_filter.velocity_low);
            }
            Message::SetMidiEditorFilterPositionEnabled(enabled) => {
                self.midi_editor_position_filter.enabled = enabled;
            }
            Message::SetMidiEditorFilterPositionLow(value) => {
                self.midi_editor_filter_position_low = value;
                self.midi_editor_position_filter.range_initialized = false;
                if let Some(position) =
                    parse_midi_editor_filter_position(&self.midi_editor_filter_position_low)
                {
                    self.midi_editor_position_filter.low =
                        position.min(self.midi_editor_position_filter.high);
                    self.midi_editor_position_filter.range_initialized = true;
                }
            }
            Message::SetMidiEditorFilterPositionHigh(value) => {
                self.midi_editor_filter_position_high = value;
                self.midi_editor_position_filter.range_initialized = false;
                if let Some(position) =
                    parse_midi_editor_filter_position(&self.midi_editor_filter_position_high)
                {
                    self.midi_editor_position_filter.high =
                        position.max(self.midi_editor_position_filter.low);
                    self.midi_editor_position_filter.range_initialized = true;
                }
            }
            Message::SetMidiEditorFilterNoteLengthEnabled(enabled) => {
                self.midi_editor_note_filter.length_enabled = enabled;
            }
            Message::SetMidiEditorFilterNoteLengthLow(value) => {
                self.midi_editor_filter_note_length_low = value;
                self.midi_editor_note_filter.length_range_initialized = false;
                if let Some(length) = parse_midi_editor_filter_length(
                    &self.midi_editor_filter_note_length_low,
                    self.project.settings().ppq(),
                ) {
                    self.midi_editor_note_filter.length_low_ticks = length;
                    self.midi_editor_note_filter.length_range_initialized = true;
                }
            }
            Message::SetMidiEditorFilterNoteLengthHigh(value) => {
                self.midi_editor_filter_note_length_high = value;
                self.midi_editor_note_filter.length_range_initialized = false;
                if let Some(length) = parse_midi_editor_filter_length(
                    &self.midi_editor_filter_note_length_high,
                    self.project.settings().ppq(),
                ) {
                    self.midi_editor_note_filter.length_high_ticks = length;
                    self.midi_editor_note_filter.length_range_initialized = true;
                }
            }
            Message::SetMidiEditorFilterControllerEnabled(enabled) => {
                self.midi_editor_controller_filter.controller_enabled = enabled;
            }
            Message::SetMidiEditorFilterController(controller) => {
                self.midi_editor_controller_filter.controller = controller.min(127);
            }
            Message::SetMidiEditorFilterValueEnabled(enabled) => {
                self.midi_editor_controller_filter.value_enabled = enabled;
            }
            Message::SetMidiEditorFilterValueLow(value) => {
                self.midi_editor_filter_value_low = value;
                if self.midi_editor_event_type_filter == MidiEditorEventTypeFilter::Pitch {
                    self.midi_editor_pitch_bend_filter.range_initialized = false;
                    if let Some(value) = parse_midi_editor_filter_pitch_bend_value(
                        &self.midi_editor_filter_value_low,
                    ) {
                        self.midi_editor_pitch_bend_filter.low =
                            value.min(self.midi_editor_pitch_bend_filter.high);
                        self.midi_editor_pitch_bend_filter.range_initialized = true;
                    }
                } else {
                    self.midi_editor_controller_filter.value_range_initialized = false;
                    if let Some(value) =
                        parse_midi_editor_filter_value(&self.midi_editor_filter_value_low)
                    {
                        self.midi_editor_controller_filter.value_low =
                            value.min(self.midi_editor_controller_filter.value_high);
                        self.midi_editor_controller_filter.value_range_initialized = true;
                    }
                }
            }
            Message::SetMidiEditorFilterValueHigh(value) => {
                self.midi_editor_filter_value_high = value;
                if self.midi_editor_event_type_filter == MidiEditorEventTypeFilter::Pitch {
                    self.midi_editor_pitch_bend_filter.range_initialized = false;
                    if let Some(value) = parse_midi_editor_filter_pitch_bend_value(
                        &self.midi_editor_filter_value_high,
                    ) {
                        self.midi_editor_pitch_bend_filter.high =
                            value.max(self.midi_editor_pitch_bend_filter.low);
                        self.midi_editor_pitch_bend_filter.range_initialized = true;
                    }
                } else {
                    self.midi_editor_controller_filter.value_range_initialized = false;
                    if let Some(value) =
                        parse_midi_editor_filter_value(&self.midi_editor_filter_value_high)
                    {
                        self.midi_editor_controller_filter.value_high =
                            value.max(self.midi_editor_controller_filter.value_low);
                        self.midi_editor_controller_filter.value_range_initialized = true;
                    }
                }
            }
            Message::SetMidiEditorFilterRepeatEvery(repeat_every) => {
                self.midi_editor_position_filter.repeat_every = repeat_every;
            }
            Message::ToggleMidiEditorFilterChannel(channel) if channel < 16 => {
                self.midi_editor_filter_channels ^= 1_u16 << channel;
                self.midi_editor_filter_channels_initialized = true;
            }
            Message::ToggleMidiEditorFilterChannel(_) => {}
            Message::SetMidiEditorFilterChannelsAll(all) => {
                self.midi_editor_filter_channels = if all { u16::MAX } else { 0 };
                self.midi_editor_filter_channels_initialized = true;
            }
            Message::ToggleMidiEditorFilterInvert => {
                self.midi_editor_filter_inverted = !self.midi_editor_filter_inverted;
            }
            Message::OpenMidiEditorFilter => {
                if !self.midi_editor_filter_channels_initialized {
                    self.midi_editor_filter_channels = u16::MAX;
                    self.midi_editor_filter_channels_initialized = true;
                }
                self.midi_editor_filter_dialog_open = true;
            }
            Message::SetMidiEditorEventTypeFilter(event_type_filter) => {
                self.midi_editor_event_type_filter = event_type_filter;
            }
            Message::SetMidiEditorFilterFromSelection => {
                self.set_midi_editor_filter_from_selection();
            }
            Message::ClearMidiEditorFilter => self.clear_midi_editor_filter(),
            Message::ApplyMidiEditorFilterSelection(operation) => {
                self.apply_midi_editor_filter_selection(operation);
            }
            Message::CloseMidiEditorFilter => self.midi_editor_filter_dialog_open = false,
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
                self.with_midi_editor_note_selection_history(note_ids);
            }
            Message::SelectMidiEditorEventSelection(cursor, additive) => {
                task = self.with_midi_editor_event_selection_history(cursor, additive);
            }
            Message::SelectMidiNoteEvent(note_id, additive) => {
                self.midi_editor_feedback = None;
                let visible_note_ids = self.visible_midi_note_ids();
                if !visible_note_ids.contains(&note_id) {
                    return task;
                }
                self.midi_editor_event_list_cursor = Some(MidiEditorEventListCursor::Note(note_id));
                if !additive || self.midi_editor_event_list_anchor.is_none() {
                    self.midi_editor_event_list_anchor =
                        Some(MidiEditorEventListCursor::Note(note_id));
                }
                if additive {
                    if !self.midi_editor_selected_notes.remove(&note_id) {
                        self.midi_editor_selected_notes.insert(note_id);
                    }
                } else {
                    self.midi_editor_selected_controllers.clear();
                    self.midi_editor_selected_pitch_bends.clear();
                    self.midi_editor_selected_program_changes.clear();
                    self.midi_editor_selected_poly_aftertouches.clear();
                    self.midi_editor_selected_channel_pressures.clear();
                    self.midi_editor_selected_opaque_events.clear();
                    self.midi_editor_selected_notes = HashSet::from([note_id]);
                }
            }
            Message::SelectMidiControllerEvent(item_id, event, additive)
                if self.midi_editor_item_id == Some(item_id) =>
            {
                self.midi_editor_feedback = None;
                self.midi_editor_event_list_cursor =
                    Some(MidiEditorEventListCursor::Controller(event));
                if !additive || self.midi_editor_event_list_anchor.is_none() {
                    self.midi_editor_event_list_anchor =
                        Some(MidiEditorEventListCursor::Controller(event));
                }
                if !additive {
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_selected_controllers.clear();
                    self.midi_editor_selected_pitch_bends.clear();
                    self.midi_editor_selected_program_changes.clear();
                    self.midi_editor_selected_poly_aftertouches.clear();
                    self.midi_editor_selected_channel_pressures.clear();
                    self.midi_editor_selected_opaque_events.clear();
                    self.midi_editor_selected_controllers.insert(event);
                } else if !self.midi_editor_selected_controllers.remove(&event) {
                    self.midi_editor_selected_controllers.insert(event);
                }
            }
            Message::SelectMidiBankSelectEvent(item_id, msb, lsb, additive)
                if self.midi_editor_item_id == Some(item_id) =>
            {
                let cursor = MidiEditorEventListCursor::BankSelect(msb, lsb);
                let visible = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .is_some_and(|item| {
                        view::midi_event_list_cursors(self, item).contains(&cursor)
                    });
                if !visible {
                    return task;
                }
                self.midi_editor_feedback = None;
                self.midi_editor_event_list_cursor = Some(cursor);
                if !additive || self.midi_editor_event_list_anchor.is_none() {
                    self.midi_editor_event_list_anchor = Some(cursor);
                }
                if !additive {
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_selected_controllers.clear();
                    self.midi_editor_selected_pitch_bends.clear();
                    self.midi_editor_selected_program_changes.clear();
                    self.midi_editor_selected_poly_aftertouches.clear();
                    self.midi_editor_selected_channel_pressures.clear();
                    self.midi_editor_selected_opaque_events.clear();
                    self.midi_editor_selected_controllers.extend([msb, lsb]);
                } else if self.midi_editor_selected_controllers.contains(&msb)
                    || self.midi_editor_selected_controllers.contains(&lsb)
                {
                    self.midi_editor_selected_controllers.remove(&msb);
                    self.midi_editor_selected_controllers.remove(&lsb);
                } else {
                    self.midi_editor_selected_controllers.extend([msb, lsb]);
                }
            }
            Message::SelectMidiPitchBendEvent(item_id, event, additive)
                if self.midi_editor_item_id == Some(item_id) =>
            {
                self.midi_editor_feedback = None;
                self.midi_editor_event_list_cursor =
                    Some(MidiEditorEventListCursor::PitchBend(event));
                if !additive || self.midi_editor_event_list_anchor.is_none() {
                    self.midi_editor_event_list_anchor =
                        Some(MidiEditorEventListCursor::PitchBend(event));
                }
                if !additive {
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_selected_controllers.clear();
                    self.midi_editor_selected_pitch_bends.clear();
                    self.midi_editor_selected_program_changes.clear();
                    self.midi_editor_selected_poly_aftertouches.clear();
                    self.midi_editor_selected_channel_pressures.clear();
                    self.midi_editor_selected_opaque_events.clear();
                    self.midi_editor_selected_pitch_bends.insert(event);
                } else if !self.midi_editor_selected_pitch_bends.remove(&event) {
                    self.midi_editor_selected_pitch_bends.insert(event);
                }
            }
            Message::SelectMidiProgramChangeEvent(item_id, event, additive)
                if self.midi_editor_item_id == Some(item_id) =>
            {
                self.midi_editor_feedback = None;
                self.midi_editor_event_list_cursor =
                    Some(MidiEditorEventListCursor::ProgramChange(event));
                if !additive || self.midi_editor_event_list_anchor.is_none() {
                    self.midi_editor_event_list_anchor =
                        Some(MidiEditorEventListCursor::ProgramChange(event));
                }
                if !additive {
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_selected_controllers.clear();
                    self.midi_editor_selected_pitch_bends.clear();
                    self.midi_editor_selected_program_changes.clear();
                    self.midi_editor_selected_poly_aftertouches.clear();
                    self.midi_editor_selected_channel_pressures.clear();
                    self.midi_editor_selected_opaque_events.clear();
                    self.midi_editor_selected_program_changes.insert(event);
                } else if !self.midi_editor_selected_program_changes.remove(&event) {
                    self.midi_editor_selected_program_changes.insert(event);
                }
            }
            Message::SelectMidiPolyAftertouchEvent(item_id, event, additive)
                if self.midi_editor_item_id == Some(item_id) =>
            {
                let cursor = MidiEditorEventListCursor::PolyAftertouch(event);
                let visible = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .is_some_and(|item| {
                        view::midi_event_list_cursors(self, item).contains(&cursor)
                    });
                if !visible {
                    return task;
                }
                self.midi_editor_feedback = None;
                self.midi_editor_event_list_cursor = Some(cursor);
                if !additive || self.midi_editor_event_list_anchor.is_none() {
                    self.midi_editor_event_list_anchor = Some(cursor);
                }
                if !additive {
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_selected_controllers.clear();
                    self.midi_editor_selected_pitch_bends.clear();
                    self.midi_editor_selected_program_changes.clear();
                    self.midi_editor_selected_poly_aftertouches.clear();
                    self.midi_editor_selected_channel_pressures.clear();
                    self.midi_editor_selected_opaque_events.clear();
                    self.midi_editor_selected_poly_aftertouches.insert(event);
                } else if !self.midi_editor_selected_poly_aftertouches.remove(&event) {
                    self.midi_editor_selected_poly_aftertouches.insert(event);
                }
            }
            Message::SelectMidiChannelPressureEvent(item_id, event, additive)
                if self.midi_editor_item_id == Some(item_id) =>
            {
                let cursor = MidiEditorEventListCursor::ChannelPressure(event);
                let visible = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .is_some_and(|item| {
                        view::midi_event_list_cursors(self, item).contains(&cursor)
                    });
                if !visible {
                    return task;
                }
                self.midi_editor_feedback = None;
                self.midi_editor_event_list_cursor = Some(cursor);
                if !additive || self.midi_editor_event_list_anchor.is_none() {
                    self.midi_editor_event_list_anchor = Some(cursor);
                }
                if !additive {
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_selected_controllers.clear();
                    self.midi_editor_selected_pitch_bends.clear();
                    self.midi_editor_selected_program_changes.clear();
                    self.midi_editor_selected_poly_aftertouches.clear();
                    self.midi_editor_selected_channel_pressures.clear();
                    self.midi_editor_selected_opaque_events.clear();
                    self.midi_editor_selected_channel_pressures.insert(event);
                } else if !self.midi_editor_selected_channel_pressures.remove(&event) {
                    self.midi_editor_selected_channel_pressures.insert(event);
                }
            }
            Message::SelectMidiOpaqueEvent(item_id, index, additive)
                if self.midi_editor_item_id == Some(item_id) =>
            {
                let cursor = MidiEditorEventListCursor::Opaque(index);
                let visible = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .is_some_and(|item| {
                        item.opaque_events().get(index as usize).is_some()
                            && view::midi_event_list_cursors(self, item).contains(&cursor)
                    });
                if !visible {
                    return task;
                }
                self.midi_editor_feedback = None;
                self.midi_editor_event_list_cursor = Some(cursor);
                if !additive || self.midi_editor_event_list_anchor.is_none() {
                    self.midi_editor_event_list_anchor = Some(cursor);
                }
                if !additive {
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_selected_controllers.clear();
                    self.midi_editor_selected_pitch_bends.clear();
                    self.midi_editor_selected_program_changes.clear();
                    self.midi_editor_selected_poly_aftertouches.clear();
                    self.midi_editor_selected_channel_pressures.clear();
                    self.midi_editor_selected_opaque_events.clear();
                    self.midi_editor_selected_opaque_events.insert(index);
                } else if !self.midi_editor_selected_opaque_events.remove(&index) {
                    self.midi_editor_selected_opaque_events.insert(index);
                }
            }
            Message::SelectMidiEditorEventListEnd(additive) => {
                self.midi_editor_event_list_cursor = Some(MidiEditorEventListCursor::End);
                if !additive || self.midi_editor_event_list_anchor.is_none() {
                    self.midi_editor_event_list_anchor = Some(MidiEditorEventListCursor::End);
                }
                if !additive {
                    self.midi_editor_selected_notes.clear();
                    self.midi_editor_selected_controllers.clear();
                    self.midi_editor_selected_pitch_bends.clear();
                    self.midi_editor_selected_program_changes.clear();
                    self.midi_editor_selected_poly_aftertouches.clear();
                    self.midi_editor_selected_channel_pressures.clear();
                    self.midi_editor_selected_opaque_events.clear();
                }
            }
            Message::SelectMidiControllerEvent(..)
            | Message::SelectMidiBankSelectEvent(..)
            | Message::SelectMidiPitchBendEvent(..)
            | Message::SelectMidiProgramChangeEvent(..)
            | Message::SelectMidiPolyAftertouchEvent(..)
            | Message::SelectMidiChannelPressureEvent(..)
            | Message::SelectMidiOpaqueEvent(..) => {}
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
                                channel: note.channel(),
                                pitch: note.pitch(),
                                tick: note.tick() - first_tick,
                                duration: note.duration(),
                                velocity: note.velocity(),
                                off_velocity: note.off_velocity(),
                            })
                            .collect();
                        self.midi_note_clipboard
                            .notes
                            .sort_by_key(|note| (note.tick, note.channel, note.pitch));
                        self.midi_note_clipboard.span_ticks = self
                            .midi_note_clipboard
                            .notes
                            .iter()
                            .map(|note| note.tick.saturating_add(note.duration))
                            .max()
                            .unwrap_or(0);
                        let copy_grid = if self.timeline.snap_enabled {
                            self.timeline
                                .midi_editor_snap_grid
                                .tick_interval(self.project.settings().ppq())
                                .unwrap_or(1)
                        } else {
                            1
                        };
                        let default_paste_tick =
                            self.midi_editor_edit_cursor_tick.unwrap_or_else(|| {
                                snap_tick_up(
                                    view::midi_item_local_tick(item, first_tick)
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
                let Some(source_start_tick) = view::midi_source_tick_for_local(item, tick) else {
                    let feedback = "Paste rejected: cursor is before the MIDI source start";
                    self.status = feedback.to_owned();
                    self.midi_editor_feedback = Some(feedback.to_owned());
                    return task;
                };
                let (item_source_start_tick, item_source_end_tick) = if item.is_loop_source() {
                    let source_start = item.source_offset_ticks();
                    (
                        source_start,
                        source_start.saturating_add(item.length_ticks()),
                    )
                } else {
                    view::midi_source_edit_bounds(item)
                };
                let notes = self
                    .midi_note_clipboard
                    .notes
                    .iter()
                    .map(|note| aaadaw_core::MidiNoteData {
                        tick: source_start_tick.saturating_add(note.tick),
                        ..*note
                    })
                    .collect::<Vec<_>>();
                if notes.iter().any(|note| {
                    note.tick < item_source_start_tick
                        || note.tick.saturating_add(note.duration) > item_source_end_tick
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
                let selection_before = self
                    .timeline
                    .inline_midi_note_selection
                    .get(&item_id)
                    .cloned()
                    .unwrap_or_default();
                let editor_selection_before = (self.midi_editor_item_id == Some(item_id))
                    .then(|| self.midi_editor_selection_state());
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
                    let copied_ids = item
                        .notes()
                        .iter()
                        .map(|note| note.id())
                        .filter(|id| !old_ids.contains(id))
                        .collect::<HashSet<_>>();
                    self.timeline
                        .inline_midi_note_selection
                        .insert(item_id, copied_ids.clone());
                    let editor_selection_after = editor_selection_before.as_ref().map(|before| {
                        let mut after = before.clone();
                        after.notes = copied_ids.clone();
                        after
                    });
                    if let Some(editor_selection_after) = editor_selection_after.as_ref() {
                        self.restore_midi_editor_selection_state(editor_selection_after);
                    }
                    if matches!(self.app_undo_history.last(), Some(AppHistoryEntry::Project)) {
                        self.app_undo_history.pop();
                        self.app_undo_history.push(
                            AppHistoryEntry::ProjectWithInlineMidiSelection(
                                InlineMidiSelectionHistory {
                                    item_id,
                                    before: selection_before,
                                    after: copied_ids,
                                    editor_before: editor_selection_before,
                                    editor_after: editor_selection_after,
                                    project_history_cursor: self.project.history_cursor(),
                                },
                            ),
                        );
                    }
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
            Message::TransposeSelectedMidiNotes(semitones) if semitones != 0 => {
                let Some(item_id) = self.midi_editor_item_id else {
                    return task;
                };
                let Some(item) = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                else {
                    return task;
                };
                let selected = &self.midi_editor_selected_notes;
                let mut edits = Vec::with_capacity(selected.len());
                for note in item
                    .notes()
                    .iter()
                    .filter(|note| selected.contains(&note.id()))
                {
                    let Some(pitch) = note.pitch().checked_add_signed(semitones) else {
                        return task;
                    };
                    edits.push((
                        note.id(),
                        MidiNoteData {
                            pitch,
                            channel: note.channel(),
                            tick: note.tick(),
                            duration: note.duration(),
                            velocity: note.velocity(),
                            off_velocity: note.off_velocity(),
                        },
                    ));
                }
                if !edits.is_empty() {
                    task = self.update(Message::EditMidiNotes(item_id, edits));
                }
            }
            Message::TransposeSelectedMidiNotes(_) => {}
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
                let before = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .map(|item| item.controllers().to_vec())
                    .unwrap_or_default();
                let mut selected = self.midi_editor_selected_controllers.clone();
                if before.len() == controllers.len() {
                    for (old, new) in before.iter().zip(&controllers) {
                        if old != new && selected.remove(old) {
                            selected.insert(*new);
                        }
                    }
                }
                selected.retain(|event| controllers.contains(event));
                let previous_revision = self.revision;
                self.apply_action(
                    DawAction::SetMidiControllers {
                        item_id,
                        controllers,
                    },
                    "MIDI controller lane edited",
                );
                if self.revision != previous_revision && self.midi_editor_item_id == Some(item_id) {
                    self.midi_editor_selected_controllers = selected;
                }
            }
            Message::SetMidiPitchBends(item_id, pitch_bends) => {
                self.midi_editor_feedback = None;
                let before = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .map(|item| item.pitch_bends().to_vec())
                    .unwrap_or_default();
                let mut selected = self.midi_editor_selected_pitch_bends.clone();
                if before.len() == pitch_bends.len() {
                    for (old, new) in before.iter().zip(&pitch_bends) {
                        if old != new && selected.remove(old) {
                            selected.insert(*new);
                        }
                    }
                }
                selected.retain(|event| pitch_bends.contains(event));
                let previous_revision = self.revision;
                self.apply_action(
                    DawAction::SetMidiPitchBends {
                        item_id,
                        pitch_bends,
                    },
                    "MIDI pitch-bend lane edited",
                );
                if self.revision != previous_revision && self.midi_editor_item_id == Some(item_id) {
                    self.midi_editor_selected_pitch_bends = selected;
                }
            }
            Message::SetMidiProgramChanges(item_id, program_changes) => {
                self.midi_editor_feedback = None;
                let before = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .map(|item| item.program_changes().to_vec())
                    .unwrap_or_default();
                let mut selected = self.midi_editor_selected_program_changes.clone();
                if before.len() == program_changes.len() {
                    for (old, new) in before.iter().zip(&program_changes) {
                        if old != new && selected.remove(old) {
                            selected.insert(*new);
                        }
                    }
                }
                selected.retain(|event| program_changes.contains(event));
                let previous_revision = self.revision;
                self.apply_action(
                    DawAction::SetMidiProgramChanges {
                        item_id,
                        program_changes,
                    },
                    "MIDI program changes edited",
                );
                if self.revision != previous_revision && self.midi_editor_item_id == Some(item_id) {
                    self.midi_editor_selected_program_changes = selected;
                }
            }
            Message::SetMidiPolyAftertouches(item_id, poly_aftertouches) => {
                self.midi_editor_feedback = None;
                let before = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .map(|item| item.poly_aftertouches().to_vec())
                    .unwrap_or_default();
                let mut selected = self.midi_editor_selected_poly_aftertouches.clone();
                if before.len() == poly_aftertouches.len() {
                    for (old, new) in before.iter().zip(&poly_aftertouches) {
                        if old != new && selected.remove(old) {
                            selected.insert(*new);
                        }
                    }
                }
                selected.retain(|event| poly_aftertouches.contains(event));
                let previous_revision = self.revision;
                self.apply_action(
                    DawAction::SetMidiPolyAftertouches {
                        item_id,
                        poly_aftertouches,
                    },
                    "MIDI Poly Aftertouch events edited",
                );
                if self.revision != previous_revision && self.midi_editor_item_id == Some(item_id) {
                    self.midi_editor_selected_poly_aftertouches = selected;
                }
            }
            Message::SetMidiChannelPressures(item_id, channel_pressures) => {
                self.midi_editor_feedback = None;
                let before = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .map(|item| item.channel_pressures().to_vec())
                    .unwrap_or_default();
                let mut selected = self.midi_editor_selected_channel_pressures.clone();
                if before.len() == channel_pressures.len() {
                    for (old, new) in before.iter().zip(&channel_pressures) {
                        if old != new && selected.remove(old) {
                            selected.insert(*new);
                        }
                    }
                }
                selected.retain(|event| channel_pressures.contains(event));
                let previous_revision = self.revision;
                self.apply_action(
                    DawAction::SetMidiChannelPressures {
                        item_id,
                        channel_pressures,
                    },
                    "MIDI Channel Pressure events edited",
                );
                if self.revision != previous_revision && self.midi_editor_item_id == Some(item_id) {
                    self.midi_editor_selected_channel_pressures = selected;
                }
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
                if self.action_list_window_id == Some(window_id) {
                    self.action_list.results_viewport_height =
                        (size.height - action_list::RESULTS_VIEWPORT_CHROME_HEIGHT).max(1.0);
                }
                if self.fx_chain_window_id == Some(window_id) {
                    self.fx_chain_window_size = size;
                    self.resize_fx_editor_host();
                }
            }
            Message::AndroidAppFocusChanged(focused) => {
                self.android_app_unfocused = !focused;
            }
            Message::StartShortcutCapture(action_id) => {
                self.shortcut_capture_id = Some(action_id);
                self.shortcut_capture_append = false;
                self.shortcut_editor_feedback =
                    "Press a shortcut; Backspace clears it; Escape cancels".to_owned();
            }
            Message::AddShortcutBinding(action_id) => {
                self.shortcut_capture_id = Some(action_id);
                self.shortcut_capture_append = true;
                self.shortcut_editor_feedback =
                    "Press an additional shortcut; Escape cancels".to_owned();
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
            Message::SetRenderBounds(bounds) => {
                if self.render_dialog.source == RenderSource::RegionRenderMatrix
                    && !matches!(
                        bounds,
                        RenderBounds::AllProjectRegions | RenderBounds::SelectedProjectRegions
                    )
                {
                    self.render_dialog.bounds = RenderBounds::AllProjectRegions;
                    self.status =
                        "Region Render Matrix source uses project region bounds".to_owned();
                } else {
                    if bounds == RenderBounds::CustomTimeRange {
                        self.initialize_custom_render_range();
                    }
                    self.render_dialog.bounds = bounds;
                }
            }
            Message::SetRenderCustomStart(value) => {
                self.render_dialog.custom_start_text = value;
            }
            Message::SetRenderCustomEnd(value) => {
                self.render_dialog.custom_end_text = value;
            }
            Message::SetRenderSampleRate(sample_rate) => {
                self.render_dialog.sample_rate = sample_rate;
            }
            Message::SetRenderSource(source) => {
                self.render_dialog.source = source;
                if source == RenderSource::RegionRenderMatrix {
                    self.render_dialog.bounds = RenderBounds::AllProjectRegions;
                }
            }
            Message::SetRenderStemsPreFader(enabled) => {
                if self.can_render_stems_pre_fader() {
                    self.render_dialog.stems_pre_fader = enabled;
                }
            }
            Message::SetRenderChannels(channels) => {
                self.render_dialog.channels = channels;
            }
            Message::SetRenderPeakNormalization(enabled) => {
                self.render_dialog.normalize_peak = enabled;
            }
            Message::SetRenderPeakTarget(value) => {
                self.render_dialog.normalize_peak_target_text = value;
            }
            Message::SetRenderTailEnabled(enabled) => {
                self.render_dialog.tail_enabled = enabled;
            }
            Message::SetRenderTailDuration(value) => {
                self.render_dialog.tail_duration_text = value;
            }
            Message::SetRenderFilenamePattern(value) => {
                self.render_dialog.filename_pattern = value;
            }
            Message::SetRenderWildcardAllowSlash(enabled) => {
                self.render_dialog.allow_slash_in_wildcard_substitutions = enabled;
                if let Err(error) = render_wildcard_preferences::save(enabled) {
                    self.status =
                        format!("Render wildcard preferences could not be saved: {error}");
                }
            }
            Message::ToggleRenderWildcardMenu => {
                self.render_dialog.wildcard_menu_open = !self.render_dialog.wildcard_menu_open;
            }
            Message::InsertRenderFilenameWildcard(wildcard) => {
                self.render_dialog
                    .filename_pattern
                    .push_str(wildcard.token());
                self.render_dialog.wildcard_menu_open = false;
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
                any(target_os = "windows", target_os = "macos", target_os = "android")
            ))]
            Message::CpalInputDevicesLoaded(result) => {
                self.finish_cpal_input_device_enumeration(result);
            }
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos", target_os = "android")
            ))]
            Message::RefreshCpalInputDevices => {
                task = self.refresh_cpal_input_devices();
            }
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos", target_os = "android")
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
            Message::OpenActionList => task = self.open_action_list(),
            Message::OpenActionMacroEditor => {
                self.settings_category = SettingsCategory::ActionMacros;
                task = self.open_settings();
            }
            Message::CloseActionList => task = self.close_action_list(),
            Message::ActionListQueryChanged(query) => {
                self.action_list.set_query(query);
                task = self.reset_action_list_results_scroll();
            }
            Message::ActionListResultsScrolled { offset, height } => {
                self.action_list.results_scroll_offset = offset;
                self.action_list.results_viewport_height = height;
            }
            Message::ActionListFilterSubmitted => {
                if self.action_list_selected_entry().is_none()
                    && let Some(id) = self
                        .action_list_entries()
                        .first()
                        .and_then(|entry| commands::stable_id(entry.id))
                {
                    self.action_list.select(id);
                }
                task = iced::advanced::widget::operate(
                    iced::advanced::widget::operation::focusable::unfocus(),
                );
            }
            Message::ActionListSelect(id) => self.action_list.select(id),
            Message::ActionListSelectBinding(index) => {
                self.action_list.selected_binding = Some(index)
            }
            Message::ActionListAddBinding => task = self.open_action_input(),
            Message::ActionInputSpecialKey => {
                if self.action_input_window_id.is_some()
                    && matches!(
                        self.action_list.capture,
                        Some(action_list::CaptureMode::Add)
                    )
                {
                    self.action_list.special_key_capture_pending = true;
                    self.action_list.input_draft = None;
                    self.action_list.feedback.clear();
                }
            }
            Message::ActionInputConfirm => task = self.confirm_action_input(),
            Message::ActionInputCancel => task = self.close_action_input(),
            Message::ActionListFindShortcut => {
                self.action_list.feedback.clear();
                self.action_list.capture = Some(action_list::CaptureMode::Find)
            }
            Message::ActionListDeleteBinding => self.delete_action_list_binding(),
            Message::ActionListRun(close) => task = self.run_action_list(close),
            Message::RuntimeKeyboardEvent(event, status, window_id) => {
                if let iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) =
                    &event
                {
                    self.keyboard_modifiers = *modifiers;
                    self.timeline.modifiers = *modifiers;
                }
                if self.pending_project_template_overwrite.is_some() {
                    if self.main_window_id == Some(window_id)
                        && status == iced::event::Status::Ignored
                        && let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                            key: iced::keyboard::Key::Named(key),
                            modifiers,
                            repeat: false,
                            ..
                        }) = &event
                        && modifiers.is_empty()
                    {
                        match key {
                            iced::keyboard::key::Named::Enter => {
                                return self.update(Message::ConfirmProjectTemplateOverwrite);
                            }
                            iced::keyboard::key::Named::Escape => {
                                return self.update(Message::CancelProjectTemplateOverwrite);
                            }
                            _ => {}
                        }
                    }
                    return Task::none();
                }
                if self.pending_focused_track_deletion.is_some() {
                    if self.main_window_id == Some(window_id)
                        && status == iced::event::Status::Ignored
                        && let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                            key: iced::keyboard::Key::Named(key),
                            repeat: false,
                            ..
                        }) = &event
                    {
                        match key {
                            iced::keyboard::key::Named::Enter => {
                                return self.update(Message::ConfirmFocusedTrackDeletion);
                            }
                            iced::keyboard::key::Named::Escape => {
                                return self.update(Message::CancelFocusedTrackDeletion);
                            }
                            _ => {}
                        }
                    }
                    return Task::none();
                }
                if self.project_settings_window_id == Some(window_id) {
                    if status == iced::event::Status::Ignored
                        && matches!(
                            &event,
                            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                                repeat: false,
                                ..
                            })
                        )
                    {
                        if let Some(draft) = &mut self.project_settings
                            && draft.menu_open
                        {
                            draft.menu_open = false;
                            draft.menu_epoch = draft.menu_epoch.wrapping_add(1);
                            return Task::none();
                        }
                        return self.close_project_settings();
                    }
                    if status == iced::event::Status::Ignored
                        && matches!(&event, iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {key:iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),modifiers,repeat:false,..}) if modifiers.is_empty())
                    {
                        if let Some(draft) = &mut self.project_settings
                            && draft.menu_open
                        {
                            draft.menu_open = false;
                            draft.menu_epoch = draft.menu_epoch.wrapping_add(1);
                            return Task::none();
                        }
                        return self.apply_project_settings();
                    }
                    return Task::none();
                }
                if self.item_nudge_window_id == Some(window_id) {
                    if status == iced::event::Status::Ignored
                        && let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                            key,
                            modifiers,
                            repeat: false,
                            ..
                        }) = &event
                    {
                        let toggle = {
                            let bindings = self
                                .shortcut_bindings
                                .read()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            commands::from_shortcut(
                                &key.as_ref(),
                                *modifiers,
                                &bindings,
                                &self.action_macros,
                            ) == Some(CommandId::OpenItemNudgeDialog)
                        };
                        if toggle {
                            return self.close_item_nudge();
                        }
                    }
                    if status == iced::event::Status::Ignored
                        && matches!(
                            &event,
                            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                                repeat: false,
                                ..
                            })
                        )
                    {
                        return self.close_item_nudge();
                    }
                    return Task::none();
                }
                if self.item_properties_window_id == Some(window_id) {
                    if status == iced::event::Status::Ignored
                        && let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                            key,
                            modifiers,
                            repeat: false,
                            ..
                        }) = &event
                    {
                        let toggle = {
                            let bindings = self
                                .shortcut_bindings
                                .read()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            commands::from_shortcut(
                                &key.as_ref(),
                                *modifiers,
                                &bindings,
                                &self.action_macros,
                            ) == Some(CommandId::ToggleItemProperties)
                        };
                        if toggle {
                            return self.close_item_properties();
                        }
                    }
                    if matches!(
                        &event,
                        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                            ..
                        })
                    ) {
                        return self.close_item_properties();
                    }
                    return Task::none();
                }
                if self.marker_properties_window_id == Some(window_id) {
                    if status == iced::event::Status::Ignored
                        && matches!(
                            &event,
                            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                                repeat: false,
                                ..
                            })
                        )
                    {
                        return self.close_marker_properties();
                    }
                    if status == iced::event::Status::Ignored
                        && matches!(
                            &event,
                            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
                                modifiers,
                                repeat: false,
                                ..
                            }) if modifiers.is_empty()
                        )
                    {
                        return self.apply_marker_properties();
                    }
                    return Task::none();
                }
                if self.region_properties_window_id == Some(window_id) {
                    if status == iced::event::Status::Ignored
                        && matches!(
                            &event,
                            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                                repeat: false,
                                ..
                            })
                        )
                    {
                        return self.close_region_properties();
                    }
                    if status == iced::event::Status::Ignored
                        && matches!(
                            &event,
                            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
                                modifiers,
                                repeat: false,
                                ..
                            }) if modifiers.is_empty()
                        )
                    {
                        return self.apply_region_properties();
                    }
                    return Task::none();
                }
                if self.marker_manager_window_id == Some(window_id) {
                    if status == iced::event::Status::Ignored
                        && matches!(
                            &event,
                            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                                repeat: false,
                                ..
                            })
                        )
                    {
                        return self.close_marker_manager();
                    }
                    if let Some(message) = keyboard_shortcut_event(
                        event,
                        status,
                        window_id,
                        Some(window_id),
                        None,
                        None,
                    ) {
                        return self.update(message);
                    }
                    return Task::none();
                }
                if self.region_render_matrix_window_id == Some(window_id) {
                    if status == iced::event::Status::Ignored
                        && matches!(
                            &event,
                            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                                repeat: false,
                                ..
                            })
                        )
                    {
                        return self.close_region_render_matrix();
                    }
                    return Task::none();
                }
                if self.routing_matrix_window_id == Some(window_id) {
                    if status == iced::event::Status::Ignored
                        && matches!(
                            &event,
                            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                                repeat: false,
                                ..
                            })
                        )
                    {
                        return self.close_routing_matrix();
                    }
                    if let Some(message) = keyboard_shortcut_event(
                        event,
                        status,
                        window_id,
                        Some(window_id),
                        None,
                        None,
                    ) {
                        return self.update(message);
                    }
                    return Task::none();
                }
                if self.routing_window_id == Some(window_id) {
                    if matches!(
                        event,
                        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                            ..
                        })
                    ) {
                        return self.close_track_routing();
                    }
                    if let Some(message) = keyboard_shortcut_event(
                        event,
                        status,
                        window_id,
                        Some(window_id),
                        None,
                        None,
                    ) {
                        return self.update(message);
                    }
                    return Task::none();
                }
                if self.action_input_window_id == Some(window_id) {
                    return self.action_list_keyboard_event(event, status);
                }
                if self.action_input_window_id.is_some() {
                    return Task::none();
                }
                if self.action_list_window_id == Some(window_id) {
                    return self.action_list_keyboard_event(event, status);
                }

                if self.midi_editor_window_id == Some(window_id)
                    && self.midi_editor_mode == MidiEditorMode::EventList
                    && self.midi_event_properties_dialog.is_none()
                    && self.midi_quantize_dialog.is_none()
                    && self.midi_humanize_dialog.is_none()
                    && self.midi_transpose_dialog.is_none()
                    && !self.midi_editor_filter_dialog_open
                    && status == iced::event::Status::Ignored
                    && let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                        key: iced::keyboard::Key::Named(key),
                        modifiers,
                        repeat: false,
                        ..
                    }) = &event
                {
                    if modifiers.is_empty()
                        && matches!(
                            key,
                            iced::keyboard::key::Named::Delete
                                | iced::keyboard::key::Named::Backspace
                        )
                        && let Some(item_id) = self.midi_editor_item_id
                    {
                        return self.update(Message::DeleteSelectedMidiEvents(item_id));
                    }
                    if modifiers.is_empty()
                        || *modifiers == iced::keyboard::Modifiers::CTRL
                        || *modifiers == iced::keyboard::Modifiers::SHIFT
                    {
                        let direction = match key {
                            iced::keyboard::key::Named::ArrowUp => {
                                Some(MidiEditorEventListCursorDirection::Previous)
                            }
                            iced::keyboard::key::Named::ArrowDown => {
                                Some(MidiEditorEventListCursorDirection::Next)
                            }
                            _ => None,
                        };
                        if let Some(direction) = direction {
                            return self.update(Message::MoveMidiEditorEventListCursor(
                                direction,
                                modifiers == &iced::keyboard::Modifiers::CTRL,
                                modifiers == &iced::keyboard::Modifiers::SHIFT,
                            ));
                        }
                    }
                    if modifiers.is_empty() {
                        let boundary = match key {
                            iced::keyboard::key::Named::Home => {
                                Some(MidiEditorCursorBoundary::FirstEvent)
                            }
                            iced::keyboard::key::Named::End => Some(MidiEditorCursorBoundary::End),
                            _ => None,
                        };
                        if let Some(boundary) = boundary {
                            return self.update(Message::MoveMidiEditorCursorToBoundary(boundary));
                        }
                    }
                }

                if self.shortcut_capture_id.is_some() {
                    if let Some(message) = keyboard_shortcut_event(
                        event,
                        status,
                        window_id,
                        self.main_window_id,
                        self.settings_window_id,
                        self.shortcut_capture_id.as_deref(),
                    ) {
                        return self.update(message);
                    }
                    return Task::none();
                }
                if self.main_window_id == Some(window_id)
                    && self.active_menu.is_none()
                    && status == iced::event::Status::Ignored
                    && let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                        key: iced::keyboard::Key::Named(iced::keyboard::key::Named::F10),
                        modifiers,
                        repeat: false,
                        ..
                    }) = &event
                    && *modifiers == iced::keyboard::Modifiers::NONE
                {
                    let bindings = self
                        .shortcut_bindings
                        .read()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let command = commands::from_shortcut(
                        &iced::keyboard::Key::Named(iced::keyboard::key::Named::F10),
                        *modifiers,
                        &bindings,
                        &self.action_macros,
                    );
                    drop(bindings);
                    if let Some(command) = command {
                        return self.update(Message::ExecuteCommand(command));
                    }
                }
                let message = menu_navigation_event(
                    &event,
                    status,
                    window_id,
                    self.main_window_id,
                    self.active_menu,
                )
                .or_else(|| {
                    mobile_back_event(
                        &event,
                        window_id,
                        self.main_window_id,
                        self.is_mobile_main_window() && self.mobile_panel != MobilePanel::Editor,
                    )
                })
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
                self.recent_projects_menu_open = false;
                self.project_templates_menu_open = false;
                self.offline_jobs_panel_open = false;
            }
            Message::ToggleOfflineJobsPanel => {
                self.offline_jobs_panel_open = !self.offline_jobs_panel_open;
            }
            Message::ToggleTransportDetails => {
                self.transport_details_open = !self.transport_details_open;
            }
            Message::Escape => {
                if self.pending_midi_file_import.take().is_some() {
                    self.midi_file_import_options = MidiFileImportOptions::default();
                    self.status = "MIDI file import cancelled".to_owned();
                    return Task::none();
                }
                if self.midi_event_properties_dialog.take().is_some() {
                    self.midi_editor_feedback = None;
                    return Task::none();
                }
                if self.midi_quantize_dialog.take().is_some()
                    || self.midi_humanize_dialog.take().is_some()
                    || self.midi_transpose_dialog.take().is_some()
                {
                    return Task::none();
                }
                if self.pending_empty_midi_export.is_some() {
                    self.dismiss_empty_midi_export();
                    return Task::none();
                }
                if self.pending_project_template_overwrite.take().is_some() {
                    self.status = "Project template overwrite cancelled".to_owned();
                    return Task::none();
                }
                if self.midi_export_window_id.is_some() {
                    return self.close_midi_export_dialog();
                }
                if self.pending_focused_track_deletion.take().is_some() {
                    return Task::none();
                }
                if self.timeline.context_fade.take().is_some() {
                    return Task::none();
                }
                if self.timeline.item_fade_preview().is_some() {
                    self.cancel_item_fades();
                    return Task::none();
                }
                if self.master_mix_gesture.is_some() {
                    self.cancel_master_mix();
                    return Task::none();
                }
                if !self.cancel_shortcut_capture() {
                    if self.io_busy
                        && self.pending_project_transition.is_some()
                        && self.pending_project_template_working_copy.is_some()
                    {
                        self.status =
                            "Wait for template opening to finish before cancelling".to_owned();
                        return Task::none();
                    }
                    if self.pending_project_transition.is_some() {
                        self.pending_project_transition = None;
                        self.pending_project_path_to_open = None;
                        self.pending_project_template_source = None;
                        self.pending_project_template_working_copy = None;
                    } else if self.offline_jobs_panel_open {
                        self.offline_jobs_panel_open = false;
                    } else if self.active_menu.take().is_some() {
                        self.menu_selected_command = None;
                        self.recent_projects_menu_open = false;
                        self.project_templates_menu_open = false;
                    } else if !self.cancel_active_track_draft() {
                        let dismissed_item_menu = self.timeline.context_item.take().is_some();
                        let dismissed_track_menu = self.timeline.context_track.take().is_some();
                        let dismissed_automation_menu =
                            self.timeline.context_automation_point.take().is_some();
                        if dismissed_item_menu || dismissed_track_menu || dismissed_automation_menu
                        {
                            self.timeline.context_item_position = None;
                            self.timeline.context_automation_position = None;
                        } else if self.is_mobile_main_window()
                            && self.mobile_panel != MobilePanel::Editor
                        {
                            self.navigate_back_mobile_panel();
                        } else {
                            self.timeline
                                .handle(timeline::TimelineEvent::ClearTimeSelectionAndLoopPoints);
                        }
                    }
                }
            }
            Message::NewProject => {
                task = self.begin_project_transition(PendingProjectTransition::NewProject)
            }
            Message::OpenRecentProject(path) => {
                task = self.open_recent_project(path);
            }
            Message::OpenProjectTemplate(path) => {
                task = self.open_project_template(path);
            }
            Message::ProjectTemplateCopied(source, result) => {
                self.io_busy = false;
                if self.pending_project_template_source.as_deref() != Some(source.as_path()) {
                    if let Ok((directory, project_path)) = result {
                        drop(project_templates::WorkingCopy {
                            directory,
                            project_path,
                        });
                    }
                } else {
                    match result {
                        Ok((directory, project_path)) => {
                            self.pending_project_template_working_copy =
                                Some(project_templates::WorkingCopy {
                                    directory,
                                    project_path: project_path.clone(),
                                });
                            self.pending_project_path_to_open = Some(project_path);
                            task = self.continue_open_project_transition();
                        }
                        Err(error) => {
                            self.pending_project_transition = None;
                            self.pending_project_path_to_open = None;
                            self.pending_project_template_source = None;
                            self.pending_project_template_working_copy = None;
                            self.status = format!("Could not prepare project template: {error}");
                        }
                    }
                }
            }
            Message::CloseProject => {
                task = self.begin_project_transition(PendingProjectTransition::CloseProject)
            }
            Message::Timeline(timeline::TimelineEvent::SetFadeCurvePreset {
                item_id,
                edge,
                shape,
            }) => {
                self.timeline.context_fade = None;
                if let Some(item) = self
                    .project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                {
                    let mut fades = item.fades();
                    let fade = match edge {
                        timeline::ItemFadeEdge::In => &mut fades.fade_in,
                        timeline::ItemFadeEdge::Out => &mut fades.fade_out,
                    };
                    *fade = aaadaw_core::AudioFade::with_curve(
                        fade.length_samples(),
                        aaadaw_core::FadeCurve::current_preset(shape),
                    )
                    .expect("validated existing fade length");
                    if fades != item.fades() {
                        self.apply_action(
                            DawAction::SetAudioItemFades { item_id, fades },
                            "Audio item fade curve changed",
                        );
                    }
                }
            }
            Message::Timeline(timeline::TimelineEvent::PreviewItemFades { item_id, fades }) => {
                if self
                    .project
                    .audio_items()
                    .iter()
                    .any(|item| item.id() == item_id)
                    && !self
                        .project
                        .tracks()
                        .iter()
                        .any(|track| track.frozen_audio_item_id() == Some(item_id))
                {
                    self.timeline.set_item_fade_preview(item_id, fades);
                    #[cfg(feature = "audio-device")]
                    if let Some(playback) = &self.playback {
                        let _ = playback.set_item_fades(item_id, fades);
                    }
                }
            }
            Message::Timeline(timeline::TimelineEvent::CommitItemFades) => {
                if let Some((item_id, fades)) = self.timeline.take_item_fade_preview()
                    && self
                        .project
                        .audio_items()
                        .iter()
                        .any(|item| item.id() == item_id && item.fades() != fades)
                {
                    self.apply_action(
                        DawAction::SetAudioItemFades { item_id, fades },
                        "Audio item fades changed",
                    );
                }
            }
            Message::Timeline(timeline::TimelineEvent::CancelItemFades) => self.cancel_item_fades(),
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
            Message::Timeline(timeline::TimelineEvent::EditProjectMarker { marker_id }) => {
                task = self.open_marker_properties(marker_id);
            }
            Message::Timeline(timeline::TimelineEvent::SelectHoveredItem(mode)) => {
                let label = match mode {
                    timeline::HoveredItemSelectionMode::Replace => "Select item under mouse cursor",
                    timeline::HoveredItemSelectionMode::Add => {
                        "Select item under mouse cursor (leaving other items selected)"
                    }
                    timeline::HoveredItemSelectionMode::Toggle => {
                        "Toggle selection of item under mouse cursor"
                    }
                };
                self.with_item_selection_history(label, |app| {
                    app.handle_timeline_view_event(timeline::TimelineEvent::SelectHoveredItem(mode))
                });
            }
            Message::Timeline(timeline::TimelineEvent::SetInlineMidiNoteSelection {
                item_id,
                note_ids,
            }) => {
                self.with_inline_midi_note_selection_history(item_id, note_ids);
            }
            Message::Timeline(event) => {
                #[cfg(feature = "audio-device")]
                let previous_loop_points = self.timeline.loop_points;
                self.handle_timeline_view_event(event);
                #[cfg(feature = "audio-device")]
                if previous_loop_points != self.timeline.loop_points
                    && self.repeat_enabled
                    && self.playback.is_some()
                    && !self.playback_busy
                {
                    self.repeat_reconfiguration_previous = Some((self.repeat_enabled, true));
                    self.playback_graph_dirty = true;
                    task = self.prepare_playback(self.playhead_sample, self.playback_playing);
                    if !self.playback_busy {
                        self.restore_repeat_after_failed_reconfiguration(
                            "updated loop points could not be prepared",
                        );
                    }
                }
            }
            Message::BeginTrackNameEdit(track_id) => task = self.begin_track_name_edit(track_id),
            Message::TcpScrolled { offset, height } => {
                let offset_changed = (offset - self.timeline.vertical_scroll).abs() > 0.5;
                self.timeline.set_vertical_scroll(offset);
                self.timeline.viewport_height = height;
                if offset_changed {
                    task = scroll_arrangement_to(timeline::TIMELINE_SCROLL_ID, offset);
                }
            }
            Message::TimelineScrolled { offset, height } => {
                let offset_changed = (offset - self.timeline.vertical_scroll).abs() > 0.5;
                self.timeline.set_vertical_scroll(offset);
                self.timeline.viewport_height = height;
                if offset_changed {
                    task = scroll_arrangement_to(timeline::TCP_SCROLL_ID, offset);
                }
            }
            #[cfg(feature = "audio-device")]
            Message::TogglePlayStop => {
                task = self.update(if self.recording.is_some() || self.recording_starting {
                    Message::StopRecording
                } else if self.playback_playing || self.playback_paused {
                    Message::StopPlayback
                } else {
                    Message::StartPlayback
                });
            }
            #[cfg(feature = "audio-device")]
            Message::TogglePlayback => {
                if self.playback_playing {
                    self.pause_playback();
                } else {
                    task = self.start_playback();
                }
            }
            #[cfg(feature = "audio-device")]
            Message::ToggleRepeat => task = self.toggle_repeat(),
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
            Message::OpenTrackRouting(track_id) => task = self.open_track_routing(track_id),
            Message::OpenTrackRoutingTo(source, destination) => {
                task = self.open_track_routing_to(source, destination)
            }
            Message::CloseTrackRouting => task = self.close_track_routing(),
            Message::RoutingChange(action) => {
                let _ = self.apply_routing_change(action);
            }
            Message::RoutingSendDraft(send_id, is_pan, value) => {
                self.set_routing_send_draft(send_id, is_pan, value)
            }
            Message::CommitRoutingSend(send_id) => self.commit_routing_send(send_id),
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
            Message::AddProjectMarker => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before inserting a marker"
                            .to_owned();
                } else {
                    self.apply_action(
                        DawAction::InsertProjectMarker {
                            position_ticks: self.timeline.edit_cursor_tick,
                            name: String::new(),
                        },
                        "Project marker inserted",
                    );
                }
            }
            Message::InsertRegionFromTimeSelection => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before inserting a region"
                            .to_owned();
                } else if let Some(selection) = self.timeline.time_selection {
                    if selection.start_tick < selection.end_tick {
                        self.apply_action(
                            DawAction::InsertProjectRegion {
                                start_ticks: selection.start_tick,
                                end_ticks: selection.end_tick,
                                name: String::new(),
                            },
                            "Project region inserted",
                        );
                    } else {
                        self.status = "Create a non-empty time selection before inserting a region"
                            .to_owned();
                    }
                } else {
                    self.status = "Create a time selection before inserting a region".to_owned();
                }
            }
            Message::DeleteProjectRegionNearCursor => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before deleting a region"
                            .to_owned();
                } else if let Some(region_id) = commands::project_region_at_cursor(self) {
                    self.apply_action(
                        DawAction::DeleteProjectRegion { region_id },
                        "Project region deleted",
                    );
                } else {
                    self.status = "No project region contains the edit cursor".to_owned();
                }
            }
            Message::EditProjectRegionNearCursor => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before editing a region"
                            .to_owned();
                } else if let Some(region_id) = commands::project_region_at_cursor(self) {
                    task = self.open_region_properties(region_id);
                } else {
                    self.status = "No project region contains the edit cursor".to_owned();
                }
            }
            Message::PreviousProjectMarker => {
                let cursor = self.timeline.edit_cursor_tick_position(&self.project);
                let target = self
                    .project
                    .markers()
                    .iter()
                    .rev()
                    .find(|marker| (marker.position_ticks() as f64) < cursor)
                    .map(|marker| marker.position_ticks())
                    .unwrap_or(0);
                if (target as f64) >= cursor {
                    self.status = "No previous project marker".to_owned();
                } else {
                    self.handle_timeline_view_event(timeline::TimelineEvent::SetEditCursor(target));
                    self.status = "Moved to previous project marker".to_owned();
                }
            }
            Message::NextProjectMarker => {
                let cursor = self
                    .timeline
                    .cursor_position()
                    .sample_position(&self.project)
                    .unwrap_or(f64::INFINITY);
                let cursor_tick = self.timeline.edit_cursor_tick_position(&self.project);
                let next_marker = self
                    .project
                    .markers()
                    .iter()
                    .find(|marker| (marker.position_ticks() as f64) > cursor_tick)
                    .map(|marker| timeline::EditCursorPosition::Tick(marker.position_ticks()));
                let project_end =
                    self.project
                        .audio_items()
                        .iter()
                        .map(|item| timeline::EditCursorPosition::Sample(item.end_sample()))
                        .chain(self.project.midi_items().iter().map(|item| {
                            timeline::EditCursorPosition::Tick(
                                item.start_tick().saturating_add(item.length_ticks()),
                            )
                        }))
                        .chain(self.project.markers().iter().map(|marker| {
                            timeline::EditCursorPosition::Tick(marker.position_ticks())
                        }))
                        .filter_map(|position| {
                            Some((position.sample_position(&self.project).ok()?, position))
                        })
                        .reduce(
                            |current, next| {
                                if next.0 > current.0 { next } else { current }
                            },
                        )
                        .map(|(_, position)| position);
                let target = next_marker.or(project_end);
                if target
                    .and_then(|position| {
                        position
                            .sample_position(&self.project)
                            .ok()
                            .map(|sample| (position, sample))
                    })
                    .is_none_or(|(_, sample)| sample <= cursor)
                {
                    self.status = "No next project marker or project end".to_owned();
                } else {
                    self.set_edit_cursor_position(target.expect("validated target is present"));
                    self.status = if next_marker.is_some() {
                        "Moved to next project marker".to_owned()
                    } else {
                        "Moved to project end".to_owned()
                    };
                }
            }
            Message::DeleteProjectMarkerNearCursor => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before deleting a marker"
                            .to_owned();
                } else if let Some(marker_id) = commands::nearest_project_marker_to_delete(self) {
                    self.apply_action(
                        DawAction::DeleteProjectMarker { marker_id },
                        "Project marker deleted",
                    );
                } else {
                    self.status =
                        "No project marker less than 1 second from the edit cursor".to_owned();
                }
            }
            Message::MoveProjectMarkerSlot(slot) => {
                if !(1..=10).contains(&slot) {
                    self.status = "Numbered marker slot must be between 1 and 10".to_owned();
                } else if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before moving a marker"
                            .to_owned();
                } else {
                    let position_ticks = self.timeline.edit_cursor_tick;
                    let existing = self
                        .project
                        .markers()
                        .iter()
                        .find(|marker| marker.number() == u32::from(slot))
                        .map(|marker| (marker.id(), marker.name().to_owned()));
                    if let Some((marker_id, name)) = existing {
                        self.apply_action(
                            DawAction::UpdateProjectMarker {
                                marker_id,
                                position_ticks,
                                name,
                            },
                            "Project marker moved to edit cursor",
                        );
                    } else {
                        self.apply_action(
                            DawAction::InsertNumberedProjectMarker {
                                number: u32::from(slot),
                                position_ticks,
                                name: String::new(),
                            },
                            "Project marker added at edit cursor",
                        );
                    }
                }
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
            Message::QuantizeSelectedMidiNotes(item_id) => {
                let action = DawAction::QuantizeMidiNotes {
                    item_id,
                    note_ids: self.midi_editor_selected_notes.iter().copied().collect(),
                    grid: midi_editor_grid_fraction(self.timeline.midi_editor_snap_grid),
                    strength: 1.0,
                };
                self.apply_action(action, "Selected MIDI notes quantized to editor grid");
            }
            Message::QuantizeSelectedMidiNotesAndEnds(item_id) => {
                let action = DawAction::QuantizeMidiNotesAndEnds {
                    item_id,
                    note_ids: self.midi_editor_selected_notes.iter().copied().collect(),
                    grid: midi_editor_grid_fraction(self.timeline.midi_editor_snap_grid),
                    strength: 1.0,
                };
                self.apply_action(
                    action,
                    "Selected MIDI note positions and ends quantized to editor grid",
                );
            }
            Message::OpenMidiQuantizeDialog(item_id) => {
                if self
                    .project
                    .midi_items()
                    .iter()
                    .any(|item| item.id() == item_id)
                {
                    self.midi_editor_filter_dialog_open = false;
                    let preferences = self.midi_quantize_preferences;
                    self.midi_quantize_dialog = Some(MidiQuantizeDialogState {
                        item_id,
                        scope: preferences.scope,
                        operation: preferences.operation,
                        strength_percent: preferences.strength_percent,
                        bypass: preferences.bypass,
                        fix_overlaps: preferences.fix_overlaps,
                    });
                }
            }
            Message::OpenMidiHumanizeDialog(item_id) => {
                if self
                    .project
                    .midi_items()
                    .iter()
                    .any(|item| item.id() == item_id)
                {
                    let preferences = self.midi_humanize_preferences;
                    self.midi_editor_filter_dialog_open = false;
                    self.midi_humanize_dialog = Some(MidiHumanizeDialogState {
                        item_id,
                        scope: if preferences.all_notes {
                            MidiHumanizeScope::AllNotes
                        } else {
                            MidiHumanizeScope::SelectedNotes
                        },
                        timing_amount_percent: preferences.timing_amount_percent,
                        velocity_amount_percent: preferences.velocity_amount_percent,
                        timing_bias_percent: preferences.timing_bias_percent,
                        seed: next_midi_humanize_seed(),
                    });
                }
            }
            Message::OpenMidiTransposeDialog(item_id) => {
                if self
                    .project
                    .midi_items()
                    .iter()
                    .any(|item| item.id() == item_id)
                {
                    self.midi_editor_filter_dialog_open = false;
                    self.midi_quantize_dialog = None;
                    self.midi_humanize_dialog = None;
                    self.midi_transpose_dialog = Some(MidiTransposeDialogState {
                        item_id,
                        scope: self.midi_transpose_scope,
                        semitones: "0".to_owned(),
                    });
                }
            }
            Message::SetMidiTransposeScope(scope) => {
                self.midi_transpose_scope = scope;
                if let Some(dialog) = self.midi_transpose_dialog.as_mut() {
                    dialog.scope = scope;
                }
            }
            Message::SetMidiTransposeSemitones(semitones) => {
                if let Some(dialog) = self.midi_transpose_dialog.as_mut() {
                    dialog.semitones = semitones;
                }
            }
            Message::AcceptMidiTranspose => task = self.commit_midi_transpose_dialog(),
            Message::CancelMidiTranspose => self.midi_transpose_dialog = None,
            Message::SetMidiHumanizeScope(scope) => {
                if let Some(dialog) = self.midi_humanize_dialog.as_mut() {
                    dialog.scope = scope;
                }
            }
            Message::SetMidiHumanizeTiming(value) => {
                if let Some(dialog) = self.midi_humanize_dialog.as_mut() {
                    dialog.timing_amount_percent = value.min(100);
                }
            }
            Message::SetMidiHumanizeVelocity(value) => {
                if let Some(dialog) = self.midi_humanize_dialog.as_mut() {
                    dialog.velocity_amount_percent = value.min(100);
                }
            }
            Message::SetMidiHumanizeTimingBias(value) => {
                if let Some(dialog) = self.midi_humanize_dialog.as_mut() {
                    dialog.timing_bias_percent = value.clamp(-100, 100);
                }
            }
            Message::NewMidiHumanizeSeed => {
                if let Some(dialog) = self.midi_humanize_dialog.as_mut() {
                    dialog.seed = next_midi_humanize_seed();
                }
            }
            Message::AcceptMidiHumanize => self.commit_midi_humanize_dialog(),
            Message::CancelMidiHumanize => self.midi_humanize_dialog = None,
            Message::SetMidiQuantizeScope(scope) => {
                if let Some(dialog) = self.midi_quantize_dialog.as_mut() {
                    dialog.scope = scope;
                }
                self.remember_midi_quantize_dialog();
            }
            Message::SetMidiQuantizeOperation(operation) => {
                if let Some(dialog) = self.midi_quantize_dialog.as_mut() {
                    dialog.operation = operation;
                }
                self.remember_midi_quantize_dialog();
            }
            Message::SetMidiQuantizeStrength(strength_percent) => {
                if let Some(dialog) = self.midi_quantize_dialog.as_mut() {
                    dialog.strength_percent = strength_percent.min(100);
                }
                self.remember_midi_quantize_dialog();
            }
            Message::SetMidiQuantizeBypass(bypass) => {
                if let Some(dialog) = self.midi_quantize_dialog.as_mut() {
                    dialog.bypass = bypass;
                }
                self.remember_midi_quantize_dialog();
            }
            Message::SetMidiQuantizeFixOverlaps(fix_overlaps) => {
                if let Some(dialog) = self.midi_quantize_dialog.as_mut() {
                    dialog.fix_overlaps = fix_overlaps;
                }
                self.remember_midi_quantize_dialog();
            }
            Message::CommitMidiQuantize => self.commit_midi_quantize_dialog(false),
            Message::AcceptMidiQuantize => self.commit_midi_quantize_dialog(true),
            Message::CancelMidiQuantize => self.midi_quantize_dialog = None,
            Message::DeleteTrack(track_id) => self.delete_track(track_id),
            Message::DeleteByFocus => task = self.delete_by_focus(false),
            Message::DeleteByFocusNoPrompting => task = self.delete_by_focus(true),
            Message::ConfirmFocusedTrackDeletion => self.confirm_focused_track_deletion(),
            Message::CancelFocusedTrackDeletion => {
                self.pending_focused_track_deletion = None;
            }
            Message::MoveTrack(track_id, direction) => self.move_track(track_id, direction),
            Message::TrackNameChanged(track_id, name) => {
                self.track_name_edits.insert(track_id, name);
                self.begin_track_draft(track_id, TrackDraftField::Name);
            }
            Message::CommitTrackName(track_id) => self.commit_track_name(track_id),
            Message::TogglePhase(track_id) => {
                if let Some(track) = self
                    .project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                {
                    self.apply_action(
                        DawAction::SetTrackPhase {
                            track_id,
                            phase_inverted: !track.is_phase_inverted(),
                        },
                        "Track polarity changed",
                    );
                }
            }
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
            Message::PreviewMasterVolume(value) => {
                self.preview_master_mix(TrackMixParameter::Volume, value)
            }
            Message::PreviewMasterPan(value) => {
                self.preview_master_mix(TrackMixParameter::Pan, value)
            }
            Message::CommitMasterMix => {
                if self.master_mix_gesture.is_some() {
                    self.master_mix_commit_at = Some(Instant::now() + Duration::from_millis(350));
                }
            }
            Message::CancelMasterMix => self.cancel_master_mix(),
            Message::ResetMasterVolume => self.reset_master_mix(TrackMixParameter::Volume),
            Message::ResetMasterPan => self.reset_master_mix(TrackMixParameter::Pan),
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
            Message::TrimItemsToSelectedArea => self.trim_items_to_selected_area(),
            Message::CopyItems => self.copy_selected_audio_items(),
            Message::CopyLoopOfSelectedArea => self.copy_loop_of_selected_area(),
            Message::EncloseMediaItemsInRazorEdit => self.enclose_media_items_in_razor_edit(),
            Message::PasteItems => self.paste_audio_items(),
            Message::PasteAudioItemAsTake => self.paste_audio_item_as_take(),
            Message::ImplodeSelectedAudioItemsAsTakes => {
                self.implode_selected_audio_items_as_takes()
            }
            Message::ImplodeSelectedMidiItemsAsTakes => self.implode_selected_midi_items_as_takes(),
            Message::ImplodeSelectedAudioItemsAcrossTracksAsTakes => {
                self.implode_selected_audio_items_across_tracks_as_takes()
            }
            Message::ExplodeSelectedAudioItemTakesInPlace => {
                self.explode_selected_audio_item_takes_in_place()
            }
            Message::SelectAllItems => {
                self.with_item_selection_history("Select all items", Self::select_all_items)
            }
            Message::SelectAllItemsInTrack => self.with_item_selection_history(
                "Select all items in selected tracks",
                Self::select_all_items_in_track,
            ),
            Message::SelectAllItemsInTimeSelection => self.with_item_selection_history(
                "Select items in time selection",
                Self::select_all_items_in_time_selection,
            ),
            Message::SelectAllItemsOnSelectedTracksInTimeSelection => self
                .with_item_selection_history(
                    "Select items in time selection",
                    Self::select_all_items_on_selected_tracks_in_time_selection,
                ),
            Message::UnselectAllItems => self
                .with_item_selection_history("Unselect all items", |app| {
                    app.timeline.clear_selected_items()
                }),
            Message::InvertItemSelection => self
                .with_item_selection_history("Invert item selection", Self::invert_item_selection),
            Message::SelectNextAdjacentItem => self
                .with_item_selection_history("Select next adjacent item", |app| {
                    app.select_adjacent_non_overlapping_item(true)
                }),
            Message::SelectPreviousAdjacentItem => self
                .with_item_selection_history("Select previous adjacent item", |app| {
                    app.select_adjacent_non_overlapping_item(false)
                }),
            Message::SelectItemOnAdjacentTrack(next_track) => self.with_item_selection_history(
                if next_track {
                    "Select item on next track"
                } else {
                    "Select item on previous track"
                },
                |app| app.select_item_on_adjacent_track(next_track),
            ),
            Message::MoveEditCursorToNearestItemEdge(forward) => {
                if let Some(position) = self.nearest_item_edge(forward) {
                    self.set_edit_cursor_position(position);
                }
            }
            Message::MoveEditCursorToSelectedItemsBoundary(move_to_end) => {
                self.move_cursor_to_selected_items_boundary(move_to_end);
            }
            Message::SnapSelectedItemsLeft => {
                self.snap_selected_items(ItemSnapDirection::Left);
            }
            Message::SnapSelectedItemsRight => {
                self.snap_selected_items(ItemSnapDirection::Right);
            }
            Message::SnapSelectedItemsToNearestPoint => {
                self.snap_selected_items(ItemSnapDirection::Nearest);
            }
            Message::DuplicateSelectedItems => self.duplicate_selected_items(),
            Message::SplitSelectedItemsAtCursor => self.split_selected_items(false),
            Message::SplitSelectedItemsAtTimeSelection => self.split_selected_items(true),
            Message::HealSplitsInItems => self.heal_selected_audio_splits(),
            Message::DuplicateAudioItem(item_id) => {
                let action = duplicate_audio_item(&self.project, item_id);
                self.apply_edit(action, "Audio item duplicated");
            }
            Message::DuplicateAudioItemTake(item_id) => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before duplicating takes"
                            .to_owned();
                    return Task::none();
                }
                if self
                    .project
                    .tracks()
                    .iter()
                    .any(|track| track.frozen_audio_item_id() == Some(item_id))
                {
                    self.status = "Frozen render items cannot have takes duplicated".to_owned();
                    return Task::none();
                }
                self.apply_action(
                    DawAction::DuplicateAudioItemTake { item_id },
                    "Active take duplicated",
                );
            }
            Message::DuplicateSelectedItemActiveTakes => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before duplicating takes"
                            .to_owned();
                    return Task::none();
                }
                let selected_audio_ids = commands::selected_audio_item_ids(self);
                let selected_midi_ids = commands::selected_midi_item_ids(self);
                if selected_audio_ids.is_empty() && selected_midi_ids.is_empty() {
                    self.status =
                        "Select an audio or MIDI item to duplicate its active take".to_owned();
                    return Task::none();
                }
                if selected_audio_ids.iter().any(|item_id| {
                    self.project
                        .tracks()
                        .iter()
                        .any(|track| track.frozen_audio_item_id() == Some(*item_id))
                }) {
                    self.status = "Frozen render items cannot have takes duplicated".to_owned();
                    return Task::none();
                }
                if selected_midi_ids.iter().any(|item_id| {
                    self.project.midi_items().iter().any(|item| {
                        item.id() == *item_id
                            && self
                                .project
                                .tracks()
                                .iter()
                                .any(|track| track.id() == item.track_id() && track.is_frozen())
                    })
                }) {
                    self.status = "Frozen items cannot have takes duplicated".to_owned();
                    return Task::none();
                }
                let mut actions = selected_audio_ids
                    .iter()
                    .map(|item_id| DawAction::DuplicateAudioItemTake { item_id: *item_id })
                    .collect::<Vec<_>>();
                actions.extend(
                    selected_midi_ids
                        .iter()
                        .map(|item_id| DawAction::DuplicateMidiItemTake { item_id: *item_id }),
                );
                let action = if actions.len() == 1 {
                    actions
                        .pop()
                        .expect("one selected audio item action is present")
                } else {
                    DawAction::BatchTransaction {
                        tx_id: self.revision,
                        actions,
                    }
                };
                self.apply_action(
                    action,
                    if selected_audio_ids.len() + selected_midi_ids.len() == 1 {
                        "Active take duplicated"
                    } else {
                        "Selected active takes duplicated"
                    },
                );
            }
            Message::DeleteSelectedItemActiveTakes => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before deleting takes"
                            .to_owned();
                    return Task::none();
                }
                let selected_audio_ids = commands::selected_audio_item_ids(self);
                let selected_midi_ids = commands::selected_midi_item_ids(self);
                if selected_audio_ids.is_empty() && selected_midi_ids.is_empty() {
                    self.status =
                        "Select an audio or MIDI item to delete its active take".to_owned();
                    return Task::none();
                }
                let frozen_audio = selected_audio_ids.iter().any(|item_id| {
                    self.project
                        .tracks()
                        .iter()
                        .any(|track| track.frozen_audio_item_id() == Some(*item_id))
                });
                let frozen_midi = selected_midi_ids.iter().any(|item_id| {
                    self.project.midi_items().iter().any(|item| {
                        item.id() == *item_id
                            && self
                                .project
                                .tracks()
                                .iter()
                                .any(|track| track.id() == item.track_id() && track.is_frozen())
                    })
                });
                if frozen_audio || frozen_midi {
                    self.status = "Frozen items cannot have takes deleted".to_owned();
                    return Task::none();
                }
                for item_id in &selected_audio_ids {
                    self.audio_item_start_edits.remove(item_id);
                }
                let mut actions = selected_audio_ids
                    .iter()
                    .map(|item_id| DawAction::DeleteActiveAudioItemTake { item_id: *item_id })
                    .collect::<Vec<_>>();
                actions.extend(
                    selected_midi_ids
                        .iter()
                        .map(|item_id| DawAction::DeleteActiveMidiItemTake { item_id: *item_id }),
                );
                let selected_count = actions.len();
                let action = if actions.len() == 1 {
                    actions.pop().expect("one selected item action is present")
                } else {
                    DawAction::BatchTransaction {
                        tx_id: self.revision,
                        actions,
                    }
                };
                self.apply_action(
                    action,
                    if selected_count == 1 {
                        "Active take deleted"
                    } else {
                        "Selected active takes deleted"
                    },
                );
            }
            Message::MoveSelectedItemActiveTakesToTop => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before reordering takes"
                            .to_owned();
                    return Task::none();
                }
                let selected_multi_take_count = commands::selected_multi_take_item_ids(self).len();
                let move_targets = commands::move_active_take_to_top_targets(self);
                if selected_multi_take_count == 0 {
                    self.status =
                        "Select a multi-take audio or MIDI item to move its active take to the top"
                            .to_owned();
                    return Task::none();
                }
                if move_targets.is_empty() {
                    self.status = if selected_multi_take_count == 1 {
                        "Active take is already at the top"
                    } else {
                        "Active takes are already at the top"
                    }
                    .to_owned();
                    return Task::none();
                }
                if move_targets.iter().any(|item_id| {
                    self.project
                        .tracks()
                        .iter()
                        .any(|track| track.frozen_audio_item_id() == Some(*item_id))
                        || self.project.midi_items().iter().any(|item| {
                            item.id() == *item_id
                                && self
                                    .project
                                    .tracks()
                                    .iter()
                                    .any(|track| track.id() == item.track_id() && track.is_frozen())
                        })
                }) {
                    self.status = "Frozen items cannot be edited".to_owned();
                    return Task::none();
                }
                let moved_count = move_targets.len();
                let actions = move_targets
                    .into_iter()
                    .map(|item_id| {
                        if self
                            .project
                            .audio_items()
                            .iter()
                            .any(|item| item.id() == item_id)
                        {
                            DawAction::MoveAudioItemActiveTakeToTop { item_id }
                        } else {
                            DawAction::MoveMidiItemActiveTakeToTop { item_id }
                        }
                    })
                    .collect::<Vec<_>>();
                let action = if actions.len() == 1 {
                    actions.into_iter().next().unwrap()
                } else {
                    DawAction::BatchTransaction {
                        tx_id: self.revision,
                        actions,
                    }
                };
                let status = if moved_count == 1 {
                    "Active take moved to top"
                } else {
                    "Active takes moved to top"
                };
                self.apply_action(action, status);
            }
            Message::CropSelectedItemsToActiveTake => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before cropping takes"
                            .to_owned();
                    return Task::none();
                }
                let crop_targets = commands::crop_active_take_targets(self);
                if crop_targets.is_empty() {
                    self.status =
                        "Select a multi-take audio or MIDI item to crop to its active take"
                            .to_owned();
                    return Task::none();
                }
                if crop_targets.iter().any(|item_id| {
                    self.project
                        .tracks()
                        .iter()
                        .any(|track| track.frozen_audio_item_id() == Some(*item_id))
                        || self.project.midi_items().iter().any(|item| {
                            item.id() == *item_id
                                && self
                                    .project
                                    .tracks()
                                    .iter()
                                    .any(|track| track.id() == item.track_id() && track.is_frozen())
                        })
                }) {
                    self.status = "Frozen items cannot be edited".to_owned();
                    return Task::none();
                }
                let actions = crop_targets
                    .into_iter()
                    .map(|item_id| {
                        if self
                            .project
                            .audio_items()
                            .iter()
                            .any(|item| item.id() == item_id)
                        {
                            DawAction::CropAudioItemToActiveTake { item_id }
                        } else {
                            DawAction::CropMidiItemToActiveTake { item_id }
                        }
                    })
                    .collect::<Vec<_>>();
                let action = if actions.len() == 1 {
                    actions.into_iter().next().unwrap()
                } else {
                    DawAction::BatchTransaction {
                        tx_id: self.revision,
                        actions,
                    }
                };
                self.apply_action(action, "Inactive takes removed");
            }
            Message::SwitchSelectedAudioItemTakes(next) => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before switching takes"
                            .to_owned();
                    return Task::none();
                }
                let targets = commands::switch_media_item_take_indices(self, next);
                if targets.is_empty() {
                    self.status = "Select a multi-take item".to_owned();
                    return Task::none();
                }
                if targets.iter().any(|(item_id, _)| {
                    self.project
                        .audio_items()
                        .iter()
                        .find(|item| item.id() == *item_id)
                        .is_some_and(|_| {
                            self.project
                                .tracks()
                                .iter()
                                .any(|track| track.frozen_audio_item_id() == Some(*item_id))
                        })
                        || self
                            .project
                            .midi_items()
                            .iter()
                            .find(|item| item.id() == *item_id)
                            .is_some_and(|item| {
                                self.project
                                    .tracks()
                                    .iter()
                                    .any(|track| track.id() == item.track_id() && track.is_frozen())
                            })
                }) {
                    self.status = "Frozen items cannot be edited".to_owned();
                    return Task::none();
                }
                let actions = targets
                    .iter()
                    .map(|(item_id, take_index)| {
                        if self
                            .project
                            .midi_items()
                            .iter()
                            .any(|item| item.id() == *item_id)
                        {
                            DawAction::SetMidiItemActiveTake {
                                item_id: *item_id,
                                take_index: *take_index,
                            }
                        } else {
                            DawAction::SetAudioItemActiveTake {
                                item_id: *item_id,
                                take_index: *take_index,
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                let action = if actions.len() == 1 {
                    actions.into_iter().next().unwrap()
                } else {
                    DawAction::BatchTransaction {
                        tx_id: self.revision,
                        actions,
                    }
                };
                self.apply_action(
                    action,
                    if targets.len() == 1 {
                        "Active take changed"
                    } else {
                        "Selected active takes changed"
                    },
                );
            }
            Message::SwitchMidiItemTake(item_id, next) => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before switching takes"
                            .to_owned();
                    return Task::none();
                }
                let Some(item) = self
                    .project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                else {
                    self.status = "MIDI item no longer exists".to_owned();
                    return Task::none();
                };
                if self
                    .project
                    .tracks()
                    .iter()
                    .any(|track| track.id() == item.track_id() && track.is_frozen())
                {
                    self.status = "Frozen items cannot be edited".to_owned();
                    return Task::none();
                }
                let take_count = item.take_count() as u64;
                if take_count <= 1 {
                    self.status = "MIDI item has only one take".to_owned();
                    return Task::none();
                }
                let active = u64::from(item.active_take_index());
                let take_index = if next {
                    (active + 1) % take_count
                } else {
                    (active + take_count - 1) % take_count
                } as u32;
                self.apply_action(
                    DawAction::SetMidiItemActiveTake {
                        item_id,
                        take_index,
                    },
                    "Active take changed",
                );
            }
            Message::AddEmptyTakes(before_active) => {
                if commands::project_edit_busy(self) {
                    self.status =
                        "Stop playback or finish the current project operation before adding takes"
                            .to_owned();
                    return Task::none();
                }
                let audio_item_ids = commands::selected_audio_item_ids(self);
                let midi_item_ids = commands::selected_midi_item_ids(self);
                if audio_item_ids.is_empty() && midi_item_ids.is_empty() {
                    self.status = "Select an audio or MIDI item to add an empty take".to_owned();
                    return Task::none();
                }
                if audio_item_ids.iter().any(|item_id| {
                    self.project.audio_items().iter().any(|item| {
                        item.id() == *item_id
                            && self.project.tracks().iter().any(|track| {
                                track.id() == item.track_id()
                                    && track.frozen_audio_item_id().is_some()
                            })
                    })
                }) || midi_item_ids.iter().any(|item_id| {
                    self.project.midi_items().iter().any(|item| {
                        item.id() == *item_id
                            && self
                                .project
                                .tracks()
                                .iter()
                                .any(|track| track.id() == item.track_id() && track.is_frozen())
                    })
                }) {
                    self.status = "Frozen items cannot have takes added".to_owned();
                    return Task::none();
                }
                let item_ids = audio_item_ids
                    .iter()
                    .copied()
                    .chain(midi_item_ids.iter().copied())
                    .collect::<Vec<_>>();
                let actions = item_ids
                    .into_iter()
                    .map(|item_id| {
                        if audio_item_ids.contains(&item_id) && before_active {
                            DawAction::AddEmptyAudioItemTakeBefore { item_id }
                        } else if audio_item_ids.contains(&item_id) {
                            DawAction::AddEmptyAudioItemTakeAfter { item_id }
                        } else if before_active {
                            DawAction::AddEmptyMidiItemTakeBefore { item_id }
                        } else {
                            DawAction::AddEmptyMidiItemTakeAfter { item_id }
                        }
                    })
                    .collect::<Vec<_>>();
                let action = if actions.len() == 1 {
                    actions.into_iter().next().unwrap()
                } else {
                    DawAction::BatchTransaction {
                        tx_id: self.revision,
                        actions,
                    }
                };
                self.apply_action(
                    action,
                    if before_active {
                        "Empty take added before active take"
                    } else {
                        "Empty take added after active take"
                    },
                );
            }
            Message::SwitchAudioItemTake(item_id, next) => {
                let Some(item) = self
                    .project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                else {
                    self.status = "Audio item no longer exists".to_owned();
                    return Task::none();
                };
                if self
                    .project
                    .tracks()
                    .iter()
                    .any(|track| track.frozen_audio_item_id() == Some(item_id))
                {
                    self.status = "Frozen render items cannot be edited".to_owned();
                    return Task::none();
                }
                let take_count = item.take_count() as u64;
                if take_count <= 1 {
                    self.status = "Audio item has only one take".to_owned();
                    return Task::none();
                }
                let active = u64::from(item.active_take_index());
                let take_index = if next {
                    (active + 1) % take_count
                } else {
                    (active + take_count - 1) % take_count
                } as u32;
                self.apply_action(
                    DawAction::SetAudioItemActiveTake {
                        item_id,
                        take_index,
                    },
                    "Active take changed",
                );
            }
            Message::DuplicateMidiItem(item_id) => {
                self.apply_action(
                    DawAction::DuplicateMidiItem { item_id },
                    "MIDI item duplicated",
                );
            }
            Message::Undo => {
                self.cancel_item_fades();
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
                self.cancel_item_fades();
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
            Message::ShortcutPressed(input) => {
                if self.pending_project_transition.is_some() {
                    return Task::none();
                }
                let shortcut = {
                    let bindings = self
                        .shortcut_bindings
                        .read()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    commands::from_shortcut_input(&input, &bindings, &self.action_macros)
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
            Message::ToggleMediaBrowserPanel => {
                if self.is_mobile_main_window() {
                    if self.mobile_panel == MobilePanel::MediaBrowser {
                        self.navigate_back_mobile_panel();
                    } else {
                        self.show_mobile_panel(MobilePanel::MediaBrowser);
                    }
                } else {
                    self.media_panel_dock.toggle();
                    self.schedule_desktop_layout_save();
                }
            }
            Message::MediaPanelResized(split, ratio) => {
                self.media_panel_dock.resize(split, ratio);
                self.schedule_desktop_layout_save();
            }
            Message::PickPath(target) => task = self.pick_path(target),
            Message::PathPicked(target, result) => task = self.path_picked(target, result),
            Message::ActionListOptionsToggled => {
                self.action_list.options_open = !self.action_list.options_open;
            }
            Message::ActionListImportShortcutMap => {
                self.action_list.options_open = false;
                task = self.pick_path(PathPickerTarget::ImportShortcutMap);
            }
            Message::ActionListExportShortcutMap => {
                self.action_list.options_open = false;
                task = self.pick_path(PathPickerTarget::ExportShortcutMap);
            }
            Message::ActionListImportReaperKeyMap => {
                self.action_list.options_open = false;
                task = self.pick_path(PathPickerTarget::ImportReaperKeyMap);
            }
            Message::ActionListExportReaperKeyMap => {
                self.action_list.options_open = false;
                task = self.pick_path(PathPickerTarget::ExportReaperKeyMap);
            }
            Message::ShortcutMapImported(result) => match result {
                Ok(bindings) => {
                    self.path_picker_busy = false;
                    self.import_shortcut_map(bindings);
                }
                Err(error) => {
                    self.path_picker_busy = false;
                    let message = format!("Shortcut map was not imported: {error}");
                    self.action_list.feedback = message.clone();
                    self.status = message;
                }
            },
            Message::ShortcutMapExported(result) => match result {
                Ok(()) => {
                    self.path_picker_busy = false;
                    self.status = "Shortcut map exported".to_owned();
                    self.action_list.feedback = self.status.clone();
                }
                Err(error) => {
                    self.path_picker_busy = false;
                    let message = format!("Shortcut map could not be exported: {error}");
                    self.action_list.feedback = message.clone();
                    self.status = message;
                }
            },
            Message::ReaperKeyMapImported(result) => match result {
                Ok(bindings) => {
                    self.path_picker_busy = false;
                    self.import_reaper_key_map(bindings);
                }
                Err(error) => {
                    self.path_picker_busy = false;
                    let message = format!("REAPER key map was not imported: {error}");
                    self.action_list.feedback = message.clone();
                    self.status = message;
                }
            },
            Message::ReaperKeyMapExported(result) => match result {
                Ok(()) => {
                    self.path_picker_busy = false;
                    self.status = "REAPER key map exported".to_owned();
                    self.action_list.feedback = self.status.clone();
                }
                Err(error) => {
                    self.path_picker_busy = false;
                    let message = format!("REAPER key map could not be exported: {error}");
                    self.action_list.feedback = message.clone();
                    self.status = message;
                }
            },
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
            Message::SaveProjectNewVersion => {
                task = self.save_new_project_version_command();
            }
            Message::SaveBeforeProjectTransition => {
                task = self.save_before_project_transition();
            }
            Message::DiscardProjectChanges => {
                task = self.discard_and_continue_project_transition();
            }
            Message::CancelProjectTransition => {
                self.pending_project_transition = None;
                self.pending_project_path_to_open = None;
                self.pending_project_template_source = None;
                self.pending_project_template_working_copy = None;
            }
            Message::ImportAudio => task = self.start_audio_import(),
            Message::AudioFilePathChanged(path) => self.audio_file_path_query = path,
            Message::ReimportAudioItem(item_id) => task = self.reimport_audio_item(item_id),
            Message::CancelAudioImport => self.cancel_audio_import(),
            Message::AudioImportStarted(worker) => self.audio_import_started(worker),
            Message::AudioImportFinished(result) => task = self.finish_audio_import(result),
            Message::MidiFileImportFinished(result) => {
                self.import_busy = false;
                match result {
                    Err(error) => self.status = format!("MIDI import failed: {error}"),
                    Ok(imported) => {
                        self.pending_midi_file_import = Some(imported);
                        self.midi_file_import_options = MidiFileImportOptions::default();
                        self.status = "Choose MIDI file import options".to_owned();
                    }
                }
            }
            Message::SetMidiFileImportExpandTracks(enabled) => {
                self.midi_file_import_options.expand_tracks = enabled;
            }
            Message::SetMidiFileImportTempoMap(enabled) => {
                self.midi_file_import_options.import_tempo_map = enabled;
            }
            Message::SetMidiFileImportMarkers(enabled) => {
                self.midi_file_import_options.import_markers = enabled;
            }
            Message::ConfirmMidiFileImport => self.apply_midi_file_import(),
            Message::CancelMidiFileImport => {
                self.pending_midi_file_import = None;
                self.midi_file_import_options = MidiFileImportOptions::default();
                self.status = "MIDI file import cancelled".to_owned();
            }
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
                self.flush_desktop_layout_if_due();
                self.flush_loop_time_preferences_if_due();
                self.flush_midi_humanize_preferences_if_due();
                self.flush_midi_quantize_preferences_if_due();
                #[cfg(all(feature = "audio-device", target_os = "android"))]
                if self.settings_category == SettingsCategory::Audio {
                    match crate::android_platform::midi_port_counts() {
                        Ok((inputs, outputs)) => {
                            self.android_midi_input_ports = inputs;
                            self.android_midi_output_ports = outputs;
                        }
                        Err(error) => {
                            self.android_midi_feedback =
                                format!("MIDI status unavailable: {error}");
                        }
                    }
                }
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
                if self
                    .master_mix_commit_at
                    .is_some_and(|deadline| Instant::now() >= deadline)
                {
                    self.commit_master_mix();
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
                    #[cfg(target_os = "android")]
                    for owner in self.clap_instrument_owners.values_mut() {
                        owner.service_main_thread_callback();
                    }
                    let output_device_lost = self.update_playback_stats();
                    self.follow_midi_editor_playhead();
                    let recording_failed = self
                        .recording
                        .as_ref()
                        .is_some_and(|recording| recording.control.has_failed())
                        && !self.recording_stopping;
                    self.update_audio_waveforms();
                    let cleanup_output = if output_device_lost {
                        self.handle_playback_device_lost()
                    } else {
                        Task::none()
                    };
                    let background_tasks = if recording_failed {
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
                    #[cfg(target_os = "android")]
                    let background_tasks = if !recording_failed {
                        Task::batch([background_tasks, self.recover_recording_input_if_needed()])
                    } else {
                        background_tasks
                    };
                    task = Task::batch([cleanup_output, background_tasks]);
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
                        let template_working_copy =
                            self.pending_project_template_working_copy.take();
                        let template_source = self.pending_project_template_source.take();
                        let opened_from_template = template_source.as_ref().is_some_and(|_| {
                            template_working_copy
                                .as_ref()
                                .is_some_and(|copy| copy.project_path == path)
                        });
                        self.project_lock = Some(project_lock);
                        self.project_template_source = if opened_from_template {
                            template_source
                        } else {
                            None
                        };
                        self.project_template_working_copy = if opened_from_template {
                            template_working_copy
                        } else {
                            None
                        };
                        self.clear_app_history();
                        self.routing_track_id = None;
                        self.routing_destination_filter = None;
                        self.routing_send_drafts.clear();
                        self.project = project;
                        self.refresh_tempo_map_edits();
                        self.refresh_meter_map_edits();
                        self.tempo_map_feedback.clear();
                        self.meter_map_feedback.clear();
                        self.midi_note_clipboard.last_paste = None;
                        self.timeline
                            .replace_project(&self.project, arrangement_view_state.as_ref());
                        self.restore_repeat_from_view_state(arrangement_view_state.as_ref());
                        self.timeline.selected_item = None;
                        self.timeline.selected_track = None;
                        self.timeline.selected_tracks.clear();
                        self.timeline.origin_tick = 0;
                        self.timeline.set_edit_cursor_tick(0);
                        self.timeline.vertical_scroll = 0.0;
                        task = Task::batch([
                            scroll_arrangement_to(timeline::TCP_SCROLL_ID, 0.0),
                            scroll_arrangement_to(timeline::TIMELINE_SCROLL_ID, 0.0),
                        ]);
                        self.clear_track_draft_state();
                        self.track_mix_gesture = None;
                        self.track_mix_commit_at = None;
                        self.master_mix_gesture = None;
                        self.master_mix_commit_at = None;
                        self.audio_item_start_edits.clear();
                        self.audio_asset_source_statuses.clear();
                        self.project_path_query = path.to_string_lossy().into_owned();
                        self.project_path = Some(path.clone());
                        self.retire_unsaved_session_media();
                        self.unsaved_session_snapshot_revision = 0;
                        self.recording_recovery_candidates.clear();
                        self.project_generation = self.project_generation.wrapping_add(1);
                        self.selected_render_markers.clear();
                        self.selected_render_regions.clear();
                        self.start_audio_waveform_scan(true);
                        self.revision = 0;
                        self.saved_revision = 0;
                        if !opened_from_template {
                            self.remember_recent_project(&path);
                        }
                        self.status = if opened_from_template {
                            format!(
                                "Created project from template {}",
                                self.project_template_source
                                    .as_deref()
                                    .and_then(std::path::Path::file_name)
                                    .map_or_else(
                                        || "Project template".to_owned(),
                                        |name| name.to_string_lossy().into_owned(),
                                    )
                            )
                        } else {
                            format!("Opened {}", path.display())
                        };
                        task = Task::batch([task, self.show_project_notes_on_load()]);
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
                        self.pending_project_path_to_open = None;
                        self.pending_project_template_source = None;
                        self.pending_project_template_working_copy = None;
                        self.project_path_query = self
                            .project_path
                            .as_ref()
                            .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
                        tracing::error!(error = %error, "project open failed");
                        self.status = format!("Open failed: {error}");
                    }
                    None => {
                        self.pending_project_transition = None;
                        self.pending_project_path_to_open = None;
                        self.pending_project_template_source = None;
                        self.pending_project_template_working_copy = None;
                        self.project_path_query = self
                            .project_path
                            .as_ref()
                            .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
                        self.status = "Project open result was unavailable".to_owned();
                    }
                }
            }
            Message::ProjectTemplateSaved(path, result) => {
                self.io_busy = false;
                match result {
                    Ok(()) => {
                        match project_templates::load() {
                            Ok(paths) => self.project_template_paths = paths,
                            Err(error) => {
                                tracing::warn!(%error, "saved project templates could not be reloaded")
                            }
                        }
                        self.status = format!("Saved project template {}", path.display());
                    }
                    Err(error) => {
                        self.status = format!("Could not save project template: {error}");
                    }
                }
            }
            Message::ConfirmProjectTemplateOverwrite => {
                if let Some(path) = self.pending_project_template_overwrite.take() {
                    task = self.overwrite_project_template(path);
                }
            }
            Message::CancelProjectTemplateOverwrite => {
                if self.pending_project_template_overwrite.take().is_some() {
                    self.status = "Project template overwrite cancelled".to_owned();
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
                        self.project_template_source = None;
                        self.project_template_working_copy = None;
                        self.retire_unsaved_session_media();
                        self.unsaved_session_snapshot_revision = 0;
                        self.saved_revision = revision;
                        self.remember_recent_project(&path);
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
                        self.clear_app_history();
                        self.routing_track_id = None;
                        self.routing_destination_filter = None;
                        self.routing_send_drafts.clear();
                        self.project = project;
                        self.project_path = None;
                        self.project_path_query.clear();
                        self.project_lock = None;
                        self.project_generation = self.project_generation.wrapping_add(1);
                        self.selected_render_markers.clear();
                        self.selected_render_regions.clear();
                        self.revision = 1;
                        self.saved_revision = 0;
                        self.unsaved_session_snapshot_revision = self.revision;
                        self.unsaved_session_recovery_candidates
                            .retain(|candidate| candidate != &path);
                        self.timeline
                            .replace_project(&self.project, arrangement_view_state.as_ref());
                        self.restore_repeat_from_view_state(arrangement_view_state.as_ref());
                        self.clear_track_draft_state();
                        self.start_audio_waveform_scan(true);
                        self.status = "Unsaved project session recovered".to_owned();
                        task = self.show_project_notes_on_load();
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
                if self.recording.is_some() || self.recording_starting {
                    task = self.stop_recording();
                } else {
                    task = self.stop_transport_to_start();
                }
            }
            #[cfg(feature = "audio-device")]
            Message::PanicMidi => self.panic_midi(),
            #[cfg(feature = "audio-device")]
            Message::StartRecording => task = self.start_recording(),
            #[cfg(all(feature = "audio-device", target_os = "android"))]
            Message::MicrophonePermissionResult(result) => {
                self.recording_starting = false;
                if self.recording_cancel_requested {
                    self.recording_cancel_requested = false;
                    self.status = "Recording setup cancelled".to_owned();
                } else {
                    match result {
                        Ok(true) => task = self.start_recording(),
                        Ok(false) => {
                            self.status = "Microphone permission is required to record".to_owned();
                        }
                        Err(error) => {
                            self.status = format!("Microphone permission failed: {error}")
                        }
                    }
                }
            }
            #[cfg(feature = "audio-device")]
            Message::StopRecording => task = self.stop_recording(),
            #[cfg(feature = "audio-device")]
            Message::RecordingStarted(result) => task = self.finish_recording_start(result),
            #[cfg(all(feature = "audio-device", target_os = "android"))]
            Message::RecordingInputReconnected(result) => {
                task = self.finish_recording_input_recovery(result);
            }
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
                        #[cfg(target_os = "android")]
                        if self.playback.is_none() {
                            crate::android_platform::stop_recording_service();
                        }
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
                    any(target_os = "windows", target_os = "macos", target_os = "android")
                ),
                all(
                    feature = "pipewire-backend",
                    feature = "cpal-backend",
                    any(target_os = "windows", target_os = "macos", target_os = "android")
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
        }
        #[cfg(all(feature = "audio-device", target_os = "android"))]
        crate::android_platform::set_midi_input_target_track(self.selected_midi_input_track_id());
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
        let view_state = self.project_arrangement_view_state();
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

    pub(super) fn project_arrangement_view_state(&self) -> ArrangementViewState {
        let mut view_state = self.timeline.arrangement_view_state(&self.project);
        view_state.repeat_enabled = self.repeat_enabled;
        view_state
    }

    fn restore_repeat_from_view_state(&mut self, view_state: Option<&ArrangementViewState>) {
        self.repeat_enabled = view_state.is_some_and(|view_state| view_state.repeat_enabled);
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
        let edit_cursor_moves = matches!(
            &event,
            timeline::TimelineEvent::SelectEmpty(_)
                | timeline::TimelineEvent::SetEditCursor(_)
                | timeline::TimelineEvent::SetEditCursorPosition { .. }
                | timeline::TimelineEvent::SelectProjectMarker { .. }
                | timeline::TimelineEvent::SelectProjectRegion { .. }
        );
        if let timeline::TimelineEvent::MoveProjectMarker {
            marker_id,
            position_ticks,
        } = &event
        {
            if commands::project_edit_busy(self) {
                self.status =
                    "Stop playback or finish the current project operation before moving a marker"
                        .to_owned();
                return;
            }
            let Some(name) = self
                .project
                .markers()
                .iter()
                .find(|marker| marker.id() == *marker_id)
                .map(|marker| marker.name().to_owned())
            else {
                self.status = "Project marker no longer exists".to_owned();
                return;
            };
            self.timeline.selected_project_marker = Some(*marker_id);
            self.apply_action(
                DawAction::UpdateProjectMarker {
                    marker_id: *marker_id,
                    position_ticks: *position_ticks,
                    name,
                },
                "Project marker moved",
            );
            return;
        }
        if let timeline::TimelineEvent::MoveProjectRegion {
            region_id,
            original_start_ticks,
            original_end_ticks,
            start_ticks,
            end_ticks,
        } = &event
        {
            if commands::project_edit_busy(self) {
                self.status =
                    "Stop playback or finish the current project operation before moving a region"
                        .to_owned();
                return;
            }
            let Some((number, name, current_start_ticks, current_end_ticks)) = self
                .project
                .regions()
                .iter()
                .find(|region| region.id() == *region_id)
                .map(|region| {
                    (
                        region.number(),
                        region.name().to_owned(),
                        region.start_ticks(),
                        region.end_ticks(),
                    )
                })
            else {
                self.status = "Project region no longer exists".to_owned();
                return;
            };
            if current_start_ticks != *original_start_ticks
                || current_end_ticks != *original_end_ticks
            {
                self.status = "Project region changed before the drag completed".to_owned();
                return;
            }
            self.apply_action(
                DawAction::UpdateProjectRegion {
                    region_id: *region_id,
                    number,
                    start_ticks: *start_ticks,
                    end_ticks: *end_ticks,
                    name,
                },
                "Project region moved",
            );
            if self.project.regions().iter().any(|region| {
                region.id() == *region_id
                    && region.start_ticks() == *start_ticks
                    && region.end_ticks() == *end_ticks
            }) {
                self.handle_timeline_view_event(timeline::TimelineEvent::SelectProjectRegion {
                    region_id: *region_id,
                    start_ticks: *start_ticks,
                    end_ticks: *end_ticks,
                });
            }
            return;
        }
        let event = match event {
            timeline::TimelineEvent::SelectProjectMarker {
                marker_id,
                position_ticks,
            } => {
                if !self
                    .project
                    .markers()
                    .iter()
                    .any(|marker| marker.id() == marker_id)
                {
                    self.status = "Project marker no longer exists".to_owned();
                    return;
                }
                self.selected_render_markers.clear();
                self.selected_render_regions.clear();
                self.selected_render_markers.insert(marker_id);
                timeline::TimelineEvent::SelectProjectMarker {
                    marker_id,
                    position_ticks,
                }
            }
            timeline::TimelineEvent::SelectProjectRegion { region_id, .. } => {
                let Some(region) = self
                    .project
                    .regions()
                    .iter()
                    .find(|region| region.id() == region_id)
                else {
                    self.status = "Project region no longer exists".to_owned();
                    return;
                };
                let start_ticks = region.start_ticks();
                let end_ticks = region.end_ticks();
                self.timeline.selected_project_marker = None;
                self.selected_render_markers.clear();
                self.selected_render_regions.clear();
                self.selected_render_regions.insert(region_id);
                timeline::TimelineEvent::SelectProjectRegion {
                    region_id,
                    start_ticks,
                    end_ticks,
                }
            }
            event => event,
        };
        let previous_view_state = matches!(
            &event,
            timeline::TimelineEvent::ToggleVolumeAutomation(_)
                | timeline::TimelineEvent::CycleFolderCompact(_)
                | timeline::TimelineEvent::ToggleFxAutomation { .. }
                | timeline::TimelineEvent::ResizeFxAutomationLane { .. }
                | timeline::TimelineEvent::ToggleSnap
                | timeline::TimelineEvent::SetSnapGrid(_)
                | timeline::TimelineEvent::SetSnapSwingEnabled(_)
                | timeline::TimelineEvent::ToggleSnapSwing
                | timeline::TimelineEvent::SetSnapSwingAmountPercent(_)
                | timeline::TimelineEvent::ToggleMidiEditorSnap
                | timeline::TimelineEvent::SetMidiEditorSnapGrid(_)
        )
        .then(|| self.project_arrangement_view_state());
        let previous_time_ranges = (self.timeline.time_selection, self.timeline.loop_points);
        let previous_loop_points_linked = self.timeline.loop_points_linked;
        let item_trim_preview_changed = matches!(
            &event,
            timeline::TimelineEvent::BeginItemTrim { .. }
                | timeline::TimelineEvent::UpdateItemTrim { .. }
        );
        self.timeline.handle(event);
        if previous_loop_points_linked != self.timeline.loop_points_linked {
            self.schedule_loop_time_preferences_save();
        }
        if item_trim_preview_changed && let Some(preview) = self.timeline.item_trim_preview() {
            let valid = self.item_trim_preview_is_valid(preview);
            self.timeline.set_item_trim_valid(valid);
        }
        #[cfg(feature = "audio-device")]
        if edit_cursor_moves
            && !self.playback_playing
            && !self.playback_paused
            && !self.playback_busy
            && let Ok(sample) = self.timeline.edit_cursor_sample(&self.project)
        {
            self.playhead_sample = sample;
            self.playback_start_sample = sample;
            self.seek_sample_query = sample.to_string();
            self.playback_position_dirty = self.playback.is_some();
        }
        if previous_view_state
            .is_some_and(|previous| previous != self.project_arrangement_view_state())
            || previous_time_ranges != (self.timeline.time_selection, self.timeline.loop_points)
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

        self.routing_track_id = None;
        self.routing_destination_filter = None;
        self.routing_send_drafts.clear();
        let (new_project, defaults_error) =
            match project_defaults::load().and_then(|defaults| defaults.create_project()) {
                Ok(project) => (project, None),
                Err(error) => (Project::new(), Some(error)),
            };
        self.clear_app_history();
        self.project = new_project;
        self.restore_repeat_from_view_state(None);
        self.refresh_tempo_map_edits();
        self.refresh_meter_map_edits();
        self.tempo_map_feedback.clear();
        self.meter_map_feedback.clear();
        self.project_generation = self.project_generation.wrapping_add(1);
        self.selected_render_markers.clear();
        self.selected_render_regions.clear();
        self.recording_recovery_candidates.clear();
        self.recording_recovery_scanning = false;
        self.midi_note_clipboard.last_paste = None;
        self.project_path = None;
        self.project_template_source = None;
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
        self.project_template_working_copy = None;
        self.project_path_query.clear();
        self.revision = 0;
        self.saved_revision = 0;
        self.timeline.replace_project(&self.project, None);
        self.timeline.selected_track = None;
        self.timeline.selected_tracks.clear();
        self.timeline.selected_item = None;
        self.timeline.selected_items.clear();
        self.timeline.time_selection = None;
        self.timeline.loop_points = None;
        self.timeline.origin_tick = 0;
        self.timeline.set_edit_cursor_tick(0);
        self.timeline.vertical_scroll = 0.0;
        self.clear_track_draft_state();
        self.track_mix_gesture = None;
        self.track_mix_commit_at = None;
        self.master_mix_gesture = None;
        self.master_mix_commit_at = None;
        self.audio_item_start_edits.clear();
        self.audio_waveforms.clear();
        self.audio_asset_source_statuses.clear();
        self.status = new_project_status(session_media_error.as_deref());
        if let Some(error) = defaults_error {
            self.status = format!("Default project settings unavailable: {error}");
        }
    }

    fn begin_project_transition(&mut self, transition: PendingProjectTransition) -> Task<Message> {
        self.cancel_item_fades();
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
            PendingProjectTransition::NewProject
                | PendingProjectTransition::CloseProject
                | PendingProjectTransition::OpenProject
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
            Some(PendingProjectTransition::CloseProject) => {
                self.pending_project_transition = None;
                self.new_project();
                self.status = "Project closed".to_owned();
                Task::none()
            }
            Some(PendingProjectTransition::OpenProject) => self.continue_open_project_transition(),
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
            Some(PendingProjectTransition::CloseProject) => {
                self.pending_project_transition = None;
                self.new_project();
                self.status = "Project closed".to_owned();
                Task::none()
            }
            Some(PendingProjectTransition::OpenProject) => self.continue_open_project_transition(),
            Some(PendingProjectTransition::CloseMainWindow(window_id)) => {
                self.pending_project_transition = None;
                self.close_main_window(window_id)
            }
            None => Task::none(),
        }
    }

    fn continue_open_project_transition(&mut self) -> Task<Message> {
        if let Some(source) = self
            .pending_project_template_source
            .clone()
            .filter(|_| self.pending_project_template_working_copy.is_none())
        {
            self.io_busy = true;
            self.status = format!("Preparing project template {}…", source.display());
            let source_for_copy = source.clone();
            return Task::perform(
                run_blocking("aaadaw-template-working-copy", move || {
                    project_templates::create_working_copy(&source_for_copy)
                }),
                move |result| Message::ProjectTemplateCopied(source, result),
            );
        }
        if let Some(path) = self.pending_project_path_to_open.take() {
            self.project_path_query = path.to_string_lossy().into_owned();
            project_io::open_project(self)
        } else {
            self.pick_path(PathPickerTarget::OpenProject)
        }
    }

    fn close_main_window(&mut self, window_id: iced::window::Id) -> Task<Message> {
        self.close_fx_editor_resources();
        iced::window::close(window_id)
    }

    fn open_about(&mut self) -> Task<Message> {
        if let Some(window_id) = self.about_window_id {
            return iced::window::gain_focus(window_id);
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(440.0, 280.0),
            min_size: Some(iced::Size::new(400.0, 240.0)),
            ..iced::window::Settings::default()
        });
        self.about_window_id = Some(window_id);
        task.discard()
    }

    fn open_settings(&mut self) -> Task<Message> {
        if self.is_mobile_main_window() {
            self.show_mobile_panel(MobilePanel::Settings);
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos", target_os = "android")
            ))]
            return Task::batch([
                self.refresh_cpal_output_devices(),
                self.refresh_cpal_input_devices(),
            ]);
            #[cfg(not(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos", target_os = "android")
            )))]
            return Task::none();
        }
        if let Some(window_id) = self.settings_window_id {
            let focus = iced::window::gain_focus(window_id);
            #[cfg(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos", target_os = "android")
            ))]
            return Task::batch([
                focus,
                self.refresh_cpal_output_devices(),
                self.refresh_cpal_input_devices(),
            ]);
            #[cfg(not(all(
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos", target_os = "android")
            )))]
            return focus;
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: if cfg!(target_os = "android") {
                iced::Size::new(420.0, 640.0)
            } else {
                iced::Size::new(760.0, 620.0)
            },
            min_size: Some(if cfg!(target_os = "android") {
                iced::Size::new(360.0, 480.0)
            } else {
                iced::Size::new(640.0, 460.0)
            }),
            ..iced::window::Settings::default()
        });
        self.settings_window_id = Some(window_id);
        #[cfg(all(
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos", target_os = "android")
        ))]
        return Task::batch([
            task.discard(),
            self.refresh_cpal_output_devices(),
            self.refresh_cpal_input_devices(),
        ]);
        #[cfg(not(all(
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos", target_os = "android")
        )))]
        task.discard()
    }

    fn open_render_window(&mut self) -> Task<Message> {
        self.initialize_custom_render_range();
        if let Some(window_id) = self.render_window_id {
            return iced::window::gain_focus(window_id);
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(640.0, 720.0),
            min_size: Some(iced::Size::new(420.0, 360.0)),
            ..iced::window::Settings::default()
        });
        self.render_window_id = Some(window_id);
        task.discard()
    }

    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
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
        any(target_os = "windows", target_os = "macos", target_os = "android")
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
        any(target_os = "windows", target_os = "macos", target_os = "android")
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
        any(target_os = "windows", target_os = "macos", target_os = "android")
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
        if self.is_mobile_main_window() {
            self.refresh_tempo_map_edits();
            self.refresh_meter_map_edits();
            self.show_mobile_panel(MobilePanel::TimeMap);
            return Task::none();
        }
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
        if cfg!(target_os = "android") {
            let Some(window_id) = self.main_window_id else {
                self.status = "The Android editor window is not ready".to_owned();
                return Task::none();
            };
            let item_changed = self.midi_editor_item_id != Some(item_id);
            if item_changed {
                self.release_midi_preview();
                self.midi_editor_origin_tick = 0;
                self.midi_editor_edit_cursor_tick = None;
            }
            self.midi_editor_window_id = Some(window_id);
            self.midi_editor_item_id = Some(item_id);
            self.midi_editor_window_size = self
                .main_window_size
                .unwrap_or(iced::Size::new(420.0, 640.0));
            self.midi_editor_feedback = None;
            self.midi_editor_selected_notes.clear();
            self.midi_editor_selected_controllers.clear();
            self.midi_editor_selected_pitch_bends.clear();
            self.midi_editor_selected_program_changes.clear();
            self.midi_editor_selected_opaque_events.clear();
            self.midi_editor_event_list_cursor = None;
            self.midi_editor_event_list_anchor = None;
            if item_changed {
                self.fit_midi_editor_to_item(item_id);
            }
            self.show_mobile_panel(MobilePanel::MidiEditor);
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
            self.midi_editor_selected_controllers.clear();
            self.midi_editor_selected_pitch_bends.clear();
            self.midi_editor_selected_program_changes.clear();
            self.midi_editor_selected_opaque_events.clear();
            self.midi_editor_event_list_cursor = None;
            self.midi_editor_event_list_anchor = None;
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
        self.midi_editor_selected_controllers.clear();
        self.midi_editor_selected_pitch_bends.clear();
        self.midi_editor_selected_program_changes.clear();
        self.midi_editor_selected_opaque_events.clear();
        self.midi_editor_event_list_cursor = None;
        self.midi_editor_event_list_anchor = None;
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
        let source_start_tick = view::midi_source_display_start(item);
        let source_end_tick = view::midi_source_display_end(item);
        let visible_note_spans = item
            .notes()
            .iter()
            .flat_map(|note| {
                view::source_event_occurrences(
                    item,
                    note.tick(),
                    source_start_tick.saturating_sub(note.duration()),
                    source_end_tick,
                )
                .map(move |display_tick| {
                    let local_tick = display_tick.saturating_sub(source_start_tick);
                    let visible_duration = view::visible_note_duration(item, note, display_tick);
                    (note, local_tick, visible_duration)
                })
            })
            .filter(|(_, _, duration)| *duration > 0)
            .collect::<Vec<_>>();
        let visible_notes = visible_note_spans
            .iter()
            .map(|(note, _, _)| *note)
            .collect::<Vec<_>>();
        let first_tick = visible_note_spans
            .iter()
            .map(|(_, local_tick, _)| *local_tick)
            .min()
            .unwrap_or(0);
        let last_tick = visible_note_spans
            .iter()
            .map(|(_, local_tick, duration)| local_tick.saturating_add(*duration))
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
        let binding = if self.shortcut_capture_append {
            let current = commands::staged_binding_for_id(self, &action_id);
            if current.is_empty() {
                binding
            } else {
                format!("{current}; {binding}")
            }
        } else {
            binding
        };
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
                any(target_os = "windows", target_os = "macos", target_os = "android")
            ),
            all(
                feature = "pipewire-backend",
                feature = "cpal-backend",
                any(target_os = "windows", target_os = "macos", target_os = "android")
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
                any(target_os = "windows", target_os = "macos", target_os = "android")
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
                any(target_os = "windows", target_os = "macos", target_os = "android")
            ))
        ))]
        {
            PlaybackBackend::PipeWire
        }
        #[cfg(all(
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos", target_os = "android"),
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
                any(target_os = "windows", target_os = "macos", target_os = "android")
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
            any(target_os = "windows", target_os = "macos", target_os = "android")
        ),
        all(
            feature = "pipewire-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos", target_os = "android")
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
                any(target_os = "windows", target_os = "macos", target_os = "android")
            ))]
            PlaybackBackend::Cpal => audio_config::PlaybackBackendSetting::Cpal,
        }
    }

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
                any(target_os = "windows", target_os = "macos", target_os = "android")
            ))]
            audio_config::PlaybackBackendSetting::Cpal => PlaybackBackend::Cpal,
            _ => self.playback_backend,
        };
    }

    #[cfg(feature = "audio-device")]
    fn playback_name(&self) -> &'static str {
        self.selected_playback_backend().name()
    }

    fn release_midi_preview(&self) {
        #[cfg(feature = "audio-device")]
        if let Some(playback) = &self.playback {
            playback.release_midi_preview();
        }
    }

    #[cfg(feature = "audio-device")]
    fn toggle_repeat(&mut self) -> Task<Message> {
        if self.playback_busy || self.io_busy {
            self.status = "Wait for current operation to finish before changing Repeat".to_owned();
            return Task::none();
        }
        let previous = self.repeat_enabled;
        self.repeat_enabled = !previous;
        if self.playback.is_some() {
            self.repeat_reconfiguration_previous = Some((previous, false));
            let task = self.prepare_playback(self.playhead_sample, self.playback_playing);
            if !self.playback_busy {
                self.restore_repeat_after_failed_reconfiguration("playback could not be prepared");
            }
            task
        } else {
            self.status = if self.repeat_enabled {
                "Repeat enabled".to_owned()
            } else {
                "Repeat disabled".to_owned()
            };
            Task::none()
        }
    }

    #[cfg(feature = "audio-device")]
    fn restore_repeat_after_failed_reconfiguration(&mut self, reason: &str) {
        let Some((previous_enabled, loop_points_changed)) =
            self.repeat_reconfiguration_previous.take()
        else {
            return;
        };
        if self.playback_playing {
            self.pause_playback();
        }
        self.repeat_enabled = previous_enabled;
        self.playback_graph_dirty = loop_points_changed;
        self.status = format!("Repeat change was not applied; playback paused: {reason}");
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
                    #[cfg(target_os = "android")]
                    if let Err(error) = crate::android_platform::start_playback_service() {
                        let _ = playback.stop();
                        self.status = format!(
                            "Could not keep Android playback active in background: {error}"
                        );
                        return Task::none();
                    }
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
                    #[cfg(target_os = "android")]
                    crate::android_platform::stop_playback_service();
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
                #[cfg(target_os = "android")]
                crate::android_platform::stop_playback_service();
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
        #[cfg(all(feature = "audio-device", target_os = "android"))]
        {
            crate::android_platform::set_midi_input_sender(None);
            crate::android_platform::set_midi_output_receiver(None);
        }
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
        #[cfg(target_os = "android")]
        crate::android_platform::stop_playback_service();
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
        let loop_points_ticks = self
            .repeat_enabled
            .then_some(self.timeline.loop_points)
            .flatten()
            .filter(|range| range.start_tick < range.end_tick)
            .map(|range| (range.start_tick, range.end_tick));
        self.playback_busy = true;
        self.status = format!("Preparing playback at sample {target_sample}…");
        let result = Arc::new(Mutex::new(None));
        let message_result = Arc::clone(&result);
        Task::perform(
            run_blocking("aaadaw-playback-prepare", move || {
                prepare_project_playback_file_with_loop_points(
                    path,
                    snapshot,
                    target_sample,
                    loop_points_ticks,
                )
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
                self.restore_repeat_after_failed_reconfiguration(&error.to_string());
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
                self.restore_repeat_after_failed_reconfiguration(&error.to_string());
                tracing::error!(error = %error, "playback instrument preparation failed");
                self.status = format!("Playback preparation failed: {error}");
                return;
            }
        };

        let fx_owner_ids = match self.install_track_fx_processors(&mut prepared) {
            Ok(ids) => ids,
            Err(error) => {
                self.restore_repeat_after_failed_reconfiguration(&error.to_string());
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
                    self.repeat_reconfiguration_previous = None;
                    self.playhead_sample = target_sample;
                    self.seek_sample_query = target_sample.to_string();
                    self.playback_graph_dirty = false;
                    self.playback_position_dirty = false;
                    self.reset_track_meters();
                    if let Err(error) = play_result {
                        self.playback_playing = false;
                        #[cfg(target_os = "android")]
                        crate::android_platform::stop_playback_service();
                        self.status = format!("{} play failed: {error}", self.playback_name());
                        return;
                    }
                    #[cfg(target_os = "android")]
                    if start_when_ready
                        && let Err(error) = crate::android_platform::start_playback_service()
                    {
                        if let Some(playback) = self.playback.as_mut() {
                            let _ = playback.stop();
                        }
                        self.playback_playing = false;
                        self.status = format!(
                            "Could not keep Android playback active in background: {error}"
                        );
                        return;
                    }
                    #[cfg(target_os = "android")]
                    if !start_when_ready {
                        crate::android_platform::stop_playback_service();
                    }
                    self.playback_playing = start_when_ready;
                    self.playback_paused = !start_when_ready && self.playback_paused;
                    self.status = format!("Queued seek to sample {target_sample}");
                }
                Err(error) => {
                    self.restore_repeat_after_failed_reconfiguration(&error.to_string());
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
            any(target_os = "windows", target_os = "macos", target_os = "android")
        ))]
        let output_result = prepared.into_output(
            self.selected_playback_backend(),
            self.audio_settings.cpal_output_device_id.as_deref(),
        );
        #[cfg(not(all(
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos", target_os = "android")
        )))]
        let output_result = prepared.into_output(self.selected_playback_backend(), None);
        let mut playback = match output_result {
            Ok(playback) => playback,
            Err(error) => {
                tracing::error!(backend = self.playback_name(), error = %error, "audio output setup failed");
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
        #[cfg(target_os = "android")]
        let service_error = if start_when_ready && play_error.is_none() {
            crate::android_platform::start_playback_service().err()
        } else {
            None
        };
        #[cfg(not(target_os = "android"))]
        let service_error: Option<String> = None;
        if service_error.is_some() {
            let _ = playback.stop();
        }
        self.playback = Some(playback);
        #[cfg(all(feature = "audio-device", target_os = "android"))]
        let midi_input_target_track = self.selected_midi_input_track_id();
        #[cfg(all(feature = "audio-device", target_os = "android"))]
        if let Some(playback) = self.playback.as_mut() {
            crate::android_platform::set_midi_input_target_track(midi_input_target_track);
            crate::android_platform::set_midi_input_sender(playback.take_midi_input_sender());
            crate::android_platform::set_midi_output_receiver(playback.take_midi_output_receiver());
        }
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
            self.status = format!("{} play failed: {error}", self.playback_name());
            return;
        }
        if let Some(error) = service_error {
            self.playback_playing = false;
            self.status = format!("Could not keep Android playback active in background: {error}");
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

    #[cfg(feature = "audio-device")]
    fn update_playback_stats(&mut self) -> bool {
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
        output_device_lost
    }

    #[cfg(feature = "audio-device")]
    fn handle_playback_device_lost(&mut self) -> Task<Message> {
        tracing::error!(
            backend = self.playback_name(),
            "playback output device was lost"
        );
        let resume_sample = Self::route_loss_resume_sample(
            cfg!(target_os = "android"),
            self.playback_playing,
            self.playhead_sample,
        );
        self.playback_playing = false;
        self.playback_paused = false;
        self.reset_track_meters();
        let cleanup = self.close_playback();

        if let Some(resume_sample) = resume_sample
            && self.project_path.is_some()
        {
            self.playhead_sample = resume_sample;
            self.status = format!(
                "Android audio route changed; reopening output and resuming from sample {resume_sample}…"
            );
            let recovery = self.prepare_playback(resume_sample, true);
            return Task::batch([cleanup, recovery]);
        }

        self.status = "System audio output device unavailable after a route change; playback stopped and stream closed. Reconnect or select an output, then start playback again".to_owned();
        cleanup
    }

    #[cfg(feature = "audio-device")]
    fn route_loss_resume_sample(
        is_android: bool,
        playback_was_running: bool,
        playhead_sample: u64,
    ) -> Option<u64> {
        (is_android && playback_was_running).then_some(playhead_sample)
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
        self.cancel_item_fades();
        let master_mix_before = self.project.master_mix();
        let project_history_cursor_before = self.project.history_cursor();
        #[cfg(feature = "audio-device")]
        let sample_rate_before = self.project.settings().sample_rate();
        let publish_item_mix = action_updates_item_mix(&action);
        #[cfg(feature = "audio-device")]
        let rebuild_playback_graph = action_rebuilds_playback_graph(&action);
        let live_mix_track = match &action {
            DawAction::SetTrackVolume { track_id, .. }
            | DawAction::SetTrackPhase { track_id, .. }
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
                if self.project.history_cursor() > project_history_cursor_before {
                    self.app_undo_history.push(AppHistoryEntry::Project);
                    self.app_redo_history.clear();
                }
                #[cfg(feature = "audio-device")]
                if self.project.settings().sample_rate() != sample_rate_before {
                    let sample_rate_after = self.project.settings().sample_rate();
                    self.playhead_sample = rescale_transport_sample(
                        self.playhead_sample,
                        sample_rate_before,
                        sample_rate_after,
                    );
                    self.playback_start_sample = rescale_transport_sample(
                        self.playback_start_sample,
                        sample_rate_before,
                        sample_rate_after,
                    );
                }
                self.midi_note_clipboard.last_paste = None;
                self.revision = self.revision.wrapping_add(1);
                self.timeline.rebuild(&self.project);
                self.refresh_item_properties_take_selection();
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
                if self.project.master_mix() != master_mix_before {
                    self.sync_master_mix_to_playback();
                }
                if publish_item_mix {
                    for item in self.project.audio_items() {
                        self.sync_item_fades_to_playback(item.id());
                        self.sync_item_volume_to_playback(item.id());
                        self.sync_item_mute_to_playback(item.id());
                        self.sync_item_take_mix_to_playback(item.id());
                    }
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
            let _ = playback.set_track_phase(track_id, track.is_phase_inverted());
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

    fn cancel_item_fades(&mut self) {
        if let Some((item_id, _)) = self.timeline.take_item_fade_preview() {
            self.sync_item_fades_to_playback(item_id);
        }
    }

    fn sync_item_fades_to_playback(&self, item_id: ItemId) {
        #[cfg(feature = "audio-device")]
        if let Some(item) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            && let Some(playback) = &self.playback
        {
            let _ = playback.set_item_fades(item_id, item.fades());
        }
        #[cfg(not(feature = "audio-device"))]
        let _ = item_id;
    }

    fn sync_item_volume_to_playback(&self, item_id: ItemId) {
        #[cfg(feature = "audio-device")]
        if let Some(item) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            && let Some(playback) = &self.playback
        {
            let _ = playback.set_item_volume(item_id, item.volume());
        }
        #[cfg(not(feature = "audio-device"))]
        let _ = item_id;
    }

    fn sync_item_take_mix_to_playback(&self, item_id: ItemId) {
        #[cfg(feature = "audio-device")]
        if let Some(item) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            && let Some(playback) = &self.playback
        {
            let _ = playback.set_item_take_mix(item_id, item.take_volume(), item.take_pan());
        }
        #[cfg(not(feature = "audio-device"))]
        let _ = item_id;
    }

    fn sync_item_mute_to_playback(&self, item_id: ItemId) {
        #[cfg(feature = "audio-device")]
        if let Some(item) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            && let Some(playback) = &self.playback
        {
            let _ = playback.set_item_muted(item_id, item.is_muted());
        }
        #[cfg(not(feature = "audio-device"))]
        let _ = item_id;
    }

    fn sync_item_phase_to_playback(&self, item_id: ItemId) {
        #[cfg(feature = "audio-device")]
        if let Some(item) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            && let Some(playback) = &self.playback
        {
            let _ = playback.set_item_phase(item_id, item.is_phase_inverted());
        }
        #[cfg(not(feature = "audio-device"))]
        let _ = item_id;
    }

    fn sync_all_track_mix_to_playback(&self) {
        self.sync_master_mix_to_playback();
        for item in self.project.audio_items() {
            self.sync_item_fades_to_playback(item.id());
            self.sync_item_volume_to_playback(item.id());
            self.sync_item_mute_to_playback(item.id());
            self.sync_item_phase_to_playback(item.id());
            self.sync_item_take_mix_to_playback(item.id());
        }
        for track in self.project.tracks() {
            self.sync_track_mix_to_playback(track.id());
        }
    }

    fn sync_master_mix_to_playback(&self) {
        #[cfg(feature = "audio-device")]
        if let Some(playback) = &self.playback {
            playback.set_master_mix(self.project.master_mix());
        }
    }

    fn preview_master_mix(&mut self, parameter: TrackMixParameter, value: f32) {
        if !value.is_finite() {
            return;
        }
        self.cancel_track_mix_gesture();
        if self
            .master_mix_gesture
            .is_some_and(|(active, _)| active != parameter)
        {
            if self.master_mix_commit_at.is_some() {
                self.commit_master_mix();
            } else {
                self.cancel_master_mix();
            }
        }
        let current = self
            .master_mix_gesture
            .map_or(self.project.master_mix(), |(_, mix)| mix);
        let mix = match parameter {
            TrackMixParameter::Volume => aaadaw_core::MasterMix::new(
                value.clamp(fader::SILENCE_DB, fader::MAX_DB),
                current.pan(),
            ),
            TrackMixParameter::Pan => {
                aaadaw_core::MasterMix::new(current.volume_db(), value.clamp(-1.0, 1.0))
            }
        };
        if let Ok(mix) = mix {
            self.master_mix_commit_at = None;
            self.master_mix_gesture = Some((parameter, mix));
            #[cfg(feature = "audio-device")]
            if let Some(playback) = &self.playback {
                playback.set_master_mix(mix);
            }
        }
    }

    fn commit_master_mix(&mut self) {
        self.master_mix_commit_at = None;
        if let Some((_, mix)) = self.master_mix_gesture.take() {
            if mix != self.project.master_mix() {
                self.apply_action(DawAction::SetMasterMix { mix }, "Master mix changed");
            } else {
                self.sync_master_mix_to_playback();
            }
        }
    }

    fn cancel_master_mix(&mut self) {
        self.master_mix_commit_at = None;
        if self.master_mix_gesture.take().is_some() {
            self.sync_master_mix_to_playback();
        }
    }

    fn reset_master_mix(&mut self, parameter: TrackMixParameter) {
        // A double click's first press can have queued a gesture: replace that preview
        // so the reset remains one history entry, like the ordinary track controls.
        if self
            .master_mix_gesture
            .is_some_and(|(active, _)| active != parameter)
        {
            self.commit_master_mix();
        }
        self.cancel_master_mix();
        let current = self.project.master_mix();
        let mix = match parameter {
            TrackMixParameter::Volume => aaadaw_core::MasterMix::new(0.0, current.pan()),
            TrackMixParameter::Pan => aaadaw_core::MasterMix::new(current.volume_db(), 0.0),
        }
        .expect("existing Master controls are validated");
        if mix != current {
            self.apply_action(DawAction::SetMasterMix { mix }, "Master mix reset");
        }
    }

    fn preview_track_mix(&mut self, track_id: TrackId, parameter: TrackMixParameter, value: f32) {
        self.cancel_master_mix();
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
            TrackMixParameter::Volume => {
                gesture.after_volume_db = value.clamp(fader::SILENCE_DB, fader::MAX_DB)
            }
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
                && item.signed_source_offset_ticks() == source_offset_ticks
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
    ) -> Result<Option<(u64, i64, u64)>, String> {
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
        if item.active_take_is_empty() {
            return Ok(Some((start_sample, 0, end_sample - start_sample)));
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
            .and_then(|offset| i64::try_from(offset).ok())
            .ok_or_else(|| "Audio trim exceeds the supported signed source range".to_owned())?;
        let length_samples = end_sample - start_sample;
        Ok(Some((start_sample, source_offset_samples, length_samples)))
    }

    fn midi_item_trim_bounds(
        &self,
        item: &aaadaw_core::MidiItem,
        preview: timeline::ItemTrimPreview,
    ) -> Result<(u64, u64, i64), String> {
        let length_ticks = preview
            .end_tick
            .checked_sub(preview.start_tick)
            .filter(|length| *length > 0)
            .ok_or_else(|| "MIDI Items must remain at least one tick long".to_owned())?;
        let source_offset_ticks = if preview.edge == timeline::ItemTrimEdge::Start {
            let delta = i128::from(preview.start_tick) - i128::from(item.start_tick());
            let offset = i64::try_from(i128::from(item.signed_source_offset_ticks()) + delta)
                .map_err(|_| "MIDI trim exceeds the supported source range".to_owned())?;
            if offset < 0 {
                return Err("MIDI trim cannot extend before the source content".to_owned());
            }
            offset
        } else {
            item.signed_source_offset_ticks()
        };
        i64::try_from(length_ticks)
            .ok()
            .and_then(|length| source_offset_ticks.checked_add(length))
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
                    actions.push(DawAction::DuplicateAudioItemAt {
                        item_id: item.id(),
                        track_id: target_track_id,
                        start_sample: target_sample,
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

    fn delete_by_focus(&mut self, no_prompting: bool) -> Task<Message> {
        match self.timeline.delete_focus {
            timeline::DeleteFocusTarget::Arrange => self.delete_selected_items(),
            timeline::DeleteFocusTarget::Automation => {
                if let Some(event) = self.timeline.selected_automation_point_delete_event() {
                    return self.update(Message::Timeline(event));
                }
            }
            timeline::DeleteFocusTarget::TrackPanel => {
                let selected_track_ids = self.selected_track_ids();
                if selected_track_ids.is_empty() {
                    return Task::none();
                }
                if no_prompting {
                    self.delete_selected_tracks(selected_track_ids);
                } else {
                    self.pending_focused_track_deletion = Some(selected_track_ids);
                }
            }
        }
        Task::none()
    }

    fn confirm_focused_track_deletion(&mut self) {
        if let Some(track_ids) = self.pending_focused_track_deletion.take() {
            self.delete_selected_tracks(track_ids);
        }
    }

    fn delete_selected_tracks(&mut self, track_ids: Vec<TrackId>) {
        if self.project_graph_edit_busy() {
            self.status = "Stop playback or recording before deleting tracks".into();
            return;
        }
        let selected = track_ids
            .into_iter()
            .filter(|id| self.project.tracks().iter().any(|track| track.id() == *id))
            .collect::<HashSet<_>>();
        if selected.is_empty() {
            return;
        }

        let mut remaining = selected.clone();
        let mut ordered = Vec::with_capacity(remaining.len());
        while !remaining.is_empty() {
            let next = self.project.tracks().iter().find_map(|candidate| {
                (remaining.contains(&candidate.id())
                    && !self.project.tracks().iter().any(|dependent| {
                        dependent.id() != candidate.id()
                            && remaining.contains(&dependent.id())
                            && (dependent.parent_track() == Some(candidate.id())
                                || dependent.output_track() == Some(candidate.id())
                                || dependent
                                    .sends()
                                    .iter()
                                    .any(|send| send.destination() == candidate.id()))
                    }))
                .then_some(candidate.id())
            });
            let Some(track_id) = next else {
                self.status = "Selected tracks have a cyclic deletion dependency".into();
                return;
            };
            remaining.remove(&track_id);
            ordered.push(track_id);
        }

        if self
            .track_mix_gesture
            .is_some_and(|gesture| selected.contains(&gesture.track_id))
        {
            self.cancel_track_mix_gesture();
        }
        let revision = self.revision;
        let actions = ordered
            .iter()
            .copied()
            .map(|track_id| DawAction::DeleteTrack { track_id })
            .collect::<Vec<_>>();
        let action = if actions.len() == 1 {
            actions.into_iter().next().expect("one selected track")
        } else {
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            }
        };
        self.apply_action(action, "Selected tracks deleted");
        if self.revision != revision {
            for track_id in selected {
                self.clear_track_drafts(track_id);
            }
        }
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
            -1 => (0..index).rev().find(|candidate| {
                self.project
                    .folder_depth(self.project.tracks()[*candidate].id())
                    <= self.project.folder_depth(track_id)
            }),
            1 => self
                .project
                .tracks()
                .get(index + self.project.track_subtree_len(track_id))
                .map(|next| index + self.project.track_subtree_len(next.id())),
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

    fn can_undo_action(&self) -> bool {
        self.app_selection_is_next_undo() || self.project.can_undo()
    }

    fn can_redo_action(&self) -> bool {
        self.app_selection_is_next_redo() || self.project.can_redo()
    }

    fn app_selection_is_next_undo(&self) -> bool {
        self.item_selection_is_next_undo() || self.inline_midi_note_selection_is_next_undo()
    }

    fn app_selection_is_next_redo(&self) -> bool {
        self.item_selection_is_next_redo() || self.inline_midi_note_selection_is_next_redo()
    }

    fn inline_midi_note_selection_is_next_undo(&self) -> bool {
        matches!(
            self.app_undo_history.last(),
            Some(AppHistoryEntry::InlineMidiSelection(history))
                if history.project_history_cursor == self.project.history_cursor()
        )
    }

    fn inline_midi_note_selection_is_next_redo(&self) -> bool {
        matches!(
            self.app_redo_history.last(),
            Some(AppHistoryEntry::InlineMidiSelection(history))
                if history.project_history_cursor == self.project.history_cursor()
        )
    }

    fn inline_midi_copy_selection_is_next_undo(&self) -> Option<InlineMidiSelectionHistory> {
        match self.app_undo_history.last() {
            Some(AppHistoryEntry::ProjectWithInlineMidiSelection(history))
                if history.project_history_cursor == self.project.history_cursor() =>
            {
                Some(history.clone())
            }
            _ => None,
        }
    }

    fn project_item_selection_is_next_undo(&self) -> Option<ItemSelectionHistory> {
        match self.app_undo_history.last() {
            Some(AppHistoryEntry::ProjectWithItemSelection(history))
                if history.project_history_cursor_after == self.project.history_cursor() =>
            {
                Some(history.clone())
            }
            _ => None,
        }
    }

    fn project_item_selection_is_next_redo(&self) -> Option<ItemSelectionHistory> {
        match self.app_redo_history.last() {
            Some(AppHistoryEntry::ProjectWithItemSelection(history))
                if history.project_history_cursor_before == self.project.history_cursor() =>
            {
                Some(history.clone())
            }
            _ => None,
        }
    }

    fn restore_project_item_selection_history(
        &mut self,
        history: &ItemSelectionHistory,
        redo: bool,
    ) {
        self.timeline.restore_item_selection_state(if redo {
            &history.after
        } else {
            &history.before
        });
    }

    fn restore_inline_midi_selection_history(
        &mut self,
        history: &InlineMidiSelectionHistory,
        redo: bool,
    ) {
        let selection = if redo {
            &history.after
        } else {
            &history.before
        };
        if selection.is_empty() {
            self.timeline
                .inline_midi_note_selection
                .remove(&history.item_id);
        } else {
            self.timeline
                .inline_midi_note_selection
                .insert(history.item_id, selection.clone());
        }

        if self.midi_editor_item_id == Some(history.item_id) {
            let editor_selection = if redo {
                &history.editor_after
            } else {
                &history.editor_before
            };
            if let Some(editor_selection) = editor_selection {
                self.restore_midi_editor_selection_state(editor_selection);
            }
        }
    }

    fn midi_editor_selection_state(&self) -> MidiEditorSelectionState {
        MidiEditorSelectionState {
            notes: self.midi_editor_selected_notes.clone(),
            controllers: self.midi_editor_selected_controllers.clone(),
            pitch_bends: self.midi_editor_selected_pitch_bends.clone(),
            program_changes: self.midi_editor_selected_program_changes.clone(),
            poly_aftertouches: self.midi_editor_selected_poly_aftertouches.clone(),
            channel_pressures: self.midi_editor_selected_channel_pressures.clone(),
            opaque_events: self.midi_editor_selected_opaque_events.clone(),
            event_list_cursor: self.midi_editor_event_list_cursor,
            event_list_anchor: self.midi_editor_event_list_anchor,
        }
    }

    fn restore_midi_editor_selection_state(&mut self, selection: &MidiEditorSelectionState) {
        self.midi_editor_selected_notes = selection.notes.clone();
        self.midi_editor_selected_controllers = selection.controllers.clone();
        self.midi_editor_selected_pitch_bends = selection.pitch_bends.clone();
        self.midi_editor_selected_program_changes = selection.program_changes.clone();
        self.midi_editor_selected_poly_aftertouches = selection.poly_aftertouches.clone();
        self.midi_editor_selected_channel_pressures = selection.channel_pressures.clone();
        self.midi_editor_selected_opaque_events = selection.opaque_events.clone();
        self.midi_editor_event_list_cursor = selection.event_list_cursor;
        self.midi_editor_event_list_anchor = selection.event_list_anchor;
    }

    fn midi_editor_selected_events_changed(
        before: &MidiEditorSelectionState,
        after: &MidiEditorSelectionState,
    ) -> bool {
        before.notes != after.notes
            || before.controllers != after.controllers
            || before.pitch_bends != after.pitch_bends
            || before.program_changes != after.program_changes
            || before.poly_aftertouches != after.poly_aftertouches
            || before.channel_pressures != after.channel_pressures
            || before.opaque_events != after.opaque_events
    }

    fn record_midi_editor_selection_history(
        &mut self,
        item_id: ItemId,
        before_inline: HashSet<NoteId>,
        before_editor: MidiEditorSelectionState,
    ) {
        if self.midi_editor_selected_notes.is_empty() {
            self.timeline.inline_midi_note_selection.remove(&item_id);
        } else {
            self.timeline
                .inline_midi_note_selection
                .insert(item_id, self.midi_editor_selected_notes.clone());
        }
        let after_inline = self
            .timeline
            .inline_midi_note_selection
            .get(&item_id)
            .cloned()
            .unwrap_or_default();
        let after_editor = self.midi_editor_selection_state();
        if before_inline == after_inline && before_editor == after_editor {
            return;
        }

        self.project.discard_redo_history();
        self.app_redo_history.clear();
        self.app_undo_history
            .push(AppHistoryEntry::InlineMidiSelection(
                InlineMidiSelectionHistory {
                    item_id,
                    before: before_inline,
                    after: after_inline,
                    editor_before: Some(before_editor),
                    editor_after: Some(after_editor),
                    project_history_cursor: self.project.history_cursor(),
                },
            ));
    }

    fn move_midi_editor_event_list_cursor_with_history(
        &mut self,
        direction: MidiEditorEventListCursorDirection,
        preserve_selection: bool,
        extend_selection: bool,
    ) {
        let Some(item_id) = self.midi_editor_item_id else {
            self.move_midi_editor_event_list_cursor(
                direction,
                preserve_selection,
                extend_selection,
            );
            return;
        };
        let before_editor = self.midi_editor_selection_state();
        let before_inline = self
            .timeline
            .inline_midi_note_selection
            .get(&item_id)
            .cloned()
            .unwrap_or_default();
        self.move_midi_editor_event_list_cursor(direction, preserve_selection, extend_selection);
        let after_editor = self.midi_editor_selection_state();
        if Self::midi_editor_selected_events_changed(&before_editor, &after_editor) {
            self.record_midi_editor_selection_history(item_id, before_inline, before_editor);
        }
    }

    fn item_selection_is_next_undo(&self) -> bool {
        matches!(
            self.app_undo_history.last(),
            Some(AppHistoryEntry::ItemSelection {
                project_history_cursor,
                ..
            }) if *project_history_cursor == self.project.history_cursor()
        )
    }

    fn item_selection_is_next_redo(&self) -> bool {
        matches!(
            self.app_redo_history.last(),
            Some(AppHistoryEntry::ItemSelection {
                project_history_cursor,
                ..
            }) if *project_history_cursor == self.project.history_cursor()
        )
    }

    fn clear_app_history(&mut self) {
        self.app_undo_history.clear();
        self.app_redo_history.clear();
    }

    fn with_item_selection_history(&mut self, label: &'static str, action: impl FnOnce(&mut Self)) {
        let before = self.timeline.item_selection_state();
        action(self);
        let after = self.timeline.item_selection_state();
        if before == after {
            return;
        }

        self.project.discard_redo_history();
        self.app_redo_history.clear();
        self.app_undo_history.push(AppHistoryEntry::ItemSelection {
            label,
            before: Box::new(before),
            after: Box::new(after),
            project_history_cursor: self.project.history_cursor(),
        });
    }

    fn with_inline_midi_note_selection_history(
        &mut self,
        item_id: ItemId,
        note_ids: Vec<aaadaw_core::NoteId>,
    ) {
        let before = self
            .timeline
            .inline_midi_note_selection
            .get(&item_id)
            .cloned()
            .unwrap_or_default();
        let editor_before =
            (self.midi_editor_item_id == Some(item_id)).then(|| self.midi_editor_selection_state());

        self.handle_timeline_view_event(timeline::TimelineEvent::SetInlineMidiNoteSelection {
            item_id,
            note_ids,
        });

        let after = self
            .timeline
            .inline_midi_note_selection
            .get(&item_id)
            .cloned()
            .unwrap_or_default();
        let editor_after = editor_before.as_ref().map(|before| {
            let mut editor_after = before.clone();
            editor_after.notes = after.clone();
            editor_after
        });
        if let Some(editor_after) = editor_after.as_ref() {
            self.restore_midi_editor_selection_state(editor_after);
        }
        if before == after && editor_before == editor_after {
            return;
        }

        self.project.discard_redo_history();
        self.app_redo_history.clear();
        self.app_undo_history
            .push(AppHistoryEntry::InlineMidiSelection(
                InlineMidiSelectionHistory {
                    item_id,
                    before,
                    after,
                    editor_before,
                    editor_after,
                    project_history_cursor: self.project.history_cursor(),
                },
            ));
    }

    fn with_midi_editor_note_selection_history(&mut self, note_ids: HashSet<aaadaw_core::NoteId>) {
        let Some(item_id) = self.midi_editor_item_id else {
            self.select_midi_notes(note_ids);
            return;
        };
        let before = self
            .timeline
            .inline_midi_note_selection
            .get(&item_id)
            .cloned()
            .unwrap_or_default();
        let editor_before = self.midi_editor_selection_state();

        self.select_midi_notes(note_ids);
        if self.midi_editor_selected_notes.is_empty() {
            self.timeline.inline_midi_note_selection.remove(&item_id);
        } else {
            self.timeline
                .inline_midi_note_selection
                .insert(item_id, self.midi_editor_selected_notes.clone());
        }

        let after = self
            .timeline
            .inline_midi_note_selection
            .get(&item_id)
            .cloned()
            .unwrap_or_default();
        let editor_after = self.midi_editor_selection_state();
        if before == after && editor_before == editor_after {
            return;
        }

        self.project.discard_redo_history();
        self.app_redo_history.clear();
        self.app_undo_history
            .push(AppHistoryEntry::InlineMidiSelection(
                InlineMidiSelectionHistory {
                    item_id,
                    before,
                    after,
                    editor_before: Some(editor_before),
                    editor_after: Some(editor_after),
                    project_history_cursor: self.project.history_cursor(),
                },
            ));
    }

    fn with_midi_editor_event_selection_history(
        &mut self,
        cursor: MidiEditorEventListCursor,
        additive: bool,
    ) -> Task<Message> {
        let Some(item_id) = self.midi_editor_item_id else {
            return Task::none();
        };
        let before_editor = self.midi_editor_selection_state();
        let before_inline = self
            .timeline
            .inline_midi_note_selection
            .get(&item_id)
            .cloned()
            .unwrap_or_default();
        let message = match cursor {
            MidiEditorEventListCursor::Note(note_id) => {
                Message::SelectMidiNoteEvent(note_id, additive)
            }
            MidiEditorEventListCursor::Controller(event) => {
                Message::SelectMidiControllerEvent(item_id, event, additive)
            }
            MidiEditorEventListCursor::BankSelect(msb, lsb) => {
                Message::SelectMidiBankSelectEvent(item_id, msb, lsb, additive)
            }
            MidiEditorEventListCursor::PitchBend(event) => {
                Message::SelectMidiPitchBendEvent(item_id, event, additive)
            }
            MidiEditorEventListCursor::ProgramChange(event) => {
                Message::SelectMidiProgramChangeEvent(item_id, event, additive)
            }
            MidiEditorEventListCursor::PolyAftertouch(event) => {
                Message::SelectMidiPolyAftertouchEvent(item_id, event, additive)
            }
            MidiEditorEventListCursor::ChannelPressure(event) => {
                Message::SelectMidiChannelPressureEvent(item_id, event, additive)
            }
            MidiEditorEventListCursor::Opaque(index) => {
                Message::SelectMidiOpaqueEvent(item_id, index, additive)
            }
            MidiEditorEventListCursor::End => Message::SelectMidiEditorEventListEnd(additive),
        };
        let task = self.update(message);
        self.record_midi_editor_selection_history(item_id, before_inline, before_editor);
        task
    }

    fn apply_midi_editor_filter_selection(
        &mut self,
        operation: MidiEditorFilterSelectionOperation,
    ) {
        let Some(item_id) = self.midi_editor_item_id else {
            return;
        };
        let Some(matching) = self.midi_editor_filter_event_selection(item_id) else {
            return;
        };
        let before_editor = self.midi_editor_selection_state();
        let before_inline = self
            .timeline
            .inline_midi_note_selection
            .get(&item_id)
            .cloned()
            .unwrap_or_default();

        match operation {
            MidiEditorFilterSelectionOperation::Set => {
                self.midi_editor_selected_notes.clear();
                self.midi_editor_selected_controllers.clear();
                self.midi_editor_selected_pitch_bends.clear();
                self.midi_editor_selected_program_changes.clear();
                self.midi_editor_selected_poly_aftertouches.clear();
                self.midi_editor_selected_channel_pressures.clear();
                self.midi_editor_selected_opaque_events.clear();
                self.midi_editor_selected_notes.extend(matching.notes);
                self.midi_editor_selected_controllers
                    .extend(matching.controllers);
                self.midi_editor_selected_pitch_bends
                    .extend(matching.pitch_bends);
                self.midi_editor_selected_program_changes
                    .extend(matching.program_changes);
                self.midi_editor_selected_poly_aftertouches
                    .extend(matching.poly_aftertouches);
                self.midi_editor_selected_channel_pressures
                    .extend(matching.channel_pressures);
                self.midi_editor_selected_opaque_events
                    .extend(matching.opaque_events);
            }
            MidiEditorFilterSelectionOperation::Add => {
                self.midi_editor_selected_notes.extend(matching.notes);
                self.midi_editor_selected_controllers
                    .extend(matching.controllers);
                self.midi_editor_selected_pitch_bends
                    .extend(matching.pitch_bends);
                self.midi_editor_selected_program_changes
                    .extend(matching.program_changes);
                self.midi_editor_selected_poly_aftertouches
                    .extend(matching.poly_aftertouches);
                self.midi_editor_selected_channel_pressures
                    .extend(matching.channel_pressures);
                self.midi_editor_selected_opaque_events
                    .extend(matching.opaque_events);
            }
            MidiEditorFilterSelectionOperation::Remove => {
                self.midi_editor_selected_notes
                    .retain(|event| !matching.notes.contains(event));
                self.midi_editor_selected_controllers
                    .retain(|event| !matching.controllers.contains(event));
                self.midi_editor_selected_pitch_bends
                    .retain(|event| !matching.pitch_bends.contains(event));
                self.midi_editor_selected_program_changes
                    .retain(|event| !matching.program_changes.contains(event));
                self.midi_editor_selected_poly_aftertouches
                    .retain(|event| !matching.poly_aftertouches.contains(event));
                self.midi_editor_selected_channel_pressures
                    .retain(|event| !matching.channel_pressures.contains(event));
                self.midi_editor_selected_opaque_events
                    .retain(|event| !matching.opaque_events.contains(event));
            }
        }

        self.midi_editor_feedback = None;
        self.record_midi_editor_selection_history(item_id, before_inline, before_editor);
    }

    fn clear_midi_editor_filter(&mut self) {
        self.midi_editor_channel_selection = MidiEditorChannelSelection::AllChannels;
        self.midi_editor_event_type_filter = MidiEditorEventTypeFilter::AllEvents;
        self.midi_editor_note_filter = MidiEditorNoteFilter::default();
        self.midi_editor_controller_filter = MidiEditorControllerFilter::default();
        self.midi_editor_pitch_bend_filter = MidiEditorPitchBendFilter::default();
        self.midi_editor_program_change_filter = MidiEditorProgramChangeFilter::default();
        self.midi_editor_position_filter = MidiEditorPositionFilter::default();
        self.midi_editor_filter_position_low.clear();
        self.midi_editor_filter_position_high.clear();
        self.midi_editor_filter_note_length_low.clear();
        self.midi_editor_filter_note_length_high.clear();
        self.midi_editor_filter_value_low.clear();
        self.midi_editor_filter_value_high.clear();
        self.midi_editor_draw_channel = 0;
        self.midi_editor_filter_enabled = false;
        self.midi_editor_event_filter_enabled = false;
        self.midi_editor_event_list_filter_active = false;
        self.midi_editor_filter_channels = u16::MAX;
        self.midi_editor_filter_channels_initialized = true;
        self.midi_editor_filter_inverted = false;
    }

    fn set_midi_editor_filter_from_selection(&mut self) {
        let has_notes = !self.midi_editor_selected_notes.is_empty();
        let has_controllers = !self.midi_editor_selected_controllers.is_empty();
        let has_pitch_bends = !self.midi_editor_selected_pitch_bends.is_empty();
        let has_program_changes = !self.midi_editor_selected_program_changes.is_empty();
        let has_poly_aftertouches = !self.midi_editor_selected_poly_aftertouches.is_empty();
        let has_channel_pressures = !self.midi_editor_selected_channel_pressures.is_empty();
        let has_opaque_events = !self.midi_editor_selected_opaque_events.is_empty();
        let selected_kinds = [
            has_notes,
            has_controllers,
            has_pitch_bends,
            has_program_changes,
            has_poly_aftertouches,
            has_channel_pressures,
            has_opaque_events,
        ]
        .into_iter()
        .filter(|selected| *selected)
        .count();
        if selected_kinds == 0 {
            return;
        }
        if has_opaque_events
            && selected_kinds != 1
            && !(selected_kinds == 2 && (has_controllers || has_notes || has_pitch_bends))
            && !(selected_kinds == 2 && has_program_changes)
            && !(selected_kinds == 2 && has_poly_aftertouches)
            && !(selected_kinds == 2 && has_channel_pressures)
            && !(selected_kinds == 3 && has_notes && has_controllers)
            && !(selected_kinds == 3 && has_notes && has_pitch_bends)
            && !(selected_kinds == 3 && has_controllers && has_pitch_bends)
            && !(selected_kinds == 3 && has_controllers && has_program_changes)
            && !(selected_kinds == 3 && has_controllers && has_channel_pressures)
            && !(selected_kinds == 4
                && has_controllers
                && has_program_changes
                && has_channel_pressures)
            && !(selected_kinds == 4
                && has_controllers
                && has_program_changes
                && has_poly_aftertouches)
            && !(selected_kinds == 3 && has_program_changes && has_channel_pressures)
            && !(selected_kinds == 3 && has_program_changes && has_poly_aftertouches)
            && !(selected_kinds == 4
                && has_program_changes
                && has_poly_aftertouches
                && has_channel_pressures)
        {
            return;
        }
        let Some(item_id) = self.midi_editor_item_id else {
            return;
        };
        if !self
            .project
            .midi_items()
            .iter()
            .any(|item| item.id() == item_id)
        {
            return;
        }

        // Channel-bearing-only selections derive criteria with the Event List
        // funnel off. REAPER activates the funnel for Note+opaque selections
        // without enabling Show-only; CC+opaque also enables Show-only.
        self.midi_editor_event_list_filter_active = false;
        self.midi_editor_event_filter_enabled = false;

        if selected_kinds > 1 {
            if has_opaque_events {
                if has_notes && has_controllers {
                    self.set_midi_editor_filter_from_mixed_note_controller_and_opaque_selection(
                        item_id,
                    );
                } else if has_notes && has_pitch_bends {
                    self.set_midi_editor_filter_from_mixed_note_pitch_and_opaque_selection(item_id);
                } else if has_controllers && has_pitch_bends {
                    self.set_midi_editor_filter_from_mixed_controller_pitch_and_opaque_selection(
                        item_id,
                    );
                } else if has_controllers && has_channel_pressures {
                    self.set_midi_editor_filter_from_mixed_controller_channel_pressure_and_opaque_selection(
                        item_id,
                    );
                } else if has_controllers && has_program_changes && has_channel_pressures {
                    self.set_midi_editor_filter_from_mixed_controller_program_channel_pressure_and_opaque_selection(
                        item_id,
                    );
                } else if has_controllers && has_program_changes && has_poly_aftertouches {
                    self.set_midi_editor_filter_from_mixed_controller_program_poly_aftertouch_and_opaque_selection(
                        item_id,
                    );
                } else if has_controllers && has_program_changes {
                    self.set_midi_editor_filter_from_mixed_controller_program_and_opaque_selection(
                        item_id,
                    );
                } else if has_program_changes && has_poly_aftertouches && has_channel_pressures {
                    self.set_midi_editor_filter_from_mixed_program_poly_channel_pressure_and_opaque_selection(
                        item_id,
                    );
                } else if has_program_changes && has_poly_aftertouches {
                    self.set_midi_editor_filter_from_mixed_program_poly_aftertouch_and_opaque_selection(
                        item_id,
                    );
                } else if has_program_changes && has_channel_pressures {
                    self.set_midi_editor_filter_from_mixed_program_channel_pressure_and_opaque_selection(
                        item_id,
                    );
                } else if has_program_changes {
                    self.set_midi_editor_filter_from_mixed_program_and_opaque_selection(item_id);
                } else if has_poly_aftertouches {
                    self.set_midi_editor_filter_from_mixed_poly_aftertouch_and_opaque_selection(
                        item_id,
                    );
                } else if has_channel_pressures {
                    self.set_midi_editor_filter_from_mixed_channel_pressure_and_opaque_selection(
                        item_id,
                    );
                } else if has_notes {
                    self.set_midi_editor_filter_from_mixed_note_and_opaque_selection(item_id);
                } else if has_pitch_bends {
                    self.set_midi_editor_filter_from_mixed_pitch_and_opaque_selection(item_id);
                } else {
                    self.set_midi_editor_filter_from_mixed_controller_and_opaque_selection(item_id);
                }
            } else {
                self.set_midi_editor_filter_from_mixed_channel_event_selection(item_id);
            }
            return;
        }

        match () {
            _ if has_opaque_events => self.set_midi_editor_filter_from_selected_opaque_events(),
            _ if has_notes => self.set_midi_editor_filter_from_selected_notes(),
            _ if has_controllers => self.set_midi_editor_filter_from_selected_controllers(),
            _ if has_pitch_bends => self.set_midi_editor_filter_from_selected_pitch_bends(),
            _ if has_program_changes => self.set_midi_editor_filter_from_selected_program_changes(),
            _ if has_poly_aftertouches => {
                self.set_midi_editor_filter_from_selected_poly_aftertouches()
            }
            _ if has_channel_pressures => {
                self.set_midi_editor_filter_from_selected_channel_pressures()
            }
            _ => {}
        }
    }

    fn set_midi_editor_filter_from_selected_opaque_events(&mut self) {
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
        let mut selected_ticks = item
            .opaque_events()
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                self.midi_editor_selected_opaque_events
                    .contains(&(*index as u32))
            })
            .map(|(_, event)| event.tick);
        let Some(first_tick) = selected_ticks.next() else {
            return;
        };
        let ppq = self.project.settings().ppq();
        let measure_ticks = midi_editor_measure_length_ticks(
            ppq,
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let (position_low, position_high) = selected_ticks.fold(
            (
                midi_editor_filter_position_value(first_tick, measure_ticks),
                midi_editor_filter_position_high_value(first_tick, measure_ticks),
            ),
            |(position_low, position_high), tick| {
                (
                    position_low.min(midi_editor_filter_position_value(tick, measure_ticks)),
                    position_high.max(midi_editor_filter_position_high_value(tick, measure_ticks)),
                )
            },
        );

        // REAPER groups Text Meta and SysEx under one type and automatically
        // enables Show-only when deriving the filter from opaque selections.
        self.midi_editor_channel_selection = MidiEditorChannelSelection::AllChannels;
        self.midi_editor_filter_channels = u16::MAX;
        self.midi_editor_filter_channels_initialized = true;
        self.midi_editor_event_type_filter = MidiEditorEventTypeFilter::SysExMeta;
        self.midi_editor_note_filter = MidiEditorNoteFilter::default();
        self.midi_editor_controller_filter = MidiEditorControllerFilter::default();
        self.midi_editor_pitch_bend_filter = MidiEditorPitchBendFilter::default();
        self.midi_editor_program_change_filter = MidiEditorProgramChangeFilter::default();
        self.midi_editor_position_filter = MidiEditorPositionFilter {
            enabled: true,
            range_initialized: true,
            low: position_low,
            high: position_high,
            repeat_every: MidiEditorFilterRepeat::Measure,
        };
        self.midi_editor_filter_value_low.clear();
        self.midi_editor_filter_value_high.clear();
        self.midi_editor_filter_position_low = format_midi_editor_filter_position(position_low);
        self.midi_editor_filter_position_high = format_midi_editor_filter_position(position_high);
        self.midi_editor_filter_note_length_low.clear();
        self.midi_editor_filter_note_length_high.clear();
        self.midi_editor_event_filter_enabled = true;
        self.midi_editor_event_list_filter_active = true;
    }

    fn set_midi_editor_filter_from_mixed_controller_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .controllers()
            .iter()
            .filter(|event| self.midi_editor_selected_controllers.contains(event))
            .map(|event| event.tick)
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            true,
        );
    }

    fn set_midi_editor_filter_from_mixed_program_and_opaque_selection(&mut self, item_id: ItemId) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .program_changes()
            .iter()
            .filter(|event| self.midi_editor_selected_program_changes.contains(event))
            .map(|event| event.tick)
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            false,
        );
    }

    fn set_midi_editor_filter_from_mixed_controller_program_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .controllers()
            .iter()
            .filter(|event| self.midi_editor_selected_controllers.contains(event))
            .map(|event| event.tick)
            .chain(
                item.program_changes()
                    .iter()
                    .filter(|event| self.midi_editor_selected_program_changes.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            false,
        );
    }

    fn set_midi_editor_filter_from_mixed_controller_program_poly_aftertouch_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .controllers()
            .iter()
            .filter(|event| self.midi_editor_selected_controllers.contains(event))
            .map(|event| event.tick)
            .chain(
                item.program_changes()
                    .iter()
                    .filter(|event| self.midi_editor_selected_program_changes.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.poly_aftertouches()
                    .iter()
                    .filter(|event| self.midi_editor_selected_poly_aftertouches.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            false,
        );
        // REAPER leaves the funnel off for CC + Program Change + Poly Aftertouch + opaque.
        self.midi_editor_event_list_filter_active = false;
    }

    fn set_midi_editor_filter_from_mixed_program_channel_pressure_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .program_changes()
            .iter()
            .filter(|event| self.midi_editor_selected_program_changes.contains(event))
            .map(|event| event.tick)
            .chain(
                item.channel_pressures()
                    .iter()
                    .filter(|event| self.midi_editor_selected_channel_pressures.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            true,
        );
    }

    fn set_midi_editor_filter_from_mixed_program_poly_aftertouch_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .program_changes()
            .iter()
            .filter(|event| self.midi_editor_selected_program_changes.contains(event))
            .map(|event| event.tick)
            .chain(
                item.poly_aftertouches()
                    .iter()
                    .filter(|event| self.midi_editor_selected_poly_aftertouches.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            true,
        );
    }

    fn set_midi_editor_filter_from_mixed_program_poly_channel_pressure_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .program_changes()
            .iter()
            .filter(|event| self.midi_editor_selected_program_changes.contains(event))
            .map(|event| event.tick)
            .chain(
                item.poly_aftertouches()
                    .iter()
                    .filter(|event| self.midi_editor_selected_poly_aftertouches.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.channel_pressures()
                    .iter()
                    .filter(|event| self.midi_editor_selected_channel_pressures.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            false,
        );
    }

    fn set_midi_editor_filter_from_mixed_poly_aftertouch_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .poly_aftertouches()
            .iter()
            .filter(|event| self.midi_editor_selected_poly_aftertouches.contains(event))
            .map(|event| event.tick)
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            false,
        );
    }

    fn set_midi_editor_filter_from_mixed_channel_pressure_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .channel_pressures()
            .iter()
            .filter(|event| self.midi_editor_selected_channel_pressures.contains(event))
            .map(|event| event.tick)
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            false,
        );
    }

    fn set_midi_editor_filter_from_mixed_controller_channel_pressure_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .controllers()
            .iter()
            .filter(|event| self.midi_editor_selected_controllers.contains(event))
            .map(|event| event.tick)
            .chain(
                item.channel_pressures()
                    .iter()
                    .filter(|event| self.midi_editor_selected_channel_pressures.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            false,
        );
        // REAPER keeps the Event List funnel off after this three-kind Set.
        self.midi_editor_event_list_filter_active = false;
    }

    fn set_midi_editor_filter_from_mixed_controller_program_channel_pressure_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .controllers()
            .iter()
            .filter(|event| self.midi_editor_selected_controllers.contains(event))
            .map(|event| event.tick)
            .chain(
                item.program_changes()
                    .iter()
                    .filter(|event| self.midi_editor_selected_program_changes.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.channel_pressures()
                    .iter()
                    .filter(|event| self.midi_editor_selected_channel_pressures.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            false,
        );
        // REAPER keeps Show-only and the Event List funnel off after this
        // four-kind Set selection.
        self.midi_editor_event_list_filter_active = false;
    }

    fn set_midi_editor_filter_from_mixed_note_and_opaque_selection(&mut self, item_id: ItemId) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .notes()
            .iter()
            .filter(|event| self.midi_editor_selected_notes.contains(&event.id()))
            .map(|event| event.tick())
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            false,
        );
    }

    fn set_midi_editor_filter_from_mixed_pitch_and_opaque_selection(&mut self, item_id: ItemId) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .pitch_bends()
            .iter()
            .filter(|event| self.midi_editor_selected_pitch_bends.contains(event))
            .map(|event| event.tick)
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            true,
        );
    }

    fn set_midi_editor_filter_from_mixed_controller_pitch_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .controllers()
            .iter()
            .filter(|event| self.midi_editor_selected_controllers.contains(event))
            .map(|event| event.tick)
            .chain(
                item.pitch_bends()
                    .iter()
                    .filter(|event| self.midi_editor_selected_pitch_bends.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            true,
        );
    }

    fn set_midi_editor_filter_from_mixed_note_controller_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .notes()
            .iter()
            .filter(|event| self.midi_editor_selected_notes.contains(&event.id()))
            .map(|event| event.tick())
            .chain(
                item.controllers()
                    .iter()
                    .filter(|event| self.midi_editor_selected_controllers.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            true,
        );
    }

    fn set_midi_editor_filter_from_mixed_note_pitch_and_opaque_selection(
        &mut self,
        item_id: ItemId,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let selected_ticks = item
            .notes()
            .iter()
            .filter(|event| self.midi_editor_selected_notes.contains(&event.id()))
            .map(|event| event.tick())
            .chain(
                item.pitch_bends()
                    .iter()
                    .filter(|event| self.midi_editor_selected_pitch_bends.contains(event))
                    .map(|event| event.tick),
            )
            .chain(
                item.opaque_events()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        self.midi_editor_selected_opaque_events
                            .contains(&(*index as u32))
                    })
                    .map(|(_, event)| event.tick),
            )
            .collect();
        self.set_midi_editor_filter_from_mixed_opaque_selection_ticks(
            item_id,
            selected_ticks,
            true,
        );
    }

    fn set_midi_editor_filter_from_mixed_opaque_selection_ticks(
        &mut self,
        item_id: ItemId,
        selected_ticks: Vec<u64>,
        show_only: bool,
    ) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let mut selected_ticks = selected_ticks.into_iter();
        let Some(first_tick) = selected_ticks.next() else {
            return;
        };
        let ppq = self.project.settings().ppq();
        let measure_ticks = midi_editor_measure_length_ticks(
            ppq,
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let (position_low, position_high) = selected_ticks.fold(
            (
                midi_editor_filter_position_value(first_tick, measure_ticks),
                midi_editor_filter_position_high_value(first_tick, measure_ticks),
            ),
            |(position_low, position_high), tick| {
                (
                    position_low.min(midi_editor_filter_position_value(tick, measure_ticks)),
                    position_high.max(midi_editor_filter_position_high_value(tick, measure_ticks)),
                )
            },
        );

        // REAPER expands measured Note/CC/Pitch Bend + opaque combinations to
        // all event types and channels and uses their shared Position range.
        // Note + opaque activates the funnel without Show-only; the measured
        // CC and Pitch Bend combinations enable Show-only.
        self.midi_editor_channel_selection = MidiEditorChannelSelection::AllChannels;
        self.midi_editor_filter_channels = u16::MAX;
        self.midi_editor_filter_channels_initialized = true;
        self.midi_editor_event_type_filter = MidiEditorEventTypeFilter::AllEvents;
        self.midi_editor_note_filter = MidiEditorNoteFilter::default();
        self.midi_editor_controller_filter = MidiEditorControllerFilter::default();
        self.midi_editor_pitch_bend_filter = MidiEditorPitchBendFilter::default();
        self.midi_editor_program_change_filter = MidiEditorProgramChangeFilter::default();
        self.midi_editor_position_filter = MidiEditorPositionFilter {
            enabled: true,
            range_initialized: true,
            low: position_low,
            high: position_high,
            repeat_every: MidiEditorFilterRepeat::Measure,
        };
        self.midi_editor_filter_value_low.clear();
        self.midi_editor_filter_value_high.clear();
        self.midi_editor_filter_position_low = format_midi_editor_filter_position(position_low);
        self.midi_editor_filter_position_high = format_midi_editor_filter_position(position_high);
        self.midi_editor_filter_note_length_low.clear();
        self.midi_editor_filter_note_length_high.clear();
        self.midi_editor_event_filter_enabled = show_only;
        self.midi_editor_event_list_filter_active = true;
    }

    fn set_midi_editor_filter_from_mixed_channel_event_selection(&mut self, item_id: ItemId) {
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        else {
            return;
        };
        let mut selected_events = Vec::new();
        selected_events.extend(
            item.notes()
                .iter()
                .filter(|event| self.midi_editor_selected_notes.contains(&event.id()))
                .map(|event| (event.tick(), event.channel())),
        );
        selected_events.extend(
            item.controllers()
                .iter()
                .filter(|event| self.midi_editor_selected_controllers.contains(event))
                .map(|event| (event.tick, event.channel)),
        );
        selected_events.extend(
            item.pitch_bends()
                .iter()
                .filter(|event| self.midi_editor_selected_pitch_bends.contains(event))
                .map(|event| (event.tick, event.channel)),
        );
        selected_events.extend(
            item.program_changes()
                .iter()
                .filter(|event| self.midi_editor_selected_program_changes.contains(event))
                .map(|event| (event.tick, event.channel)),
        );
        selected_events.extend(
            item.poly_aftertouches()
                .iter()
                .filter(|event| self.midi_editor_selected_poly_aftertouches.contains(event))
                .map(|event| (event.tick, event.channel)),
        );
        selected_events.extend(
            item.channel_pressures()
                .iter()
                .filter(|event| self.midi_editor_selected_channel_pressures.contains(event))
                .map(|event| (event.tick, event.channel)),
        );
        let Some((first_tick, first_channel)) = selected_events
            .iter()
            .copied()
            .min_by_key(|(tick, channel)| (*tick, *channel))
        else {
            return;
        };
        let ppq = self.project.settings().ppq();
        let measure_ticks = midi_editor_measure_length_ticks(
            ppq,
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let (channels, position_low, position_high) = selected_events.iter().fold(
            (
                0_u16,
                midi_editor_filter_position_value(first_tick, measure_ticks),
                midi_editor_filter_position_high_value(first_tick, measure_ticks),
            ),
            |(channels, position_low, position_high), (tick, channel)| {
                (
                    channels | (1_u16 << (*channel).min(15)),
                    position_low.min(midi_editor_filter_position_value(*tick, measure_ticks)),
                    position_high.max(midi_editor_filter_position_high_value(*tick, measure_ticks)),
                )
            },
        );
        if channels == 0 {
            return;
        }

        self.midi_editor_channel_selection = if channels.count_ones() == 1 {
            MidiEditorChannelSelection::Channel(channels.trailing_zeros() as u8)
        } else {
            MidiEditorChannelSelection::Multichannel
        };
        self.midi_editor_filter_channels = channels;
        self.midi_editor_filter_channels_initialized = true;
        self.midi_editor_event_type_filter = MidiEditorEventTypeFilter::AllEvents;
        self.midi_editor_note_filter = MidiEditorNoteFilter::default();
        self.midi_editor_controller_filter = MidiEditorControllerFilter::default();
        self.midi_editor_pitch_bend_filter = MidiEditorPitchBendFilter::default();
        self.midi_editor_program_change_filter = MidiEditorProgramChangeFilter::default();
        self.midi_editor_position_filter = MidiEditorPositionFilter {
            enabled: true,
            range_initialized: true,
            low: position_low,
            high: position_high,
            repeat_every: MidiEditorFilterRepeat::Measure,
        };
        self.midi_editor_filter_value_low.clear();
        self.midi_editor_filter_value_high.clear();
        self.midi_editor_filter_position_low = format_midi_editor_filter_position(position_low);
        self.midi_editor_filter_position_high = format_midi_editor_filter_position(position_high);
        self.midi_editor_filter_note_length_low.clear();
        self.midi_editor_filter_note_length_high.clear();
        self.midi_editor_draw_channel = first_channel.min(15);
    }

    fn set_midi_editor_filter_from_selected_notes(&mut self) {
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
        let ppq = self.project.settings().ppq();
        let measure_ticks = midi_editor_measure_length_ticks(
            ppq,
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let mut selected_notes = item
            .notes()
            .iter()
            .filter(|note| self.midi_editor_selected_notes.contains(&note.id()));
        let Some(first_note) = selected_notes.next() else {
            return;
        };
        let first_position = midi_editor_filter_position_value(first_note.tick(), measure_ticks);
        let first_position_high =
            midi_editor_filter_position_high_value(first_note.tick(), measure_ticks);
        let (
            channels,
            pitch_low,
            pitch_high,
            velocity_low,
            velocity_high,
            position_low,
            position_high,
            length_low_ticks,
            length_high_ticks,
        ) = selected_notes.fold(
            (
                1_u16 << first_note.channel().min(15),
                first_note.pitch(),
                first_note.pitch(),
                first_note.velocity(),
                first_note.velocity(),
                first_position,
                first_position_high,
                first_note.duration(),
                first_note.duration(),
            ),
            |(
                channels,
                pitch_low,
                pitch_high,
                velocity_low,
                velocity_high,
                position_low,
                highest_position,
                length_low_ticks,
                length_high_ticks,
            ),
             note| {
                let position = midi_editor_filter_position_value(note.tick(), measure_ticks);
                let event_position_high =
                    midi_editor_filter_position_high_value(note.tick(), measure_ticks);
                (
                    channels | (1_u16 << note.channel().min(15)),
                    pitch_low.min(note.pitch()),
                    pitch_high.max(note.pitch()),
                    velocity_low.min(note.velocity()),
                    velocity_high.max(note.velocity()),
                    position_low.min(position),
                    highest_position.max(event_position_high),
                    length_low_ticks.min(note.duration()),
                    length_high_ticks.max(note.duration()),
                )
            },
        );
        if channels == 0 {
            return;
        }

        self.midi_editor_channel_selection = if channels.count_ones() == 1 {
            MidiEditorChannelSelection::Channel(channels.trailing_zeros() as u8)
        } else {
            MidiEditorChannelSelection::Multichannel
        };
        self.midi_editor_filter_channels = channels;
        self.midi_editor_filter_channels_initialized = true;
        self.midi_editor_event_type_filter = MidiEditorEventTypeFilter::Notes;
        self.midi_editor_note_filter = MidiEditorNoteFilter {
            pitch_enabled: true,
            pitch_range_initialized: true,
            pitch_low,
            pitch_high,
            velocity_enabled: true,
            velocity_range_initialized: true,
            velocity_low,
            velocity_high,
            length_enabled: true,
            length_range_initialized: true,
            length_low_ticks,
            length_high_ticks,
        };
        self.midi_editor_controller_filter = MidiEditorControllerFilter::default();
        self.midi_editor_pitch_bend_filter = MidiEditorPitchBendFilter::default();
        self.midi_editor_program_change_filter = MidiEditorProgramChangeFilter::default();
        self.midi_editor_position_filter = MidiEditorPositionFilter {
            enabled: true,
            range_initialized: true,
            low: position_low,
            high: position_high,
            repeat_every: MidiEditorFilterRepeat::Measure,
        };
        self.midi_editor_filter_value_low.clear();
        self.midi_editor_filter_value_high.clear();
        self.midi_editor_filter_position_low = format_midi_editor_filter_position(position_low);
        self.midi_editor_filter_position_high = format_midi_editor_filter_position(position_high);
        self.midi_editor_filter_note_length_low =
            format_midi_editor_filter_length(length_low_ticks, ppq);
        self.midi_editor_filter_note_length_high =
            format_midi_editor_filter_length(length_high_ticks, ppq);
        self.midi_editor_draw_channel = 0;
    }

    fn set_midi_editor_filter_from_selected_pitch_bends(&mut self) {
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
        let selected_bends = item
            .pitch_bends()
            .iter()
            .copied()
            .filter(|event| self.midi_editor_selected_pitch_bends.contains(event))
            .collect::<Vec<_>>();
        let Some(first_bend) = selected_bends.first().copied() else {
            return;
        };
        let ppq = self.project.settings().ppq();
        let measure_ticks = midi_editor_measure_length_ticks(
            ppq,
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let (channels, value_low, value_high, position_low, position_high) =
            selected_bends.iter().fold(
                (
                    1_u16 << first_bend.channel.min(15),
                    first_bend.value,
                    first_bend.value,
                    midi_editor_filter_position_value(first_bend.tick, measure_ticks),
                    midi_editor_filter_position_high_value(first_bend.tick, measure_ticks),
                ),
                |(channels, value_low, value_high, position_low, position_high), event| {
                    (
                        channels | (1_u16 << event.channel.min(15)),
                        value_low.min(event.value),
                        value_high.max(event.value),
                        position_low
                            .min(midi_editor_filter_position_value(event.tick, measure_ticks)),
                        position_high.max(midi_editor_filter_position_high_value(
                            event.tick,
                            measure_ticks,
                        )),
                    )
                },
            );
        if channels == 0 {
            return;
        }

        let value_low = value_low as i32 - 8_192;
        let value_high = value_high as i32 - 8_192;
        self.midi_editor_channel_selection = if channels.count_ones() == 1 {
            MidiEditorChannelSelection::Channel(channels.trailing_zeros() as u8)
        } else {
            MidiEditorChannelSelection::Multichannel
        };
        self.midi_editor_filter_channels = channels;
        self.midi_editor_filter_channels_initialized = true;
        self.midi_editor_event_type_filter = MidiEditorEventTypeFilter::Pitch;
        self.midi_editor_note_filter = MidiEditorNoteFilter::default();
        self.midi_editor_controller_filter = MidiEditorControllerFilter::default();
        self.midi_editor_program_change_filter = MidiEditorProgramChangeFilter::default();
        self.midi_editor_pitch_bend_filter = MidiEditorPitchBendFilter {
            enabled: true,
            range_initialized: true,
            low: value_low as i16,
            high: value_high as i16,
        };
        self.midi_editor_position_filter = MidiEditorPositionFilter {
            enabled: true,
            range_initialized: true,
            low: position_low,
            high: position_high,
            repeat_every: MidiEditorFilterRepeat::Measure,
        };
        self.midi_editor_filter_value_low = value_low.to_string();
        self.midi_editor_filter_value_high = value_high.to_string();
        self.midi_editor_filter_position_low = format_midi_editor_filter_position(position_low);
        self.midi_editor_filter_position_high = format_midi_editor_filter_position(position_high);
        self.midi_editor_filter_note_length_low.clear();
        self.midi_editor_filter_note_length_high.clear();
        self.midi_editor_draw_channel = first_bend.channel.min(15);
    }

    fn set_midi_editor_filter_from_selected_program_changes(&mut self) {
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
        let selected_program_changes = item
            .program_changes()
            .iter()
            .copied()
            .filter(|event| self.midi_editor_selected_program_changes.contains(event))
            .collect::<Vec<_>>();
        let Some(first_program_change) = selected_program_changes.first().copied() else {
            return;
        };
        let ppq = self.project.settings().ppq();
        let measure_ticks = midi_editor_measure_length_ticks(
            ppq,
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let (channels, position_low, position_high) = selected_program_changes.iter().fold(
            (
                1_u16 << first_program_change.channel.min(15),
                midi_editor_filter_position_value(first_program_change.tick, measure_ticks),
                midi_editor_filter_position_high_value(first_program_change.tick, measure_ticks),
            ),
            |(channels, position_low, position_high), event| {
                (
                    channels | (1_u16 << event.channel.min(15)),
                    position_low.min(midi_editor_filter_position_value(event.tick, measure_ticks)),
                    position_high.max(midi_editor_filter_position_high_value(
                        event.tick,
                        measure_ticks,
                    )),
                )
            },
        );
        if channels == 0 {
            return;
        }

        self.midi_editor_channel_selection = if channels.count_ones() == 1 {
            MidiEditorChannelSelection::Channel(channels.trailing_zeros() as u8)
        } else {
            MidiEditorChannelSelection::Multichannel
        };
        self.midi_editor_filter_channels = channels;
        self.midi_editor_filter_channels_initialized = true;
        self.midi_editor_event_type_filter = MidiEditorEventTypeFilter::ProgramChange;
        self.midi_editor_note_filter = MidiEditorNoteFilter::default();
        self.midi_editor_controller_filter = MidiEditorControllerFilter::default();
        self.midi_editor_pitch_bend_filter = MidiEditorPitchBendFilter::default();
        // REAPER leaves the Program value range unchecked when deriving from selection.
        self.midi_editor_program_change_filter = MidiEditorProgramChangeFilter::default();
        self.midi_editor_position_filter = MidiEditorPositionFilter {
            enabled: true,
            range_initialized: true,
            low: position_low,
            high: position_high,
            repeat_every: MidiEditorFilterRepeat::Measure,
        };
        self.midi_editor_filter_value_low.clear();
        self.midi_editor_filter_value_high.clear();
        self.midi_editor_filter_position_low = format_midi_editor_filter_position(position_low);
        self.midi_editor_filter_position_high = format_midi_editor_filter_position(position_high);
        self.midi_editor_filter_note_length_low.clear();
        self.midi_editor_filter_note_length_high.clear();
        self.midi_editor_draw_channel = first_program_change.channel.min(15);
    }

    fn set_midi_editor_filter_from_selected_poly_aftertouches(&mut self) {
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
        let selected_aftertouches = item
            .poly_aftertouches()
            .iter()
            .copied()
            .filter(|event| self.midi_editor_selected_poly_aftertouches.contains(event))
            .collect::<Vec<_>>();
        let Some(first_aftertouch) = selected_aftertouches.first().copied() else {
            return;
        };
        let measure_ticks = midi_editor_measure_length_ticks(
            self.project.settings().ppq(),
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let (
            channels,
            note_low,
            note_high,
            pressure_low,
            pressure_high,
            position_low,
            position_high,
        ) = selected_aftertouches.iter().fold(
            (
                1_u16 << first_aftertouch.channel.min(15),
                first_aftertouch.note,
                first_aftertouch.note,
                first_aftertouch.value,
                first_aftertouch.value,
                midi_editor_filter_position_value(first_aftertouch.tick, measure_ticks),
                midi_editor_filter_position_high_value(first_aftertouch.tick, measure_ticks),
            ),
            |(
                channels,
                note_low,
                note_high,
                pressure_low,
                pressure_high,
                position_low,
                position_high,
            ),
             event| {
                (
                    channels | (1_u16 << event.channel.min(15)),
                    note_low.min(event.note),
                    note_high.max(event.note),
                    pressure_low.min(event.value),
                    pressure_high.max(event.value),
                    position_low.min(midi_editor_filter_position_value(event.tick, measure_ticks)),
                    position_high.max(midi_editor_filter_position_high_value(
                        event.tick,
                        measure_ticks,
                    )),
                )
            },
        );
        if channels == 0 {
            return;
        }

        self.midi_editor_channel_selection = if channels.count_ones() == 1 {
            MidiEditorChannelSelection::Channel(channels.trailing_zeros() as u8)
        } else {
            MidiEditorChannelSelection::Multichannel
        };
        self.midi_editor_filter_channels = channels;
        self.midi_editor_filter_channels_initialized = true;
        self.midi_editor_event_type_filter = MidiEditorEventTypeFilter::PolyAftertouch;
        self.midi_editor_note_filter = MidiEditorNoteFilter {
            pitch_enabled: true,
            pitch_range_initialized: true,
            pitch_low: note_low,
            pitch_high: note_high,
            velocity_enabled: true,
            velocity_range_initialized: true,
            velocity_low: pressure_low,
            velocity_high: pressure_high,
            ..MidiEditorNoteFilter::default()
        };
        self.midi_editor_controller_filter = MidiEditorControllerFilter::default();
        self.midi_editor_pitch_bend_filter = MidiEditorPitchBendFilter::default();
        self.midi_editor_program_change_filter = MidiEditorProgramChangeFilter::default();
        self.midi_editor_position_filter = MidiEditorPositionFilter {
            enabled: true,
            range_initialized: true,
            low: position_low,
            high: position_high,
            repeat_every: MidiEditorFilterRepeat::Measure,
        };
        self.midi_editor_filter_value_low.clear();
        self.midi_editor_filter_value_high.clear();
        self.midi_editor_filter_position_low = format_midi_editor_filter_position(position_low);
        self.midi_editor_filter_position_high = format_midi_editor_filter_position(position_high);
        self.midi_editor_filter_note_length_low.clear();
        self.midi_editor_filter_note_length_high.clear();
        self.midi_editor_draw_channel = first_aftertouch.channel.min(15);
    }

    fn set_midi_editor_filter_from_selected_channel_pressures(&mut self) {
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
        let selected_pressures = item
            .channel_pressures()
            .iter()
            .copied()
            .filter(|event| self.midi_editor_selected_channel_pressures.contains(event))
            .collect::<Vec<_>>();
        let Some(first_pressure) = selected_pressures.first().copied() else {
            return;
        };
        let measure_ticks = midi_editor_measure_length_ticks(
            self.project.settings().ppq(),
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let (channels, position_low, position_high) = selected_pressures.iter().fold(
            (
                1_u16 << first_pressure.channel.min(15),
                midi_editor_filter_position_value(first_pressure.tick, measure_ticks),
                midi_editor_filter_position_high_value(first_pressure.tick, measure_ticks),
            ),
            |(channels, position_low, position_high), event| {
                (
                    channels | (1_u16 << event.channel.min(15)),
                    position_low.min(midi_editor_filter_position_value(event.tick, measure_ticks)),
                    position_high.max(midi_editor_filter_position_high_value(
                        event.tick,
                        measure_ticks,
                    )),
                )
            },
        );
        if channels == 0 {
            return;
        }

        self.midi_editor_channel_selection = if channels.count_ones() == 1 {
            MidiEditorChannelSelection::Channel(channels.trailing_zeros() as u8)
        } else {
            MidiEditorChannelSelection::Multichannel
        };
        self.midi_editor_filter_channels = channels;
        self.midi_editor_filter_channels_initialized = true;
        self.midi_editor_event_type_filter = MidiEditorEventTypeFilter::ChannelPressure;
        self.midi_editor_note_filter = MidiEditorNoteFilter::default();
        self.midi_editor_controller_filter = MidiEditorControllerFilter::default();
        self.midi_editor_pitch_bend_filter = MidiEditorPitchBendFilter::default();
        self.midi_editor_program_change_filter = MidiEditorProgramChangeFilter::default();
        self.midi_editor_position_filter = MidiEditorPositionFilter {
            enabled: true,
            range_initialized: true,
            low: position_low,
            high: position_high,
            repeat_every: MidiEditorFilterRepeat::Measure,
        };
        self.midi_editor_filter_value_low.clear();
        self.midi_editor_filter_value_high.clear();
        self.midi_editor_filter_position_low = format_midi_editor_filter_position(position_low);
        self.midi_editor_filter_position_high = format_midi_editor_filter_position(position_high);
        self.midi_editor_filter_note_length_low.clear();
        self.midi_editor_filter_note_length_high.clear();
        self.midi_editor_draw_channel = first_pressure.channel.min(15);
    }

    fn set_midi_editor_filter_from_selected_controllers(&mut self) {
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
        let ppq = self.project.settings().ppq();
        let measure_ticks = midi_editor_measure_length_ticks(
            ppq,
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let selected_controllers = item
            .controllers()
            .iter()
            .copied()
            .filter(|event| self.midi_editor_selected_controllers.contains(event))
            .collect::<Vec<_>>();
        let Some(first_controller) = selected_controllers.first().copied() else {
            return;
        };
        let controller_enabled = !selected_controllers
            .iter()
            .any(|event| event.controller != first_controller.controller);

        let (channels, value_low, value_high, position_low, position_high) =
            selected_controllers.iter().fold(
                (
                    1_u16 << first_controller.channel.min(15),
                    first_controller.value,
                    first_controller.value,
                    midi_editor_filter_position_value(first_controller.tick, measure_ticks),
                    midi_editor_filter_position_high_value(first_controller.tick, measure_ticks),
                ),
                |(channels, value_low, value_high, position_low, position_high), event| {
                    (
                        channels | (1_u16 << event.channel.min(15)),
                        value_low.min(event.value),
                        value_high.max(event.value),
                        position_low
                            .min(midi_editor_filter_position_value(event.tick, measure_ticks)),
                        position_high.max(midi_editor_filter_position_high_value(
                            event.tick,
                            measure_ticks,
                        )),
                    )
                },
            );
        if channels == 0 {
            return;
        }

        self.midi_editor_channel_selection = if channels.count_ones() == 1 {
            MidiEditorChannelSelection::Channel(channels.trailing_zeros() as u8)
        } else {
            MidiEditorChannelSelection::Multichannel
        };
        self.midi_editor_filter_channels = channels;
        self.midi_editor_filter_channels_initialized = true;
        self.midi_editor_event_type_filter = MidiEditorEventTypeFilter::ControlChange;
        self.midi_editor_note_filter = MidiEditorNoteFilter::default();
        self.midi_editor_pitch_bend_filter = MidiEditorPitchBendFilter::default();
        self.midi_editor_program_change_filter = MidiEditorProgramChangeFilter::default();
        self.midi_editor_controller_filter = MidiEditorControllerFilter {
            controller_enabled,
            controller: if controller_enabled {
                first_controller.controller
            } else {
                0
            },
            value_enabled: true,
            value_range_initialized: true,
            value_low,
            value_high,
        };
        self.midi_editor_position_filter = MidiEditorPositionFilter {
            enabled: true,
            range_initialized: true,
            low: position_low,
            high: position_high,
            repeat_every: MidiEditorFilterRepeat::Measure,
        };
        self.midi_editor_filter_value_low = value_low.to_string();
        self.midi_editor_filter_value_high = value_high.to_string();
        self.midi_editor_filter_position_low = format_midi_editor_filter_position(position_low);
        self.midi_editor_filter_position_high = format_midi_editor_filter_position(position_high);
        self.midi_editor_filter_note_length_low.clear();
        self.midi_editor_filter_note_length_high.clear();
        self.midi_editor_draw_channel = 0;
    }

    fn midi_editor_filter_event_selection(
        &self,
        item_id: ItemId,
    ) -> Option<MidiEditorSelectionState> {
        let item = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)?;
        let measure_ticks = midi_editor_measure_length_ticks(
            self.project.settings().ppq(),
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let mut selection = MidiEditorSelectionState::default();
        selection.notes.extend(
            item.notes()
                .iter()
                .filter(|note| self.midi_editor_filter_note_matches(item, note))
                .map(|note| note.id()),
        );
        selection.controllers.extend(
            item.controllers()
                .iter()
                .copied()
                .filter(|event| self.midi_editor_filter_controller_matches(item, *event)),
        );
        selection.pitch_bends.extend(
            item.pitch_bends()
                .iter()
                .copied()
                .filter(|event| self.midi_editor_filter_pitch_bend_matches(item, *event)),
        );
        selection.program_changes.extend(
            item.program_changes()
                .iter()
                .copied()
                .filter(|event| self.midi_editor_filter_program_change_matches(item, *event)),
        );
        selection.poly_aftertouches.extend(
            item.poly_aftertouches()
                .iter()
                .copied()
                .filter(|event| self.midi_editor_filter_poly_aftertouch_matches(item, *event)),
        );
        selection.channel_pressures.extend(
            item.channel_pressures()
                .iter()
                .copied()
                .filter(|event| self.midi_editor_filter_channel_pressure_matches(item, *event)),
        );
        selection.opaque_events.extend(
            item.opaque_events()
                .iter()
                .enumerate()
                .filter(|(_, event)| {
                    let event_kind = match &event.kind {
                        MidiOpaqueEventKind::SystemExclusive { .. } => MidiEditorEventKind::SysEx,
                        MidiOpaqueEventKind::Meta { .. } => MidiEditorEventKind::Text,
                    };
                    let position = midi_editor_filter_repeat_position_value(
                        event.tick,
                        self.project.settings().ppq(),
                        measure_ticks,
                        self.midi_editor_position_filter.repeat_every,
                    );
                    self.midi_editor_filter_event_matches(None, event_kind, position)
                })
                .map(|(index, _)| index as u32),
        );
        Some(selection)
    }

    fn midi_editor_filter_event_matches(
        &self,
        channel: Option<u8>,
        event_kind: MidiEditorEventKind,
        position_in_repeat: u32,
    ) -> bool {
        MidiEditorFilterState {
            channel_selection: MidiEditorChannelSelection::AllChannels,
            channel_filter_enabled: false,
            event_filter_enabled: true,
            channels: self.midi_editor_filter_channels,
            inverted: self.midi_editor_filter_inverted,
            event_type: self.midi_editor_event_type_filter,
            note: self.midi_editor_note_filter,
            controller: self.midi_editor_controller_filter,
            pitch_bend: self.midi_editor_pitch_bend_filter,
            program_change: self.midi_editor_program_change_filter,
            position: self.midi_editor_position_filter,
        }
        .event_visible_at(channel, event_kind, position_in_repeat)
    }

    fn midi_editor_filter_controller_matches(
        &self,
        item: &MidiItem,
        event: MidiControllerData,
    ) -> bool {
        let measure_ticks = midi_editor_measure_length_ticks(
            self.project.settings().ppq(),
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let position = midi_editor_filter_repeat_position_value(
            event.tick,
            self.project.settings().ppq(),
            measure_ticks,
            self.midi_editor_position_filter.repeat_every,
        );
        MidiEditorFilterState {
            channel_selection: MidiEditorChannelSelection::AllChannels,
            channel_filter_enabled: false,
            event_filter_enabled: true,
            channels: self.midi_editor_filter_channels,
            inverted: self.midi_editor_filter_inverted,
            event_type: self.midi_editor_event_type_filter,
            note: self.midi_editor_note_filter,
            controller: self.midi_editor_controller_filter,
            pitch_bend: self.midi_editor_pitch_bend_filter,
            program_change: self.midi_editor_program_change_filter,
            position: self.midi_editor_position_filter,
        }
        .controller_visible_at(event.channel, event.controller, event.value, position)
    }

    fn midi_editor_filter_pitch_bend_matches(
        &self,
        item: &MidiItem,
        event: MidiPitchBendData,
    ) -> bool {
        let measure_ticks = midi_editor_measure_length_ticks(
            self.project.settings().ppq(),
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let position = midi_editor_filter_position_value(event.tick, measure_ticks);
        MidiEditorFilterState {
            channel_selection: MidiEditorChannelSelection::AllChannels,
            channel_filter_enabled: false,
            event_filter_enabled: true,
            channels: self.midi_editor_filter_channels,
            inverted: self.midi_editor_filter_inverted,
            event_type: self.midi_editor_event_type_filter,
            note: self.midi_editor_note_filter,
            controller: self.midi_editor_controller_filter,
            pitch_bend: self.midi_editor_pitch_bend_filter,
            program_change: self.midi_editor_program_change_filter,
            position: self.midi_editor_position_filter,
        }
        .pitch_bend_visible_at(event.channel, event.value, position)
    }

    fn midi_editor_filter_program_change_matches(
        &self,
        item: &MidiItem,
        event: MidiProgramChangeData,
    ) -> bool {
        let measure_ticks = midi_editor_measure_length_ticks(
            self.project.settings().ppq(),
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let position = midi_editor_filter_repeat_position_value(
            event.tick,
            self.project.settings().ppq(),
            measure_ticks,
            self.midi_editor_position_filter.repeat_every,
        );
        MidiEditorFilterState {
            channel_selection: MidiEditorChannelSelection::AllChannels,
            channel_filter_enabled: false,
            event_filter_enabled: true,
            channels: self.midi_editor_filter_channels,
            inverted: self.midi_editor_filter_inverted,
            event_type: self.midi_editor_event_type_filter,
            note: self.midi_editor_note_filter,
            controller: self.midi_editor_controller_filter,
            pitch_bend: self.midi_editor_pitch_bend_filter,
            program_change: self.midi_editor_program_change_filter,
            position: self.midi_editor_position_filter,
        }
        .program_change_visible_at(event.channel, event.program, position)
    }

    fn midi_editor_filter_poly_aftertouch_matches(
        &self,
        item: &MidiItem,
        event: MidiPolyAftertouchData,
    ) -> bool {
        let measure_ticks = midi_editor_measure_length_ticks(
            self.project.settings().ppq(),
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let position = midi_editor_filter_repeat_position_value(
            event.tick,
            self.project.settings().ppq(),
            measure_ticks,
            self.midi_editor_position_filter.repeat_every,
        );
        MidiEditorFilterState {
            channel_selection: MidiEditorChannelSelection::AllChannels,
            channel_filter_enabled: false,
            event_filter_enabled: true,
            channels: self.midi_editor_filter_channels,
            inverted: self.midi_editor_filter_inverted,
            event_type: self.midi_editor_event_type_filter,
            note: self.midi_editor_note_filter,
            controller: self.midi_editor_controller_filter,
            pitch_bend: self.midi_editor_pitch_bend_filter,
            program_change: self.midi_editor_program_change_filter,
            position: self.midi_editor_position_filter,
        }
        .poly_aftertouch_visible_at(event.channel, event.note, event.value, position)
    }

    fn midi_editor_filter_channel_pressure_matches(
        &self,
        item: &MidiItem,
        event: MidiChannelPressureData,
    ) -> bool {
        let measure_ticks = midi_editor_measure_length_ticks(
            self.project.settings().ppq(),
            self.project.time_signature_at_tick(item.start_tick()),
        );
        let position = midi_editor_filter_repeat_position_value(
            event.tick,
            self.project.settings().ppq(),
            measure_ticks,
            self.midi_editor_position_filter.repeat_every,
        );
        MidiEditorFilterState {
            channel_selection: MidiEditorChannelSelection::AllChannels,
            channel_filter_enabled: false,
            event_filter_enabled: true,
            channels: self.midi_editor_filter_channels,
            inverted: self.midi_editor_filter_inverted,
            event_type: self.midi_editor_event_type_filter,
            note: self.midi_editor_note_filter,
            controller: self.midi_editor_controller_filter,
            pitch_bend: self.midi_editor_pitch_bend_filter,
            program_change: self.midi_editor_program_change_filter,
            position: self.midi_editor_position_filter,
        }
        .channel_pressure_visible_at(event.channel, event.value, position)
    }

    fn midi_editor_filter_note_matches(
        &self,
        item: &MidiItem,
        note: &aaadaw_core::MidiNote,
    ) -> bool {
        let measure_ticks = midi_editor_measure_length_ticks(
            self.project.settings().ppq(),
            self.project.time_signature_at_tick(item.start_tick()),
        );
        MidiEditorFilterState {
            channel_selection: MidiEditorChannelSelection::AllChannels,
            channel_filter_enabled: false,
            event_filter_enabled: false,
            channels: self.midi_editor_filter_channels,
            inverted: self.midi_editor_filter_inverted,
            event_type: self.midi_editor_event_type_filter,
            note: self.midi_editor_note_filter,
            controller: self.midi_editor_controller_filter,
            pitch_bend: self.midi_editor_pitch_bend_filter,
            program_change: self.midi_editor_program_change_filter,
            position: self.midi_editor_position_filter,
        }
        .note_passes_event_filter(
            note.channel(),
            midi_editor_note_filter_event(
                note,
                self.project.settings().ppq(),
                measure_ticks,
                self.midi_editor_position_filter.repeat_every,
            ),
        )
    }

    fn select_midi_notes(&mut self, note_ids: HashSet<aaadaw_core::NoteId>) {
        self.midi_editor_feedback = None;
        if !self.keyboard_modifiers.command() && !self.keyboard_modifiers.control() {
            self.midi_editor_selected_controllers.clear();
            self.midi_editor_selected_pitch_bends.clear();
            self.midi_editor_selected_program_changes.clear();
            self.midi_editor_selected_poly_aftertouches.clear();
            self.midi_editor_selected_channel_pressures.clear();
            self.midi_editor_selected_opaque_events.clear();
        }
        let visible_note_ids = self.visible_midi_note_ids();
        self.midi_editor_selected_notes = note_ids
            .into_iter()
            .filter(|note_id| visible_note_ids.contains(note_id))
            .collect();
    }

    fn undo_inline_midi_note_selection_if_next(&mut self) -> bool {
        if !self.inline_midi_note_selection_is_next_undo() {
            return false;
        }
        let Some(AppHistoryEntry::InlineMidiSelection(history)) = self.app_undo_history.pop()
        else {
            return false;
        };
        self.restore_inline_midi_selection_history(&history, false);
        self.app_redo_history
            .push(AppHistoryEntry::InlineMidiSelection(history));
        self.status = "Undid MIDI note selection".to_owned();
        true
    }

    fn redo_inline_midi_note_selection_if_next(&mut self) -> bool {
        if !self.inline_midi_note_selection_is_next_redo() {
            return false;
        }
        let Some(AppHistoryEntry::InlineMidiSelection(history)) = self.app_redo_history.pop()
        else {
            return false;
        };
        self.restore_inline_midi_selection_history(&history, true);
        self.app_undo_history
            .push(AppHistoryEntry::InlineMidiSelection(history));
        self.status = "Redid MIDI note selection".to_owned();
        true
    }

    fn undo_item_selection_if_next(&mut self) -> bool {
        if !self.item_selection_is_next_undo() {
            return false;
        }
        let Some(AppHistoryEntry::ItemSelection {
            label,
            before,
            after,
            project_history_cursor,
        }) = self.app_undo_history.pop()
        else {
            return false;
        };
        self.timeline.restore_item_selection_state(&before);
        self.app_redo_history.push(AppHistoryEntry::ItemSelection {
            label,
            before,
            after,
            project_history_cursor,
        });
        self.status = format!("Undid {label}");
        true
    }

    fn redo_item_selection_if_next(&mut self) -> bool {
        if !self.item_selection_is_next_redo() {
            return false;
        }
        let Some(AppHistoryEntry::ItemSelection {
            label,
            before,
            after,
            project_history_cursor,
        }) = self.app_redo_history.pop()
        else {
            return false;
        };
        self.timeline.restore_item_selection_state(&after);
        self.app_undo_history.push(AppHistoryEntry::ItemSelection {
            label,
            before,
            after,
            project_history_cursor,
        });
        self.status = format!("Redid {label}");
        true
    }

    fn undo(&mut self) {
        if self.undo_inline_midi_note_selection_if_next() {
            return;
        }
        if self.undo_item_selection_if_next() {
            return;
        }
        let inline_note_selection_history = self.inline_midi_copy_selection_is_next_undo();
        let project_item_selection_history = self.project_item_selection_is_next_undo();
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
        let selected_controllers = self.midi_editor_selected_controllers.clone();
        let selected_pitch_bends = self.midi_editor_selected_pitch_bends.clone();
        let selected_program_changes = self.midi_editor_selected_program_changes.clone();
        let selected_poly_aftertouches = self.midi_editor_selected_poly_aftertouches.clone();
        let selected_channel_pressures = self.midi_editor_selected_channel_pressures.clone();
        let selected_opaque_events = self.selected_midi_opaque_event_data();
        let midi_editor_item_id = self.midi_editor_item_id;
        #[cfg(feature = "audio-device")]
        let sample_rate_before = self.project.settings().sample_rate();
        self.status = match self.project.undo() {
            Ok(true) => {
                if let Some(item_id) = midi_editor_item_id
                    && let Some(item) = self
                        .project
                        .midi_items()
                        .iter()
                        .find(|item| item.id() == item_id)
                {
                    self.midi_editor_selected_controllers = Self::remap_selected_controller_events(
                        &selected_controllers,
                        item.controllers(),
                    );
                    self.midi_editor_selected_pitch_bends = Self::remap_selected_pitch_bend_events(
                        &selected_pitch_bends,
                        item.pitch_bends(),
                    );
                    self.midi_editor_selected_program_changes =
                        Self::remap_selected_midi_point_events(
                            &selected_program_changes,
                            item.program_changes(),
                            |left, right| {
                                left.channel == right.channel && left.program == right.program
                            },
                            |left, right| left.channel == right.channel,
                            |event| event.tick,
                        );
                    self.midi_editor_selected_poly_aftertouches =
                        Self::remap_selected_midi_point_events(
                            &selected_poly_aftertouches,
                            item.poly_aftertouches(),
                            |left, right| {
                                left.channel == right.channel
                                    && left.note == right.note
                                    && left.value == right.value
                            },
                            |left, right| left.channel == right.channel && left.note == right.note,
                            |event| event.tick,
                        );
                    self.midi_editor_selected_channel_pressures =
                        Self::remap_selected_midi_point_events(
                            &selected_channel_pressures,
                            item.channel_pressures(),
                            |left, right| {
                                left.channel == right.channel && left.value == right.value
                            },
                            |left, right| left.channel == right.channel,
                            |event| event.tick,
                        );
                    self.midi_editor_selected_opaque_events =
                        Self::remap_selected_midi_opaque_events(
                            &selected_opaque_events,
                            item.opaque_events(),
                        );
                }
                let project_history_entry = if inline_note_selection_history.is_some()
                    || project_item_selection_history.is_some()
                    || matches!(self.app_undo_history.last(), Some(AppHistoryEntry::Project))
                {
                    self.app_undo_history.pop()
                } else {
                    None
                }
                .unwrap_or(AppHistoryEntry::Project);
                self.app_redo_history.push(project_history_entry);
                #[cfg(feature = "audio-device")]
                if self.project.settings().sample_rate() != sample_rate_before {
                    let sample_rate_after = self.project.settings().sample_rate();
                    self.playhead_sample = rescale_transport_sample(
                        self.playhead_sample,
                        sample_rate_before,
                        sample_rate_after,
                    );
                    self.playback_start_sample = rescale_transport_sample(
                        self.playback_start_sample,
                        sample_rate_before,
                        sample_rate_after,
                    );
                }
                self.restore_fx_chain_selection(selected_fx_instance_id);
                self.midi_note_clipboard.last_paste = None;
                self.revision = self.revision.wrapping_add(1);
                self.timeline.rebuild(&self.project);
                self.refresh_item_properties_take_selection();
                if let Some(history) = project_item_selection_history.as_ref() {
                    self.restore_project_item_selection_history(history, false);
                }
                if let Some(history) = inline_note_selection_history.as_ref() {
                    self.restore_inline_midi_selection_history(history, false);
                }
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
        if self.redo_inline_midi_note_selection_if_next() {
            return;
        }
        if self.redo_item_selection_if_next() {
            return;
        }
        let inline_note_selection_history = match self.app_redo_history.last() {
            Some(AppHistoryEntry::ProjectWithInlineMidiSelection(history)) => Some(history.clone()),
            _ => None,
        };
        let project_item_selection_history = self.project_item_selection_is_next_redo();
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
        let selected_controllers = self.midi_editor_selected_controllers.clone();
        let selected_pitch_bends = self.midi_editor_selected_pitch_bends.clone();
        let selected_program_changes = self.midi_editor_selected_program_changes.clone();
        let selected_poly_aftertouches = self.midi_editor_selected_poly_aftertouches.clone();
        let selected_channel_pressures = self.midi_editor_selected_channel_pressures.clone();
        let selected_opaque_events = self.selected_midi_opaque_event_data();
        let midi_editor_item_id = self.midi_editor_item_id;
        #[cfg(feature = "audio-device")]
        let sample_rate_before = self.project.settings().sample_rate();
        self.status = match self.project.redo() {
            Ok(true) => {
                if let Some(item_id) = midi_editor_item_id
                    && let Some(item) = self
                        .project
                        .midi_items()
                        .iter()
                        .find(|item| item.id() == item_id)
                {
                    self.midi_editor_selected_controllers = Self::remap_selected_controller_events(
                        &selected_controllers,
                        item.controllers(),
                    );
                    self.midi_editor_selected_pitch_bends = Self::remap_selected_pitch_bend_events(
                        &selected_pitch_bends,
                        item.pitch_bends(),
                    );
                    self.midi_editor_selected_program_changes =
                        Self::remap_selected_midi_point_events(
                            &selected_program_changes,
                            item.program_changes(),
                            |left, right| {
                                left.channel == right.channel && left.program == right.program
                            },
                            |left, right| left.channel == right.channel,
                            |event| event.tick,
                        );
                    self.midi_editor_selected_poly_aftertouches =
                        Self::remap_selected_midi_point_events(
                            &selected_poly_aftertouches,
                            item.poly_aftertouches(),
                            |left, right| {
                                left.channel == right.channel
                                    && left.note == right.note
                                    && left.value == right.value
                            },
                            |left, right| left.channel == right.channel && left.note == right.note,
                            |event| event.tick,
                        );
                    self.midi_editor_selected_channel_pressures =
                        Self::remap_selected_midi_point_events(
                            &selected_channel_pressures,
                            item.channel_pressures(),
                            |left, right| {
                                left.channel == right.channel && left.value == right.value
                            },
                            |left, right| left.channel == right.channel,
                            |event| event.tick,
                        );
                    self.midi_editor_selected_opaque_events =
                        Self::remap_selected_midi_opaque_events(
                            &selected_opaque_events,
                            item.opaque_events(),
                        );
                }
                let project_history_entry = if inline_note_selection_history.is_some()
                    || project_item_selection_history.is_some()
                    || matches!(self.app_redo_history.last(), Some(AppHistoryEntry::Project))
                {
                    self.app_redo_history.pop()
                } else {
                    None
                }
                .unwrap_or(AppHistoryEntry::Project);
                self.app_undo_history.push(project_history_entry);
                #[cfg(feature = "audio-device")]
                if self.project.settings().sample_rate() != sample_rate_before {
                    let sample_rate_after = self.project.settings().sample_rate();
                    self.playhead_sample = rescale_transport_sample(
                        self.playhead_sample,
                        sample_rate_before,
                        sample_rate_after,
                    );
                    self.playback_start_sample = rescale_transport_sample(
                        self.playback_start_sample,
                        sample_rate_before,
                        sample_rate_after,
                    );
                }
                self.restore_fx_chain_selection(selected_fx_instance_id);
                self.midi_note_clipboard.last_paste =
                    self.midi_editor_item_id.and_then(|item_id| {
                        self.selected_clipboard_paste_start(item_id)
                            .map(|start_tick| (item_id, start_tick))
                    });
                self.revision = self.revision.wrapping_add(1);
                self.timeline.rebuild(&self.project);
                self.refresh_item_properties_take_selection();
                if let Some(history) = project_item_selection_history.as_ref() {
                    self.restore_project_item_selection_history(history, true);
                }
                if let Some(history) = inline_note_selection_history.as_ref() {
                    self.restore_inline_midi_selection_history(history, true);
                }
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
        selected.sort_by_key(|note| (note.tick(), note.channel(), note.pitch()));
        let start_tick = selected.iter().map(|note| note.tick()).min()?;
        let matches_clipboard =
            selected
                .iter()
                .zip(&self.midi_note_clipboard.notes)
                .all(|(note, copied)| {
                    note.tick() - start_tick == copied.tick
                        && note.channel() == copied.channel
                        && note.pitch() == copied.pitch
                        && note.duration() == copied.duration
                        && note.velocity() == copied.velocity
                });
        matches_clipboard.then(|| view::midi_item_local_tick(item, start_tick))
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
        let (source_start_tick, item_end_tick) = if item.is_loop_source() {
            let source_start = item.source_offset_ticks();
            (
                source_start,
                source_start.saturating_add(item.length_ticks()),
            )
        } else {
            view::midi_source_edit_bounds(item)
        };
        let item_length = item.length_ticks();
        let selected = item
            .notes()
            .iter()
            .filter(|note| self.midi_editor_selected_notes.contains(&note.id()))
            .map(|note| {
                (
                    note.id(),
                    aaadaw_core::MidiNoteData {
                        channel: note.channel(),
                        pitch: note.pitch(),
                        tick: note.tick(),
                        duration: note.duration(),
                        velocity: note.velocity(),
                        off_velocity: note.off_velocity(),
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
        let next_edit_cursor_tick =
            view::midi_item_local_tick(item, snap_tick_up(target.saturating_add(span), grid))
                .min(item_length);

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
            self.midi_editor_edit_cursor_tick = Some(next_edit_cursor_tick);
        }
    }

    fn midi_editor_snap_interval(&self) -> u64 {
        if self.timeline.snap_enabled {
            self.timeline
                .midi_editor_snap_grid
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
        self.recent_projects_menu_open = false;
        self.project_templates_menu_open = false;
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
        const MENUS: [MainMenu; 9] = [
            MainMenu::File,
            MainMenu::Edit,
            MainMenu::View,
            MainMenu::Insert,
            MainMenu::Item,
            MainMenu::Track,
            MainMenu::Options,
            MainMenu::Actions,
            MainMenu::Help,
        ];
        match navigation {
            MenuNavigation::OpenMenu(menu) => self.open_main_menu(menu),
            MenuNavigation::NextMenu | MenuNavigation::PreviousMenu => {
                let on_recent_projects_parent =
                    self.menu_selected_command == Some(CommandId::OpenRecentProjects);
                let on_project_templates_parent =
                    self.menu_selected_command == Some(CommandId::OpenProjectTemplates);
                let enter_recent_projects =
                    navigation == MenuNavigation::NextMenu && on_recent_projects_parent;
                let enter_project_templates =
                    navigation == MenuNavigation::NextMenu && on_project_templates_parent;
                let leave_recent_projects =
                    navigation == MenuNavigation::PreviousMenu && self.recent_projects_menu_open;
                let leave_project_templates =
                    navigation == MenuNavigation::PreviousMenu && self.project_templates_menu_open;
                if self.active_menu == Some(MainMenu::File)
                    && (enter_recent_projects
                        || enter_project_templates
                        || leave_recent_projects
                        || leave_project_templates)
                {
                    self.recent_projects_menu_open = enter_recent_projects;
                    self.project_templates_menu_open = enter_project_templates;
                    self.menu_selected_command =
                        Some(if enter_project_templates || leave_project_templates {
                            CommandId::OpenProjectTemplates
                        } else {
                            CommandId::OpenRecentProjects
                        });
                    return Task::none();
                }
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
            MenuNavigation::FirstCommand
            | MenuNavigation::LastCommand
            | MenuNavigation::NextCommand
            | MenuNavigation::PreviousCommand => {
                let Some(menu) = self.active_menu else {
                    return Task::none();
                };
                if menu == MainMenu::File
                    && (self.recent_projects_menu_open || self.project_templates_menu_open)
                {
                    let parent_command = if self.project_templates_menu_open {
                        CommandId::OpenProjectTemplates
                    } else {
                        CommandId::OpenRecentProjects
                    };
                    let submenu_commands = if self.project_templates_menu_open {
                        commands::project_template_entries(self)
                    } else {
                        commands::recent_project_entries(self)
                    }
                    .into_iter()
                    .filter(|entry| entry.enabled)
                    .map(|entry| entry.id)
                    .collect::<Vec<_>>();
                    if submenu_commands.is_empty() {
                        self.menu_selected_command = Some(parent_command);
                        return Task::none();
                    }
                    let current = self.menu_selected_command.and_then(|selected| {
                        submenu_commands
                            .iter()
                            .position(|command| *command == selected)
                    });
                    let next = menu_command_index(current, submenu_commands.len(), navigation);
                    self.menu_selected_command = Some(submenu_commands[next]);
                    return Task::none();
                }
                let commands = self.menu_command_ids(menu);
                if commands.is_empty() {
                    self.menu_selected_command = None;
                    return Task::none();
                }
                let current = self
                    .menu_selected_command
                    .and_then(|selected| commands.iter().position(|command| *command == selected));
                let next = menu_command_index(current, commands.len(), navigation);
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
        self.timeline.delete_focus = timeline::DeleteFocusTarget::TrackPanel;
        self.timeline.select_track_only(track_id);
        let input_id = messages::track_name_input_id(track_id);
        Task::batch([
            iced::widget::operation::focus(input_id.clone()),
            iced::widget::operation::move_cursor_to_end(input_id),
        ])
    }

    pub(super) fn can_split_selected_items_at_time_selection(&self) -> bool {
        if self.timeline.time_selection.is_some() {
            if self.timeline.selected_items.is_empty() {
                self.all_items_time_selection_split_action(None)
                    .is_ok_and(|action| action.is_some())
            } else {
                self.selected_item_split_action(true, None)
                    .is_ok_and(|action| action.is_some())
            }
        } else {
            self.all_items_razor_area_split_action(None)
                .is_ok_and(|action| action.is_some())
        }
    }

    fn selected_item_split_action(
        &self,
        use_time_selection: bool,
        source_sample_rates: Option<&HashMap<String, u32>>,
    ) -> Result<Option<DawAction>, String> {
        let tick_boundaries = if use_time_selection {
            let Some(selection) = self.timeline.time_selection else {
                return Ok(None);
            };
            vec![selection.start_tick, selection.end_tick]
        } else {
            vec![self.timeline.edit_cursor_tick]
        };
        let cursor_sample = if use_time_selection {
            None
        } else {
            Some(
                self.timeline
                    .edit_cursor_sample(&self.project)
                    .map_err(|error| error.to_string())?,
            )
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
                if let Some(sample) = cursor_sample {
                    split_audio_item_actions_at_samples(
                        &self.project,
                        item,
                        &[sample],
                        source_sample_rate,
                        &mut actions,
                    )?;
                } else {
                    split_audio_item_actions(
                        &self.project,
                        item,
                        &tick_boundaries,
                        source_sample_rate,
                        &mut actions,
                    )?;
                }
            } else if let Some(item) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                split_midi_item_actions(item, &tick_boundaries, &mut actions)?;
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
            [selection.start_tick, selection.end_tick]
                .into_iter()
                .filter_map(|tick| self.project.sample_at_tick(tick).ok())
                .collect::<Vec<_>>()
        } else {
            vec![
                self.timeline
                    .edit_cursor_sample(&self.project)
                    .map_err(|error| error.to_string())?,
            ]
        };
        let media_refs = self
            .project
            .audio_items()
            .iter()
            .filter(|item| self.timeline.selected_items.contains(&item.id()))
            .filter(|item| {
                boundaries
                    .iter()
                    .any(|sample| item.start_sample() < *sample && *sample < item.end_sample())
            })
            .map(|item| item.media_ref().to_owned())
            .collect::<Vec<_>>();
        self.audio_source_sample_rates(media_refs)
    }

    fn audio_source_sample_rates(
        &self,
        media_refs: impl IntoIterator<Item = String>,
    ) -> Result<HashMap<String, u32>, String> {
        let mut media_refs = media_refs.into_iter().collect::<Vec<_>>();
        media_refs.sort_unstable();
        media_refs.dedup();
        if media_refs.is_empty() {
            return Ok(HashMap::new());
        }

        let path = self
            .media_store_path()
            .filter(|path| path.is_file())
            .ok_or_else(|| "audio media store is unavailable".to_owned())?;
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
        if use_time_selection {
            if let Some(selection) = self.timeline.time_selection {
                if self.timeline.selected_items.is_empty() {
                    self.split_all_items_at_time_selection(selection);
                    return;
                }
            } else {
                self.split_items_in_razor_areas();
                return;
            }
        }
        if !use_time_selection && self.timeline.selected_items.is_empty() {
            self.split_all_items_at_cursor();
            return;
        }
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
        let time_selection_range = if use_time_selection {
            let Some(selection) = self.timeline.time_selection else {
                self.status = "No time selection to split".to_owned();
                return;
            };
            let start_sample = match self.project.sample_at_tick(selection.start_tick) {
                Ok(sample) => sample,
                Err(error) => {
                    self.status = format!("Split failed: {error}");
                    return;
                }
            };
            let end_sample = match self.project.sample_at_tick(selection.end_tick) {
                Ok(sample) => sample,
                Err(error) => {
                    self.status = format!("Split failed: {error}");
                    return;
                }
            };
            Some((selection, start_sample, end_sample))
        } else {
            None
        };
        let previously_selected_ids = self.timeline.selected_items.clone();
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
        if let Some((selection, start_sample, end_sample)) = time_selection_range {
            let candidates = previously_selected_ids
                .into_iter()
                .chain(new_ids)
                .collect::<HashSet<_>>();
            let selected = self
                .project
                .audio_items()
                .iter()
                .filter(|item| {
                    candidates.contains(&item.id())
                        && start_sample <= item.start_sample()
                        && item.end_sample() <= end_sample
                })
                .map(|item| (item.id(), item.track_id()))
                .chain(self.project.midi_items().iter().filter_map(|item| {
                    let item_end = item.start_tick().checked_add(item.length_ticks())?;
                    (candidates.contains(&item.id())
                        && selection.start_tick <= item.start_tick()
                        && item_end <= selection.end_tick)
                        .then_some((item.id(), item.track_id()))
                }))
                .collect();
            self.timeline.select_items(selected);
        } else {
            let selected = self
                .project
                .audio_items()
                .iter()
                .filter(|item| new_ids.contains(&item.id()))
                .map(|item| (item.id(), item.track_id()))
                .chain(self.project.midi_items().iter().filter_map(|item| {
                    new_ids
                        .contains(&item.id())
                        .then_some((item.id(), item.track_id()))
                }))
                .collect();
            self.timeline.select_items(selected);
        }
        self.audio_item_start_edits.clear();
    }

    fn all_items_time_selection_split_action(
        &self,
        source_sample_rates: Option<&HashMap<String, u32>>,
    ) -> Result<Option<DawAction>, String> {
        let Some(selection) = self.timeline.time_selection else {
            return Ok(None);
        };
        let boundaries = [selection.start_tick, selection.end_tick];
        let mut actions = Vec::new();
        for item in self.project.audio_items() {
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
        }
        for item in self.project.midi_items() {
            split_midi_item_actions(item, &boundaries, &mut actions)?;
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

    fn all_items_razor_area_split_action(
        &self,
        source_sample_rates: Option<&HashMap<String, u32>>,
    ) -> Result<Option<(DawAction, TrackRazorAreas)>, String> {
        let track_areas = self
            .project
            .tracks()
            .iter()
            .filter(|track| !track.razor_edit_areas().is_empty())
            .map(|track| (track.id(), track.razor_edit_areas().to_vec()))
            .collect::<Vec<_>>();
        if track_areas.is_empty() {
            return Ok(None);
        }
        let areas_by_track = track_areas.iter().cloned().collect::<HashMap<_, _>>();
        let mut actions = Vec::new();
        for item in self.project.audio_items() {
            let Some(areas) = areas_by_track.get(&item.track_id()) else {
                continue;
            };
            let boundaries = areas
                .iter()
                .flat_map(|area| [area.start_sample(), area.end_sample()])
                .collect::<Vec<_>>();
            let source_sample_rate = source_sample_rates
                .and_then(|rates| rates.get(item.media_ref()).copied())
                .unwrap_or_else(|| self.project.settings().sample_rate());
            split_audio_item_actions_at_samples(
                &self.project,
                item,
                &boundaries,
                source_sample_rate,
                &mut actions,
            )?;
        }
        for item in self.project.midi_items() {
            let Some(areas) = areas_by_track.get(&item.track_id()) else {
                continue;
            };
            let boundaries = areas
                .iter()
                .flat_map(|area| [area.start_sample(), area.end_sample()])
                .map(|sample| {
                    self.project
                        .tick_at_sample(sample)
                        .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            split_midi_item_actions(item, &boundaries, &mut actions)?;
        }
        actions.push(DawAction::SetTrackRazorEditAreas {
            track_areas: track_areas
                .iter()
                .map(|(track_id, _)| (*track_id, Vec::new()))
                .collect(),
        });
        Ok(Some((
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            },
            track_areas,
        )))
    }

    fn split_all_items_at_time_selection(&mut self, selection: timeline::TimeSelection) {
        let start_sample = match self.project.sample_at_tick(selection.start_tick) {
            Ok(sample) => sample,
            Err(error) => {
                self.status = format!("Split failed: {error}");
                return;
            }
        };
        let end_sample = match self.project.sample_at_tick(selection.end_tick) {
            Ok(sample) => sample,
            Err(error) => {
                self.status = format!("Split failed: {error}");
                return;
            }
        };
        let media_refs = self
            .project
            .audio_items()
            .iter()
            .filter(|item| {
                [selection.start_tick, selection.end_tick]
                    .iter()
                    .any(|tick| {
                        self.project.sample_at_tick(*tick).is_ok_and(|sample| {
                            item.start_sample() < sample && sample < item.end_sample()
                        })
                    })
            })
            .map(|item| item.media_ref().to_owned())
            .collect::<Vec<_>>();
        let source_sample_rates = match self.audio_source_sample_rates(media_refs) {
            Ok(rates) => rates,
            Err(error) => {
                self.status = format!("Split failed: {error}");
                return;
            }
        };
        let action = match self.all_items_time_selection_split_action(Some(&source_sample_rates)) {
            Ok(Some(action)) => action,
            Ok(None) => {
                self.status = "No item crosses the time selection".to_owned();
                return;
            }
            Err(error) => {
                self.status = format!("Split failed: {error}");
                return;
            }
        };
        let previous_revision = self.revision;
        self.apply_action(action, "Items split at time selection");
        if self.revision == previous_revision {
            return;
        }
        let mut selected = self
            .project
            .audio_items()
            .iter()
            .filter(|item| start_sample <= item.start_sample() && item.end_sample() <= end_sample)
            .map(|item| (item.id(), item.track_id()))
            .collect::<Vec<_>>();
        selected.extend(self.project.midi_items().iter().filter_map(|item| {
            let end_tick = item.start_tick().checked_add(item.length_ticks())?;
            (selection.start_tick <= item.start_tick() && end_tick <= selection.end_tick)
                .then_some((item.id(), item.track_id()))
        }));
        self.timeline.select_items(selected);
    }

    fn split_all_items_at_cursor(&mut self) {
        let cursor_sample = match self.timeline.edit_cursor_sample(&self.project) {
            Ok(sample) => sample,
            Err(error) => {
                self.status = format!("Split failed: {error}");
                return;
            }
        };
        let media_refs = self
            .project
            .audio_items()
            .iter()
            .filter(|item| item.start_sample() < cursor_sample && cursor_sample < item.end_sample())
            .map(|item| item.media_ref().to_owned())
            .collect::<Vec<_>>();
        let source_sample_rates = match self.audio_source_sample_rates(media_refs) {
            Ok(rates) => rates,
            Err(error) => {
                self.status = format!("Split failed: {error}");
                return;
            }
        };
        let action = match self.all_items_cursor_split_action(Some(&source_sample_rates)) {
            Ok(Some(action)) => action,
            Ok(None) => {
                self.status = "No item crosses the split point".to_owned();
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
        self.apply_action(action, "Items split at edit cursor");
        if self.revision == previous_revision {
            return;
        }
        let selected = self
            .project
            .audio_items()
            .iter()
            .filter(|item| !previous_ids.contains(&item.id()))
            .map(|item| (item.id(), item.track_id()))
            .chain(self.project.midi_items().iter().filter_map(|item| {
                (!previous_ids.contains(&item.id())).then_some((item.id(), item.track_id()))
            }))
            .collect();
        self.timeline.select_items(selected);
        self.audio_item_start_edits.clear();
    }

    fn all_items_cursor_split_action(
        &self,
        source_sample_rates: Option<&HashMap<String, u32>>,
    ) -> Result<Option<DawAction>, String> {
        let tick_boundaries = [self.timeline.edit_cursor_tick];
        let sample_boundaries = [self
            .timeline
            .edit_cursor_sample(&self.project)
            .map_err(|error| error.to_string())?];
        let mut actions = Vec::new();
        for item in self.project.audio_items() {
            let source_sample_rate = source_sample_rates
                .and_then(|rates| rates.get(item.media_ref()).copied())
                .unwrap_or_else(|| self.project.settings().sample_rate());
            split_audio_item_actions_at_samples(
                &self.project,
                item,
                &sample_boundaries,
                source_sample_rate,
                &mut actions,
            )?;
        }
        for item in self.project.midi_items() {
            split_midi_item_actions(item, &tick_boundaries, &mut actions)?;
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

    fn heal_selected_audio_splits(&mut self) {
        let media_refs = self.selected_audio_heal_media_refs();
        if media_refs.is_empty() {
            self.status = "No adjacent selected audio splits to heal".to_owned();
            return;
        }
        let source_sample_rates = match self.audio_source_sample_rates(media_refs) {
            Ok(rates) => rates,
            Err(error) => {
                self.status = format!("Heal splits failed: {error}");
                return;
            }
        };
        let (action, deleted_ids) = match self.selected_audio_heal_action(&source_sample_rates) {
            Ok(Some(action)) => action,
            Ok(None) => {
                self.status = "No adjacent selected audio splits to heal".to_owned();
                return;
            }
            Err(error) => {
                self.status = format!("Heal splits failed: {error}");
                return;
            }
        };
        let previously_selected_ids = self.timeline.selected_items.clone();
        let previous_revision = self.revision;
        self.apply_action(action, "Audio item splits healed");
        if self.revision == previous_revision {
            return;
        }
        let selected_ids = previously_selected_ids
            .difference(&deleted_ids)
            .copied()
            .collect::<HashSet<_>>();
        let selected = self
            .project
            .audio_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
            .map(|item| (item.id(), item.track_id()))
            .chain(self.project.midi_items().iter().filter_map(|item| {
                selected_ids
                    .contains(&item.id())
                    .then_some((item.id(), item.track_id()))
            }))
            .collect();
        self.timeline.select_items(selected);
        self.audio_item_start_edits.clear();
    }

    fn selected_audio_heal_media_refs(&self) -> Vec<String> {
        let mut media_refs = Vec::new();
        for track in self.project.tracks() {
            if track.is_frozen() {
                continue;
            }
            let mut items = self
                .project
                .audio_items()
                .iter()
                .filter(|item| item.track_id() == track.id())
                .collect::<Vec<_>>();
            items.sort_unstable_by_key(|item| item.start_sample());
            for pair in items.windows(2) {
                let [left, right] = pair else {
                    continue;
                };
                if self.timeline.selected_items.contains(&left.id())
                    && self.timeline.selected_items.contains(&right.id())
                    && left.media_ref() == right.media_ref()
                    && left.start_sample().checked_add(left.length_samples())
                        == Some(right.start_sample())
                {
                    media_refs.push(left.media_ref().to_owned());
                }
            }
        }
        media_refs.sort_unstable();
        media_refs.dedup();
        media_refs
    }

    fn selected_audio_heal_action(
        &self,
        source_sample_rates: &HashMap<String, u32>,
    ) -> Result<Option<(DawAction, HashSet<ItemId>)>, String> {
        let selected_ids = &self.timeline.selected_items;
        let mut actions = Vec::new();
        let mut deleted_ids = HashSet::new();
        let project_sample_rate = self.project.settings().sample_rate();

        for track in self.project.tracks() {
            if track.is_frozen() {
                continue;
            }
            let mut items = self
                .project
                .audio_items()
                .iter()
                .filter(|item| item.track_id() == track.id())
                .collect::<Vec<_>>();
            items.sort_unstable_by_key(|item| item.start_sample());
            let mut index = 0;
            while index < items.len() {
                let first = items[index];
                if !selected_ids.contains(&first.id()) {
                    index += 1;
                    continue;
                }
                let mut last_index = index;
                let mut merged_length = first.length_samples();
                while let Some(next) = items.get(last_index + 1) {
                    if !selected_ids.contains(&next.id()) {
                        break;
                    }
                    let Some(source_rate) = source_sample_rates.get(first.media_ref()).copied()
                    else {
                        return Err(format!(
                            "source sample rate is unavailable for {}",
                            first.media_ref()
                        ));
                    };
                    let current = items[last_index];
                    if current.track_id() != next.track_id()
                        || current.media_ref() != next.media_ref()
                        || current.start_sample().checked_add(current.length_samples())
                            != Some(next.start_sample())
                    {
                        break;
                    }
                    let source_delta = (u128::from(current.length_samples())
                        * u128::from(source_rate)
                        + u128::from(project_sample_rate) / 2)
                        / u128::from(project_sample_rate);
                    let source_delta = i64::try_from(source_delta)
                        .map_err(|_| "audio source offset exceeds the supported range")?;
                    if current.source_offset_samples().checked_add(source_delta)
                        != Some(next.source_offset_samples())
                    {
                        break;
                    }
                    let Some(length) = merged_length.checked_add(next.length_samples()) else {
                        return Err("healed audio item exceeds the supported length".to_owned());
                    };
                    merged_length = length;
                    last_index += 1;
                }

                if last_index == index {
                    index += 1;
                    continue;
                }
                actions.push(DawAction::EditAudioItem {
                    item_id: first.id(),
                    media_ref: first.media_ref().to_owned(),
                    start_sample: first.start_sample(),
                    source_offset_samples: first.source_offset_samples(),
                    length_samples: merged_length,
                });
                for item in &items[index + 1..=last_index] {
                    actions.push(DawAction::DeleteAudioItem { item_id: item.id() });
                    deleted_ids.insert(item.id());
                }
                index = last_index + 1;
            }
        }

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some((
                DawAction::BatchTransaction {
                    tx_id: self.revision,
                    actions,
                },
                deleted_ids,
            )))
        }
    }

    fn split_items_in_razor_areas(&mut self) {
        let track_areas = self
            .project
            .tracks()
            .iter()
            .filter(|track| !track.razor_edit_areas().is_empty())
            .map(|track| (track.id(), track.razor_edit_areas().to_vec()))
            .collect::<Vec<_>>();
        let areas_by_track = track_areas.iter().cloned().collect::<HashMap<_, _>>();
        let media_refs = self
            .project
            .audio_items()
            .iter()
            .filter_map(|item| {
                let areas = areas_by_track.get(&item.track_id())?;
                areas
                    .iter()
                    .any(|area| {
                        item.start_sample() < area.start_sample()
                            && area.start_sample() < item.end_sample()
                            || item.start_sample() < area.end_sample()
                                && area.end_sample() < item.end_sample()
                    })
                    .then(|| item.media_ref().to_owned())
            })
            .collect::<Vec<_>>();
        let source_sample_rates = match self.audio_source_sample_rates(media_refs) {
            Ok(rates) => rates,
            Err(error) => {
                self.status = format!("Split failed: {error}");
                return;
            }
        };
        let (action, areas) =
            match self.all_items_razor_area_split_action(Some(&source_sample_rates)) {
                Ok(Some(action)) => action,
                Ok(None) => {
                    self.status = "No razor edit area to split".to_owned();
                    return;
                }
                Err(error) => {
                    self.status = format!("Split failed: {error}");
                    return;
                }
            };
        let previous_revision = self.revision;
        self.apply_action(action, "Items split at razor edit boundaries");
        if self.revision == previous_revision {
            return;
        }
        let area_ticks = areas
            .iter()
            .map(|(track_id, track_areas)| {
                let ticks = track_areas
                    .iter()
                    .filter_map(|area| {
                        Some((
                            self.project.tick_at_sample(area.start_sample()).ok()?,
                            self.project.tick_at_sample(area.end_sample()).ok()?,
                        ))
                    })
                    .collect::<Vec<_>>();
                (*track_id, ticks)
            })
            .collect::<HashMap<_, _>>();
        let mut selected = self
            .project
            .audio_items()
            .iter()
            .filter(|item| {
                areas.iter().any(|(track_id, track_areas)| {
                    *track_id == item.track_id()
                        && track_areas.iter().any(|area| {
                            area.start_sample() <= item.start_sample()
                                && item.end_sample() <= area.end_sample()
                        })
                })
            })
            .map(|item| (item.id(), item.track_id()))
            .collect::<Vec<_>>();
        selected.extend(self.project.midi_items().iter().filter_map(|item| {
            let item_end = item.start_tick().checked_add(item.length_ticks())?;
            area_ticks
                .get(&item.track_id())?
                .iter()
                .any(|(start_tick, end_tick)| {
                    *start_tick <= item.start_tick() && item_end <= *end_tick
                })
                .then_some((item.id(), item.track_id()))
        }));
        self.timeline.select_items(selected);
    }

    fn select_all_items(&mut self) {
        let track_ids = self
            .project
            .tracks()
            .iter()
            .map(|track| track.id())
            .collect::<Vec<_>>();
        let selected = self.items_for_tracks(track_ids);
        self.timeline.select_items(selected);
    }

    fn invert_item_selection(&mut self) {
        let track_ids = self.project.tracks().iter().map(|track| track.id());
        let selected = self
            .items_for_tracks(track_ids)
            .into_iter()
            .filter(|(item_id, _)| !self.timeline.selected_items.contains(item_id))
            .collect();
        self.timeline.select_items(selected);
    }

    fn project_item_ranges(&self) -> Vec<(ItemId, TrackId, u64, u64)> {
        let mut ranges = self
            .project
            .audio_items()
            .iter()
            .filter_map(|item| {
                Some((
                    item.id(),
                    item.track_id(),
                    self.project.tick_at_sample(item.start_sample()).ok()?,
                    self.project.tick_at_sample(item.end_sample()).ok()?,
                ))
            })
            .chain(self.project.midi_items().iter().filter_map(|item| {
                Some((
                    item.id(),
                    item.track_id(),
                    item.start_tick(),
                    item.start_tick().checked_add(item.length_ticks())?,
                ))
            }))
            .collect::<Vec<_>>();
        let track_order = self
            .project
            .tracks()
            .iter()
            .enumerate()
            .map(|(index, track)| (track.id(), index))
            .collect::<HashMap<_, _>>();
        ranges.sort_by_key(|(item_id, track_id, start_tick, _)| {
            (
                track_order.get(track_id).copied().unwrap_or(usize::MAX),
                *start_tick,
                item_id.value(),
            )
        });
        ranges
    }

    pub(super) fn adjacent_non_overlapping_items(&self, forward: bool) -> Vec<(ItemId, TrackId)> {
        let ranges = self.project_item_ranges();
        ranges
            .iter()
            .filter(|(item_id, _, _, _)| self.timeline.selected_items.contains(item_id))
            .filter_map(|(active_id, track_id, active_start, active_end)| {
                let adjacent = if forward {
                    ranges.iter().find(|(_, candidate_track, start, _)| {
                        candidate_track == track_id && *start == *active_end
                    })
                } else {
                    ranges.iter().rev().find(|(_, candidate_track, _, end)| {
                        candidate_track == track_id && *end == *active_start
                    })
                }?;
                (adjacent.0 != *active_id).then_some((adjacent.0, *track_id))
            })
            .collect()
    }

    fn select_adjacent_non_overlapping_item(&mut self, forward: bool) {
        if self.timeline.selected_items.is_empty() {
            return;
        }
        let adjacent = self.adjacent_non_overlapping_items(forward);
        let selected_track = self.timeline.selected_track;
        self.timeline.select_items(adjacent);
        self.timeline.selected_track = selected_track;
    }

    pub(super) fn adjacent_track_item(&self, next_track: bool) -> Option<(ItemId, TrackId, u64)> {
        let ranges = self.project_item_ranges();
        let source = ranges
            .iter()
            .find(|(item_id, _, _, _)| self.timeline.selected_items.contains(item_id))?;
        let source_track_index = self
            .project
            .tracks()
            .iter()
            .position(|track| track.id() == source.1)?;
        let target_track_index = if next_track {
            source_track_index.checked_add(1)?
        } else {
            source_track_index.checked_sub(1)?
        };
        let target_track_id = self.project.tracks().get(target_track_index)?.id();
        ranges
            .iter()
            .find(|(_, track_id, start, end)| {
                *track_id == target_track_id && *start < source.3 && *end > source.2
            })
            .map(|(item_id, track_id, start, _)| (*item_id, *track_id, *start))
    }

    fn select_item_on_adjacent_track(&mut self, next_track: bool) {
        if self.timeline.selected_items.is_empty() {
            return;
        }
        let selected_track = self.timeline.selected_track;
        if let Some((item_id, track_id, start_tick)) = self.adjacent_track_item(next_track) {
            self.timeline.select_items(vec![(item_id, track_id)]);
            self.timeline.selected_track = selected_track;
            self.handle_timeline_view_event(timeline::TimelineEvent::SetEditCursor(start_tick));
        } else {
            self.timeline.clear_selected_items();
        }
    }

    pub(super) fn nearest_item_edge(&self, forward: bool) -> Option<timeline::EditCursorPosition> {
        let cursor_sample = self
            .timeline
            .cursor_position()
            .sample_position(&self.project)
            .ok()?;
        let audio_edges = self.project.audio_items().iter().flat_map(|item| {
            [item.start_sample(), item.end_sample()]
                .into_iter()
                .map(timeline::EditCursorPosition::Sample)
        });
        let midi_edges = self.project.midi_items().iter().flat_map(|item| {
            [
                Some(item.start_tick()),
                item.start_tick().checked_add(item.length_ticks()),
            ]
            .into_iter()
            .flatten()
            .map(timeline::EditCursorPosition::Tick)
        });
        audio_edges
            .chain(midi_edges)
            .filter_map(|position| Some((position.sample_position(&self.project).ok()?, position)))
            .filter(|(sample, _)| {
                if forward {
                    *sample > cursor_sample
                } else {
                    *sample < cursor_sample
                }
            })
            .reduce(|current, next| {
                let choose_next = if forward {
                    next.0 < current.0
                } else {
                    next.0 > current.0
                };
                if choose_next { next } else { current }
            })
            .map(|(_, position)| position)
    }

    fn move_cursor_to_selected_items_boundary(&mut self, move_to_end: bool) {
        let audio_positions = self
            .project
            .audio_items()
            .iter()
            .filter(|item| self.timeline.selected_items.contains(&item.id()))
            .map(|item| {
                timeline::EditCursorPosition::Sample(if move_to_end {
                    item.end_sample()
                } else {
                    item.start_sample()
                })
            });
        let midi_positions = self
            .project
            .midi_items()
            .iter()
            .filter(|item| self.timeline.selected_items.contains(&item.id()))
            .filter_map(|item| {
                let tick = if move_to_end {
                    item.start_tick().checked_add(item.length_ticks())?
                } else {
                    item.start_tick()
                };
                Some(timeline::EditCursorPosition::Tick(tick))
            });
        let position = audio_positions
            .chain(midi_positions)
            .filter_map(|position| Some((position.sample_position(&self.project).ok()?, position)))
            .reduce(|current, next| {
                let choose_next = if move_to_end {
                    next.0 > current.0
                } else {
                    next.0 < current.0
                };
                if choose_next { next } else { current }
            })
            .map(|(_, position)| position);
        if let Some(position) = position {
            self.set_edit_cursor_position(position);
        }
    }

    pub(super) fn set_edit_cursor_position(&mut self, position: timeline::EditCursorPosition) {
        match position.tick(&self.project) {
            Ok(tick) => {
                self.handle_timeline_view_event(timeline::TimelineEvent::SetEditCursorPosition {
                    tick,
                    position,
                });
            }
            Err(error) => {
                self.status = format!("Could not move the edit cursor: {error}");
            }
        }
    }

    fn snap_selected_items(&mut self, direction: ItemSnapDirection) {
        if commands::project_edit_busy(self) || self.timeline.selected_items.is_empty() {
            return;
        }
        let Some(grid_ticks) = self
            .timeline
            .snap_grid
            .tick_interval(self.project.settings().ppq())
        else {
            self.status = "The current snap grid is not available at this PPQ".to_owned();
            return;
        };
        let mut item_ids = self
            .timeline
            .selected_items
            .iter()
            .copied()
            .collect::<Vec<_>>();
        item_ids.sort_unstable_by_key(|item_id| item_id.value());
        let mut actions = Vec::with_capacity(item_ids.len());
        for item_id in item_ids {
            if let Some(item) = self
                .project
                .audio_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                let target_sample = match audio_item_snap_target(
                    &self.project,
                    item.start_sample(),
                    grid_ticks,
                    direction,
                    self.timeline.active_snap_swing_amount_percent(),
                ) {
                    Ok(target) => target,
                    Err(error) => {
                        self.status = format!("Could not snap selected items: {error}");
                        return;
                    }
                };
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
            {
                let target_position = match midi_item_swing_snap_target(
                    &self.project,
                    item.start_position(),
                    grid_ticks,
                    direction,
                    self.timeline.active_snap_swing_amount_percent(),
                ) {
                    Ok(target_tick) => target_tick,
                    Err(error) => {
                        self.status = format!("Could not snap selected items: {error}");
                        return;
                    }
                };
                if target_position != item.start_position() {
                    actions.push(DawAction::SetMidiItemPosition {
                        item_id,
                        position: target_position,
                    });
                }
            }
        }
        if actions.is_empty() {
            return;
        }
        self.apply_action(
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            },
            "Selected items snapped to grid",
        );
    }

    fn select_all_items_in_track(&mut self) {
        let selected = self.items_for_tracks(self.selected_track_ids());
        self.timeline.select_items(selected);
    }

    fn select_all_items_in_time_selection(&mut self) {
        let Some(selection) = self.timeline.time_selection else {
            return;
        };
        let track_ids = self.project.tracks().iter().map(|track| track.id());
        let selected = self.items_in_time_selection(track_ids, selection);
        self.timeline.select_items(selected);
    }

    fn select_all_items_on_selected_tracks_in_time_selection(&mut self) {
        let Some(selection) = self.timeline.time_selection else {
            return;
        };
        let selected = self.items_in_time_selection(self.selected_track_ids(), selection);
        self.timeline.select_items(selected);
    }

    fn selected_track_ids(&self) -> Vec<TrackId> {
        let mut selected = self
            .timeline
            .selected_tracks
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        if let Some(active_track) = self.selected_track_id() {
            selected.insert(active_track);
        }
        self.project
            .tracks()
            .iter()
            .filter(|track| selected.contains(&track.id()))
            .map(|track| track.id())
            .collect()
    }

    fn items_in_time_selection(
        &self,
        track_ids: impl IntoIterator<Item = TrackId>,
        selection: crate::timeline::TimeSelection,
    ) -> Vec<(ItemId, TrackId)> {
        let track_ids = track_ids.into_iter().collect::<Vec<_>>();
        let track_id_set = track_ids.iter().copied().collect::<HashSet<_>>();
        let matching_ids = self
            .project
            .audio_items()
            .iter()
            .filter(|item| track_id_set.contains(&item.track_id()))
            .filter_map(|item| {
                let start_tick = self.project.tick_at_sample(item.start_sample()).ok()?;
                let end_tick = self.project.tick_at_sample(item.end_sample()).ok()?;
                (start_tick < selection.end_tick && end_tick > selection.start_tick)
                    .then_some(item.id())
            })
            .chain(
                self.project
                    .midi_items()
                    .iter()
                    .filter(|item| track_id_set.contains(&item.track_id()))
                    .filter_map(|item| {
                        let end_tick = item.start_tick().checked_add(item.length_ticks())?;
                        (item.start_tick() < selection.end_tick && end_tick > selection.start_tick)
                            .then_some(item.id())
                    }),
            )
            .collect::<HashSet<_>>();
        self.items_for_tracks(track_ids)
            .into_iter()
            .filter(|(item_id, _)| matching_ids.contains(item_id))
            .collect()
    }

    fn items_for_tracks(
        &self,
        track_ids: impl IntoIterator<Item = TrackId>,
    ) -> Vec<(ItemId, TrackId)> {
        let track_ids = track_ids.into_iter().collect::<HashSet<_>>();
        let mut selected =
            Vec::with_capacity(self.project.audio_items().len() + self.project.midi_items().len());
        for track in self
            .project
            .tracks()
            .iter()
            .filter(|track| track_ids.contains(&track.id()))
        {
            let track_id = track.id();
            let mut track_items = self
                .project
                .audio_items()
                .iter()
                .filter(|item| item.track_id() == track_id)
                .map(|item| {
                    (
                        self.project
                            .tick_at_sample(item.start_sample())
                            .unwrap_or(u64::MAX),
                        item.id(),
                    )
                })
                .chain(
                    self.project
                        .midi_items()
                        .iter()
                        .filter(|item| item.track_id() == track_id)
                        .map(|item| (item.start_tick(), item.id())),
                )
                .collect::<Vec<_>>();
            track_items.sort_unstable_by_key(|(tick, item_id)| (*tick, item_id.value()));
            selected.extend(
                track_items
                    .into_iter()
                    .map(|(_, item_id)| (item_id, track_id)),
            );
        }
        selected
    }

    fn trim_items_to_selected_area(&mut self) {
        if commands::project_edit_busy(self) {
            self.status =
                "Stop playback or finish the current project operation before trimming items"
                    .to_owned();
            return;
        }
        let Some(selection) = self.timeline.time_selection else {
            return;
        };
        if selection.start_tick >= selection.end_tick || self.timeline.selected_items.is_empty() {
            return;
        }
        let (selection_start_sample, selection_end_sample) = match (
            self.project.sample_at_tick(selection.start_tick),
            self.project.sample_at_tick(selection.end_tick),
        ) {
            (Ok(start), Ok(end)) => (start, end),
            (Err(error), _) | (_, Err(error)) => {
                self.status = format!("Could not trim items to the selected area: {error}");
                return;
            }
        };

        let mut item_ids = self
            .timeline
            .selected_items
            .iter()
            .copied()
            .collect::<Vec<_>>();
        item_ids.sort_unstable_by_key(|item_id| item_id.value());
        let mut actions = Vec::with_capacity(item_ids.len());
        for item_id in item_ids {
            if let Some(item) = self
                .project
                .audio_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                if selection_start_sample >= selection_end_sample {
                    continue;
                }
                let item_start = item.start_sample();
                let Some(item_end) = item_start.checked_add(item.length_samples()) else {
                    self.status =
                        "Could not trim an audio item with an invalid position".to_owned();
                    return;
                };
                if item_end < selection_start_sample || item_start > selection_end_sample {
                    continue;
                }
                if item_end == selection_start_sample || item_start == selection_end_sample {
                    actions.push(DawAction::DeleteAudioItem { item_id });
                    continue;
                }
                let new_start = item_start.max(selection_start_sample);
                let new_end = item_end.min(selection_end_sample);
                if new_start >= new_end {
                    actions.push(DawAction::DeleteAudioItem { item_id });
                    continue;
                }
                if new_start == item_start && new_end == item_end {
                    continue;
                }
                let Some(source_delta) = i64::try_from(new_start - item_start).ok() else {
                    self.status =
                        "Could not trim an audio item past its source position".to_owned();
                    return;
                };
                let source_offset_samples = if item.active_take_is_empty() {
                    0
                } else if let Some(source_offset_samples) =
                    item.source_offset_samples().checked_add(source_delta)
                {
                    source_offset_samples
                } else {
                    self.status =
                        "Could not trim an audio item past its source position".to_owned();
                    return;
                };
                actions.push(DawAction::EditAudioItem {
                    item_id,
                    media_ref: item.media_ref().to_owned(),
                    start_sample: new_start,
                    source_offset_samples,
                    length_samples: new_end - new_start,
                });
            } else if let Some(item) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
            {
                let item_start = item.start_tick();
                let Some(item_end) = item_start.checked_add(item.length_ticks()) else {
                    self.status = "Could not trim a MIDI item with an invalid position".to_owned();
                    return;
                };
                if item_end < selection.start_tick || item_start > selection.end_tick {
                    continue;
                }
                if item_end == selection.start_tick || item_start == selection.end_tick {
                    actions.push(DawAction::DeleteMidiItem { item_id });
                    continue;
                }
                let new_start = item_start.max(selection.start_tick);
                let new_end = item_end.min(selection.end_tick);
                if new_start >= new_end {
                    actions.push(DawAction::DeleteMidiItem { item_id });
                    continue;
                }
                if new_start == item_start && new_end == item_end {
                    continue;
                }
                let new_length = new_end - new_start;
                if new_start > item_start {
                    let Some(source_offset_ticks) = i64::try_from(
                        i128::from(item.signed_source_offset_ticks()) + i128::from(new_start)
                            - i128::from(item_start),
                    )
                    .ok() else {
                        self.status =
                            "Could not trim a MIDI item past its source position".to_owned();
                        return;
                    };
                    actions.push(DawAction::TrimMidiItemStart {
                        item_id,
                        start_tick: new_start,
                        length_ticks: new_length,
                        source_offset_ticks,
                    });
                } else {
                    actions.push(DawAction::EditMidiItem {
                        item_id,
                        start_tick: new_start,
                        length_ticks: new_length,
                    });
                }
            }
        }
        if actions.is_empty() {
            return;
        }
        self.audio_item_start_edits.clear();
        let action = if actions.len() == 1 {
            actions.pop().expect("single trim action is present")
        } else {
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            }
        };
        self.apply_action(action, "Selected items trimmed to selected area");
    }

    fn selected_audio_items_for_take_clipboard(&self) -> Vec<&aaadaw_core::AudioItem> {
        self.project
            .audio_items()
            .iter()
            .filter(|item| self.timeline.selected_items.contains(&item.id()))
            .collect()
    }

    pub(super) fn can_copy_selected_audio_items(&self) -> bool {
        !self.selected_audio_items_for_take_clipboard().is_empty()
    }

    fn copy_selected_audio_items(&mut self) {
        let items = self.selected_audio_items_for_take_clipboard();
        if items.is_empty() {
            self.status = "Select an audio item to copy its active take".to_owned();
            return;
        }
        let takes = items
            .iter()
            .map(|item| item.active_take_snapshot())
            .collect::<Vec<_>>();
        let take_count = takes.len();
        self.audio_take_clipboard = Some(AudioTakeClipboard {
            project_generation: self.project_generation,
            takes,
        });
        self.status = if take_count == 1 {
            "Active audio take copied".to_owned()
        } else {
            format!("Copied {take_count} active audio takes")
        };
    }

    pub(super) fn can_paste_audio_item_as_take(&self) -> bool {
        let Some(clipboard) = self.audio_take_clipboard.as_ref() else {
            return false;
        };
        if clipboard.project_generation != self.project_generation {
            return false;
        }
        let selected_audio_ids = commands::selected_audio_item_ids(self);
        !selected_audio_ids.is_empty()
            && !selected_audio_ids.iter().any(|item_id| {
                self.project
                    .tracks()
                    .iter()
                    .any(|track| track.frozen_audio_item_id() == Some(*item_id))
            })
    }

    fn paste_audio_item_as_take(&mut self) {
        if commands::project_edit_busy(self) {
            self.status =
                "Stop playback or finish the current project operation before pasting takes"
                    .to_owned();
            return;
        }
        if !self.can_paste_audio_item_as_take() {
            return;
        }
        let selected_audio_ids = commands::selected_audio_item_ids(self);
        let Some(takes) = self
            .audio_take_clipboard
            .as_ref()
            .map(|clipboard| clipboard.takes.clone())
        else {
            return;
        };
        let mut actions = selected_audio_ids
            .iter()
            .flat_map(|item_id| {
                takes
                    .iter()
                    .cloned()
                    .map(move |take| DawAction::PasteAudioItemAsTake {
                        item_id: *item_id,
                        take,
                    })
            })
            .collect::<Vec<_>>();
        let pasted_take_count = takes.len();
        let selected_item_count = selected_audio_ids.len();
        let action = if actions.len() == 1 {
            actions
                .pop()
                .expect("one selected audio item action is present")
        } else {
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            }
        };
        self.apply_action(
            action,
            if pasted_take_count == 1 && selected_item_count == 1 {
                "Take pasted into audio item"
            } else if pasted_take_count == 1 {
                "Take pasted into selected audio items"
            } else if selected_item_count == 1 {
                "Takes pasted into audio item"
            } else {
                "Takes pasted into selected audio items"
            },
        );
    }

    fn implode_selected_audio_items_as_takes(&mut self) {
        if commands::project_edit_busy(self) {
            self.status =
                "Stop playback or finish the current project operation before imploding items"
                    .to_owned();
            return;
        }
        let Some(item_ids) = commands::audio_items_for_implode(self) else {
            self.status = "Select at least two audio items on the same track".to_owned();
            return;
        };
        if item_ids.iter().any(|item_id| {
            self.project
                .tracks()
                .iter()
                .any(|track| track.frozen_audio_item_id() == Some(*item_id))
        }) {
            self.status = "Frozen render items cannot be imploded into takes".to_owned();
            return;
        }
        let target_id = item_ids[0];
        let Some(target) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == target_id)
        else {
            return;
        };
        let track_id = target.track_id();
        let original_active_take = target.active_take_index();
        let donors = item_ids
            .iter()
            .skip(1)
            .filter_map(|item_id| {
                self.project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == *item_id)
                    .map(|item| (*item_id, item.take_snapshots()))
            })
            .collect::<Vec<_>>();
        if donors.len() + 1 != item_ids.len() {
            return;
        }

        let mut actions = donors
            .iter()
            .flat_map(|(_, takes)| {
                takes
                    .iter()
                    .cloned()
                    .map(move |take| DawAction::PasteAudioItemAsTake {
                        item_id: target_id,
                        take,
                    })
            })
            .collect::<Vec<_>>();
        actions.push(DawAction::SetAudioItemActiveTake {
            item_id: target_id,
            take_index: original_active_take,
        });
        actions.extend(
            donors
                .iter()
                .map(|(item_id, _)| DawAction::DeleteAudioItem { item_id: *item_id }),
        );

        for item_id in &item_ids {
            self.audio_item_start_edits.remove(item_id);
        }
        let previous_revision = self.revision;
        let project_history_cursor_before = self.project.history_cursor();
        self.apply_action(
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            },
            "Selected audio items imploded into takes",
        );
        if self.revision != previous_revision {
            self.timeline.select_items(vec![(target_id, track_id)]);
            let after = self.timeline.item_selection_state();
            self.timeline.clear_selected_items();
            let before = self.timeline.item_selection_state();
            self.timeline.restore_item_selection_state(&after);
            if matches!(self.app_undo_history.last(), Some(AppHistoryEntry::Project)) {
                self.app_undo_history.pop();
                self.app_undo_history
                    .push(AppHistoryEntry::ProjectWithItemSelection(
                        ItemSelectionHistory {
                            before: Box::new(before),
                            after: Box::new(after),
                            project_history_cursor_before,
                            project_history_cursor_after: self.project.history_cursor(),
                        },
                    ));
            }
        }
    }

    fn implode_selected_midi_items_as_takes(&mut self) {
        if commands::project_edit_busy(self) {
            self.status =
                "Stop playback or finish the current project operation before imploding items"
                    .to_owned();
            return;
        }
        let Some(item_ids) = commands::midi_items_for_implode(self) else {
            self.status = "Select at least two MIDI items on the same track".to_owned();
            return;
        };
        let Some(target_id) = item_ids.first().copied() else {
            return;
        };
        let Some(target) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == target_id)
        else {
            return;
        };
        let target_track_id = target.track_id();
        if self
            .project
            .tracks()
            .iter()
            .any(|track| track.id() == target_track_id && track.is_frozen())
        {
            self.status = "Frozen tracks cannot be imploded into takes".to_owned();
            return;
        }

        let previous_revision = self.revision;
        let project_history_cursor_before = self.project.history_cursor();
        let selection_before = self.timeline.item_selection_state();
        self.apply_action(
            DawAction::ImplodeMidiItemsOnSameTrackIntoTakes {
                tx_id: self.revision,
                item_ids,
            },
            "Selected MIDI items imploded into takes",
        );
        if self.revision != previous_revision {
            self.timeline
                .select_items(vec![(target_id, target_track_id)]);
            let after = self.timeline.item_selection_state();
            if matches!(self.app_undo_history.last(), Some(AppHistoryEntry::Project)) {
                self.app_undo_history.pop();
                self.app_undo_history
                    .push(AppHistoryEntry::ProjectWithItemSelection(
                        ItemSelectionHistory {
                            before: Box::new(selection_before),
                            after: Box::new(after),
                            project_history_cursor_before,
                            project_history_cursor_after: self.project.history_cursor(),
                        },
                    ));
            }
        }
    }

    fn implode_selected_midi_items_across_tracks_as_takes(&mut self) {
        let Some(item_ids) = commands::midi_items_for_cross_track_implode(self) else {
            self.status = "Select overlapping MIDI items on at least two tracks".to_owned();
            return;
        };
        if item_ids.iter().any(|item_id| {
            self.project.midi_items().iter().any(|item| {
                item.id() == *item_id
                    && self
                        .project
                        .tracks()
                        .iter()
                        .any(|track| track.id() == item.track_id() && track.is_frozen())
            })
        }) {
            self.status = "Frozen tracks cannot be imploded into takes".to_owned();
            return;
        }
        let Some(target_id) = item_ids.first().copied() else {
            return;
        };
        let Some(target) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == target_id)
        else {
            return;
        };
        let target_track_id = target.track_id();
        let target_position = target.start_position();
        let target_start_sample = match self
            .project
            .sample_at_tick_position(target_position.as_ticks_f64())
        {
            Ok(sample) => sample,
            Err(error) => {
                self.status =
                    format!("Could not align MIDI takes for cross-track implode: {error}");
                return;
            }
        };
        let mut actions = Vec::new();
        for donor_id in item_ids.iter().skip(1) {
            let Some(donor) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == *donor_id)
            else {
                return;
            };
            let donor_position = donor.start_position();
            let donor_start_sample = match self
                .project
                .sample_at_tick_position(donor_position.as_ticks_f64())
            {
                Ok(sample) => sample,
                Err(error) => {
                    self.status =
                        format!("Could not align MIDI takes for cross-track implode: {error}");
                    return;
                }
            };
            for mut take in donor.take_snapshots() {
                let donor_source_offset_samples = match self
                    .project
                    .midi_signed_source_offset_samples_at_position(
                        donor_position,
                        take.source_offset_ticks,
                    ) {
                    Ok(offset) => offset,
                    Err(error) => {
                        self.status = format!("Could not align MIDI take {}: {error}", take.name);
                        return;
                    }
                };
                let target_source_offset_samples =
                    target_start_sample - donor_start_sample + donor_source_offset_samples;
                take.source_offset_ticks = match self
                    .project
                    .midi_signed_source_offset_ticks_at_position_samples(
                        target_position,
                        target_source_offset_samples,
                    ) {
                    Ok(offset) => offset,
                    Err(error) => {
                        self.status = format!("Could not align MIDI take {}: {error}", take.name);
                        return;
                    }
                };
                actions.push(DawAction::PasteMidiItemAsTake {
                    item_id: target_id,
                    take,
                });
            }
        }
        actions.extend(
            item_ids
                .iter()
                .skip(1)
                .map(|item_id| DawAction::DeleteMidiItem { item_id: *item_id }),
        );

        let previous_revision = self.revision;
        let project_history_cursor_before = self.project.history_cursor();
        let selection_before = self.timeline.item_selection_state();
        self.apply_action(
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            },
            "Selected MIDI items imploded into takes",
        );
        if self.revision != previous_revision {
            self.timeline
                .select_items(vec![(target_id, target_track_id)]);
            let after = self.timeline.item_selection_state();
            if matches!(self.app_undo_history.last(), Some(AppHistoryEntry::Project)) {
                self.app_undo_history.pop();
                self.app_undo_history
                    .push(AppHistoryEntry::ProjectWithItemSelection(
                        ItemSelectionHistory {
                            before: Box::new(selection_before),
                            after: Box::new(after),
                            project_history_cursor_before,
                            project_history_cursor_after: self.project.history_cursor(),
                        },
                    ));
            }
        }
    }

    fn implode_selected_audio_items_across_tracks_as_takes(&mut self) {
        if commands::project_edit_busy(self) {
            self.status =
                "Stop playback or finish the current project operation before imploding items"
                    .to_owned();
            return;
        }
        if commands::midi_items_for_cross_track_implode(self).is_some() {
            self.implode_selected_midi_items_across_tracks_as_takes();
            return;
        }
        let Some(item_ids) = commands::audio_items_for_cross_track_implode(self) else {
            self.status = "Select audio items on at least two tracks".to_owned();
            return;
        };
        if item_ids.iter().any(|item_id| {
            self.project
                .tracks()
                .iter()
                .any(|track| track.frozen_audio_item_id() == Some(*item_id))
        }) {
            self.status = "Frozen render items cannot be imploded into takes".to_owned();
            return;
        }
        let target_id = item_ids[0];
        let Some(target) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == target_id)
        else {
            return;
        };
        let target_track_id = target.track_id();
        let target_start_sample = target.start_sample();
        let original_active_take = target.active_take_index();
        let donors = item_ids
            .iter()
            .skip(1)
            .filter_map(|item_id| {
                self.project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == *item_id)
                    .map(|item| (*item_id, item.start_sample(), item.take_snapshots()))
            })
            .collect::<Vec<_>>();
        if donors.len() + 1 != item_ids.len() {
            return;
        }

        let media_refs = donors
            .iter()
            .filter(|(_, start_sample, _)| *start_sample != target_start_sample)
            .flat_map(|(_, _, takes)| {
                takes
                    .iter()
                    .filter(|take| !take.is_empty)
                    .map(|take| take.media_ref.clone())
            })
            .collect::<Vec<_>>();
        let source_sample_rates = match self.audio_source_sample_rates(media_refs) {
            Ok(rates) => rates,
            Err(error) => {
                self.status =
                    format!("Could not align source takes for cross-track implode: {error}");
                return;
            }
        };
        let project_sample_rate = self.project.settings().sample_rate();
        let mut actions = Vec::new();
        for (_, donor_start_sample, takes) in &donors {
            let project_positions_differ = *donor_start_sample != target_start_sample;
            for mut take in takes.iter().cloned() {
                if project_positions_differ && !take.is_empty {
                    let Some(source_sample_rate) =
                        source_sample_rates.get(&take.media_ref).copied()
                    else {
                        self.status = format!(
                            "Could not align source take {}; sample rate is unavailable",
                            take.name
                        );
                        return;
                    };
                    let Some(source_offset_samples) = rebase_audio_take_source_offset_samples(
                        take.source_offset_samples,
                        target_start_sample,
                        *donor_start_sample,
                        source_sample_rate,
                        project_sample_rate,
                        take.playback_rate,
                    ) else {
                        self.status = format!(
                            "Could not align source take {}; resulting offset is out of range",
                            take.name
                        );
                        return;
                    };
                    take.source_offset_samples = source_offset_samples;
                }
                actions.push(DawAction::PasteAudioItemAsTake {
                    item_id: target_id,
                    take,
                });
            }
        }
        actions.push(DawAction::SetAudioItemActiveTake {
            item_id: target_id,
            take_index: original_active_take,
        });
        actions.extend(
            donors
                .iter()
                .map(|(item_id, _, _)| DawAction::DeleteAudioItem { item_id: *item_id }),
        );

        for item_id in &item_ids {
            self.audio_item_start_edits.remove(item_id);
        }
        let previous_revision = self.revision;
        let project_history_cursor_before = self.project.history_cursor();
        self.apply_action(
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            },
            "Selected audio items imploded into takes",
        );
        if self.revision != previous_revision {
            self.timeline
                .select_items(vec![(target_id, target_track_id)]);
            let after = self.timeline.item_selection_state();
            self.timeline.clear_selected_items();
            let before = self.timeline.item_selection_state();
            self.timeline.restore_item_selection_state(&after);
            if matches!(self.app_undo_history.last(), Some(AppHistoryEntry::Project)) {
                self.app_undo_history.pop();
                self.app_undo_history
                    .push(AppHistoryEntry::ProjectWithItemSelection(
                        ItemSelectionHistory {
                            before: Box::new(before),
                            after: Box::new(after),
                            project_history_cursor_before,
                            project_history_cursor_after: self.project.history_cursor(),
                        },
                    ));
            }
        }
    }

    fn explode_selected_audio_item_takes_in_place(&mut self) {
        if commands::project_edit_busy(self) {
            self.status =
                "Stop playback or finish the current project operation before exploding takes"
                    .to_owned();
            return;
        }
        let Some(item_ids) = commands::media_items_for_explode_takes(self) else {
            self.status = "Select media items to explode their takes".to_owned();
            return;
        };
        let audio_ids = item_ids
            .iter()
            .filter(|item_id| {
                self.project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == **item_id)
                    .is_some()
            })
            .copied()
            .collect::<Vec<_>>();
        let midi_ids = item_ids
            .iter()
            .filter(|item_id| {
                self.project
                    .midi_items()
                    .iter()
                    .find(|item| item.id() == **item_id)
                    .is_some()
            })
            .copied()
            .collect::<Vec<_>>();
        let has_multi_take_audio = audio_ids.iter().any(|item_id| {
            self.project
                .audio_items()
                .iter()
                .find(|item| item.id() == *item_id)
                .is_some_and(|item| item.take_count() > 1)
        });
        let has_multi_take_midi = midi_ids.iter().any(|item_id| {
            self.project
                .midi_items()
                .iter()
                .find(|item| item.id() == *item_id)
                .is_some_and(|item| item.take_count() > 1)
        });
        if !has_multi_take_audio && !has_multi_take_midi {
            self.timeline.clear_selected_items();
            self.status = "No multi-take items selected".to_owned();
            return;
        }
        if audio_ids.iter().any(|item_id| {
            self.project
                .audio_items()
                .iter()
                .find(|item| item.id() == *item_id)
                .is_some_and(|item| {
                    item.take_count() > 1
                        && self
                            .project
                            .tracks()
                            .iter()
                            .any(|track| track.frozen_audio_item_id() == Some(*item_id))
                })
        }) || midi_ids.iter().any(|item_id| {
            self.project
                .midi_items()
                .iter()
                .find(|item| item.id() == *item_id)
                .is_some_and(|item| {
                    item.take_count() > 1
                        && self
                            .project
                            .tracks()
                            .iter()
                            .any(|track| track.id() == item.track_id() && track.is_frozen())
                })
        }) {
            self.status = "Frozen items cannot be exploded".to_owned();
            return;
        }

        for item_id in &audio_ids {
            self.audio_item_start_edits.remove(item_id);
        }
        let mut actions = Vec::with_capacity(2);
        if has_multi_take_audio {
            actions.push(DawAction::ExplodeAudioItemTakesInPlace {
                tx_id: self.revision,
                item_ids: audio_ids,
            });
        }
        if has_multi_take_midi {
            actions.push(DawAction::ExplodeMidiItemTakesInPlace {
                tx_id: self.revision,
                item_ids: midi_ids,
            });
        }
        let action = if actions.len() == 1 {
            actions.pop().expect("one take explode action exists")
        } else {
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            }
        };
        self.apply_action(action, "Selected item takes exploded in place");
        self.timeline.clear_selected_items();
    }

    fn copy_loop_of_selected_area(&mut self) {
        let Some(selection) = self.timeline.time_selection else {
            return;
        };
        if selection.start_tick >= selection.end_tick {
            return;
        }
        let (selection_start_sample, selection_end_sample) = match (
            self.project.sample_at_tick(selection.start_tick),
            self.project.sample_at_tick(selection.end_tick),
        ) {
            (Ok(start), Ok(end)) => (start, end),
            (Err(error), _) | (_, Err(error)) => {
                self.status = format!("Could not copy loop of selected area: {error}");
                return;
            }
        };
        let span_samples = selection_end_sample.saturating_sub(selection_start_sample);
        if span_samples == 0 {
            return;
        }

        let mut containing_items = self
            .project
            .audio_items()
            .iter()
            .filter(|item| self.timeline.selected_items.contains(&item.id()))
            .filter_map(|item| {
                let end_sample = item.start_sample().checked_add(item.length_samples())?;
                (item.start_sample() <= selection_start_sample
                    && end_sample >= selection_end_sample)
                    .then_some(item)
            })
            .collect::<Vec<_>>();
        if containing_items.is_empty() {
            return;
        }
        containing_items.sort_unstable_by_key(|item| item.id().value());
        let source_limits = match self.audio_source_limits(
            containing_items
                .iter()
                .map(|item| item.media_ref().to_owned()),
        ) {
            Ok(rates) => rates,
            Err(error) => {
                self.status = format!("Could not copy loop of selected area: {error}");
                return;
            }
        };
        let track_indices = self
            .project
            .tracks()
            .iter()
            .enumerate()
            .map(|(index, track)| (track.id(), index))
            .collect::<HashMap<_, _>>();
        let first_track_index = containing_items
            .iter()
            .filter_map(|item| track_indices.get(&item.track_id()).copied())
            .min();
        let Some(first_track_index) = first_track_index else {
            return;
        };
        let project_sample_rate = self.project.settings().sample_rate();
        let mut entries = Vec::with_capacity(containing_items.len());
        for item in containing_items {
            let Some((source_sample_rate, frame_count)) =
                source_limits.get(item.media_ref()).copied()
            else {
                self.status = format!(
                    "Could not copy loop of selected area: source sample rate is unavailable for {}",
                    item.media_ref()
                );
                return;
            };
            let Some(track_index) = track_indices.get(&item.track_id()).copied() else {
                continue;
            };
            let Some(source_delta) = rescale_samples_at_take_rate(
                selection_start_sample - item.start_sample(),
                project_sample_rate,
                source_sample_rate,
                item.take_playback_rate(),
            ) else {
                self.status =
                    "Could not copy loop of selected area: source offset is too large".to_owned();
                return;
            };
            let Some(source_section_length_samples) = rescale_samples_at_take_rate(
                span_samples,
                project_sample_rate,
                source_sample_rate,
                item.take_playback_rate(),
            ) else {
                self.status =
                    "Could not copy loop of selected area: source section is too large".to_owned();
                return;
            };
            if source_section_length_samples == 0 {
                self.status =
                    "Could not copy loop of selected area: source section is shorter than one frame"
                        .to_owned();
                return;
            }
            let bounded_section = item
                .source_section_start_samples()
                .zip(item.source_section_length_samples());
            let source_offset = item.source_offset_samples();
            let source_position_in_view = if item.is_loop_source() {
                let view_length =
                    bounded_section.map_or(frame_count, |(_, section_length)| section_length);
                let Ok(current_offset) = u64::try_from(source_offset) else {
                    self.status = "Could not copy loop of selected area: source section is invalid"
                        .to_owned();
                    return;
                };
                if view_length == 0 || current_offset >= view_length {
                    self.status = "Could not copy loop of selected area: source section is invalid"
                        .to_owned();
                    return;
                }
                if let Some((section_start, section_length)) = bounded_section
                    && section_start
                        .checked_add(section_length)
                        .is_none_or(|section_end| section_end > frame_count)
                {
                    self.status = "Could not copy loop of selected area: source section is invalid"
                        .to_owned();
                    return;
                }
                let source_phase_delta = source_delta % view_length;
                let wrapped_phase = (u128::from(current_offset) + u128::from(source_phase_delta))
                    % u128::from(view_length);
                let Ok(wrapped_phase) = u64::try_from(wrapped_phase) else {
                    self.status =
                        "Could not copy loop of selected area: source offset is too large"
                            .to_owned();
                    return;
                };
                wrapped_phase
            } else {
                let Some(offset) = i128::from(source_offset)
                    .checked_add(i128::from(source_delta))
                    .and_then(|offset| u64::try_from(offset).ok())
                else {
                    self.status = "Could not copy loop of selected area: source section is invalid"
                        .to_owned();
                    return;
                };
                offset
            };
            let Some(source_position_samples) = bounded_section
                .map_or(Some(source_position_in_view), |(section_start, _)| {
                    section_start.checked_add(source_position_in_view)
                })
            else {
                self.status =
                    "Could not copy loop of selected area: source offset is too large".to_owned();
                return;
            };
            if let Some((section_start, section_length)) = bounded_section {
                let Some(copy_end) =
                    source_position_samples.checked_add(source_section_length_samples)
                else {
                    self.status =
                        "Could not copy loop of selected area: source section is too large"
                            .to_owned();
                    return;
                };
                let Some(original_end) = section_start.checked_add(section_length) else {
                    self.status = "Could not copy loop of selected area: source section is invalid"
                        .to_owned();
                    return;
                };
                if source_position_samples < section_start || copy_end > original_end {
                    self.status =
                        "Copy loop crossing a source-loop boundary is not supported".to_owned();
                    return;
                }
            }
            entries.push(AudioItemClipboardEntry {
                media_ref: item.media_ref().to_owned(),
                name: if item.name().is_empty() || item.name().ends_with(" loop") {
                    item.name().to_owned()
                } else {
                    format!("{} loop", item.name())
                },
                source_offset_samples: 0,
                source_section_start_samples: source_position_samples,
                source_section_length_samples,
                length_samples: span_samples,
                volume: item.volume(),
                take_volume: item.take_volume(),
                take_pan: item.take_pan(),
                take_playback_rate: item.take_playback_rate(),
                muted: item.is_muted(),
                phase_inverted: item.is_phase_inverted(),
                track_offset: track_index - first_track_index,
            });
        }
        if entries.is_empty() {
            return;
        }
        self.audio_item_clipboard = Some(AudioItemClipboard {
            project_generation: self.project_generation,
            project_sample_rate: self.project.settings().sample_rate(),
            span_samples,
            items: entries,
        });
        self.status = format!(
            "Copied loop from {} audio item{}",
            self.audio_item_clipboard
                .as_ref()
                .map_or(0, |clipboard| clipboard.items.len()),
            if self
                .audio_item_clipboard
                .as_ref()
                .is_some_and(|clipboard| clipboard.items.len() == 1)
            {
                ""
            } else {
                "s"
            }
        );
    }

    fn enclose_media_items_in_razor_edit(&mut self) {
        let mut selected_tracks = HashSet::new();
        let mut start_sample = None::<u64>;
        let mut end_sample = None::<u64>;
        for item in self.project.audio_items() {
            if !self.timeline.selected_items.contains(&item.id()) {
                continue;
            }
            selected_tracks.insert(item.track_id());
            start_sample = Some(
                start_sample.map_or(item.start_sample(), |start| start.min(item.start_sample())),
            );
            end_sample =
                Some(end_sample.map_or(item.end_sample(), |end| end.max(item.end_sample())));
        }
        for item in self.project.midi_items() {
            if !self.timeline.selected_items.contains(&item.id()) {
                continue;
            }
            let Some(end_tick) = item.start_tick().checked_add(item.length_ticks()) else {
                self.status = "Could not enclose selected items in a razor edit".to_owned();
                return;
            };
            let (start, end) = match (
                self.project.sample_at_tick(item.start_tick()),
                self.project.sample_at_tick(end_tick),
            ) {
                (Ok(start), Ok(end)) => (start, end),
                (Err(error), _) | (_, Err(error)) => {
                    self.status =
                        format!("Could not enclose selected items in a razor edit: {error}");
                    return;
                }
            };
            selected_tracks.insert(item.track_id());
            start_sample = Some(start_sample.map_or(start, |current| current.min(start)));
            end_sample = Some(end_sample.map_or(end, |current| current.max(end)));
        }

        let selected_area = start_sample
            .zip(end_sample)
            .and_then(|(start, end)| aaadaw_core::RazorEditArea::new(start, end));
        let track_areas = self
            .project
            .tracks()
            .iter()
            .map(|track| {
                let areas = selected_area
                    .filter(|_| selected_tracks.contains(&track.id()))
                    .into_iter()
                    .collect::<Vec<_>>();
                (track.id(), areas)
            })
            .collect::<Vec<_>>();
        let changed = track_areas.iter().any(|(track_id, areas)| {
            self.project
                .tracks()
                .iter()
                .find(|track| track.id() == *track_id)
                .is_some_and(|track| track.razor_edit_areas() != areas)
        });
        if changed {
            self.apply_action(
                DawAction::SetTrackRazorEditAreas { track_areas },
                if selected_area.is_some() {
                    "Razor edit area selected"
                } else {
                    "Razor edit areas cleared"
                },
            );
        } else if selected_area.is_some() {
            self.status = "Razor edit area selected".to_owned();
        }
        self.timeline.clear_selected_items();
    }

    pub(super) fn can_paste_audio_items(&self) -> bool {
        let Some(clipboard) = self.audio_item_clipboard.as_ref().filter(|clipboard| {
            clipboard.project_generation == self.project_generation && !clipboard.items.is_empty()
        }) else {
            return false;
        };
        let anchor_index = self
            .selected_track_id()
            .and_then(|track_id| {
                self.project
                    .tracks()
                    .iter()
                    .position(|track| track.id() == track_id)
            })
            .unwrap_or(0);
        clipboard
            .items
            .iter()
            .all(|item| anchor_index.checked_add(item.track_offset).is_some())
    }

    fn paste_audio_items(&mut self) {
        if commands::project_edit_busy(self) {
            self.status =
                "Stop playback or finish the current project operation before pasting items"
                    .to_owned();
            return;
        }
        if !self.can_paste_audio_items() {
            return;
        }
        let Some(clipboard) = self.audio_item_clipboard.clone() else {
            return;
        };
        let anchor_index = self
            .selected_track_id()
            .and_then(|track_id| {
                self.project
                    .tracks()
                    .iter()
                    .position(|track| track.id() == track_id)
            })
            .unwrap_or(0);
        let Ok(start_sample) = self.timeline.edit_cursor_sample(&self.project) else {
            self.status = "Could not paste items at the edit cursor".to_owned();
            return;
        };
        let Some(length_samples) = rescale_samples(
            clipboard.span_samples,
            clipboard.project_sample_rate,
            self.project.settings().sample_rate(),
        ) else {
            self.status = "Could not paste items: copied audio span is too large".to_owned();
            return;
        };
        let required_track_count = clipboard
            .items
            .iter()
            .filter_map(|item| anchor_index.checked_add(item.track_offset))
            .max()
            .and_then(|track_index| track_index.checked_add(1));
        let Some(required_track_count) = required_track_count else {
            return;
        };
        let mut destination_track_ids = self
            .project
            .tracks()
            .iter()
            .map(|track| track.id())
            .collect::<Vec<_>>();
        let tracks_to_create = required_track_count.saturating_sub(destination_track_ids.len());
        let first_new_track_id = if tracks_to_create == 0 {
            None
        } else {
            let Some(first_id) = self
                .project
                .next_track_id()
                .map(|track_id| track_id.value())
            else {
                self.status = "Could not paste items: track identifiers are exhausted".to_owned();
                return;
            };
            let Ok(create_count) = u64::try_from(tracks_to_create) else {
                self.status = "Could not paste items: too many destination tracks".to_owned();
                return;
            };
            if first_id.checked_add(create_count).is_none() {
                self.status = "Could not paste items: track identifiers are exhausted".to_owned();
                return;
            }
            Some(first_id)
        };
        let mut actions = Vec::with_capacity(tracks_to_create + clipboard.items.len());
        for offset in 0..tracks_to_create {
            let Some(raw_track_id) =
                first_new_track_id.and_then(|first| first.checked_add(u64::try_from(offset).ok()?))
            else {
                self.status = "Could not paste items: track identifiers are exhausted".to_owned();
                return;
            };
            let Some(track_id) = TrackId::from_value(raw_track_id) else {
                self.status = "Could not paste items: track identifiers are exhausted".to_owned();
                return;
            };
            actions.push(DawAction::CreateTrack {
                index: destination_track_ids.len(),
                name: String::new(),
            });
            destination_track_ids.push(track_id);
        }
        actions.extend(clipboard.items.iter().filter_map(|item| {
            let track_index = anchor_index.checked_add(item.track_offset)?;
            let track_id = *destination_track_ids.get(track_index)?;
            Some(DawAction::InsertLoopedAudioItemWithMix {
                track_id,
                media_ref: item.media_ref.clone(),
                name: item.name.clone(),
                start_sample,
                source_offset_samples: item.source_offset_samples,
                source_section_start_samples: item.source_section_start_samples,
                length_samples,
                muted: item.muted,
                phase_inverted: item.phase_inverted,
                source_section_length_samples: item.source_section_length_samples,
                volume: item.volume,
                take_volume: item.take_volume,
                take_pan: item.take_pan,
                take_playback_rate: item.take_playback_rate,
            })
        }));
        if actions.is_empty() {
            return;
        }
        let previous_ids = self
            .project
            .audio_items()
            .iter()
            .map(|item| item.id())
            .chain(self.project.midi_items().iter().map(|item| item.id()))
            .collect::<HashSet<_>>();
        let previous_revision = self.revision;
        let action = if actions.len() == 1 {
            actions
                .into_iter()
                .next()
                .expect("one paste action is present")
        } else {
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            }
        };
        self.apply_action(action, "Audio items pasted");
        if self.revision != previous_revision {
            let selected = self
                .project
                .audio_items()
                .iter()
                .filter(|item| !previous_ids.contains(&item.id()))
                .map(|item| (item.id(), item.track_id()))
                .collect::<Vec<_>>();
            self.timeline.select_items(selected);
        }
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
        if self.project_path.is_none() || self.current_project_is_template() {
            self.pick_path(PathPickerTarget::SaveProject)
        } else {
            project_io::save_project(self, None)
        }
    }

    fn save_new_project_version_command(&mut self) -> Task<Message> {
        project_io::save_new_project_version(self)
    }

    fn open_project_command(&mut self) -> Task<Message> {
        self.begin_project_transition(PendingProjectTransition::OpenProject)
    }

    fn open_recent_project(&mut self, path: PathBuf) -> Task<Message> {
        if self.project_path.as_deref() == Some(path.as_path()) && self.project_lock.is_some() {
            self.status = format!("{} is already open", path.display());
            return Task::none();
        }
        self.pending_project_path_to_open = Some(path);
        let task = self.begin_project_transition(PendingProjectTransition::OpenProject);
        if self.pending_project_transition.is_none() {
            self.pending_project_path_to_open = None;
        }
        task
    }

    fn open_project_template(&mut self, path: PathBuf) -> Task<Message> {
        self.pending_project_template_source = Some(path);
        self.pending_project_path_to_open = None;
        self.pending_project_template_working_copy = None;
        let task = self.begin_project_transition(PendingProjectTransition::OpenProject);
        if self.pending_project_transition.is_none() {
            self.pending_project_template_source = None;
            self.pending_project_path_to_open = None;
            self.pending_project_template_working_copy = None;
        }
        task
    }

    fn save_project_template(&mut self, selected_path: PathBuf) -> Task<Message> {
        let Some(template_directory) = project_templates::directory() else {
            self.status = "Project template folder is unavailable".to_owned();
            return Task::none();
        };
        self.request_project_template_save(selected_path, &template_directory)
    }

    fn request_project_template_save(
        &mut self,
        selected_path: PathBuf,
        template_directory: &std::path::Path,
    ) -> Task<Message> {
        if self.io_busy || self.unsaved_session_snapshot_busy || self.path_picker_busy {
            self.status = "Wait for the current project operation to finish".to_owned();
            return Task::none();
        }
        let path = match project_templates::resolve_save_path(&selected_path, template_directory) {
            Ok(path) => path,
            Err(error) => {
                self.status = error;
                return Task::none();
            }
        };
        match std::fs::symlink_metadata(&path) {
            Ok(_) if path.is_file() => {
                self.pending_project_template_overwrite = Some(path.clone());
                self.active_menu = None;
                self.project_templates_menu_open = false;
                self.status = format!("File exists: {}. Overwrite?", path.display());
                return Task::none();
            }
            Ok(_) => {
                self.status = format!("Project template path is not a file: {}", path.display());
                return Task::none();
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                self.status = format!("Could not inspect project template path: {error}");
                return Task::none();
            }
            Err(_) => {}
        }
        self.write_project_template(path, false)
    }

    fn overwrite_project_template(&mut self, path: PathBuf) -> Task<Message> {
        if self.io_busy || self.unsaved_session_snapshot_busy || self.path_picker_busy {
            self.status = "Wait for the current project operation to finish".to_owned();
            return Task::none();
        }
        if self
            .project_path
            .as_deref()
            .is_some_and(|project_path| project_templates::same_path(project_path, &path))
        {
            self.status = "Cannot overwrite a project template while it is open".to_owned();
            return Task::none();
        }
        match std::fs::symlink_metadata(&path) {
            Ok(_) if path.is_file() => self.write_project_template(path, true),
            Ok(_) => {
                self.status = format!("Project template path is not a file: {}", path.display());
                Task::none()
            }
            Err(error) => {
                self.status = if error.kind() == std::io::ErrorKind::NotFound {
                    "Project template no longer exists; choose a new template name".to_owned()
                } else {
                    format!("Could not inspect project template path: {error}")
                };
                Task::none()
            }
        }
    }

    fn write_project_template(&mut self, path: PathBuf, overwrite: bool) -> Task<Message> {
        if self.io_busy || self.unsaved_session_snapshot_busy || self.path_picker_busy {
            self.status = "Wait for the current project operation to finish".to_owned();
            return Task::none();
        }
        let snapshot = self.project.snapshot();
        let view_state = self.project_arrangement_view_state();
        let source_media_path = self.media_store_path();
        self.io_busy = true;
        self.status = if overwrite {
            format!("Overwriting project template {}…", path.display())
        } else {
            format!("Saving project template {}…", path.display())
        };
        Task::perform(
            run_blocking("aaadaw-project-template-save", move || {
                let save = if overwrite {
                    project_io::overwrite_project_template(
                        path.clone(),
                        snapshot,
                        view_state,
                        source_media_path,
                    )
                } else {
                    project_io::save_project_template(
                        path.clone(),
                        snapshot,
                        view_state,
                        source_media_path,
                    )
                };
                save.map(|()| path)
            }),
            |result| match result {
                Ok(path) => Message::ProjectTemplateSaved(path, Ok(())),
                Err(error) => Message::ProjectTemplateSaved(PathBuf::new(), Err(error)),
            },
        )
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
    if matches!(
        text.trim().to_lowercase().as_str(),
        "-inf" | "−inf" | "-∞" | "−∞"
    ) {
        return Ok(fader::SILENCE_DB);
    }
    let value = text
        .trim()
        .parse::<f32>()
        .map_err(|_| "Enter a valid volume in dB")?;
    if !value.is_finite() || !10.0_f32.powf(value / 20.0).is_finite() {
        return Err("Enter a finite volume in dB");
    }
    // REAPER's precise entry can exceed the display fader range (e.g. +20 dB).
    Ok(value)
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

fn rescale_samples(samples: u64, from_sample_rate: u32, to_sample_rate: u32) -> Option<u64> {
    if from_sample_rate == 0 || to_sample_rate == 0 {
        return None;
    }
    u64::try_from(
        (u128::from(samples) * u128::from(to_sample_rate) + u128::from(from_sample_rate) / 2)
            / u128::from(from_sample_rate),
    )
    .ok()
}

fn rescale_samples_at_take_rate(
    samples: u64,
    project_sample_rate: u32,
    source_sample_rate: u32,
    take_rate: f64,
) -> Option<u64> {
    if project_sample_rate == 0
        || source_sample_rate == 0
        || !take_rate.is_finite()
        || take_rate <= 0.0
        || samples > (1_u64 << 53) - 1
    {
        return None;
    }
    let source_frames = (samples as f64) * f64::from(source_sample_rate) * take_rate
        / f64::from(project_sample_rate);
    if !source_frames.is_finite() || source_frames < 0.0 || source_frames >= u64::MAX as f64 {
        return None;
    }
    Some(source_frames.round() as u64)
}

fn split_audio_item_actions(
    project: &Project,
    item: &AudioItem,
    boundaries: &[u64],
    source_sample_rate: u32,
    actions: &mut Vec<DawAction>,
) -> Result<(), String> {
    let cut_samples = boundaries
        .iter()
        .filter_map(|tick| project.sample_at_tick(*tick).ok())
        .collect::<Vec<_>>();
    split_audio_item_actions_at_samples(project, item, &cut_samples, source_sample_rate, actions)
}

fn split_audio_item_actions_at_samples(
    project: &Project,
    item: &AudioItem,
    boundaries: &[u64],
    source_sample_rate: u32,
    actions: &mut Vec<DawAction>,
) -> Result<(), String> {
    let mut cut_samples = boundaries
        .iter()
        .copied()
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
        let source_delta = i64::try_from(
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

#[cfg(test)]
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

fn menu_command_index(
    current: Option<usize>,
    command_count: usize,
    navigation: MenuNavigation,
) -> usize {
    debug_assert!(command_count > 0);
    let last = command_count - 1;
    match navigation {
        MenuNavigation::FirstCommand => 0,
        MenuNavigation::LastCommand => last,
        MenuNavigation::NextCommand => current.map_or(0, |index| (index + 1) % command_count),
        MenuNavigation::PreviousCommand => {
            current.map_or(last, |index| (index + last) % command_count)
        }
        _ => unreachable!("only command navigation has a command index"),
    }
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
#[cfg(test)]
fn prepare_project_playback_file(
    path: PathBuf,
    snapshot: ProjectSnapshot,
    target_sample: u64,
) -> Result<PreparedAudioPlayback, String> {
    prepare_project_playback_file_with_loop_points(path, snapshot, target_sample, None)
}

#[cfg(feature = "audio-device")]
fn prepare_project_playback_file_with_loop_points(
    path: PathBuf,
    snapshot: ProjectSnapshot,
    target_sample: u64,
    loop_points_ticks: Option<(u64, u64)>,
) -> Result<PreparedAudioPlayback, String> {
    if !path.is_file() {
        return Err(format!("project file {} does not exist", path.display()));
    }
    let project = Project::from_snapshot(snapshot).map_err(|error| error.to_string())?;
    let store = ProjectStore::open(path).map_err(|error| error.to_string())?;
    let transport_loop_range = loop_points_ticks
        .map(|(start_tick, end_tick)| {
            let start_sample = project
                .sample_at_tick(start_tick)
                .map_err(|error| error.to_string())?;
            let end_sample = project
                .sample_at_tick(end_tick)
                .map_err(|error| error.to_string())?;
            Ok::<_, String>(TransportLoopRange::new(start_sample, end_sample))
        })
        .transpose()?
        .flatten();
    let prepared = prepare_audio_playback_at_with_loop_range(
        &project,
        &store,
        target_sample,
        transport_loop_range,
        16_384,
        8_192,
        aaadaw_media::ResamplingQuality::Sinc64,
    )
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
    if status == iced::event::Status::Ignored && *modifiers == iced::keyboard::Modifiers::ALT {
        let menu = match key.as_ref() {
            iced::keyboard::Key::Character(value) if value.eq_ignore_ascii_case("a") => {
                Some(MainMenu::Actions)
            }
            iced::keyboard::Key::Character(value) if value.eq_ignore_ascii_case("e") => {
                Some(MainMenu::Edit)
            }
            iced::keyboard::Key::Character(value) if value.eq_ignore_ascii_case("f") => {
                Some(MainMenu::File)
            }
            iced::keyboard::Key::Character(value) if value.eq_ignore_ascii_case("h") => {
                Some(MainMenu::Help)
            }
            iced::keyboard::Key::Character(value) if value.eq_ignore_ascii_case("i") => {
                Some(MainMenu::Insert)
            }
            iced::keyboard::Key::Character(value) if value.eq_ignore_ascii_case("o") => {
                Some(MainMenu::Options)
            }
            iced::keyboard::Key::Character(value) if value.eq_ignore_ascii_case("v") => {
                Some(MainMenu::View)
            }
            _ => None,
        };
        if let Some(menu) = menu {
            return Some(Message::MenuKeyboard(MenuNavigation::OpenMenu(menu)));
        }
    }
    let navigation = match key.as_ref() {
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) if active_menu.is_some() => {
            return Some(Message::Escape);
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Home)
            if active_menu.is_some()
                && (status == iced::event::Status::Ignored
                    || active_menu == Some(MainMenu::Actions)) =>
        {
            MenuNavigation::FirstCommand
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::End)
            if active_menu.is_some()
                && (status == iced::event::Status::Ignored
                    || active_menu == Some(MainMenu::Actions)) =>
        {
            MenuNavigation::LastCommand
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

fn android_app_focus_event(
    event: iced::Event,
    _status: iced::event::Status,
    _window_id: iced::window::Id,
) -> Option<Message> {
    if !cfg!(target_os = "android") {
        return None;
    }
    match event {
        iced::Event::Window(iced::window::Event::Focused) => {
            Some(Message::AndroidAppFocusChanged(true))
        }
        iced::Event::Window(iced::window::Event::Unfocused) => {
            Some(Message::AndroidAppFocusChanged(false))
        }
        _ => None,
    }
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
            physical_key,
            location,
            repeat: false,
            ..
        }) = event
        else {
            return None;
        };
        return match key.as_ref() {
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
                if modifiers == iced::keyboard::Modifiers::NONE =>
            {
                Some(Message::CancelShortcutCapture)
            }
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Backspace)
                if modifiers == iced::keyboard::Modifiers::NONE =>
            {
                Some(Message::ClearShortcutBinding(action_id.to_owned()))
            }
            _ => {
                let input = shortcut::ShortcutInput {
                    logical_key: key,
                    physical_key,
                    location,
                    modifiers: iced::keyboard::Modifiers::NONE,
                };
                let key = shortcut::Shortcut::capture_input(&input)
                    .map(|key| key.config_label())
                    .unwrap_or_else(|_| "Unidentified".to_owned());
                Some(Message::ShortcutCaptureKey {
                    action_id: action_id.to_owned(),
                    key,
                    modifiers,
                })
            }
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
        physical_key,
        location,
        repeat: false,
        ..
    }) = event
    else {
        return None;
    };
    if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
        && modifiers == iced::keyboard::Modifiers::NONE
    {
        Some(Message::Escape)
    } else {
        let input = shortcut::ShortcutInput {
            logical_key: key,
            physical_key,
            location,
            modifiers,
        };
        if input.logical_key == iced::keyboard::Key::Unidentified
            && shortcut::Shortcut::capture_input(&input).is_err()
        {
            None
        } else {
            Some(Message::ShortcutPressed(input))
        }
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
