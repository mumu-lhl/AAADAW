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
        "C", "Câ™¯", "D", "Dâ™¯", "E", "F", "Fâ™¯", "G", "Gâ™¯", "A", "Aâ™¯", "B",
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
    let octave = if matches!(next, '#' | 'b' | 'â™¯' | 'â™­') {
        semitone += match next {
            '#' | 'â™¯' => 1,
            'b' | 'â™­' => -1,
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
        app.status = format!("Opening {}â€¦", path.display());
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
            format!("{} â€” AAADAW", self.project_display_title())
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
                            | Message::ImplodeSelectedAudioItemsAcrossTracksAsTakes
                            | Message::ExplodeSelectedAudioItemTakesInPlace
                            | Message::CropSelectedItemsToActiveTake
                            | Message::SwitchSelectedAudioItemTakes(..)
                            | Message::SwitchAudioItemTake(..)
                            | Message::SwitchMidiItemTake(..)
                            | Message::AddEmptyMidiTakes(..)
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
                        TempoCurve::Logarithmic => TempoCurve::BÃ©zier,
                        TempoCurve::BÃ©zier => TempoCurve::Step,
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
                } else if self.routing_window_id == Some(window_id) {
                    self.routing_window_id = None;
                    self.routing_track_id = None;
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
                    Ok(()) => "Scanning USB and paired Bluetooth MIDI devicesâ€¦".to_owned(),
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
            ×žù×fòµë(š+myÒæ—5÷6öÖUöæB‡Ç&Wf–÷W7Â&Wf–÷W2Ò6VÆbç&ö¦V7Eö'&ævVÖVçE÷f–Wu÷7FFR‚’¢ÇÂ&Wf–÷W5÷F–ÖU÷&ævW2Ò‡6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÂ6VÆbçF–ÖVÆ–æRæÆö÷÷ö–çG2¢°¢6VÆbç&Wf—6–öâÒ6VÆbç&Wf—6–öâçw&–æuöFBƒ“°¢Ð¢Ð ¢fâæWu÷&ö¦V7B‚f×WB6VÆb’°¢6VÆbç&VÆV6UöÖ–F•÷&Wf–Wr‚“°¢–b6VÆbæ–õö'W7¢ÇÂ6VÆbæ–×÷'Eö'W7¢ÇÂ6VÆbæVF–õö76WEöÖævVÖVçEö'W7¢ÇÂ6VÆbçF…÷–6¶W%ö'W7¢°¢6VÆbç7FGW2Ò%v—Bf÷"F†R7W'&VçB&ö¦V7B÷W&F–öâFòf–æ—6‚"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–b6VÆbçÆ–&6²æ—5÷6öÖR‚’°¢6VÆbç7FGW2Òf÷&ÖB€¢$6Æ÷6R·Ò÷WGWB&Vf÷&R7&VF–æræWr&ö¦V7B"À¢6VÆbçÆ–&6µöæÖR‚¢“°¢&WGW&ã°¢Ð ¢6VÆbç&÷WF–æu÷G&6µö–BÒæöæS°¢6VÆbç&÷WF–æu÷6VæEöG&gG2æ6ÆV"‚“°¢ÆWB†æWu÷&ö¦V7BÂFVfVÇG5öW'&÷"’Ð¢ÖF6‚&ö¦V7EöFVfVÇG3£¦ÆöB‚’ææE÷F†Vâ‡ÆFVfVÇG7ÂFVfVÇG2æ7&VFU÷&ö¦V7B‚’’°¢ö²‡&ö¦V7B’Óâ‡&ö¦V7BÂæöæR’À¢W'"†W'&÷"’Óâ…&ö¦V7C£¦æWr‚’Â6öÖR†W'&÷"’’À¢Ó°¢6VÆbæ6ÆV%öö†—7F÷'’‚“°¢6VÆbç&ö¦V7BÒæWu÷&ö¦V7C°¢6VÆbç&W7F÷&U÷&WVEög&öÕ÷f–Wu÷7FFR„æöæR“°¢6VÆbç&Vg&W6…÷FV×õöÖöVF—G2‚“°¢6VÆbç&Vg&W6…öÖWFW%öÖöVF—G2‚“°¢6VÆbçFV×õöÖöfVVF&6²æ6ÆV"‚“°¢6VÆbæÖWFW%öÖöfVVF&6²æ6ÆV"‚“°¢6VÆbç&ö¦V7EövVæW&F–öâÒ6VÆbç&ö¦V7EövVæW&F–öâçw&–æuöFBƒ“°¢6VÆbç6VÆV7FVE÷&VæFW%öÖ&¶W'2æ6ÆV"‚“°¢6VÆbç6VÆV7FVE÷&VæFW%÷&Vv–öç2æ6ÆV"‚“°¢6VÆbç&V6÷&F–æu÷&V6÷fW'•ö6æF–FFW2æ6ÆV"‚“°¢6VÆbç&V6÷&F–æu÷&V6÷fW'•÷66ææ–ærÒfÇ6S°¢6VÆbæÖ–F•öæ÷FUö6Æ—&ö&BæÆ7E÷7FRÒæöæS°¢6VÆbç&ö¦V7E÷F‚ÒæöæS°¢6VÆbç&ö¦V7E÷FV×ÆFU÷6÷W&6RÒæöæS°¢6VÆbç&WF—&U÷Vç6fVE÷6W76–öåöÖVF–‚“°¢6VÆbçVç6fVE÷6W76–öå÷6æ6†÷E÷&Wf—6–öâÒ°¢ÆWB6W76–öåöÖVF–öW'&÷"ÒÖF6‚7&VFU÷Vç6fVE÷&ö¦V7E÷6W76–öåöF—"‚’°¢ö²‡6W76–öåöF—"’Óâ°¢6VÆbç6W76–öåöÖVF–öF—"Ò6öÖR‡6W76–öåöF—"“°¢æöæP¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç6W76–öåöÖVF–öF—"ÒæöæS°¢6öÖR†W'&÷"¢Ð¢Ó°¢6VÆbç&ö¦V7EöÆö6²ÒæöæS°¢6VÆbç&ö¦V7E÷FV×ÆFU÷v÷&¶–æuö6÷’ÒæöæS°¢6VÆbç&ö¦V7E÷F…÷VW'’æ6ÆV"‚“°¢6VÆbç&Wf—6–öâÒ°¢6VÆbç6fVE÷&Wf—6–öâÒ°¢6VÆbçF–ÖVÆ–æRç&WÆ6U÷&ö¦V7B‚g6VÆbç&ö¦V7BÂæöæR“°¢6VÆbçF–ÖVÆ–æRç6VÆV7FVE÷G&6²ÒæöæS°¢6VÆbçF–ÖVÆ–æRç6VÆV7FVE÷G&6·2æ6ÆV"‚“°¢6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒæöæS°¢6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6ÆV"‚“°¢6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÒæöæS°¢6VÆbçF–ÖVÆ–æRæÆö÷÷ö–çG2ÒæöæS°¢6VÆbçF–ÖVÆ–æRæ÷&–v–å÷F–6²Ò°¢6VÆbçF–ÖVÆ–æRç6WEöVF—Eö7W'6÷%÷F–6²ƒ“°¢6VÆbçF–ÖVÆ–æRçfW'F–6Å÷67&öÆÂÒã°¢6VÆbæ6ÆV%÷G&6µöG&gE÷7FFR‚“°¢6VÆbçG&6µöÖ—…övW7GW&RÒæöæS°¢6VÆbçG&6µöÖ—…ö6öÖÖ—EöBÒæöæS°¢6VÆbæÖ7FW%öÖ—…övW7GW&RÒæöæS°¢6VÆbæÖ7FW%öÖ—…ö6öÖÖ—EöBÒæöæS°¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2æ6ÆV"‚“°¢6VÆbæVF–õ÷vfVf÷&×2æ6ÆV"‚“°¢6VÆbæVF–õö76WE÷6÷W&6U÷7FGW6W2æ6ÆV"‚“°¢6VÆbç7FGW2ÒæWu÷&ö¦V7E÷7FGW2‡6W76–öåöÖVF–öW'&÷"æ5öFW&Vb‚’“°¢–bÆWB6öÖR†W'&÷"’ÒFVfVÇG5öW'&÷"°¢6VÆbç7FGW2Òf÷&ÖB‚$FVfVÇB&ö¦V7B6WGF–æw2Væf–Æ&ÆS¢¶W'&÷'Ò"“°¢Ð¢Ð ¢fâ&Vv–å÷&ö¦V7E÷G&ç6—F–öâ‚f×WB6VÆbÂG&ç6—F–öã¢VæF–æu&ö¦V7EG&ç6—F–öâ’ÓâF6³ÄÖW76vSâ°¢6VÆbæ6æ6VÅö—FVÕöfFW2‚“°¢–b6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâæ—5÷6öÖR‚’°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢–b6VÆbæ–õö'W7¢ÇÂ6VÆbçVç6fVE÷6W76–öå÷6æ6†÷Eö'W7¢ÇÂ6VÆbçF…÷–6¶W%ö'W7¢ÇÂ6VÆbæ–×÷'Eö'W7¢ÇÂ6VÆbæVF–õö76WEöÖævVÖVçEö'W7¢ÇÂ6VÆbçÆ–&6µö'W7’‚¢°¢6VÆbç7FGW2Ò%v—Bf÷"F†R7W'&VçB&ö¦V7B÷W&F–öâFòf–æ—6‚"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÖF6†W2€¢G&ç6—F–öâÀ¢VæF–æu&ö¦V7EG&ç6—F–öã£¤æWu&ö¦V7@¢ÂVæF–æu&ö¦V7EG&ç6—F–öã£¤6Æ÷6U&ö¦V7@¢ÂVæF–æu&ö¦V7EG&ç6—F–öã£¤÷Vå&ö¦V7@¢’bb6VÆbçÆ–&6²æ—5÷6öÖR‚¢°¢6VÆbç7FGW2Òf÷&ÖB€¢$6Æ÷6R·Ò÷WGWB&Vf÷&R6†æv–ær&ö¦V7G2"À¢6VÆbçÆ–&6µöæÖR‚¢“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâÒ6öÖR‡G&ç6—F–öâ“°¢–b6VÆbæ—5öF—'G’‚’°¢6VÆbç7FGW2Ò%6fR6†ævW2FòF†R7W'&VçB&ö¦V7Cò"çFõö÷væVB‚“°¢F6³£¦æöæR‚¢ÒVÇ6R°¢6VÆbæ6öçF–çVU÷VæF–æu÷&ö¦V7E÷G&ç6—F–öâ‚¢Ð¢Ð ¢fâ6fUö&Vf÷&U÷&ö¦V7E÷G&ç6—F–öâ‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢–b6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâæ—5öæöæR‚’°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢6VÆbç6fU÷&ö¦V7Eö6öÖÖæB‚¢Ð ¢fâF—66&EöæEö6öçF–çVU÷&ö¦V7E÷G&ç6—F–öâ‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢ÖF6‚6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâ°¢6öÖR…VæF–æu&ö¦V7EG&ç6—F–öã£¤æWu&ö¦V7B’Óâ°¢6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâÒæöæS°¢6VÆbç6fVE÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢6VÆbææWu÷&ö¦V7B‚“°¢F6³£¦æöæR‚¢Ð¢6öÖR…VæF–æu&ö¦V7EG&ç6—F–öã£¤6Æ÷6U&ö¦V7B’Óâ°¢6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâÒæöæS°¢6VÆbææWu÷&ö¦V7B‚“°¢6VÆbç7FGW2Ò%&ö¦V7B6Æ÷6VB"çFõö÷væVB‚“°¢F6³£¦æöæR‚¢Ð¢6öÖR…VæF–æu&ö¦V7EG&ç6—F–öã£¤÷Vå&ö¦V7B’Óâ6VÆbæ6öçF–çVUö÷Vå÷&ö¦V7E÷G&ç6—F–öâ‚’À¢6öÖR…VæF–æu&ö¦V7EG&ç6—F–öã£¤6Æ÷6TÖ–åv–æF÷r‡v–æF÷uö–B’’Óâ°¢6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâÒæöæS°¢6VÆbæ6Æ÷6UöÖ–å÷v–æF÷r‡v–æF÷uö–B¢Ð¢æöæRÓâF6³£¦æöæR‚’À¢Ð¢Ð ¢fâ6öçF–çVU÷VæF–æu÷&ö¦V7E÷G&ç6—F–öâ‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢ÖF6‚6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâ°¢6öÖR…VæF–æu&ö¦V7EG&ç6—F–öã£¤æWu&ö¦V7B’Óâ°¢6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâÒæöæS°¢6VÆbææWu÷&ö¦V7B‚“°¢F6³£¦æöæR‚¢Ð¢6öÖR…VæF–æu&ö¦V7EG&ç6—F–öã£¤6Æ÷6U&ö¦V7B’Óâ°¢6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâÒæöæS°¢6VÆbææWu÷&ö¦V7B‚“°¢6VÆbç7FGW2Ò%&ö¦V7B6Æ÷6VB"çFõö÷væVB‚“°¢F6³£¦æöæR‚¢Ð¢6öÖR…VæF–æu&ö¦V7EG&ç6—F–öã£¤÷Vå&ö¦V7B’Óâ6VÆbæ6öçF–çVUö÷Vå÷&ö¦V7E÷G&ç6—F–öâ‚’À¢6öÖR…VæF–æu&ö¦V7EG&ç6—F–öã£¤6Æ÷6TÖ–åv–æF÷r‡v–æF÷uö–B’’Óâ°¢6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâÒæöæS°¢6VÆbæ6Æ÷6UöÖ–å÷v–æF÷r‡v–æF÷uö–B¢Ð¢æöæRÓâF6³£¦æöæR‚’À¢Ð¢Ð ¢fâ6öçF–çVUö÷Vå÷&ö¦V7E÷G&ç6—F–öâ‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢–bÆWB6öÖR‡6÷W&6R’Ò6VÆ`¢çVæF–æu÷&ö¦V7E÷FV×ÆFU÷6÷W&6P¢æ6ÆöæR‚¢æf–ÇFW"‡Å÷Â6VÆbçVæF–æu÷&ö¦V7E÷FV×ÆFU÷v÷&¶–æuö6÷’æ—5öæöæR‚’¢°¢6VÆbæ–õö'W7’ÒG'VS°¢6VÆbç7FGW2Òf÷&ÖB‚%&W&–ær&ö¦V7BFV×ÆFR·Þ(
b"Â6÷W&6RæF—7Æ’‚’“°¢ÆWB6÷W&6Uöf÷%ö6÷’Ò6÷W&6Ræ6ÆöæR‚“°¢&WGW&âF6³£§W&f÷&Ò€¢'Våö&Æö6¶–ær‚&Fr×FV×ÆFR×v÷&¶–ærÖ6÷’"ÂÖ÷fRÇÂ°¢&ö¦V7E÷FV×ÆFW3£¦7&VFU÷v÷&¶–æuö6÷’‚g6÷W&6Uöf÷%ö6÷’¢Ò’À¢Ö÷fRÇ&W7VÇGÂÖW76vS£¥&ö¦V7EFV×ÆFT6÷–VB‡6÷W&6RÂ&W7VÇB’À¢“°¢Ð¢–bÆWB6öÖR‡F‚’Ò6VÆbçVæF–æu÷&ö¦V7E÷F…÷Fõö÷VâçF¶R‚’°¢6VÆbç&ö¦V7E÷F…÷VW'’ÒF‚çFõ÷7G&–æuöÆ÷77’‚’æ–çFõö÷væVB‚“°¢&ö¦V7Eö–ó£¦÷Vå÷&ö¦V7B‡6VÆb¢ÒVÇ6R°¢6VÆbç–6µ÷F‚…F…–6¶W%F&vWC£¤÷Vå&ö¦V7B¢Ð¢Ð ¢fâ6Æ÷6UöÖ–å÷v–æF÷r‚f×WB6VÆbÂv–æF÷uö–C¢–6VC£§v–æF÷s£¤–B’ÓâF6³ÄÖW76vSâ°¢6VÆbæ6Æ÷6Uög…öVF—F÷%÷&W6÷W&6W2‚“°¢–6VC£§v–æF÷s£¦6Æ÷6R‡v–æF÷uö–B¢Ð ¢fâ÷Våö&÷WB‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢–bÆWB6öÖR‡v–æF÷uö–B’Ò6VÆbæ&÷WE÷v–æF÷uö–B°¢&WGW&â–6VC£§v–æF÷s£¦v–åöfö7W2‡v–æF÷uö–B“°¢Ð¢ÆWB‡v–æF÷uö–BÂF6²’Ò–6VC£§v–æF÷s£¦÷Vâ†–6VC£§v–æF÷s£¥6WGF–æw2°¢6—¦S¢–6VC£¥6—¦S£¦æWrƒCCãÂ#ƒã’À¢Ö–å÷6—¦S¢6öÖR†–6VC£¥6—¦S£¦æWrƒCãÂ#Cã’’À¢âæ–6VC£§v–æF÷s£¥6WGF–æw3£¦FVfVÇB‚¢Ò“°¢6VÆbæ&÷WE÷v–æF÷uö–BÒ6öÖR‡v–æF÷uö–B“°¢F6²æF—66&B‚¢Ð ¢fâ÷Vå÷6WGF–æw2‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢–b6VÆbæ—5öÖö&–ÆUöÖ–å÷v–æF÷r‚’°¢6VÆbç6†÷uöÖö&–ÆU÷æVÂ„Öö&–ÆUæVÃ£¥6WGF–æw2“°¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¢&WGW&âF6³£¦&F6‚…°¢6VÆbç&Vg&W6…ö7Åö÷WGWEöFWf–6W2‚’À¢6VÆbç&Vg&W6…ö7Åö–çWEöFWf–6W2‚’À¢Ò“°¢5¶6fr†æ÷B†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’’•Ð¢&WGW&âF6³£¦æöæR‚“°¢Ð¢–bÆWB6öÖR‡v–æF÷uö–B’Ò6VÆbç6WGF–æw5÷v–æF÷uö–B°¢ÆWBfö7W2Ò–6VC£§v–æF÷s£¦v–åöfö7W2‡v–æF÷uö–B“°¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¢&WGW&âF6³£¦&F6‚…°¢fö7W2À¢6VÆbç&Vg&W6…ö7Åö÷WGWEöFWf–6W2‚’À¢6VÆbç&Vg&W6…ö7Åö–çWEöFWf–6W2‚’À¢Ò“°¢5¶6fr†æ÷B†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’’•Ð¢&WGW&âfö7W3°¢Ð¢ÆWB‡v–æF÷uö–BÂF6²’Ò–6VC£§v–æF÷s£¦÷Vâ†–6VC£§v–æF÷s£¥6WGF–æw2°¢6—¦S¢–b6fr‡F&vWEö÷2Ò&æG&ö–B"’°¢–6VC£¥6—¦S£¦æWrƒC#ãÂcCã¢ÒVÇ6R°¢–6VC£¥6—¦S£¦æWrƒscãÂc#ã¢ÒÀ¢Ö–å÷6—¦S¢6öÖR†–b6fr‡F&vWEö÷2Ò&æG&ö–B"’°¢–6VC£¥6—¦S£¦æWrƒ3cãÂCƒã¢ÒVÇ6R°¢–6VC£¥6—¦S£¦æWrƒcCãÂCcã¢Ò’À¢âæ–6VC£§v–æF÷s£¥6WGF–æw3£¦FVfVÇB‚¢Ò“°¢6VÆbç6WGF–æw5÷v–æF÷uö–BÒ6öÖR‡v–æF÷uö–B“°¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¢&WGW&âF6³£¦&F6‚…°¢F6²æF—66&B‚’À¢6VÆbç&Vg&W6…ö7Åö÷WGWEöFWf–6W2‚’À¢6VÆbç&Vg&W6…ö7Åö–çWEöFWf–6W2‚’À¢Ò“°¢5¶6fr†æ÷B†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’’•Ð¢F6²æF—66&B‚¢Ð ¢fâ÷Vå÷&VæFW%÷v–æF÷r‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢6VÆbæ–æ—F–Æ—¦Uö7W7FöÕ÷&VæFW%÷&ævR‚“°¢–bÆWB6öÖR‡v–æF÷uö–B’Ò6VÆbç&VæFW%÷v–æF÷uö–B°¢&WGW&â–6VC£§v–æF÷s£¦v–åöfö7W2‡v–æF÷uö–B“°¢Ð¢ÆWB‡v–æF÷uö–BÂF6²’Ò–6VC£§v–æF÷s£¦÷Vâ†–6VC£§v–æF÷s£¥6WGF–æw2°¢6—¦S¢–6VC£¥6—¦S£¦æWrƒcCãÂs#ã’À¢Ö–å÷6—¦S¢6öÖR†–6VC£¥6—¦S£¦æWrƒC#ãÂ3cã’’À¢âæ–6VC£§v–æF÷s£¥6WGF–æw3£¦FVfVÇB‚¢Ò“°¢6VÆbç&VæFW%÷v–æF÷uö–BÒ6öÖR‡v–æF÷uö–B“°¢F6²æF—66&B‚¢Ð ¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¢fâ&Vg&W6…ö7Åö÷WGWEöFWf–6W2‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢7F'Eö7ÅöFWf–6UöVçVÖW&F–öâ€¢f×WB6VÆbæ7Åö÷WGWEöFWf–6W5öÆöF–ærÀ¢f×WB6VÆbæ7Åö÷WGWEöFWf–6W5öW'&÷"À¢&FrÖ7ÂÖFWf–6RÖÆ—7B"À¢ÇÂFuöVæv–æS£¦VçVÖW&FUö7Åö÷WGWEöFWf–6W2‚’æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’’À¢ÖW76vS£¤7Ä÷WGWDFWf–6W4ÆöFVBÀ¢¢Ð ¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¢fâf–æ—6…ö7Åö÷WGWEöFWf–6UöVçVÖW&F–öâ€¢f×WB6VÆbÀ¢&W7VÇC¢&W7VÇCÅfV3ÆFuöVæv–æS£¤7Ä÷WGWDFWf–6T–æfóâÂ7G&–æsâÀ¢’°¢f–æ—6…ö7ÅöFWf–6UöVçVÖW&F–öâ€¢f×WB6VÆbæ7Åö÷WGWEöFWf–6W2À¢f×WB6VÆbæ7Åö÷WGWEöFWf–6W5öÆöF–ærÀ¢f×WB6VÆbæ7Åö÷WGWEöFWf–6W5öW'&÷"À¢f×WB6VÆbæVF–õ÷6WGF–æw5öfVVF&6²À¢&÷WGWB"À¢&W7VÇBÀ¢“°¢Ð ¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¢fâ&Vg&W6…ö7Åö–çWEöFWf–6W2‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢7F'Eö7ÅöFWf–6UöVçVÖW&F–öâ€¢f×WB6VÆbæ7Åö–çWEöFWf–6W5öÆöF–ærÀ¢f×WB6VÆbæ7Åö–çWEöFWf–6W5öW'&÷"À¢&FrÖ7ÂÖ–çWBÖFWf–6RÖÆ—7B"À¢ÇÂFuöVæv–æS£¦VçVÖW&FUö7Åö–çWEöFWf–6W2‚’æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’’À¢ÖW76vS£¤7Ä–çWDFWf–6W4ÆöFVBÀ¢¢Ð ¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¢fâf–æ—6…ö7Åö–çWEöFWf–6UöVçVÖW&F–öâ€¢f×WB6VÆbÀ¢&W7VÇC¢&W7VÇCÅfV3ÆFuöVæv–æS£¤7Ä–çWDFWf–6T–æfóâÂ7G&–æsâÀ¢’°¢f–æ—6…ö7ÅöFWf–6UöVçVÖW&F–öâ€¢f×WB6VÆbæ7Åö–çWEöFWf–6W2À¢f×WB6VÆbæ7Åö–çWEöFWf–6W5öÆöF–ærÀ¢f×WB6VÆbæ7Åö–çWEöFWf–6W5öW'&÷"À¢f×WB6VÆbæVF–õ÷6WGF–æw5öfVVF&6²À¢&–çWB"À¢&W7VÇBÀ¢“°¢Ð ¢fâ÷Vå÷FV×õöÖ‚f×WB6VÆbÂF#¢F–ÖTÖF"’ÓâF6³ÄÖW76vSâ°¢6VÆbçF–ÖUöÖ÷F"ÒF#°¢–b6VÆbæ—5öÖö&–ÆUöÖ–å÷v–æF÷r‚’°¢6VÆbç&Vg&W6…÷FV×õöÖöVF—G2‚“°¢6VÆbç&Vg&W6…öÖWFW%öÖöVF—G2‚“°¢6VÆbç6†÷uöÖö&–ÆU÷æVÂ„Öö&–ÆUæVÃ£¥F–ÖTÖ“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢–bÆWB6öÖR‡v–æF÷uö–B’Ò6VÆbçFV×õöÖ÷v–æF÷uö–B°¢&WGW&â–6VC£§v–æF÷s£¦v–åöfö7W2‡v–æF÷uö–B“°¢Ð¢6VÆbç&Vg&W6…÷FV×õöÖöVF—G2‚“°¢6VÆbç&Vg&W6…öÖWFW%öÖöVF—G2‚“°¢ÆWB‡v–æF÷uö–BÂF6²’Ò–6VC£§v–æF÷s£¦÷Vâ†–6VC£§v–æF÷s£¥6WGF–æw2°¢6—¦S¢–6VC£¥6—¦S£¦æWrƒccãÂC#ã’À¢Ö–å÷6—¦S¢6öÖR†–6VC£¥6—¦S£¦æWrƒScãÂ3#ã’’À¢âæ–6VC£§v–æF÷s£¥6WGF–æw3£¦FVfVÇB‚¢Ò“°¢6VÆbçFV×õöÖ÷v–æF÷uö–BÒ6öÖR‡v–æF÷uö–B“°¢F6²æF—66&B‚¢Ð ¢fâ&Vg&W6…÷FV×õöÖöVF—G2‚f×WB6VÆb’°¢6VÆbçFV×õöÖöVF—G2Ò6VÆ`¢ç&ö¦V7@¢çFV×õ÷ö–çG2‚¢æÖ‡Â‡F–6²Â'ÒÂ7W'fR—ÂFV×ôÖVF—B°¢÷&–v–æÅ÷F–6³¢6öÖR‡F–6²’À¢F–6³¢F–6²çFõ÷7G&–ær‚’À¢'Ó¢'ÒçFõ÷7G&–ær‚’À¢7W'fRÀ¢Ò¢æ6öÆÆV7B‚“°¢Ð ¢fâ&Vg&W6…öÖWFW%öÖöVF—G2‚f×WB6VÆb’°¢6VÆbæÖWFW%öÖöVF—G2Ò6VÆ`¢ç&ö¦V7@¢çF–ÖU÷6–væGW&U÷ö–çG2‚¢æÖ‡Â‡F–6²Â6–væGW&R—ÂÖWFW$ÖVF—B°¢÷&–v–æÅ÷F–6³¢6öÖR‡F–6²’À¢F–6³¢F–6²çFõ÷7G&–ær‚’À¢çVÖW&F÷#¢6–væGW&RæçVÖW&F÷"‚’çFõ÷7G&–ær‚’À¢FVæöÖ–æF÷#¢6–væGW&RæFVæöÖ–æF÷"‚’çFõ÷7G&–ær‚’À¢Ò¢æ6öÆÆV7B‚“°¢Ð ¢fâFE÷FV×õ÷ö–çEöVF—B‚f×WB6VÆb’°¢ÆWBF–6²Ò6VÆbçF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6³°¢–b6VÆ`¢çFV×õöÖöVF—G0¢æ—FW"‚¢æç’‡Çö–çGÂö–çBçF–6²ç'6S££ÇScCâ‚’æö²‚’ÓÒ6öÖR‡F–6²’¢°¢6VÆbçFV×õöÖöfVVF&6²Ò$FV×òö–çBÇ&VG’W†—7G2BF†RVF—B7W'6÷""çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB'ÒÒ6VÆbç&ö¦V7BçFV×õöE÷F–6²‡F–6²“°¢ÆWBö–çBÒFV×ôÖVF—B°¢÷&–v–æÅ÷F–6³¢æöæRÀ¢F–6³¢F–6²çFõ÷7G&–ær‚’À¢'Ó¢'ÒçFõ÷7G&–ær‚’À¢7W'fS¢FV×ô7W'fS£¥7FWÀ¢Ó°¢ÆWB–æFW‚Ò6VÆ`¢çFV×õöÖöVF—G0¢ç'F—F–öå÷ö–çB‡ÆW†—7F–æwÂW†—7F–ærçF–6²ç'6S££ÇScCâ‚’çVçw&ö÷"‡ScC£¤Ô‚’ÂF–6²“°¢6VÆbçFV×õöÖöVF—G2æ–ç6W'B†–æFW‚Âö–çB“°¢6VÆbçFV×õöÖöfVVF&6²æ6ÆV"‚“°¢Ð ¢fâÇ•÷FV×õöÖöVF—G2‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢ÆWB×WBFW6—&VBÒfV3£§v—F…ö66—G’‡6VÆbçFV×õöÖöVF—G2æÆVâ‚’“°¢f÷"ö–çB–âg6VÆbçFV×õöÖöVF—G2°¢ÆWB„ö²‡F–6²’Âö²†'Ò’’Ò€¢ö–çBçF–6²çG&–Ò‚’ç'6S££ÇScCâ‚’À¢ö–çBæ'ÒçG&–Ò‚’ç'6S££ÆccCâ‚’À¢’VÇ6R°¢6VÆbçFV×õöÖöfVVF&6²Ò$VçFW"v†öÆR&ö¦V7BF–6²æBçVÖW&–2%Ò"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ó°¢–b'Òæ—5öf–æ—FR‚’ÇÂ'ÒÃÒã°¢6VÆbçFV×õöÖöfVVF&6²Ò$%Ò×W7B&Rf–æ—FRæBw&VFW"F†â¦W&ò"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢FW6—&VBçW6‚‚‡F–6²Â'ÒÂö–çBæ7W'fRÂö–çBæ÷&–v–æÅ÷F–6²’“°¢Ð¢FW6—&VBç6÷'Eö'•ö¶W’‡Çö–çGÂö–çBã“°¢–bFW6—&VBæf—'7B‚’æÖ‡Çö–çGÂö–çBã’Ò6öÖRƒ’°¢6VÆbçFV×õöÖöfVVF&6²Ò%F†R–æ—F–ÂFV×òö–çB×W7B7F’BF–6²¦W&ò"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢–bFW6—&VBçv–æF÷w2ƒ"’æç’‡Ç—'Â—%³ÒãÓÒ—%³Òã’°¢6VÆbçFV×õöÖöfVVF&6²Ò%FV×òö–çB÷6—F–öç2×W7B&RVæ—VR"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð ¢ÆWB7W'&VçBÒ6VÆbç&ö¦V7BçFV×õ÷ö–çG2‚’æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBFW6—&VE÷F–6·2ÒFW6—&VBæ—FW"‚’æÖ‡Çö–çGÂö–çBã’æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢ÆWB×WB7F–öç2Ò7W'&Vç@¢æ—FW"‚¢æf–ÇFW"‡Â‡F–6²ÂòÂò—Â§F–6²ÒbbFW6—&VE÷F–6·2æ6öçF–ç2‡F–6²’¢æÖ‡Â‡7F'E÷F–6²ÂòÂò—ÂFt7F–öã£¤FVÆWFUFV×õö–çB°¢7F'E÷F–6³¢§7F'E÷F–6²À¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢f÷"‡F–6²Â'ÒÂ7W'fRÂò’–âfFW6—&VB°¢–b7W'&Vç@¢æ—FW"‚¢æf–æB‡Â†7W'&VçE÷F–6²ÂòÂò—Â7W'&VçE÷F–6²ÓÒF–6²¢æ—5öæöæUö÷"‡Â…òÂ7W'&VçEö'ÒÂò—Â7W'&VçEö'ÒÒ'Ò¢°¢7F–öç2çW6‚„Ft7F–öã£¥6WEFV×ò°¢7F'E÷F–6³¢§F–6²À¢'Ó¢¦'ÒÀ¢Ò“°¢Ð¢ÆWBf–æÅ÷ö–çBÒFW6—&VBæÆ7B‚’æ—5÷6öÖUöæB‡ÆÆ7GÂÆ7BãÓÒ§F–6²“°¢ÆWBFW6—&VEö7W'fRÒ–bf–æÅ÷ö–çB°¢FV×ô7W'fS£¥7FW ¢ÒVÇ6R°¢¦7W'fP¢Ó°¢–b7W'&Vç@¢æ—FW"‚¢æf–æB‡Â†7W'&VçE÷F–6²ÂòÂò—Â7W'&VçE÷F–6²ÓÒF–6²¢æ—5öæöæUö÷"‡Â…òÂòÂ7W'&VçEö7W'fR—Â7W'&VçEö7W'fRÒfFW6—&VEö7W'fR¢°¢7F–öç2çW6‚„Ft7F–öã£¥6WEFV×ô7W'fR°¢7F'E÷F–6³¢§F–6²À¢7W'fS¢FW6—&VEö7W'fRÀ¢Ò“°¢Ð¢Ð¢–b7F–öç2æ—5öV×G’‚’°¢6VÆbçFV×õöÖöfVVF&6²Ò%FV×òÖ—2Væ6†ævVB"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢ÒÀ¢%FV×òÖVF—FVB"À¢“°¢–b6VÆbç&Wf—6–öâÒ&Wf–÷W5÷&Wf—6–öâ°¢6VÆbç&Vg&W6…÷FV×õöÖöVF—G2‚“°¢6VÆbçFV×õöÖöfVVF&6²Ò%FV×òÖÆ–VB2öæRVæFö&ÆRVF—B"çFõö÷væVB‚“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–b6VÆbçÆ–&6²æ—5÷6öÖR‚’°¢&WGW&â6VÆbç&W&U÷Æ–&6²‡6VÆbçÆ–†VE÷6×ÆRÂ6VÆbçÆ–&6µ÷Æ––ær“°¢Ð¢ÒVÇ6R°¢6VÆbçFV×õöÖöfVVF&6²Ò6VÆbç7FGW2æ6ÆöæR‚“°¢Ð¢F6³£¦æöæR‚¢Ð ¢fâFEöÖWFW%÷ö–çEöVF—B‚f×WB6VÆb’°¢ÆWBF–6²Ò6VÆbçF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6³°¢–b6VÆ`¢æÖWFW%öÖöVF—G0¢æ—FW"‚¢æç’‡Çö–çGÂö–çBçF–6²ç'6S££ÇScCâ‚’æö²‚’ÓÒ6öÖR‡F–6²’¢°¢6VÆbæÖWFW%öÖöfVVF&6²Ð¢$F–ÖR×6–væGW&Rö–çBÇ&VG’W†—7G2BF†RVF—B7W'6÷""çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB6–væGW&RÒ6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²‡F–6²“°¢ÆWBö–çBÒÖWFW$ÖVF—B°¢÷&–v–æÅ÷F–6³¢æöæRÀ¢F–6³¢F–6²çFõ÷7G&–ær‚’À¢çVÖW&F÷#¢6–væGW&RæçVÖW&F÷"‚’çFõ÷7G&–ær‚’À¢FVæöÖ–æF÷#¢6–væGW&RæFVæöÖ–æF÷"‚’çFõ÷7G&–ær‚’À¢Ó°¢ÆWB–æFW‚Ò6VÆ`¢æÖWFW%öÖöVF—G0¢ç'F—F–öå÷ö–çB‡ÆW†—7F–æwÂW†—7F–ærçF–6²ç'6S££ÇScCâ‚’çVçw&ö÷"‡ScC£¤Ô‚’ÂF–6²“°¢6VÆbæÖWFW%öÖöVF—G2æ–ç6W'B†–æFW‚Âö–çB“°¢6VÆbæÖWFW%öÖöfVVF&6²æ6ÆV"‚“°¢Ð ¢fâÇ•öÖWFW%öÖöVF—G2‚f×WB6VÆb’°¢ÆWB×WBFW6—&VBÒfV3£§v—F…ö66—G’‡6VÆbæÖWFW%öÖöVF—G2æÆVâ‚’“°¢f÷"ö–çB–âg6VÆbæÖWFW%öÖöVF—G2°¢ÆWB„ö²‡7F'E÷F–6²’Âö²†çVÖW&F÷"’Âö²†FVæöÖ–æF÷"’’Ò€¢ö–çBçF–6²çG&–Ò‚’ç'6S££ÇScCâ‚’À¢ö–çBæçVÖW&F÷"çG&–Ò‚’ç'6S££ÇS3#â‚’À¢ö–çBæFVæöÖ–æF÷"çG&–Ò‚’ç'6S££ÇS3#â‚’À¢’VÇ6R°¢6VÆbæÖWFW%öÖöfVVF&6²Ð¢$VçFW"v†öÆR&ö¦V7BF–6²ÂçVÖW&F÷"ÂæBFVæöÖ–æF÷""çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWB6–væGW&RÒÖF6‚F–ÖU6–væGW&S£¦æWr†çVÖW&F÷"ÂFVæöÖ–æF÷"’°¢ö²‡6–væGW&R’Óâ6–væGW&RÀ¢W'"†W'&÷"’Óâ°¢6VÆbæÖWFW%öÖöfVVF&6²ÒW'&÷"çFõ÷7G&–ær‚“°¢&WGW&ã°¢Ð¢Ó°¢FW6—&VBçW6‚„ÖWFW%ö–çE6æ6†÷B°¢7F'E÷F–6²À¢çVÖW&F÷#¢6–væGW&RæçVÖW&F÷"‚’À¢FVæöÖ–æF÷#¢6–væGW&RæFVæöÖ–æF÷"‚’À¢Ò“°¢Ð¢FW6—&VBç6÷'Eö'•ö¶W’‡Çö–çGÂö–çBç7F'E÷F–6²“°¢–bFW6—&VBæf—'7B‚’æÖ‡Çö–çGÂö–çBç7F'E÷F–6²’Ò6öÖRƒ’°¢6VÆbæÖWFW%öÖöfVVF&6²Ð¢%F†R–æ—F–ÂF–ÖR×6–væGW&Rö–çB×W7B7F’BF–6²¦W&ò"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢–bFW6—&V@¢çv–æF÷w2ƒ"¢æç’‡Ç—'Â—%³Òç7F'E÷F–6²ÓÒ—%³Òç7F'E÷F–6²¢°¢6VÆbæÖWFW%öÖöfVVF&6²Ò%F–ÖR×6–væGW&Rö–çB÷6—F–öç2×W7B&RVæ—VR"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢–bFW6—&VBÓÒ6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöÖ‚’°¢6VÆbæÖWFW%öÖöfVVF&6²Ò%F–ÖR×6–væGW&RÖ—2Væ6†ævVB"çFõö÷væVB‚“°¢&WGW&ã°¢Ð ¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¥6WEF–ÖU6–væGW&TÖ²ö–çG3¢FW6—&VBÒÀ¢%F–ÖR×6–væGW&RÖVF—FVB"À¢“°¢–b6VÆbç&Wf—6–öâÒ&Wf–÷W5÷&Wf—6–öâ°¢6VÆbç&Vg&W6…öÖWFW%öÖöVF—G2‚“°¢6VÆbæÖWFW%öÖöfVVF&6²Ò%F–ÖR×6–væGW&RÖÆ–VB2öæRVæFö&ÆRVF—B"çFõö÷væVB‚“°¢ÒVÇ6R°¢6VÆbæÖWFW%öÖöfVVF&6²Ò6VÆbç7FGW2æ6ÆöæR‚“°¢Ð¢Ð ¢fâ÷VåöÖ–F•öVF—F÷"‚f×WB6VÆbÂ—FVÕö–C¢—FVÔ–B’ÓâF6³ÄÖW76vSâ°¢–b6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æç’‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢6VÆbç7FGW2Ò%F†R6VÆV7FVBÔ”D’—FVÒæòÆöævW"W†—7G2"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢–b6fr‡F&vWEö÷2Ò&æG&ö–B"’°¢ÆWB6öÖR‡v–æF÷uö–B’Ò6VÆbæÖ–å÷v–æF÷uö–BVÇ6R°¢6VÆbç7FGW2Ò%F†RæG&ö–BVF—F÷"v–æF÷r—2æ÷B&VG’"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ó°¢ÆWB—FVÕö6†ævVBÒ6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BÒ6öÖR†—FVÕö–B“°¢–b—FVÕö6†ævVB°¢6VÆbç&VÆV6UöÖ–F•÷&Wf–Wr‚“°¢6VÆbæÖ–F•öVF—F÷%ö÷&–v–å÷F–6²Ò°¢6VÆbæÖ–F•öVF—F÷%öVF—Eö7W'6÷%÷F–6²ÒæöæS°¢Ð¢6VÆbæÖ–F•öVF—F÷%÷v–æF÷uö–BÒ6öÖR‡v–æF÷uö–B“°¢6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BÒ6öÖR†—FVÕö–B“°¢6VÆbæÖ–F•öVF—F÷%÷v–æF÷u÷6—¦RÒ6VÆ`¢æÖ–å÷v–æF÷u÷6—¦P¢çVçw&ö÷"†–6VC£¥6—¦S£¦æWrƒC#ãÂcCã’“°¢6VÆbæÖ–F•öVF—F÷%öfVVF&6²ÒæöæS°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eö7W'6÷"ÒæöæS°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöæ6†÷"ÒæöæS°¢–b—FVÕö6†ævVB°¢6VÆbæf—EöÖ–F•öVF—F÷%÷Fõö—FVÒ†—FVÕö–B“°¢Ð¢6VÆbç6†÷uöÖö&–ÆU÷æVÂ„Öö&–ÆUæVÃ£¤Ö–F”VF—F÷"“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢–bÆWB6öÖR‡v–æF÷uö–B’Ò6VÆbæÖ–F•öVF—F÷%÷v–æF÷uö–B°¢ÆWB—FVÕö6†ævVBÒ6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BÒ6öÖR†—FVÕö–B“°¢–b—FVÕö6†ævVB°¢6VÆbç&VÆV6UöÖ–F•÷&Wf–Wr‚“°¢6VÆbæÖ–F•öVF—F÷%ö÷&–v–å÷F–6²Ò°¢6VÆbæÖ–F•öVF—F÷%öVF—Eö7W'6÷%÷F–6²ÒæöæS°¢Ð¢6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BÒ6öÖR†—FVÕö–B“°¢–b—FVÕö6†ævVB°¢6VÆbæf—EöÖ–F•öVF—F÷%÷Fõö—FVÒ†—FVÕö–B“°¢Ð¢6VÆbæÖ–F•öVF—F÷%öfVVF&6²ÒæöæS°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eö7W'6÷"ÒæöæS°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöæ6†÷"ÒæöæS°¢&WGW&â–6VC£§v–æF÷s£¦v–åöfö7W2‡v–æF÷uö–B“°¢Ð¢ÆWB‡v–æF÷uö–BÂF6²’Ò–6VC£§v–æF÷s£¦÷Vâ†–6VC£§v–æF÷s£¥6WGF–æw2°¢6—¦S¢–6VC£¥6—¦S£¦æWrƒãÂc#ã’À¢Ö–å÷6—¦S¢6öÖR†–6VC£¥6—¦S£¦æWrƒs#ãÂC#ã’’À¢âæ–6VC£§v–æF÷s£¥6WGF–æw3£¦FVfVÇB‚¢Ò“°¢6VÆbæÖ–F•öVF—F÷%÷v–æF÷uö–BÒ6öÖR‡v–æF÷uö–B“°¢6VÆbæÖ–F•öVF—F÷%÷v–æF÷u÷6—¦RÒ–6VC£¥6—¦S£¦æWrƒãÂc#ã“°¢6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BÒ6öÖR†—FVÕö–B“°¢6VÆbæÖ–F•öVF—F÷%öfVVF&6²ÒæöæS°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eö7W'6÷"ÒæöæS°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöæ6†÷"ÒæöæS°¢6VÆbæÖ–F•öVF—F÷%ö÷&–v–å÷F–6²Ò°¢6VÆbæÖ–F•öVF—F÷%öVF—Eö7W'6÷%÷F–6²ÒæöæS°¢6VÆbæÖ–F•öVF—F÷%ö†–v…÷—F6‚ÒƒC°¢6VÆbæÖ–F•öVF—F÷%÷—F6…÷&÷w2Ò3c°¢6VÆbæÖ–F•öVF—F÷%÷—F6…÷&÷uö†V–v‡BÒ‚ã°¢6VÆbæÖ–F•öVF—F÷%÷—†VÇ5÷W%ö&VBÒ“bã°¢6VÆbæf—EöÖ–F•öVF—F÷%÷Fõö—FVÒ†—FVÕö–B“°¢F6²æF—66&B‚¢Ð ¢fâf—EöÖ–F•öVF—F÷%÷Fõö—FVÒ‚f×WB6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBÒScC£¦g&öÒ‡6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’“°¢ÆWB6÷W&6U÷7F'E÷F–6²Ò—FVÒç6÷W&6Uööfg6WE÷F–6·2‚“°¢ÆWB6÷W&6UöVæE÷F–6²Ò6÷W&6U÷7F'E÷F–6²ç6GW&F–æuöFB†—FVÒæÆVæwF…÷F–6·2‚’“°¢ÆWBf—6–&ÆUöæ÷FW2Ò—FVÐ¢ææ÷FW2‚¢æ—FW"‚¢æf–ÇFW"‡Ææ÷FWÂ‡6÷W&6U÷7F'E÷F–6²âç6÷W&6UöVæE÷F–6²’æ6öçF–ç2‚fæ÷FRçF–6²‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBf—'7E÷F–6²Òf—6–&ÆUöæ÷FW0¢æ—FW"‚¢æÖ‡Ææ÷FWÂæ÷FRçF–6²‚’ç6GW&F–æu÷7V"‡6÷W&6U÷7F'E÷F–6²’¢æÖ–â‚¢çVçw&ö÷"ƒ“°¢ÆWBÆ7E÷F–6²Òf—6–&ÆUöæ÷FW0¢æ—FW"‚¢æÖ‡Ææ÷FWÂ°¢æ÷FRçF–6²‚¢ç6GW&F–æuöFB†æ÷FRæGW&F–öâ‚’¢æÖ–â‡6÷W&6UöVæE÷F–6²¢ç6GW&F–æu÷7V"‡6÷W&6U÷7F'E÷F–6²¢Ò¢æÖ‚‚¢çVçw&ö÷"‡ç6GW&F–æuö×VÂƒB’“°¢ÆWBÖ&v–âÒòC°¢ÆWB÷&–v–å÷F–6²Òf—'7E÷F–6²ç6GW&F–æu÷7V"†Ö&v–â“°¢ÆWBf—6–&ÆU÷F–6·2ÒÆ7E÷F–6°¢ç6GW&F–æuöFB†Ö&v–â¢ç6GW&F–æu÷7V"†÷&–v–å÷F–6²¢æÖ‚‡“°¢ÆWB6öçFVçE÷v–GF‚Ð¢‡6VÆbæÖ–F•öVF—F÷%÷v–æF÷u÷6—¦Rçv–GF‚ÒÔ”D•ôTD•Dõ%ô4ôåDTåEõt”ED…ô”å4UB’æÖ‚ƒ#ã“°¢ÆWB&VG2Òf—6–&ÆU÷F–6·22c3"òæÖ‚ƒ’2c3#°¢6VÆbæÖ–F•öVF—F÷%ö÷&–v–å÷F–6²Ò÷&–v–å÷F–6³°¢6VÆbæÖ–F•öVF—F÷%÷—†VÇ5÷W%ö&VBÒ†6öçFVçE÷v–GF‚ò&VG2’æ6Æ×ƒãÂ3ã“°¢ÆWB†–v†W7E÷—F6‚Òf—6–&ÆUöæ÷FW2æ—FW"‚’æÖ‡Ææ÷FWÂæ÷FRç—F6‚‚’’æÖ‚‚“°¢ÆWBÆ÷vW7E÷—F6‚Òf—6–&ÆUöæ÷FW2æ—FW"‚’æÖ‡Ææ÷FWÂæ÷FRç—F6‚‚’’æÖ–â‚“°¢–bÆWB…6öÖR†Æ÷vW7E÷—F6‚’Â6öÖR††–v†W7E÷—F6‚’’Ò†Æ÷vW7E÷—F6‚Â†–v†W7E÷—F6‚’°¢ÆWB†–v…÷—F6‚Ò†–v†W7E÷—F6‚ç6GW&F–æuöFBƒ"’æÖ–âƒ#r“°¢ÆWBÆ÷u÷—F6‚ÒÆ÷vW7E÷—F6‚ç6GW&F–æu÷7V"ƒ"“°¢ÆWB—F6…÷&÷w2Ò†–v…÷—F6€¢ç6GW&F–æu÷7V"†Æ÷u÷—F6‚¢ç6GW&F–æuöFBƒ¢æÖ‚ƒ“°¢ÆWBf–Æ&ÆUö†V–v‡BÒ‡6VÆbæÖ–F•öVF—F÷%÷v–æF÷u÷6—¦Ræ†V–v‡BÒ#cã’æÖ‚ƒƒã“°¢6VÆbæÖ–F•öVF—F÷%ö†–v…÷—F6‚Ò†–v…÷—F6ƒ°¢6VÆbæÖ–F•öVF—F÷%÷—F6…÷&÷w2Ò—F6…÷&÷w3°¢6VÆbæÖ–F•öVF—F÷%÷—F6…÷&÷uö†V–v‡BÐ¢†f–Æ&ÆUö†V–v‡Bòc3#£¦g&öÒ‡—F6…÷&÷w2’’æ6Æ×ƒ"ãÂ‚ã“°¢ÒVÇ6R°¢6VÆbæÖ–F•öVF—F÷%ö†–v…÷—F6‚ÒƒC°¢6VÆbæÖ–F•öVF—F÷%÷—F6…÷&÷w2Ò3c°¢6VÆbæÖ–F•öVF—F÷%÷—F6…÷&÷uö†V–v‡BÒ‚ã°¢Ð¢Ð ¢fâÖ–F•öVF—F÷%÷Æ–†VE÷F–6²‚g6VÆbÂ—FVÓ¢fFuö6÷&S£¤Ö–F”—FVÒ’Óâ÷F–öãÇScCâ°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢°¢6VÆbç&ö¦V7@¢çF–6µöE÷6×ÆR‡6VÆbçÆ–†VE÷6×ÆR¢æö²‚¢ææE÷F†Vâ‡ÇF–6·ÂF–6²æ6†V6¶VE÷7V"†—FVÒç7F'E÷F–6²‚’’¢Ð¢5¶6fr†æ÷B†fVGW&RÒ&VF–òÖFWf–6R"’•Ð¢°¢ÆWBòÒ—FVÓ°¢æöæP¢Ð¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâföÆÆ÷uöÖ–F•öVF—F÷%÷Æ–†VB‚f×WB6VÆb’°¢–b6VÆbæÖ–F•öVF—F÷%öföÆÆ÷u÷Æ–†VBÇÂ6VÆbçÆ–&6µ÷Æ––ær°¢&WGW&ã°¢Ð¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6öÖR‡Æ–†VE÷F–6²’Ò6VÆbæÖ–F•öVF—F÷%÷Æ–†VE÷F–6²†—FVÒ’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBf—6–&ÆU÷—†VÇ2Ð¢‡6VÆbæÖ–F•öVF—F÷%÷v–æF÷u÷6—¦Rçv–GF‚ÒÔ”D•ôTD•Dõ%ô4ôåDTåEõt”ED…ô”å4UB’æÖ‚ƒ#ã“°¢ÆWBf—6–&ÆU÷F–6·2Ò‡f—6–&ÆU÷—†VÇ2ò6VÆbæÖ–F•öVF—F÷%÷—†VÇ5÷W%ö&V@¢¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’2c3"’2ScC°¢6VÆbæÖ–F•öVF—F÷%ö÷&–v–å÷F–6²Ð¢Æ–†VE÷F–6²ç6GW&F–æu÷7V"‡f—6–&ÆU÷F–6·2ç6GW&F–æuö×VÂƒ2’òB“°¢Ð ¢fâ6ÆV%÷6†÷'F7WEö&–æF–ær‚f×WB6VÆbÂ7F–öåö–C¢7G&–ær’°¢ÆWB×WB6æF–FFRÒ6VÆbç6†÷'F7WEö&–æF–æuöVF—G2æ6ÆöæR‚“°¢6æF–FFRæ–ç6W'B†7F–öåö–Bæ6ÆöæR‚’Â7G&–æs£¦æWr‚’“°¢ÖF6‚6öÖÖæG3£§fÆ–FFUö&–æF–æw5÷v—F…öÖ7&÷2‚f6æF–FFRÂg6VÆbæ7F–öåöÖ7&÷2’°¢ö²†&–æF–æw2’Óâ°¢6VÆbç6†÷'F7WEö&–æF–æuöVF—G2Ò&–æF–æw3°¢6VÆbç6†÷'F7WEöFVfVÇG5÷&W7F÷&VBç&VÖ÷fR‚f7F–öåö–B“°¢6VÆbç6†÷'F7WEö6GW&Uö–BÒæöæS°¢6VÆbç6†÷'F7WEöVF—F÷%öfVVF&6²Ð¢%6†÷'F7WB6ÆV&VBâ6fRFòÇ’F†—26†ævRâ"çFõö÷væVB‚“°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç6†÷'F7WEöVF—F÷%öfVVF&6²Òf÷&ÖB‚%6†÷'F7WB6÷VÆBæ÷B&R6ÆV&VC¢¶W'&÷'Ò"“°¢Ð¢Ð¢Ð ¢fâ&W7F÷&U÷6†÷'F7WEöFVfVÇB‚f×WB6VÆbÂ7F–öåö–C¢7G&–ær’°¢ÆWB×WB6æF–FFRÒ6VÆbç6†÷'F7WEö&–æF–æuöVF—G2æ6ÆöæR‚“°¢6æF–FFRç&VÖ÷fR‚f7F–öåö–B“°¢ÖF6‚6öÖÖæG3£§fÆ–FFUö&–æF–æw5÷v—F…öÖ7&÷2‚f6æF–FFRÂg6VÆbæ7F–öåöÖ7&÷2’°¢ö²†&–æF–æw2’Óâ°¢6VÆbç6†÷'F7WEö&–æF–æuöVF—G2Ò&–æF–æw3°¢6VÆbç6†÷'F7WEöFVfVÇG5÷&W7F÷&VBæ–ç6W'B†7F–öåö–Bæ6ÆöæR‚’“°¢6VÆbç6†÷'F7WEö6GW&Uö–BÒæöæS°¢6VÆbç6†÷'F7WEöVF—F÷%öfVVF&6²Òf÷&ÖB€¢'·Ò&W7F÷&VBFò—G2FVfVÇBâ6fRFòÇ’F†—26†ævRâ"À¢6öÖÖæG3£¦Æ&VÅöf÷%ö–B‡6VÆbÂf7F–öåö–B’çVçw&ö÷"†7F–öåö–B¢“°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç6†÷'F7WEöVF—F÷%öfVVF&6²Òf÷&ÖB€¢$FVfVÇB6†÷'F7WB6÷VÆBæ÷B&R&W7F÷&VC¢·Ò"À¢6öÖÖæG3£¦g&–VæFÇ•÷6†÷'F7WEöW'&÷"‚fW'&÷"Âg6VÆbæ7F–öåöÖ7&÷2¢“°¢Ð¢Ð¢Ð ¢fâ6GW&U÷6†÷'F7WEö¶W’€¢f×WB6VÆbÀ¢7F–öåö–C¢7G&–ærÀ¢¶W“¢g7G"À¢ÖöF–f–W'3¢–6VC£¦¶W–&ö&C£¤ÖöF–f–W'2À¢’°¢ÆWB&–æF–ærÒÖF6‚6öÖÖæG3£¦6GW&Uö&–æF–ær†¶W’ÂÖöF–f–W'2’°¢ö²†&–æF–ær’Óâ&–æF–ærÀ¢W'"†W'&÷"’Óâ°¢6VÆbç6†÷'F7WEöVF—F÷%öfVVF&6²ÒW'&÷#°¢&WGW&ã°¢Ð¢Ó°¢ÆWB×WB6æF–FFRÒ6VÆbç6†÷'F7WEö&–æF–æuöVF—G2æ6ÆöæR‚“°¢ÆWB&–æF–ærÒ–b6VÆbç6†÷'F7WEö6GW&UöVæB°¢ÆWB7W'&VçBÒ6öÖÖæG3£§7FvVEö&–æF–æuöf÷%ö–B‡6VÆbÂf7F–öåö–B“°¢–b7W'&VçBæ—5öV×G’‚’°¢&–æF–æp¢ÒVÇ6R°¢f÷&ÖB‚'¶7W'&VçGÓ²¶&–æF–æwÒ"¢Ð¢ÒVÇ6R°¢&–æF–æp¢Ó°¢6æF–FFRæ–ç6W'B†7F–öåö–BÂ&–æF–æræ6ÆöæR‚’“°¢ÖF6‚6öÖÖæG3£§fÆ–FFUö&–æF–æw5÷v—F…öÖ7&÷2‚f6æF–FFRÂg6VÆbæ7F–öåöÖ7&÷2’°¢ö²†&–æF–æw2’Óâ°¢–bÆWB6öÖR†7F–öåö–B’Ò6VÆbç6†÷'F7WEö6GW&Uö–BçF¶R‚’°¢6VÆbç6†÷'F7WEöFVfVÇG5÷&W7F÷&VBç&VÖ÷fR‚f7F–öåö–B“°¢Ð¢6VÆbç6†÷'F7WEö&–æF–æuöVF—G2Ò&–æF–æw3°¢6VÆbç6†÷'F7WEöVF—F÷%öfVVF&6²Òf÷&ÖB€¢%&V6÷&FVB·Òâ6fRFòÇ’F†—26†ævRâ"À¢6öÖÖæG3£¦f÷&ÖE÷6†÷'F7WEöÆ&VÂ‚f&–æF–ær¢“°¢Ð¢W'"†W'&÷"’Óâ°¢ÆWBW'&÷"Ò6öÖÖæG3£¦g&–VæFÇ•÷6†÷'F7WEöW'&÷"‚fW'&÷"Âg6VÆbæ7F–öåöÖ7&÷2“°¢6VÆbç6†÷'F7WEöVF—F÷%öfVVF&6²Òf÷&ÖB€¢'·Ó²&W72æ÷F†W"¶W’÷"W66R"À¢6öÖÖæG3£¦f÷&ÖE÷6†÷'F7WEöÆ&VÂ‚fW'&÷"¢“°¢Ð¢Ð¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâ6VÆV7FVE÷Æ–&6µö&6¶VæB‚g6VÆb’ÓâÆ–&6´&6¶VæB°¢5¶6fr†ç’€¢ÆÂ†fVGW&RÒ&¦6²Ö&6¶VæB"ÂfVGW&RÒ'—Wv—&RÖ&6¶VæB"’À¢ÆÂ€¢fVGW&RÒ&¦6²Ö&6¶VæB"À¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’À¢ÆÂ€¢fVGW&RÒ'—Wv—&RÖ&6¶VæB"À¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢¢’•Ð¢°¢6VÆbçÆ–&6µö&6¶Væ@¢Ð¢5¶6fr†ÆÂ€¢fVGW&RÒ&¦6²Ö&6¶VæB"À¢æ÷B†fVGW&RÒ'—Wv—&RÖ&6¶VæB"’À¢æ÷B†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’¢’•Ð¢°¢Æ–&6´&6¶VæC£¤¦6°¢Ð¢5¶6fr†ÆÂ€¢fVGW&RÒ'—Wv—&RÖ&6¶VæB"À¢æ÷B†fVGW&RÒ&¦6²Ö&6¶VæB"’À¢æ÷B†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’¢’•Ð¢°¢Æ–&6´&6¶VæC£¥—Uv—&P¢Ð¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"’À¢æ÷B†fVGW&RÒ&¦6²Ö&6¶VæB"’À¢æ÷B†fVGW&RÒ'—Wv—&RÖ&6¶VæB"¢’•Ð¢°¢Æ–&6´&6¶VæC£¤7À¢Ð¢5¶6fr†ÆÂ€¢æ÷B†fVGW&RÒ&¦6²Ö&6¶VæB"’À¢æ÷B†fVGW&RÒ'—Wv—&RÖ&6¶VæB"’À¢æ÷B†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’¢’•Ð¢°¢Æ–&6´&6¶VæC£¥Væf–Æ&ÆP¢Ð¢Ð ¢5¶6fr†ç’€¢ÆÂ†fVGW&RÒ&¦6²Ö&6¶VæB"ÂfVGW&RÒ'—Wv—&RÖ&6¶VæB"’À¢ÆÂ€¢fVGW&RÒ&¦6²Ö&6¶VæB"À¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’À¢ÆÂ€¢fVGW&RÒ'—Wv—&RÖ&6¶VæB"À¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢¢’•Ð¢fâÆ–&6µö&6¶VæE÷6WGF–ær†&6¶VæC¢Æ–&6´&6¶VæB’ÓâVF–õö6öæf–s£¥Æ–&6´&6¶VæE6WGF–ær°¢ÖF6‚&6¶VæB°¢5¶6fr†fVGW&RÒ&¦6²Ö&6¶VæB"•Ð¢Æ–&6´&6¶VæC£¤¦6²ÓâVF–õö6öæf–s£¥Æ–&6´&6¶VæE6WGF–æs£¤¦6²À¢5¶6fr†fVGW&RÒ'—Wv—&RÖ&6¶VæB"•Ð¢Æ–&6´&6¶VæC£¥—Uv—&RÓâVF–õö6öæf–s£¥Æ–&6´&6¶VæE6WGF–æs£¥—Uv—&RÀ¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¢Æ–&6´&6¶VæC£¤7ÂÓâVF–õö6öæf–s£¥Æ–&6´&6¶VæE6WGF–æs£¤7ÂÀ¢Ð¢Ð ¢5¶6fr†ç’€¢ÆÂ†fVGW&RÒ&¦6²Ö&6¶VæB"ÂfVGW&RÒ'—Wv—&RÖ&6¶VæB"’À¢ÆÂ€¢fVGW&RÒ&¦6²Ö&6¶VæB"À¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’À¢ÆÂ€¢fVGW&RÒ'—Wv—&RÖ&6¶VæB"À¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢¢’•Ð¢fâ&W7F÷&U÷Æ–&6µö&6¶VæB‚f×WB6VÆbÂ6WGF–æs¢÷F–öãÆVF–õö6öæf–s£¥Æ–&6´&6¶VæE6WGF–æsâ’°¢ÆWB6öÖR‡6WGF–ær’Ò6WGF–ærVÇ6R°¢&WGW&ã°¢Ó°¢6VÆbçÆ–&6µö&6¶VæBÒÖF6‚6WGF–ær°¢5¶6fr†fVGW&RÒ&¦6²Ö&6¶VæB"•Ð¢VF–õö6öæf–s£¥Æ–&6´&6¶VæE6WGF–æs£¤¦6²ÓâÆ–&6´&6¶VæC£¤¦6²À¢5¶6fr†fVGW&RÒ'—Wv—&RÖ&6¶VæB"•Ð¢VF–õö6öæf–s£¥Æ–&6´&6¶VæE6WGF–æs£¥—Uv—&RÓâÆ–&6´&6¶VæC£¥—Uv—&RÀ¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¢VF–õö6öæf–s£¥Æ–&6´&6¶VæE6WGF–æs£¤7ÂÓâÆ–&6´&6¶VæC£¤7ÂÀ¢òÓâ6VÆbçÆ–&6µö&6¶VæBÀ¢Ó°¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâÆ–&6µöæÖR‚g6VÆb’Óâbw7FF–27G"°¢6VÆbç6VÆV7FVE÷Æ–&6µö&6¶VæB‚’ææÖR‚¢Ð ¢fâ&VÆV6UöÖ–F•÷&Wf–Wr‚g6VÆb’°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6²°¢Æ–&6²ç&VÆV6UöÖ–F•÷&Wf–Wr‚“°¢Ð¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâFövvÆU÷&WVB‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢–b6VÆbçÆ–&6µö'W7’ÇÂ6VÆbæ–õö'W7’°¢6VÆbç7FGW2Ò%v—Bf÷"7W'&VçB÷W&F–öâFòf–æ—6‚&Vf÷&R6†æv–ær&WVB"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ÆWB&Wf–÷W2Ò6VÆbç&WVEöVæ&ÆVC°¢6VÆbç&WVEöVæ&ÆVBÒ&Wf–÷W3°¢–b6VÆbçÆ–&6²æ—5÷6öÖR‚’°¢6VÆbç&WVE÷&V6öæf–wW&F–öå÷&Wf–÷W2Ò6öÖR‚‡&Wf–÷W2ÂfÇ6R’“°¢ÆWBF6²Ò6VÆbç&W&U÷Æ–&6²‡6VÆbçÆ–†VE÷6×ÆRÂ6VÆbçÆ–&6µ÷Æ––ær“°¢–b6VÆbçÆ–&6µö'W7’°¢6VÆbç&W7F÷&U÷&WVEögFW%öf–ÆVE÷&V6öæf–wW&F–öâ‚'Æ–&6²6÷VÆBæ÷B&R&W&VB"“°¢Ð¢F6°¢ÒVÇ6R°¢6VÆbç7FGW2Ò–b6VÆbç&WVEöVæ&ÆVB°¢%&WVBVæ&ÆVB"çFõö÷væVB‚¢ÒVÇ6R°¢%&WVBF—6&ÆVB"çFõö÷væVB‚¢Ó°¢F6³£¦æöæR‚¢Ð¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâ&W7F÷&U÷&WVEögFW%öf–ÆVE÷&V6öæf–wW&F–öâ‚f×WB6VÆbÂ&V6öã¢g7G"’°¢ÆWB6öÖR‚‡&Wf–÷W5öVæ&ÆVBÂÆö÷÷ö–çG5ö6†ævVB’’Ð¢6VÆbç&WVE÷&V6öæf–wW&F–öå÷&Wf–÷W2çF¶R‚¢VÇ6R°¢&WGW&ã°¢Ó°¢–b6VÆbçÆ–&6µ÷Æ––ær°¢6VÆbçW6U÷Æ–&6²‚“°¢Ð¢6VÆbç&WVEöVæ&ÆVBÒ&Wf–÷W5öVæ&ÆVC°¢6VÆbçÆ–&6µöw&…öF—'G’ÒÆö÷÷ö–çG5ö6†ævVC°¢6VÆbç7FGW2Òf÷&ÖB‚%&WVB6†ævRv2æ÷BÆ–VC²Æ–&6²W6VC¢·&V6öçÒ"“°¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâ7F'E÷Æ–&6²‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢6VÆbç&VÆV6UöÖ–F•÷&Wf–Wr‚“°¢6VÆbç&V6÷&F–æuö6æ6VÆÆVE÷G&ç7÷'E÷7F'BÒfÇ6S°¢–b6VÆbçÆ–&6²æ—5÷6öÖR‚’bb6VÆbçÆ–&6µ÷Æ––ærbb6VÆbçÆ–&6µ÷W6VB°¢6VÆbçÆ–&6µ÷7F'E÷6×ÆRÒ6VÆbçÆ–†VE÷6×ÆS°¢Ð¢–b6VÆbçÆ–&6²æ—5÷6öÖR‚’bb6VÆbçÆ–&6µöw&…öF—'G’°¢6VÆbæ6Æ÷ÇVv–å÷v&æ–æw2æ6ÆV"‚“°¢–bÆWBW'"†W'&÷"’Ò6VÆbçW'6—7Eö6Æ÷ÇVv–å÷7FFW2‚’°¢6VÆbæ6Æ÷ÇVv–å÷v&æ–æw2çW6‚†f÷&ÖB€¢%6öÖRÇVv–â7FFR6÷VÆBæ÷B&R6GW&VC²&Wf–÷W6Ç’6fVB7FFRv2¶WB‡¶W'&÷'Ò’ ¢’“°¢Ð¢Ð¢–b6VÆbçÆ–&6²æ—5÷6öÖR‚’bb‡6VÆbçÆ–&6µöw&…öF—'G’ÇÂ6VÆbçÆ–&6µ÷÷6—F–öåöF—'G’’°¢&WGW&â6VÆbç&W&U÷Æ–&6²‡6VÆbçÆ–†VE÷6×ÆRÂG'VR“°¢Ð¢–bÆWB6öÖR‡Æ–&6²’Ò6VÆbçÆ–&6²æ5ö×WB‚’°¢&WGW&âÖF6‚Æ–&6²çÆ’‚’°¢ö²‚‚’’Óâ°¢5¶6fr‡F&vWEö÷2Ò&æG&ö–B"•Ð¢–bÆWBW'"†W'&÷"’Ò7&FS£¦æG&ö–E÷ÆFf÷&Ó£§7F'E÷Æ–&6µ÷6W'f–6R‚’°¢ÆWBòÒÆ–&6²ç7F÷‚“°¢6VÆbç7FGW2Òf÷&ÖB€¢$6÷VÆBæ÷B¶VWæG&ö–BÆ–&6²7F—fR–â&6¶w&÷VæC¢¶W'&÷'Ò ¢“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢6VÆbçÆ–&6µ÷Æ––ærÒG'VS°¢6VÆbçÆ–&6µ÷W6VBÒfÇ6S°¢6VÆbç7FGW2Ò%Æ–&6²7F'FVB"çFõö÷væVB‚“°¢–b6VÆbæ6Æ÷ÇVv–å÷v&æ–æw2æ—5öV×G’‚’°¢6VÆbç7FGW2çW6…÷7G"‚#²"“°¢6VÆbç7FGW2çW6…÷7G"‚g6VÆbæ6Æ÷ÇVv–å÷v&æ–æw2æ¦ö–â‚#²"’“°¢Ð¢F6³£¦æöæR‚¢Ð¢W'"†W'&÷"’Óâ°¢5¶6fr‡F&vWEö÷2Ò&æG&ö–B"•Ð¢7&FS£¦æG&ö–E÷ÆFf÷&Ó£§7F÷÷Æ–&6µ÷6W'f–6R‚“°¢G&6–æs£¦W'&÷"†&6¶VæBÒ6VÆbçÆ–&6µöæÖR‚’ÂW'&÷"ÒVW'&÷"Â'Æ–&6²7F'Bf–ÆVB"“°¢6VÆbç7FGW2Òf÷&ÖB‚'·ÒÆ’f–ÆVC¢¶W'&÷'Ò"Â6VÆbçÆ–&6µöæÖR‚’“°¢F6³£¦æöæR‚¢Ð¢Ó°¢Ð¢6VÆbç&W&U÷Æ–&6²‡6VÆbçÆ–†VE÷6×ÆRÂG'VR¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâW6U÷Æ–&6²‚f×WB6VÆb’°¢6VÆbç&VÆV6UöÖ–F•÷&Wf–Wr‚“°¢ÆWB6öÖR‡Æ–&6²’Ò6VÆbçÆ–&6²æ5ö×WB‚’VÇ6R°¢6VÆbç7FGW2Òf÷&ÖB‚'·Ò÷WGWB—2æ÷B÷Vâ"Â6VÆbçÆ–&6µöæÖR‚’“°¢&WGW&ã°¢Ó°¢ÖF6‚Æ–&6²ç7F÷‚’°¢ö²‚‚’’Óâ°¢6VÆbçÆ–&6µ÷Æ––ærÒfÇ6S°¢6VÆbçÆ–&6µ÷W6VBÒG'VS°¢6VÆbçÆ–†VE÷6×ÆRÒÆ–&6²ç7FG2‚’çÆ–†VE÷6×ÆS°¢5¶6fr‡F&vWEö÷2Ò&æG&ö–B"•Ð¢7&FS£¦æG&ö–E÷ÆFf÷&Ó£§7F÷÷Æ–&6µ÷6W'f–6R‚“°¢6VÆbç7FGW2Ò%Æ–&6²W6VB"çFõö÷væVB‚“°¢Ð¢W'"†W'&÷"’Óâ°¢G&6–æs£¦W'&÷"†&6¶VæBÒ6VÆbçÆ–&6µöæÖR‚’ÂW'&÷"ÒVW'&÷"Â'Æ–&6²7F÷f–ÆVB"“°¢6VÆbç7FGW2Òf÷&ÖB‚'·Ò7F÷f–ÆVC¢¶W'&÷'Ò"Â6VÆbçÆ–&6µöæÖR‚’“°¢Ð¢Ð¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâæ–5öÖ–F’‚f×WB6VÆb’°¢ÆWB6öÖR‡Æ–&6²’Ò6VÆbçÆ–&6²æ5ö×WB‚’VÇ6R°¢6VÆbç7FGW2Òf÷&ÖB‚'·Ò÷WGWB—2æ÷B÷Vâ"Â6VÆbçÆ–&6µöæÖR‚’“°¢&WGW&ã°¢Ó°¢ÖF6‚Æ–&6²çæ–5öÖ–F’‚’°¢ö²‚‚’’Óâ°¢6VÆbç7FGW2Ò$Ô”D’æ–26VçC¢42&W6WG2FòÔ”D’Ö6&ÆR÷'G2Â†VÆBæ÷FW2&VÆV6VBöâÆÂ–ç7G'VÖVçG3²Æ–†VBVæ6†ævVB"çFõö÷væVB‚¢Ð¢W'"†W'&÷"’Óâ°¢G&6–æs£¦W'&÷"†&6¶VæBÒ6VÆbçÆ–&6µöæÖR‚’ÂW'&÷"ÒVW'&÷"Â$Ô”D’æ–2f–ÆVB"“°¢6VÆbç7FGW2Òf÷&ÖB‚'·ÒÔ”D’æ–2f–ÆVC¢¶W'&÷'Ò"Â6VÆbçÆ–&6µöæÖR‚’¢Ð¢Ð¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâ&W7F'E÷Æ–&6²‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢ÆWB6å÷&W&RÒ6VÆbçÆ–&6µö'W7’bb6VÆbæ–õö'W7’bb6VÆbæÖVF–÷7F÷&U÷F‚‚’æ—5÷6öÖR‚“°¢ÆWBF6²Ò6VÆbç&W&U÷Æ–&6²ƒÂG'VR“°¢–b6å÷&W&R°¢6VÆbçÆ–&6µ÷7F'E÷6×ÆRÒ°¢6VÆbçÆ–&6µ÷W6VBÒfÇ6S°¢Ð¢F6°¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâ7F÷÷G&ç7÷'E÷Fõ÷7F'B‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢–b6VÆbçÆ–&6²æ—5öæöæR‚’°¢6VÆbç7FGW2Òf÷&ÖB‚'·Ò÷WGWB—2æ÷B÷Vâ"Â6VÆbçÆ–&6µöæÖR‚’“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ÆWBv5÷Æ––ærÒ6VÆbçÆ–&6µ÷Æ––æs°¢–bv5÷Æ––ær°¢6VÆbçW6U÷Æ–&6²‚“°¢Ð¢6VÆbçÆ–&6µ÷W6VBÒfÇ6S°¢ÆWB7F'E÷6×ÆRÒ6VÆbçÆ–&6µ÷7F'E÷6×ÆS°¢–bv5÷Æ––ærÇÂ6VÆbçÆ–†VE÷6×ÆRÒ7F'E÷6×ÆR°¢6VÆbç&W&U÷Æ–&6²‡7F'E÷6×ÆRÂfÇ6R¢ÒVÇ6R°¢6VÆbç7FGW2Ò%Æ–&6²7F÷VB"çFõö÷væVB‚“°¢F6³£¦æöæR‚¢Ð¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâ6VVµ÷Fõ÷6×ÆR‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢ÖF6‚6VÆbç6VVµ÷6×ÆU÷VW'’çG&–Ò‚’ç'6S££ÇScCâ‚’°¢ö²‡F&vWE÷6×ÆR’Óâ°¢–b6VÆbçÆ–&6µ÷Æ––ær°¢6VÆbçÆ–&6µ÷7F'E÷6×ÆRÒF&vWE÷6×ÆS°¢Ð¢6VÆbç&W&U÷Æ–&6²‡F&vWE÷6×ÆRÂ6VÆbçÆ–&6µ÷Æ––ær¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚$–çfÆ–B6VV²6×ÆS¢¶W'&÷'Ò"“°¢F6³£¦æöæR‚¢Ð¢Ð¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâ6Æ÷6U÷Æ–&6²‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢5¶6fr†ÆÂ†fVGW&RÒ&VF–òÖFWf–6R"ÂF&vWEö÷2Ò&æG&ö–B"’•Ð¢°¢7&FS£¦æG&ö–E÷ÆFf÷&Ó£§6WEöÖ–F•ö–çWE÷6VæFW"„æöæR“°¢7&FS£¦æG&ö–E÷ÆFf÷&Ó£§6WEöÖ–F•ö÷WGWE÷&V6V—fW"„æöæR“°¢Ð¢6VÆbç&VÆV6UöÖ–F•÷&Wf–Wr‚“°¢6VÆbç7FæF'•öÖöæ—F÷%÷G&6²ÒæöæS°¢6VÆbç7FæF'•öÖöæ—F÷%övVæW&F–öâÒ6VÆbç7FæF'•öÖöæ—F÷%övVæW&F–öâçw&–æuöFBƒ“°¢6VÆbç&W6WE÷G&6µöÖWFW'2‚“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢ÆWB7FFUöW'&÷"Ò6VÆbçW'6—7Eö6Æ÷ÇVv–å÷7FFW2‚’æW'"‚“°¢ÆWB×WB6‡WFF÷våöW'&÷"ÒæöæS°¢ÆWB×WB7FæF'•ö–çWBÒæöæS°¢–bÆWB6öÖR†×WBÆ–&6²’Ò6VÆbçÆ–&6²çF¶R‚’°¢Æ–&6²æF—6&ÆUö–çWEöÖöæ—F÷&–ær‚“°¢7FæF'•ö–çWBÒÆ–&6²çF¶U÷7FæF'•ö–çWB‚“°¢ÖF6‚Æ–&6²ç6‡WFF÷vâ‚’°¢ö²‚†–ç7G'VÖVçG2ÂVffV7G2’’Óâ°¢6VÆbæFV7F—fFU÷7F÷VEö–ç7G'VÖVçG2†–ç7G'VÖVçG2“°¢6VÆbæFV7F—fFU÷7F÷VEöVffV7G2†VffV7G2“°¢Ð¢W'"†W'&÷"’Óâ°¢6‡WFF÷våöW'&÷"Ò6öÖR†W'&÷"çFõ÷7G&–ær‚’“°¢ÆWB÷væW%ö–G2Ò6VÆbæ6ÆöVffV7Eö÷væW'2æ¶W—2‚’æ6÷–VB‚’æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–bÆWB6öÖR†6ÆVçWöW'&÷"’Ò6VÆbæF—66&E÷VçW6VEöVffV7Eö÷væW'2‚f÷væW%ö–G2’°¢6‡WFF÷våöW'&÷"Ò6öÖR†ÖF6‚6‡WFF÷våöW'&÷"çF¶R‚’°¢6öÖR†W'&÷"’Óâf÷&ÖB‚'¶W'&÷'Ó²¶6ÆVçWöW'&÷'Ò"’À¢æöæRÓâ6ÆVçWöW'&÷"À¢Ò“°¢Ð¢ÆWB÷væW%ö–G2Ò6VÆ`¢æ6Æö–ç7G'VÖVçEö÷væW'0¢æ¶W—2‚¢æ6÷–VB‚¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–bÆWB6öÖR†6ÆVçWöW'&÷"’Ò6VÆbæF—66&E÷VçW6VEö–ç7G'VÖVçEö÷væW'2‚f÷væW%ö–G2’°¢6‡WFF÷våöW'&÷"Ò6öÖR†ÖF6‚6‡WFF÷våöW'&÷"çF¶R‚’°¢6öÖR†W'&÷"’Óâf÷&ÖB‚'¶W'&÷'Ó²¶6ÆVçWöW'&÷'Ò"’À¢æöæRÓâ6ÆVçWöW'&÷"À¢Ò“°¢Ð¢Ð¢Ð¢Ð¢ÆWB†VÇW%ö–G2Ò6VÆ`¢æ6Æö–ç7G'VÖVçEö†VÇW%ö÷væW'0¢æ¶W—2‚¢æ6÷–VB‚¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–bÆWB6öÖR†6ÆVçWöW'&÷"’Ò6VÆbæF—66&E÷VçW6VEö–ç7G'VÖVçEö÷væW'2‚f†VÇW%ö–G2’°¢6‡WFF÷våöW'&÷"Ò6öÖR†ÖF6‚6‡WFF÷våöW'&÷"çF¶R‚’°¢6öÖR†W'&÷"’Óâf÷&ÖB‚'¶W'&÷'Ó²¶6ÆVçWöW'&÷'Ò"’À¢æöæRÓâ6ÆVçWöW'&÷"À¢Ò“°¢Ð¢6VÆbçÆ–&6µ÷Æ––ærÒfÇ6S°¢6VÆbçÆ–&6µ÷W6VBÒfÇ6S°¢6VÆbçÆ–&6µ÷÷6—F–öåöF—'G’ÒfÇ6S°¢5¶6fr‡F&vWEö÷2Ò&æG&ö–B"•Ð¢7&FS£¦æG&ö–E÷ÆFf÷&Ó£§7F÷÷Æ–&6µ÷6W'f–6R‚“°¢6VÆbçÆ–†VE÷6×ÆRÒ°¢6VÆbçÆ–&6µ÷7F'E÷6×ÆRÒ°¢6VÆbç6VVµ÷6×ÆU÷VW'’Ò#"çFõö÷væVB‚“°¢6VÆbç7FGW2Ò6‡WFF÷våöW'&÷"æÖö÷%öVÇ6R€¢ÇÂf÷&ÖB‚'·Ò÷WGWB6Æ÷6VB"Â6VÆbçÆ–&6µöæÖR‚’’À¢ÆW'&÷'Âf÷&ÖB‚'·Ò6‡WFF÷vâf–ÆVC¢¶W'&÷'Ò"Â6VÆbçÆ–&6µöæÖR‚’’À¢“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR†W'&÷"’Ò7FFUöW'&÷"°¢6VÆbç7FGW0¢çW6…÷7G"‚ff÷&ÖB‚#²ÇVv–â7FFR6fRf–ÆVC¢¶W'&÷'Ò"’“°¢Ð¢7FæF'•ö–çWBæÖö÷%öVÇ6R…F6³£¦æöæRÂÇ7FæF'•ö–çWGÂ°¢6VÆbæ6Æ÷6U÷7FæF'•ö–çWEö7–æ2‡7FæF'•ö–çWB¢Ò¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâ&W&U÷Æ–&6²‚f×WB6VÆbÂF&vWE÷6×ÆS¢ScBÂ7F'E÷v†Vå÷&VG“¢&ööÂ’ÓâF6³ÄÖW76vSâ°¢–b6VÆbçÆ–&6µö'W7’ÇÂ6VÆbæ–õö'W7’°¢6VÆbç7FGW2Ò%v—Bf÷"7W'&VçB÷W&F–öâFòf–æ—6‚"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ÆWB6öÖR‡F‚’Ò6VÆbæÖVF–÷7F÷&U÷F‚‚’VÇ6R°¢6VÆbç7FGW2Ò%FV×÷&'’&ö¦V7BÖVF–7F÷&vR—2Væf–Æ&ÆR"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ó°¢6VÆbæ6Æ÷ÇVv–å÷v&æ–æw2æ6ÆV"‚“°¢–bÆWBW'"†W'&÷"’Ò6VÆbçW'6—7Eö6Æ÷ÇVv–å÷7FFW2‚’°¢6VÆbæ6Æ÷ÇVv–å÷v&æ–æw2çW6‚†f÷&ÖB€¢%6öÖRÇVv–â7FFR6÷VÆBæ÷B&R6GW&VC²&Wf–÷W6Ç’6fVB7FFRv–ÆÂ&RW6VB‡¶W'&÷'Ò’ ¢’“°¢Ð¢ÆWB6æ6†÷BÒ6VÆbç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBÆö÷÷ö–çG5÷F–6·2Ò6VÆ`¢ç&WVEöVæ&ÆV@¢çF†Vå÷6öÖR‡6VÆbçF–ÖVÆ–æRæÆö÷÷ö–çG2¢æfÆGFVâ‚¢æf–ÇFW"‡Ç&ævWÂ&ævRç7F'E÷F–6²Â&ævRæVæE÷F–6²¢æÖ‡Ç&ævWÂ‡&ævRç7F'E÷F–6²Â&ævRæVæE÷F–6²’“°¢6VÆbçÆ–&6µö'W7’ÒG'VS°¢6VÆbç7FGW2Òf÷&ÖB‚%&W&–ærÆ–&6²B6×ÆR·F&vWE÷6×ÆWÞ(
b"“°¢ÆWB&W7VÇBÒ&3£¦æWr„×WFWƒ£¦æWr„æöæR’“°¢ÆWBÖW76vU÷&W7VÇBÒ&3£¦6ÆöæR‚g&W7VÇB“°¢F6³£§W&f÷&Ò€¢'Våö&Æö6¶–ær‚&Fr×Æ–&6²×&W&R"ÂÖ÷fRÇÂ°¢&W&U÷&ö¦V7E÷Æ–&6µöf–ÆU÷v—F…öÆö÷÷ö–çG2€¢F‚À¢6æ6†÷BÀ¢F&vWE÷6×ÆRÀ¢Æö÷÷ö–çG5÷F–6·2À¢¢Ò’À¢Ö÷fRÇ&W&VGÂ°¢¦ÖW76vU÷&W7VÇ@¢æÆö6²‚¢çVçw&ö÷%öVÇ6R‡7FC£§7–æ3£¥ö—6öäW'&÷#£¦–çFõö–ææW"’Ò6öÖR‡&W&VB“°¢ÖW76vS£¥Æ–&6µ&W&VB°¢F&vWE÷6×ÆRÀ¢7F'E÷v†Vå÷&VG’À¢&W7VÇC¢6†&VE&W&VEÆ–&6²‡&W7VÇB’À¢Ð¢ÒÀ¢¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâf–æ—6…÷Æ–&6µ÷&W&F–öâ€¢f×WB6VÆbÀ¢F&vWE÷6×ÆS¢ScBÀ¢7F'E÷v†Vå÷&VG“¢&ööÂÀ¢&W7VÇC¢6†&VE&W&VEÆ–&6²À¢’°¢ÆWB7F'E÷v†Vå÷&VG’Ò7F'E÷v†Vå÷&VG’bb6VÆbç&V6÷&F–æuö6æ6VÆÆVE÷G&ç7÷'E÷7F'C°¢6VÆbç&V6÷&F–æuö6æ6VÆÆVE÷G&ç7÷'E÷7F'BÒfÇ6S°¢6VÆbçÆ–&6µö'W7’ÒfÇ6S°¢ÆWB&W7VÇBÒ&W7VÇBãæÆö6²‚’æö²‚’ææE÷F†Vâ‡Æ×WB&W7VÇGÂ&W7VÇBçF¶R‚’“°¢ÆWB×WB&W&VBÒÖF6‚&W7VÇB°¢6öÖR„ö²‡&W&VB’’Óâ&W&VBÀ¢6öÖR„W'"†W'&÷"’’Óâ°¢6VÆbç&W7F÷&U÷&WVEögFW%öf–ÆVE÷&V6öæf–wW&F–öâ‚fW'&÷"çFõ÷7G&–ær‚’“°¢G&6–æs£¦W'&÷"†W'&÷"ÒVW'&÷"Â'Æ–&6²&W&F–öâf–ÆVB"“°¢6VÆbç7FGW2Òf÷&ÖB‚%Æ–&6²&W&F–öâf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢æöæRÓâ°¢6VÆbç7FGW2Ò%Æ–&6²&W&F–öâ&W7VÇBv2Væf–Æ&ÆR"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢Ó°¢&W&VBç6WEöÖ7FW%ö÷WGWEö6V–Æ–æuöF&g2‡6VÆbæVF–õ÷6WGF–æw2æÖ7FW%ö÷WGWEö6V–Æ–ær“° ¢ÆWB–ç7G'VÖVçEö÷væW%ö–G2ÒÖF6‚6VÆbæ–ç7FÆÅ÷G&6µö–ç7G'VÖVçE÷&ö6W76÷'2‚f×WB&W&VB’°¢ö²†–G2’Óâ–G2À¢W'"†W'&÷"’Óâ°¢6VÆbç&W7F÷&U÷&WVEögFW%öf–ÆVE÷&V6öæf–wW&F–öâ‚fW'&÷"çFõ÷7G&–ær‚’“°¢G&6–æs£¦W'&÷"†W'&÷"ÒVW'&÷"Â'Æ–&6²–ç7G'VÖVçB&W&F–öâf–ÆVB"“°¢6VÆbç7FGW2Òf÷&ÖB‚%Æ–&6²&W&F–öâf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó° ¢ÆWBg…ö÷væW%ö–G2ÒÖF6‚6VÆbæ–ç7FÆÅ÷G&6µög…÷&ö6W76÷'2‚f×WB&W&VB’°¢ö²†–G2’Óâ–G2À¢W'"†W'&÷"’Óâ°¢6VÆbç&W7F÷&U÷&WVEögFW%öf–ÆVE÷&V6öæf–wW&F–öâ‚fW'&÷"çFõ÷7G&–ær‚’“°¢G&6–æs£¦W'&÷"†W'&÷"ÒVW'&÷"Â'Æ–&6²VffV7B&W&F–öâf–ÆVB"“°¢G&÷‡&W&VB“°¢ÆWB6ÆVçWöW'&÷"Ò6VÆbæF—66&E÷VçW6VEö–ç7G'VÖVçEö÷væW'2‚f–ç7G'VÖVçEö÷væW%ö–G2“°¢6VÆbç7FGW2Òf÷&ÖB‚%Æ–&6²&W&F–öâf–ÆVC¢¶W'&÷'Ò"“°¢–bÆWB6öÖR†6ÆVçWöW'&÷"’Ò6ÆVçWöW'&÷"°¢6VÆbç7FGW2çW6…÷7G"‚ff÷&ÖB‚#²¶6ÆVçWöW'&÷'Ò"’“°¢Ð¢&WGW&ã°¢Ð¢Ó° ¢–b6VÆbçÆ–&6²æ—5÷6öÖR‚’°¢6VÆbç&VÆV6UöÖ–F•÷&Wf–Wr‚“°¢ÆWB‡&W7VÇBÂÆ•÷&W7VÇBÂ&WF—&VEö–ç7G'VÖVçG2Â&WF—&VEöVffV7G2’Ò°¢ÆWBÆ–&6²Ò6VÆbçÆ–&6²æ5ö×WB‚’æW‡V7B‚'Æ–&6²W†—7G2"“°¢ÆWB&W7VÇBÒÆ–&6²ç&WÆ6Uöw&‚‡&W&VB“°¢ÆWBÆ•÷&W7VÇBÒ–b&W7VÇBæ—5öö²‚’bb7F'E÷v†Vå÷&VG’°¢Æ–&6²çÆ’‚¢ÒVÇ6R°¢ö²‚‚’¢Ó°¢Æ–&6²æ6öÆÆV7E÷&WF—&VEöw&‡2‚“°¢€¢&W7VÇBÀ¢Æ•÷&W7VÇBÀ¢Æ–&6²çF¶U÷&WF—&VEö–ç7G'VÖVçE÷&ö6W76÷'2‚’À¢Æ–&6²çF¶U÷&WF—&VEög…÷&ö6W76÷'2‚’À¢¢Ó°¢6VÆbæFV7F—fFU÷7F÷VEö–ç7G'VÖVçG2‡&WF—&VEö–ç7G'VÖVçG2“°¢6VÆbæFV7F—fFU÷7F÷VEöVffV7G2‡&WF—&VEöVffV7G2“°¢ÖF6‚&W7VÇB°¢ö²‚‚’’Óâ°¢6VÆbç&WVE÷&V6öæf–wW&F–öå÷&Wf–÷W2ÒæöæS°¢6VÆbçÆ–†VE÷6×ÆRÒF&vWE÷6×ÆS°¢6VÆbç6VVµ÷6×ÆU÷VW'’ÒF&vWE÷6×ÆRçFõ÷7G&–ær‚“°¢6VÆbçÆ–&6µöw&…öF—'G’ÒfÇ6S°¢6VÆbçÆ–&6µ÷÷6—F–öåöF—'G’ÒfÇ6S°¢6VÆbç&W6WE÷G&6µöÖWFW'2‚“°¢–bÆWBW'"†W'&÷"’ÒÆ•÷&W7VÇB°¢6VÆbçÆ–&6µ÷Æ––ærÒfÇ6S°¢5¶6fr‡F&vWEö÷2Ò&æG&ö–B"•Ð¢7&FS£¦æG&ö–E÷ÆFf÷&Ó£§7F÷÷Æ–&6µ÷6W'f–6R‚“°¢6VÆbç7FGW2Òf÷&ÖB‚'·ÒÆ’f–ÆVC¢¶W'&÷'Ò"Â6VÆbçÆ–&6µöæÖR‚’“°¢&WGW&ã°¢Ð¢5¶6fr‡F&vWEö÷2Ò&æG&ö–B"•Ð¢–b7F'E÷v†Vå÷&VG¢bbÆWBW'"†W'&÷"’Ò7&FS£¦æG&ö–E÷ÆFf÷&Ó£§7F'E÷Æ–&6µ÷6W'f–6R‚¢°¢–bÆWB6öÖR‡Æ–&6²’Ò6VÆbçÆ–&6²æ5ö×WB‚’°¢ÆWBòÒÆ–&6²ç7F÷‚“°¢Ð¢6VÆbçÆ–&6µ÷Æ––ærÒfÇ6S°¢6VÆbç7FGW2Òf÷&ÖB€¢$6÷VÆBæ÷B¶VWæG&ö–BÆ–&6²7F—fR–â&6¶w&÷VæC¢¶W'&÷'Ò ¢“°¢&WGW&ã°¢Ð¢5¶6fr‡F&vWEö÷2Ò&æG&ö–B"•Ð¢–b7F'E÷v†Vå÷&VG’°¢7&FS£¦æG&ö–E÷ÆFf÷&Ó£§7F÷÷Æ–&6µ÷6W'f–6R‚“°¢Ð¢6VÆbçÆ–&6µ÷Æ––ærÒ7F'E÷v†Vå÷&VG“°¢6VÆbçÆ–&6µ÷W6VBÒ7F'E÷v†Vå÷&VG’bb6VÆbçÆ–&6µ÷W6VC°¢6VÆbç7FGW2Òf÷&ÖB‚%VWVVB6VV²Fò6×ÆR·F&vWE÷6×ÆWÒ"“°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç&W7F÷&U÷&WVEögFW%öf–ÆVE÷&V6öæf–wW&F–öâ‚fW'&÷"çFõ÷7G&–ær‚’“°¢ÆWB×WB6ÆVçWöW'&÷"Ò6VÆbæF—66&E÷VçW6VEöVffV7Eö÷væW'2‚fg…ö÷væW%ö–G2“°¢–bÆWB6öÖR†–ç7G'VÖVçEöW'&÷"’Ð¢6VÆbæF—66&E÷VçW6VEö–ç7G'VÖVçEö÷væW'2‚f–ç7G'VÖVçEö÷væW%ö–G2¢°¢6ÆVçWöW'&÷"Ò6öÖR†ÖF6‚6ÆVçWöW'&÷"°¢6öÖR†W'&÷"’Óâf÷&ÖB‚'¶W'&÷'Ó²¶–ç7G'VÖVçEöW'&÷'Ò"’À¢æöæRÓâ–ç7G'VÖVçEöW'&÷"À¢Ò“°¢Ð¢6VÆbç7FGW2Ð¢f÷&ÖB‚'·Òw&‚&WÆ6VÖVçBf–ÆVC¢¶W'&÷'Ò"Â6VÆbçÆ–&6µöæÖR‚’“°¢–bÆWB6öÖR†6ÆVçWöW'&÷"’Ò6ÆVçWöW'&÷"°¢6VÆbç7FGW2çW6…÷7G"‚ff÷&ÖB‚#²¶6ÆVçWöW'&÷'Ò"’“°¢Ð¢Ð¢Ð¢&WGW&ã°¢Ð ¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¢ÆWB÷WGWE÷&W7VÇBÒ&W&VBæ–çFõö÷WGWB€¢6VÆbç6VÆV7FVE÷Æ–&6µö&6¶VæB‚’À¢6VÆbæVF–õ÷6WGF–æw2æ7Åö÷WGWEöFWf–6Uö–Bæ5öFW&Vb‚’À¢“°¢5¶6fr†æ÷B†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’’•Ð¢ÆWB÷WGWE÷&W7VÇBÒ&W&VBæ–çFõö÷WGWB‡6VÆbç6VÆV7FVE÷Æ–&6µö&6¶VæB‚’ÂæöæR“°¢ÆWB×WBÆ–&6²ÒÖF6‚÷WGWE÷&W7VÇB°¢ö²‡Æ–&6²’ÓâÆ–&6²À¢W'"†W'&÷"’Óâ°¢G&6–æs£¦W'&÷"†&6¶VæBÒ6VÆbçÆ–&6µöæÖR‚’ÂW'&÷"ÒVW'&÷"Â&VF–ò÷WGWB6WGWf–ÆVB"“°¢ÆWB×WB6ÆVçWöW'&÷"Ò6VÆbæF—66&E÷VçW6VEöVffV7Eö÷væW'2‚fg…ö÷væW%ö–G2“°¢–bÆWB6öÖR†–ç7G'VÖVçEöW'&÷"’Ð¢6VÆbæF—66&E÷VçW6VEö–ç7G'VÖVçEö÷væW'2‚f–ç7G'VÖVçEö÷væW%ö–G2¢°¢6ÆVçWöW'&÷"Ò6öÖR†ÖF6‚6ÆVçWöW'&÷"°¢6öÖR†W'&÷"’Óâf÷&ÖB‚'¶W'&÷'Ó²¶–ç7G'VÖVçEöW'&÷'Ò"’À¢æöæRÓâ–ç7G'VÖVçEöW'&÷"À¢Ò“°¢Ð¢6VÆbç7FGW2Òf÷&ÖB‚'·Ò÷WGWB6WGWf–ÆVC¢¶W'&÷'Ò"Â6VÆbçÆ–&6µöæÖR‚’“°¢–bÆWB6öÖR†6ÆVçWöW'&÷"’Ò6ÆVçWöW'&÷"°¢6VÆbç7FGW2çW6…÷7G"‚ff÷&ÖB‚#²¶6ÆVçWöW'&÷'Ò"’“°¢Ð¢&WGW&ã°¢Ð¢Ó°¢ÆWBÆ•öW'&÷"Ò–b7F'E÷v†Vå÷&VG’°¢Æ–&6²çÆ’‚’æW'"‚¢ÒVÇ6R°¢æöæP¢Ó°¢5¶6fr‡F&vWEö÷2Ò&æG&ö–B"•Ð¢ÆWB6W'f–6UöW'&÷"Ò–b7F'E÷v†Vå÷&VG’bbÆ•öW'&÷"æ—5öæöæR‚’°¢7&FS£¦æG&ö–E÷ÆFf÷&Ó£§7F'E÷Æ–&6µ÷6W'f–6R‚’æW'"‚¢ÒVÇ6R°¢æöæP¢Ó°¢5¶6fr†æ÷B‡F&vWEö÷2Ò&æG&ö–B"’•Ð¢ÆWB6W'f–6UöW'&÷#¢÷F–öãÅ7G&–æsâÒæöæS°¢–b6W'f–6UöW'&÷"æ—5÷6öÖR‚’°¢ÆWBòÒÆ–&6²ç7F÷‚“°¢Ð¢6VÆbçÆ–&6²Ò6öÖR‡Æ–&6²“°¢5¶6fr†ÆÂ†fVGW&RÒ&VF–òÖFWf–6R"ÂF&vWEö÷2Ò&æG&ö–B"’•Ð¢ÆWBÖ–F•ö–çWE÷F&vWE÷G&6²Ò6VÆbç6VÆV7FVEöÖ–F•ö–çWE÷G&6µö–B‚“°¢5¶6fr†ÆÂ†fVGW&RÒ&VF–òÖFWf–6R"ÂF&vWEö÷2Ò&æG&ö–B"’•Ð¢–bÆWB6öÖR‡Æ–&6²’Ò6VÆbçÆ–&6²æ5ö×WB‚’°¢7&FS£¦æG&ö–E÷ÆFf÷&Ó£§6WEöÖ–F•ö–çWE÷F&vWE÷G&6²†Ö–F•ö–çWE÷F&vWE÷G&6²“°¢7&FS£¦æG&ö–E÷ÆFf÷&Ó£§6WEöÖ–F•ö–çWE÷6VæFW"‡Æ–&6²çF¶UöÖ–F•ö–çWE÷6VæFW"‚’“°¢7&FS£¦æG&ö–E÷ÆFf÷&Ó£§6WEöÖ–F•ö÷WGWE÷&V6V—fW"‡Æ–&6²çF¶UöÖ–F•ö÷WGWE÷&V6V—fW"‚’“°¢Ð¢ÆWB&WF—&VEö†VÇW%ö–G2Ò6VÆ`¢æ6Æö–ç7G'VÖVçEö†VÇW%ö÷væW'0¢æ¶W—2‚¢æ6÷–VB‚¢æf–ÇFW"‡Æ–GÂ–ç7G'VÖVçEö÷væW%ö–G2æ6öçF–ç2†–B’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–bÆWB6öÖR†W'&÷"’Ò6VÆbæF—66&E÷VçW6VEö–ç7G'VÖVçEö÷væW'2‚g&WF—&VEö†VÇW%ö–G2’°¢6VÆbæ6Æ÷ÇVv–å÷v&æ–æw2çW6‚†W'&÷"“°¢Ð¢6VÆbç&W6WE÷G&6µöÖWFW'2‚“°¢6VÆbçÆ–&6µöw&…öF—'G’ÒfÇ6S°¢6VÆbçÆ–&6µ÷÷6—F–öåöF—'G’ÒfÇ6S°¢6VÆbçÆ–&6µ÷Æ––ærÒ7F'E÷v†Vå÷&VG’bbÆ•öW'&÷"æ—5öæöæR‚“°¢6VÆbçÆ–&6µ÷W6VBÒfÇ6S°¢–b7F'E÷v†Vå÷&VG’°¢6VÆbçÆ–&6µ÷7F'E÷6×ÆRÒF&vWE÷6×ÆS°¢Ð¢6VÆbçÆ–†VE÷6×ÆRÒF&vWE÷6×ÆS°¢6VÆbç6VVµ÷6×ÆU÷VW'’ÒF&vWE÷6×ÆRçFõ÷7G&–ær‚“°¢–bÆWB6öÖR†W'&÷"’ÒÆ•öW'&÷"°¢G&6–æs£¦W'&÷"†&6¶VæBÒ6VÆbçÆ–&6µöæÖR‚’ÂW'&÷"ÒVW'&÷"Â'Æ–&6²7F'Bf–ÆVB"“°¢6VÆbç7FGW2Òf÷&ÖB‚'·ÒÆ’f–ÆVC¢¶W'&÷'Ò"Â6VÆbçÆ–&6µöæÖR‚’“°¢&WGW&ã°¢Ð¢–bÆWB6öÖR†W'&÷"’Ò6W'f–6UöW'&÷"°¢6VÆbçÆ–&6µ÷Æ––ærÒfÇ6S°¢6VÆbç7FGW2Òf÷&ÖB‚$6÷VÆBæ÷B¶VWæG&ö–BÆ–&6²7F—fR–â&6¶w&÷VæC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢6VÆbç7FGW2Ò–b7F'E÷v†Vå÷&VG’°¢%Æ–&6²7F'FVB"çFõö÷væVB‚¢ÒVÇ6R°¢f÷&ÖB‚'·Ò÷WGWB&VG’"Â6VÆbçÆ–&6µöæÖR‚’¢Ó°¢–b6VÆbæ6Æ÷ÇVv–å÷v&æ–æw2æ—5öV×G’‚’°¢6VÆbç7FGW2çW6…÷7G"‚#²"“°¢6VÆbç7FGW2çW6…÷7G"‚g6VÆbæ6Æ÷ÇVv–å÷v&æ–æw2æ¦ö–â‚#²"’“°¢Ð¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâWFFU÷Æ–&6µ÷7FG2‚f×WB6VÆb’Óâ&ööÂ°¢ÆWB‡&WF—&VEö–ç7G'VÖVçG2Â&WF—&VEöVffV7G2Â÷WGWEöFWf–6UöÆ÷7B’Ð¢–bÆWB6öÖR‡Æ–&6²’Ò6VÆbçÆ–&6²æ5ö×WB‚’°¢ÆWB7FG2ÒÆ–&6²ç7FG2‚“°¢6VÆbçÆ–†VE÷6×ÆRÒ7FG2çÆ–†VE÷6×ÆS°¢Æ–&6²æ6öÆÆV7E÷&WF—&VEöw&‡2‚“°¢€¢Æ–&6²çF¶U÷&WF—&VEö–ç7G'VÖVçE÷&ö6W76÷'2‚’À¢Æ–&6²çF¶U÷&WF—&VEög…÷&ö6W76÷'2‚’À¢7FG2æ÷WGWEöFWf–6UöÆ÷7BÀ¢¢ÒVÇ6R°¢…fV3£¦æWr‚’ÂfV3£¦æWr‚’ÂfÇ6R¢Ó°¢6VÆbæFV7F—fFU÷7F÷VEö–ç7G'VÖVçG2‡&WF—&VEö–ç7G'VÖVçG2“°¢6VÆbæFV7F—fFU÷7F÷VEöVffV7G2‡&WF—&VEöVffV7G2“°¢ÆWB×WBæWuö†VÇW%öf–ÇW&W2ÒfV3£¦æWr‚“°¢f÷"†–ç7Fæ6Uö–BÂ÷væW"’–â6VÆbæ6Æö–ç7G'VÖVçEö†VÇW%ö÷væW'2æ—FW%ö×WB‚’°¢ÆWBòÒ÷væW"çG'•÷v—B‚“°¢÷væW"æ—5÷7FÆÆVB„GW&F–öã£¦g&öÕ÷6V72ƒR’“°¢ÆWBfVÇFVBÒ÷væW"ç&Vv–öâ‚’æ—5öfVÇFVB‚“°¢ÆWBVæFW''Vç2Ò÷væW"ç&Vv–öâ‚’çVæFW''Våö6÷VçB‚“°¢–b†fVÇFVBÇÂVæFW''Vç2â¢bb6VÆ`¢æ6Æ÷ÇVv–å÷v&æ–æw0¢æ—FW"‚¢æç’‡Çv&æ–æwÂv&æ–æræ6öçF–ç2‚ff÷&ÖB‚&†VÇW"¶–ç7Fæ6Uö–GÒ"’’¢°¢ÆWB‡G&6µö–BÂÇVv–åö–B’Ò6VÆ`¢æ6Æö–ç7G'VÖVçEö†VÇW%÷F&vWG0¢ævWB†–ç7Fæ6Uö–B¢æÖ‡Â‡G&6µö–BÂÇVv–åö–B—Â†f÷&ÖB‚'·G&6µö–C£÷Ò"’ÂÇVv–åö–Bæ5÷7G"‚’’¢çVçw&ö÷%öVÇ6R‡ÇÂ‚'Væ¶æ÷vâG&6²"çFõö÷væVB‚’Â'Væ¶æ÷vâÇVv–â"’“°¢æWuö†VÇW%öf–ÇW&W2çW6‚†–bfVÇFVB°¢f÷&ÖB€¢$—6öÆFVB4Ä–ç7G'VÖVçB·ÇVv–åö–GÒöâ·G&6µö–GÒf–ÆVB††VÇW"¶–ç7Fæ6Uö–GÒÂfVÇB·Ò“²F†BG&6²—26–ÆVçB"À¢÷væW"ç&Vv–öâ‚’æfVÇEö6öFR‚¢¢ÒVÇ6R°¢f÷&ÖB€¢$—6öÆFVB4Ä–ç7G'VÖVçB·ÇVv–åö–GÒöâ·G&6µö–GÒÖ—76VB·VæFW''Vç7ÒVF–ò&Æö6·2††VÇW"¶–ç7Fæ6Uö–GÒ“²ÆFR&Æö6·2&R6–ÆVçB ¢¢Ò“°¢Ð¢ÆWBwV•÷7FGW2Ò÷væW"æwV•÷7FGW2‚“°¢–bÖF6†W2†wV•÷7FGW2Â"Â2¢bb6VÆ`¢æ6Æ÷ÇVv–å÷v&æ–æw0¢æ—FW"‚¢æç’‡Çv&æ–æwÂv&æ–æræ6öçF–ç2‚ff÷&ÖB‚&VF—F÷"††VÇW"¶–ç7Fæ6Uö–GÒ’"’’¢°¢ÆWB‡G&6µö–BÂÇVv–åö–B’Ò6VÆ`¢æ6Æö–ç7G'VÖVçEö†VÇW%÷F&vWG0¢ævWB†–ç7Fæ6Uö–B¢æÖ‡Â‡G&6µö–BÂÇVv–åö–B—Â†f÷&ÖB‚'·G&6µö–C£÷Ò"’ÂÇVv–åö–Bæ5÷7G"‚’’¢çVçw&ö÷%öVÇ6R‡ÇÂ‚'Væ¶æ÷vâG&6²"çFõö÷væVB‚’Â'Væ¶æ÷vâÇVv–â"’“°¢æWuö†VÇW%öf–ÇW&W2çW6‚†–bwV•÷7FGW2ÓÒ"°¢f÷&ÖB‚$4Ä–ç7G'VÖVçB·ÇVv–åö–GÒöâ·G&6µö–GÒFöW2æ÷B7W÷'BæF—fRVF—F÷"††VÇW"¶–ç7Fæ6Uö–GÒ’"¢ÒVÇ6R°¢f÷&ÖB‚$4Ä–ç7G'VÖVçB·ÇVv–åö–GÒöâ·G&6µö–GÒ6÷VÆBæ÷B÷Vâ—G2æF—fRVF—F÷"††VÇW"¶–ç7Fæ6Uö–GÒ’"¢Ò“°¢Ð¢Ð¢–b6VÆbçÆ–&6²æ—5öæöæR‚’°¢ÆWB&WF—&VEö–G2Ò6VÆ`¢æ6Æö–ç7G'VÖVçEö†VÇW%÷F&vWG0¢æ—FW"‚¢æf–ÇFW%öÖ‡Â†–ç7Fæ6Uö–BÂ‡G&6µö–BÂÇVv–åö–B’—Â°¢ÆWB7F–ÆÅö76–væVBÒ6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒ§G&6µö–B¢ææE÷F†Vâ‡ÇG&6·ÂG&6²æ–ç7G'VÖVçB‚’¢æ—5÷6öÖUöæB‡Æ–ç7G'VÖVçGÂ–ç7G'VÖVçBçÇVv–åö–B‚’ÓÒÇVv–åö–B“°¢‚7F–ÆÅö76–væVB’çF†Vå÷6öÖR‚¦–ç7Fæ6Uö–B¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–bÆWB6öÖR†W'&÷"’Ò6VÆbæF—66&E÷VçW6VEö–ç7G'VÖVçEö÷væW'2‚g&WF—&VEö–G2’°¢6VÆbæ6Æ÷ÇVv–å÷v&æ–æw2çW6‚†W'&÷"“°¢Ð¢Ð¢–bæWuö†VÇW%öf–ÇW&W2æ—5öV×G’‚’°¢6VÆbæ6Æ÷ÇVv–å÷v&æ–æw0¢æW‡FVæB†æWuö†VÇW%öf–ÇW&W2æ—FW"‚’æ6ÆöæVB‚’“°¢6VÆbç7FGW2çW6…÷7G"‚#²"“°¢6VÆbç7FGW2çW6…÷7G"‚fæWuö†VÇW%öf–ÇW&W2æ¦ö–â‚#²"’“°¢Ð¢÷WGWEöFWf–6UöÆ÷7@¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâ†æFÆU÷Æ–&6µöFWf–6UöÆ÷7B‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢G&6–æs£¦W'&÷"€¢&6¶VæBÒ6VÆbçÆ–&6µöæÖR‚’À¢'Æ–&6²÷WGWBFWf–6Rv2Æ÷7B ¢“°¢ÆWB&W7VÖU÷6×ÆRÒ6VÆc£§&÷WFUöÆ÷75÷&W7VÖU÷6×ÆR€¢6fr‡F&vWEö÷2Ò&æG&ö–B"’À¢6VÆbçÆ–&6µ÷Æ––ærÀ¢6VÆbçÆ–†VE÷6×ÆRÀ¢“°¢6VÆbçÆ–&6µ÷Æ––ærÒfÇ6S°¢6VÆbçÆ–&6µ÷W6VBÒfÇ6S°¢6VÆbç&W6WE÷G&6µöÖWFW'2‚“°¢ÆWB6ÆVçWÒ6VÆbæ6Æ÷6U÷Æ–&6²‚“° ¢–bÆWB6öÖR‡&W7VÖU÷6×ÆR’Ò&W7VÖU÷6×ÆP¢bb6VÆbç&ö¦V7E÷F‚æ—5÷6öÖR‚¢°¢6VÆbçÆ–†VE÷6×ÆRÒ&W7VÖU÷6×ÆS°¢6VÆbç7FGW2Òf÷&ÖB€¢$æG&ö–BVF–ò&÷WFR6†ævVC²&V÷Væ–ær÷WGWBæB&W7VÖ–ærg&öÒ6×ÆR·&W7VÖU÷6×ÆWÞ(
b ¢“°¢ÆWB&V6÷fW'’Ò6VÆbç&W&U÷Æ–&6²‡&W7VÖU÷6×ÆRÂG'VR“°¢&WGW&âF6³£¦&F6‚…¶6ÆVçWÂ&V6÷fW'•Ò“°¢Ð ¢6VÆbç7FGW2Ò%7—7FVÒVF–ò÷WGWBFWf–6RVæf–Æ&ÆRgFW"&÷WFR6†ævS²Æ–&6²7F÷VBæB7G&VÒ6Æ÷6VBâ&V6öææV7B÷"6VÆV7Bâ÷WGWBÂF†Vâ7F'BÆ–&6²v–â"çFõö÷væVB‚“°¢6ÆVçW ¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâ&÷WFUöÆ÷75÷&W7VÖU÷6×ÆR€¢—5öæG&ö–C¢&ööÂÀ¢Æ–&6µ÷v5÷'Vææ–æs¢&ööÂÀ¢Æ–†VE÷6×ÆS¢ScBÀ¢’Óâ÷F–öãÇScCâ°¢†—5öæG&ö–BbbÆ–&6µ÷v5÷'Vææ–ær’çF†Vå÷6öÖR‡Æ–†VE÷6×ÆR¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâWFFU÷G&6µ÷VµöÆWfVÇ2‚f×WB6VÆb’°¢ÆWBÖWFW%ö7F—fRÐ¢6VÆbçÆ–&6²æ5÷&Vb‚’æ—5÷6öÖUöæB‡ÇÆ–&6·Â°¢6VÆbçÆ–&6µ÷Æ––ærÇÂÆ–&6²æ†5öVæ&ÆVEö–çWEöÖöæ—F÷"‚¢Ò’ÇÂ6VÆbç&V6÷&F–æræ—5÷6öÖR‚“°¢–bÖWFW%ö7F—fR°¢6VÆbç&W6WE÷G&6µöÖWFW'2‚“°¢&WGW&ã°¢Ð ¢ÆWBö'6W'fVEö–çWBÒ6VÆ`¢ç&V6÷&F–æp¢æ5÷&Vb‚¢æÖ‡Ç&V6÷&F–æwÂ&V6÷&F–æræ6öçG&öÂçF¶Uö–çWE÷V²‚’¢æ÷%öVÇ6R‡ÇÂ°¢6VÆbçÆ–&6°¢æ5÷&Vb‚¢ææE÷F†Vâ‡ÇÆ–&6·ÂÆ–&6²çF¶U÷7FæF'•ö–çWE÷V²‚’¢Ò¢çVçw&ö÷"…³ã²%Ò“°¢f÷"†6†ææVÂÂö'6W'fVB’–âö'6W'fVEö–çWBæ–çFõö—FW"‚’æVçVÖW&FR‚’°¢6VÆbæ–çWE÷VµöÆWfVÅ¶6†ææVÅÒÒö'6W'fVBæÖ‚‡6VÆbæ–çWE÷VµöÆWfVÅ¶6†ææVÅÒ¢ã“b“°¢Ð¢6VÆbæ–çWE÷Vµö†öÆBæö'6W'fR†ö'6W'fVEö–çWB“° ¢–bÆWB6öÖR‡Æ–&6²’Ò6VÆbçÆ–&6²æ5÷&Vb‚’°¢ÆWBö'6W'fVBÒÆ–&6²çF¶UöÖ7FW%ö÷WGWE÷V²‚“°¢f÷"†ÆWfVÂÂV²’–â6VÆbæÖ7FW%÷VµöÆWfVÂæ—FW%ö×WB‚’ç¦—†ö'6W'fVB’°¢¦ÆWfVÂÒV²æÖ‚‚¦ÆWfVÂ¢ã“b“°¢Ð¢6VÆbæÖ7FW%÷Vµö†öÆBæö'6W'fR†ö'6W'fVB“°¢–bÆ–&6²çF¶UöÖ7FW%öwV&Eö7F—fR‚’°¢6VÆbæÖ7FW%öwV&E÷F–6·5÷&VÖ–æ–ærÒS°¢ÒVÇ6R°¢6VÆbæÖ7FW%öwV&E÷F–6·5÷&VÖ–æ–ærÐ¢6VÆbæÖ7FW%öwV&E÷F–6·5÷&VÖ–æ–ærç6GW&F–æu÷7V"ƒ“°¢Ð¢Ð ¢f÷"G&6²–â6VÆbç&ö¦V7BçG&6·2‚’°¢ÆWBÆWfVÇ2Ò6VÆbçG&6µ÷VµöÆWfVÇ2æVçG'’‡G&6²æ–B‚’’æ÷%öFVfVÇB‚“°¢ÆWBö'6W'fVBÒ6VÆ`¢çÆ–&6°¢æ5÷&Vb‚¢ææE÷F†Vâ‡ÇÆ–&6·ÂÆ–&6²çF¶U÷G&6µ÷V²‡G&6²æ–B‚’’¢çVçw&ö÷"…³ã²%Ò“°¢f÷"6†ææVÂ–ââã"°¢ÆWfVÇ5¶6†ææVÅÒÒö'6W'fVE¶6†ææVÅÒæÖ‚†ÆWfVÇ5¶6†ææVÅÒ¢ã“b“°¢Ð¢6VÆbçG&6µ÷Vµö†öÆG0¢æVçG'’‡G&6²æ–B‚’¢æ÷%öFVfVÇB‚¢æö'6W'fR†ö'6W'fVB“°¢Ð¢6VÆbçG&6µ÷VµöÆWfVÇ2ç&WF–â‡ÇG&6µö–BÂ÷Â°¢6VÆbç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æç’‡ÇG&6·ÂG&6²æ–B‚’ÓÒ§G&6µö–B¢Ò“°¢6VÆbçG&6µ÷Vµö†öÆG2ç&WF–â‡ÇG&6µö–BÂ÷Â°¢6VÆbç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æç’‡ÇG&6·ÂG&6²æ–B‚’ÓÒ§G&6µö–B¢Ò“°¢Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢fâ&W6WE÷G&6µöÖWFW'2‚f×WB6VÆb’°¢6VÆbçG&6µ÷VµöÆWfVÇ2æ6ÆV"‚“°¢6VÆbæ–çWE÷VµöÆWfVÂÒ³ã²%Ó°¢6VÆbæÖ7FW%÷VµöÆWfVÂÒ³ã²%Ó°¢6VÆbæÖ7FW%öwV&E÷F–6·5÷&VÖ–æ–ærÒ°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR‡Æ–&6²’Ò6VÆbçÆ–&6²æ5÷&Vb‚’°¢Æ–&6²ç&W6WE÷G&6µ÷V·2‚“°¢Æ–&6²ç&W6WEöÖ7FW%ö÷WGWEöÖWFW"‚“°¢Ð¢Ð ¢fâFE÷G&6²‚f×WB6VÆb’°¢ÆWB–æFW‚Ò6VÆbç&ö¦V7BçG&6·2‚’æÆVâ‚“°¢ÆWBv5öV×G’Ò–æFW‚ÓÒ°¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¤7&VFUG&6²°¢–æFW‚À¢æÖS¢f÷&ÖB‚$VF–ò·Ò"Â–æFW‚²’À¢ÒÀ¢%G&6²7&VFVB"À¢“°¢–bv5öV×G¢bb6VÆbç&Wf—6–öâÒ&Wf–÷W5÷&Wf—6–öà¢bbÆWB6öÖR‡G&6µö–B’Ò6VÆbç&ö¦V7BçG&6·2‚’æf—'7B‚’æÖ‡ÇG&6·ÂG&6²æ–B‚’¢°¢6VÆbçF–ÖVÆ–æRç6VÆV7E÷G&6µööæÇ’‡G&6µö–B“°¢Ð¢Ð ¢fâFEöÖ–F•ö—FVÒ‚f×WB6VÆb’°¢ÆWB6öÖR‡G&6µö–B’Ò6VÆbç6VÆV7FVE÷G&6µö–B‚’VÇ6R°¢6VÆbç7FGW2Ò–b6VÆbç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’°¢$VF—Bf–ÆVC¢FBG&6²&Vf÷&R7&VF–ærÔ”D’—FVÒ"çFõö÷væVB‚¢ÒVÇ6R°¢%6VÆV7B&VwVÆ"G&6²&Vf÷&R7&VF–ærÔ”D’—FVÒ"çFõö÷væVB‚¢Ó°¢&WGW&ã°¢Ó°¢ÆWB6öÖR‡G&6²’Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢–bG&6²æ—5ö'W2‚’°¢6VÆbç7FGW2Ò%6VÆV7B&VwVÆ"G&6²&Vf÷&R7&VF–ærÔ”D’—FVÒ"çFõö÷væVB‚“°¢&WGW&ã°¢Ð ¢ÆWB7F'E÷F–6²Ò6VÆbçF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6³°¢ÆWBÆVæwF…÷F–6·2Ò6VÆ`¢çF–ÖVÆ–æP¢çF–ÖU÷6VÆV7F–öà¢æÖö÷"‡ScC£¦g&öÒ‡6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’’¢BÂÇ6VÆV7F–öçÂ°¢6VÆV7F–öâæVæE÷F–6²Ò6VÆV7F–öâç7F'E÷F–6°¢Ò“°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6²À¢ÆVæwF…÷F–6·2À¢ÒÀ¢$Ô”D’—FVÒ7&VFVB"À¢“°¢Ð ¢fâFEö'W5÷G&6²‚f×WB6VÆb’°¢ÆWB–æFW‚Ò6VÆbç&ö¦V7BçG&6·2‚’æÆVâ‚“°¢ÆWB'W5öçVÖ&W"Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–ÇFW"‡ÇG&6·ÂG&6²æ—5ö'W2‚’¢æ6÷VçB‚¢²°¢ÆWBG&6µö–BÒ6VÆbç&ö¦V7BçG&6·2‚’ævWB†–æFW‚’æÖ‡ÇG&6·ÂG&6²æ–B‚’“°¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¤7&VFT'W5G&6²°¢–æFW‚À¢æÖS¢f÷&ÖB‚$'W2¶'W5öçVÖ&W'Ò"’À¢ÒÀ¢$'W2G&6²7&VFVB"À¢“°¢–b6VÆbç&Wf—6–öâÒ&Wf–÷W5÷&Wf—6–öà¢bbÆWB6öÖR‡G&6µö–B’Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æÆ7B‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ÷"‡G&6µö–B¢°¢6VÆbçF–ÖVÆ–æRç6VÆV7E÷G&6µööæÇ’‡G&6µö–B“°¢Ð¢Ð ¢fâÇ•öVF—CÄS¢7FC£¦f×C£¤F—7Æ“â‚f×WB6VÆbÂ7F–öã¢&W7VÇCÄFt7F–öâÂSâÂ7V66W73¢g7G"’°¢ÖF6‚7F–öâ°¢ö²†7F–öâ’Óâ6VÆbæÇ•ö7F–öâ†7F–öâÂ7V66W72’À¢W'"†W'&÷"’Óâ6VÆbç7FGW2Òf÷&ÖB‚$VF—Bf–ÆVC¢¶W'&÷'Ò"’À¢Ð¢Ð ¢fâÇ•ö7F–öâ‚f×WB6VÆbÂ7F–öã¢Ft7F–öâÂ7V66W73¢g7G"’°¢6VÆbæ6æ6VÅö—FVÕöfFW2‚“°¢ÆWBÖ7FW%öÖ—…ö&Vf÷&RÒ6VÆbç&ö¦V7BæÖ7FW%öÖ—‚‚“°¢ÆWB&ö¦V7Eö†—7F÷'•ö7W'6÷%ö&Vf÷&RÒ6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢ÆWB6×ÆU÷&FUö&Vf÷&RÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚“°¢ÆWBV&Æ—6…ö—FVÕöÖ—‚Ò7F–öå÷WFFW5ö—FVÕöÖ—‚‚f7F–öâ“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢ÆWB&V'V–ÆE÷Æ–&6µöw&‚Ò7F–öå÷&V'V–ÆG5÷Æ–&6µöw&‚‚f7F–öâ“°¢ÆWBÆ—fUöÖ—…÷G&6²ÒÖF6‚f7F–öâ°¢Ft7F–öã£¥6WEG&6µföÇVÖR²G&6µö–BÂââÐ¢ÂFt7F–öã£¥6WEG&6µ†6R²G&6µö–BÂââÐ¢ÂFt7F–öã£¥6WEG&6µâ²G&6µö–BÂââÒÓâ6öÖR‚§G&6µö–B’À¢òÓâæöæRÀ¢Ó°¢ÆWBÆ—fUö×WFU÷6öÆõ÷G&6²ÒÖF6‚f7F–öâ°¢Ft7F–öã£¥6WEG&6´×WFR²G&6µö–BÂââÒÂFt7F–öã£¥6WEG&6µ6öÆò²G&6µö–BÂââÒÓâ°¢6öÖR‚§G&6µö–B¢Ð¢òÓâæöæRÀ¢Ó°¢6VÆbç7FGW2ÒÖF6‚6VÆbç&ö¦V7BæÇ’†7F–öâ’°¢ö²‚‚’’Óâ°¢–b6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’â&ö¦V7Eö†—7F÷'•ö7W'6÷%ö&Vf÷&R°¢6VÆbæ÷VæFõö†—7F÷'’çW6‚„†—7F÷'”VçG'“£¥&ö¦V7B“°¢6VÆbæ÷&VFõö†—7F÷'’æ6ÆV"‚“°¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–b6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’Ò6×ÆU÷&FUö&Vf÷&R°¢ÆWB6×ÆU÷&FUögFW"Ò6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚“°¢6VÆbçÆ–†VE÷6×ÆRÒ&W66ÆU÷G&ç7÷'E÷6×ÆR€¢6VÆbçÆ–†VE÷6×ÆRÀ¢6×ÆU÷&FUö&Vf÷&RÀ¢6×ÆU÷&FUögFW"À¢“°¢6VÆbçÆ–&6µ÷7F'E÷6×ÆRÒ&W66ÆU÷G&ç7÷'E÷6×ÆR€¢6VÆbçÆ–&6µ÷7F'E÷6×ÆRÀ¢6×ÆU÷&FUö&Vf÷&RÀ¢6×ÆU÷&FUögFW"À¢“°¢Ð¢6VÆbæÖ–F•öæ÷FUö6Æ—&ö&BæÆ7E÷7FRÒæöæS°¢6VÆbç&Wf—6–öâÒ6VÆbç&Wf—6–öâçw&–æuöFBƒ“°¢6VÆbçF–ÖVÆ–æRç&V'V–ÆB‚g6VÆbç&ö¦V7B“°¢6VÆbç&Vg&W6…ö—FVÕ÷&÷W'F–W5÷F¶U÷6VÆV7F–öâ‚“°¢ÆWBÆ—fUöÖ–F•ö—FVÕö–G2Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â—FVÒæ–B‚’¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢6VÆbæÖ–F•ö—FVÕöæÖUöVF—G0¢ç&WF–â‡Æ—FVÕö–BÂ÷ÂÆ—fUöÖ–F•ö—FVÕö–G2æ6öçF–ç2†—FVÕö–B’“°¢6VÆbæÖ–F•ö—FVÕöæÖUöW'&÷'0¢ç&WF–â‡Æ—FVÕö–BÂ÷ÂÆ—fUöÖ–F•ö—FVÕö–G2æ6öçF–ç2†—FVÕö–B’“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–b&V'V–ÆE÷Æ–&6µöw&‚°¢6VÆbçÆ–&6µöw&…öF—'G’ÒG'VS°¢Ð¢–bÆWB6öÖR‡G&6µö–B’ÒÆ—fUöÖ—…÷G&6²°¢6VÆbç7–æ5÷G&6µöÖ—…÷Fõ÷Æ–&6²‡G&6µö–B“°¢Ð¢–b6VÆbç&ö¦V7BæÖ7FW%öÖ—‚‚’ÒÖ7FW%öÖ—…ö&Vf÷&R°¢6VÆbç7–æ5öÖ7FW%öÖ—…÷Fõ÷Æ–&6²‚“°¢Ð¢–bV&Æ—6…ö—FVÕöÖ—‚°¢f÷"—FVÒ–â6VÆbç&ö¦V7BæVF–õö—FV×2‚’°¢6VÆbç7–æ5ö—FVÕöfFW5÷Fõ÷Æ–&6²†—FVÒæ–B‚’“°¢6VÆbç7–æ5ö—FVÕ÷föÇVÖU÷Fõ÷Æ–&6²†—FVÒæ–B‚’“°¢6VÆbç7–æ5ö—FVÕö×WFU÷Fõ÷Æ–&6²†—FVÒæ–B‚’“°¢6VÆbç7–æ5ö—FVÕ÷F¶UöÖ—…÷Fõ÷Æ–&6²†—FVÒæ–B‚’“°¢Ð¢Ð¢–bÆWB6öÖR‡G&6µö–B’ÒÆ—fUö×WFU÷6öÆõ÷G&6²°¢6VÆbç7–æ5÷G&6µö×WFU÷6öÆõ÷Fõ÷Æ–&6²‡G&6µö–B“°¢Ð¢7V66W72çFõö÷væVB‚¢Ð¢W'"†W'&÷"’Óâf÷&ÖB‚$7F–öâf–ÆVC¢¶W'&÷'Ò"’À¢Ó°¢Ð ¢fâ7–æ5÷G&6µöÖ—…÷Fõ÷Æ–&6²‚g6VÆbÂG&6µö–C¢G&6´–B’°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR‡G&6²’Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢bbÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6°¢°¢ÆWBòÒÆ–&6²ç6WE÷G&6µöÖ—‚‡G&6µö–BÂG&6²çföÇVÖUöF"‚’ÂG&6²çâ‚’“°¢ÆWBòÒÆ–&6²ç6WE÷G&6µ÷†6R‡G&6µö–BÂG&6²æ—5÷†6Uö–çfW'FVB‚’“°¢Ð¢5¶6fr†æ÷B†fVGW&RÒ&VF–òÖFWf–6R"’•Ð¢ÆWBòÒG&6µö–C°¢Ð ¢fâ7–æ5÷G&6µö×WFU÷6öÆõ÷Fõ÷Æ–&6²‚g6VÆbÂG&6µö–C¢G&6´–B’°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR‡G&6²’Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢bbÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6°¢°¢ÆWBòÒÆ–&6²ç6WE÷G&6µö×WFU÷6öÆò‡G&6µö–BÂG&6²æ—5ö×WFVB‚’ÂG&6²æ—5÷6öÆò‚’“°¢Ð¢5¶6fr†æ÷B†fVGW&RÒ&VF–òÖFWf–6R"’•Ð¢ÆWBòÒG&6µö–C°¢Ð ¢fâ6æ6VÅö—FVÕöfFW2‚f×WB6VÆb’°¢–bÆWB6öÖR‚†—FVÕö–BÂò’’Ò6VÆbçF–ÖVÆ–æRçF¶Uö—FVÕöfFU÷&Wf–Wr‚’°¢6VÆbç7–æ5ö—FVÕöfFW5÷Fõ÷Æ–&6²†—FVÕö–B“°¢Ð¢Ð ¢fâ7–æ5ö—FVÕöfFW5÷Fõ÷Æ–&6²‚g6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢bbÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6°¢°¢ÆWBòÒÆ–&6²ç6WEö—FVÕöfFW2†—FVÕö–BÂ—FVÒæfFW2‚’“°¢Ð¢5¶6fr†æ÷B†fVGW&RÒ&VF–òÖFWf–6R"’•Ð¢ÆWBòÒ—FVÕö–C°¢Ð ¢fâ7–æ5ö—FVÕ÷föÇVÖU÷Fõ÷Æ–&6²‚g6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢bbÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6°¢°¢ÆWBòÒÆ–&6²ç6WEö—FVÕ÷föÇVÖR†—FVÕö–BÂ—FVÒçföÇVÖR‚’“°¢Ð¢5¶6fr†æ÷B†fVGW&RÒ&VF–òÖFWf–6R"’•Ð¢ÆWBòÒ—FVÕö–C°¢Ð ¢fâ7–æ5ö—FVÕ÷F¶UöÖ—…÷Fõ÷Æ–&6²‚g6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢bbÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6°¢°¢ÆWBòÒÆ–&6²ç6WEö—FVÕ÷F¶UöÖ—‚†—FVÕö–BÂ—FVÒçF¶U÷föÇVÖR‚’Â—FVÒçF¶U÷â‚’“°¢Ð¢5¶6fr†æ÷B†fVGW&RÒ&VF–òÖFWf–6R"’•Ð¢ÆWBòÒ—FVÕö–C°¢Ð ¢fâ7–æ5ö—FVÕö×WFU÷Fõ÷Æ–&6²‚g6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢bbÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6°¢°¢ÆWBòÒÆ–&6²ç6WEö—FVÕö×WFVB†—FVÕö–BÂ—FVÒæ—5ö×WFVB‚’“°¢Ð¢5¶6fr†æ÷B†fVGW&RÒ&VF–òÖFWf–6R"’•Ð¢ÆWBòÒ—FVÕö–C°¢Ð ¢fâ7–æ5ö—FVÕ÷†6U÷Fõ÷Æ–&6²‚g6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢bbÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6°¢°¢ÆWBòÒÆ–&6²ç6WEö—FVÕ÷†6R†—FVÕö–BÂ—FVÒæ—5÷†6Uö–çfW'FVB‚’“°¢Ð¢5¶6fr†æ÷B†fVGW&RÒ&VF–òÖFWf–6R"’•Ð¢ÆWBòÒ—FVÕö–C°¢Ð ¢fâ7–æ5öÆÅ÷G&6µöÖ—…÷Fõ÷Æ–&6²‚g6VÆb’°¢6VÆbç7–æ5öÖ7FW%öÖ—…÷Fõ÷Æ–&6²‚“°¢f÷"—FVÒ–â6VÆbç&ö¦V7BæVF–õö—FV×2‚’°¢6VÆbç7–æ5ö—FVÕöfFW5÷Fõ÷Æ–&6²†—FVÒæ–B‚’“°¢6VÆbç7–æ5ö—FVÕ÷föÇVÖU÷Fõ÷Æ–&6²†—FVÒæ–B‚’“°¢6VÆbç7–æ5ö—FVÕö×WFU÷Fõ÷Æ–&6²†—FVÒæ–B‚’“°¢6VÆbç7–æ5ö—FVÕ÷†6U÷Fõ÷Æ–&6²†—FVÒæ–B‚’“°¢6VÆbç7–æ5ö—FVÕ÷F¶UöÖ—…÷Fõ÷Æ–&6²†—FVÒæ–B‚’“°¢Ð¢f÷"G&6²–â6VÆbç&ö¦V7BçG&6·2‚’°¢6VÆbç7–æ5÷G&6µöÖ—…÷Fõ÷Æ–&6²‡G&6²æ–B‚’“°¢Ð¢Ð ¢fâ7–æ5öÖ7FW%öÖ—…÷Fõ÷Æ–&6²‚g6VÆb’°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6²°¢Æ–&6²ç6WEöÖ7FW%öÖ—‚‡6VÆbç&ö¦V7BæÖ7FW%öÖ—‚‚’“°¢Ð¢Ð ¢fâ&Wf–WuöÖ7FW%öÖ—‚‚f×WB6VÆbÂ&ÖWFW#¢G&6´Ö—…&ÖWFW"ÂfÇVS¢c3"’°¢–bfÇVRæ—5öf–æ—FR‚’°¢&WGW&ã°¢Ð¢6VÆbæ6æ6VÅ÷G&6µöÖ—…övW7GW&R‚“°¢–b6VÆ`¢æÖ7FW%öÖ—…övW7GW&P¢æ—5÷6öÖUöæB‡Â†7F—fRÂò—Â7F—fRÒ&ÖWFW"¢°¢–b6VÆbæÖ7FW%öÖ—…ö6öÖÖ—EöBæ—5÷6öÖR‚’°¢6VÆbæ6öÖÖ—EöÖ7FW%öÖ—‚‚“°¢ÒVÇ6R°¢6VÆbæ6æ6VÅöÖ7FW%öÖ—‚‚“°¢Ð¢Ð¢ÆWB7W'&VçBÒ6VÆ`¢æÖ7FW%öÖ—…övW7GW&P¢æÖö÷"‡6VÆbç&ö¦V7BæÖ7FW%öÖ—‚‚’ÂÂ…òÂÖ—‚—ÂÖ—‚“°¢ÆWBÖ—‚ÒÖF6‚&ÖWFW"°¢G&6´Ö—…&ÖWFW#£¥föÇVÖRÓâFuö6÷&S£¤Ö7FW$Ö—ƒ£¦æWr€¢fÇVRæ6Æ×†fFW#£¥4”ÄTä4UôD"ÂfFW#£¤Ô…ôD"’À¢7W'&VçBçâ‚’À¢’À¢G&6´Ö—…&ÖWFW#£¥âÓâ°¢Fuö6÷&S£¤Ö7FW$Ö—ƒ£¦æWr†7W'&VçBçföÇVÖUöF"‚’ÂfÇVRæ6Æ×‚ÓãÂã’¢Ð¢Ó°¢–bÆWBö²†Ö—‚’ÒÖ—‚°¢6VÆbæÖ7FW%öÖ—…ö6öÖÖ—EöBÒæöæS°¢6VÆbæÖ7FW%öÖ—…övW7GW&RÒ6öÖR‚‡&ÖWFW"ÂÖ—‚’“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6²°¢Æ–&6²ç6WEöÖ7FW%öÖ—‚†Ö—‚“°¢Ð¢Ð¢Ð ¢fâ6öÖÖ—EöÖ7FW%öÖ—‚‚f×WB6VÆb’°¢6VÆbæÖ7FW%öÖ—…ö6öÖÖ—EöBÒæöæS°¢–bÆWB6öÖR‚…òÂÖ—‚’’Ò6VÆbæÖ7FW%öÖ—…övW7GW&RçF¶R‚’°¢–bÖ—‚Ò6VÆbç&ö¦V7BæÖ7FW%öÖ—‚‚’°¢6VÆbæÇ•ö7F–öâ„Ft7F–öã£¥6WDÖ7FW$Ö—‚²Ö—‚ÒÂ$Ö7FW"Ö—‚6†ævVB"“°¢ÒVÇ6R°¢6VÆbç7–æ5öÖ7FW%öÖ—…÷Fõ÷Æ–&6²‚“°¢Ð¢Ð¢Ð ¢fâ6æ6VÅöÖ7FW%öÖ—‚‚f×WB6VÆb’°¢6VÆbæÖ7FW%öÖ—…ö6öÖÖ—EöBÒæöæS°¢–b6VÆbæÖ7FW%öÖ—…övW7GW&RçF¶R‚’æ—5÷6öÖR‚’°¢6VÆbç7–æ5öÖ7FW%öÖ—…÷Fõ÷Æ–&6²‚“°¢Ð¢Ð ¢fâ&W6WEöÖ7FW%öÖ—‚‚f×WB6VÆbÂ&ÖWFW#¢G&6´Ö—…&ÖWFW"’°¢òòF÷V&ÆR6Æ–6²w2f—'7B&W726â†fRVWVVBvW7GW&S¢&WÆ6RF†B&Wf–Wp¢òò6òF†R&W6WB&VÖ–ç2öæR†—7F÷'’VçG'’ÂÆ–¶RF†R÷&F–æ'’G&6²6öçG&öÇ2à¢–b6VÆ`¢æÖ7FW%öÖ—…övW7GW&P¢æ—5÷6öÖUöæB‡Â†7F—fRÂò—Â7F—fRÒ&ÖWFW"¢°¢6VÆbæ6öÖÖ—EöÖ7FW%öÖ—‚‚“°¢Ð¢6VÆbæ6æ6VÅöÖ7FW%öÖ—‚‚“°¢ÆWB7W'&VçBÒ6VÆbç&ö¦V7BæÖ7FW%öÖ—‚‚“°¢ÆWBÖ—‚ÒÖF6‚&ÖWFW"°¢G&6´Ö—…&ÖWFW#£¥föÇVÖRÓâFuö6÷&S£¤Ö7FW$Ö—ƒ£¦æWrƒãÂ7W'&VçBçâ‚’’À¢G&6´Ö—…&ÖWFW#£¥âÓâFuö6÷&S£¤Ö7FW$Ö—ƒ£¦æWr†7W'&VçBçföÇVÖUöF"‚’Âã’À¢Ð¢æW‡V7B‚&W†—7F–ærÖ7FW"6öçG&öÇ2&RfÆ–FFVB"“°¢–bÖ—‚Ò7W'&VçB°¢6VÆbæÇ•ö7F–öâ„Ft7F–öã£¥6WDÖ7FW$Ö—‚²Ö—‚ÒÂ$Ö7FW"Ö—‚&W6WB"“°¢Ð¢Ð ¢fâ&Wf–Wu÷G&6µöÖ—‚‚f×WB6VÆbÂG&6µö–C¢G&6´–BÂ&ÖWFW#¢G&6´Ö—…&ÖWFW"ÂfÇVS¢c3"’°¢6VÆbæ6æ6VÅöÖ7FW%öÖ—‚‚“°¢ÖF6‚&ÖWFW"°¢G&6´Ö—…&ÖWFW#£¥föÇVÖRÓâ°¢ÆWBF—66&FVEöG&gBÒ6VÆbçG&6µ÷föÇVÖUöVF—G2ç&VÖ÷fR‚gG&6µö–B’æ—5÷6öÖR‚“°¢6VÆbæ6ÆV%÷G&6µöG&gB‡G&6µö–BÂG&6´G&gDf–VÆC£¥föÇVÖR“°¢–bF—66&FVEöG&gB°¢6VÆbç7FGW2Ò%G&6²föÇVÖRG&gBF—66&FVB'’6Æ–FW"F§W7FÖVçB"çFõö÷væVB‚“°¢Ð¢Ð¢G&6´Ö—…&ÖWFW#£¥âÓâ°¢ÆWBF—66&FVEöG&gBÒ6VÆbçG&6µ÷åöVF—G2ç&VÖ÷fR‚gG&6µö–B’æ—5÷6öÖR‚“°¢6VÆbæ6ÆV%÷G&6µöG&gB‡G&6µö–BÂG&6´G&gDf–VÆC£¥â“°¢–bF—66&FVEöG&gB°¢6VÆbç7FGW2Ò%G&6²âG&gBF—66&FVB'’6Æ–FW"F§W7FÖVçB"çFõö÷væVB‚“°¢Ð¢Ð¢Ð¢ÆWB6öÖR‡G&6²’Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB7W'&VçE÷föÇVÖRÒG&6²çföÇVÖUöF"‚“°¢ÆWB7W'&VçE÷âÒG&6²çâ‚“°¢–b6VÆ`¢çG&6µöÖ—…övW7GW&P¢æ—5÷6öÖUöæB‡ÆvW7GW&WÂvW7GW&RçG&6µö–BÒG&6µö–BÇÂvW7GW&Rç&ÖWFW"Ò&ÖWFW"¢°¢6VÆbæ6æ6VÅ÷G&6µöÖ—…övW7GW&R‚“°¢Ð¢ÆWBvW7GW&RÒ6VÆbçG&6µöÖ—…övW7GW&RævWEö÷%ö–ç6W'B…G&6´Ö—„vW7GW&R°¢G&6µö–BÀ¢&ÖWFW"À¢&Vf÷&U÷föÇVÖUöF#¢7W'&VçE÷föÇVÖRÀ¢&Vf÷&U÷ã¢7W'&VçE÷âÀ¢gFW%÷föÇVÖUöF#¢7W'&VçE÷föÇVÖRÀ¢gFW%÷ã¢7W'&VçE÷âÀ¢Ò“°¢ÖF6‚&ÖWFW"°¢G&6´Ö—…&ÖWFW#£¥föÇVÖRÓâ°¢vW7GW&RægFW%÷föÇVÖUöF"ÒfÇVRæ6Æ×†fFW#£¥4”ÄTä4UôD"ÂfFW#£¤Ô…ôD"¢Ð¢G&6´Ö—…&ÖWFW#£¥âÓâvW7GW&RægFW%÷âÒfÇVRæ6Æ×‚ÓãÂã’À¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6°¢bbÆ–&6²ç6WE÷G&6µöÖ—‚‡G&6µö–BÂvW7GW&RægFW%÷föÇVÖUöF"ÂvW7GW&RægFW%÷â¢°¢6VÆbç7FGW2Ò$6÷VÆBæ÷BWFFRF†R7F—fRG&6²Ö—‚"çFõö÷væVB‚“°¢Ð¢Ð ¢fâf–æ—6…÷G&6µöÖ—…övW7GW&R‚f×WB6VÆbÂG&6µö–C¢G&6´–BÂ&ÖWFW#¢G&6´Ö—…&ÖWFW"’°¢ÆWB6öÖR†vW7GW&R’Ò6VÆbçG&6µöÖ—…övW7GW&RVÇ6R°¢&WGW&ã°¢Ó°¢–bvW7GW&RçG&6µö–BÒG&6µö–BÇÂvW7GW&Rç&ÖWFW"Ò&ÖWFW"°¢&WGW&ã°¢Ð¢ÆWB†&Vf÷&RÂgFW"’ÒÖF6‚&ÖWFW"°¢G&6´Ö—…&ÖWFW#£¥föÇVÖRÓâ†vW7GW&Ræ&Vf÷&U÷föÇVÖUöF"ÂvW7GW&RægFW%÷föÇVÖUöF"’À¢G&6´Ö—…&ÖWFW#£¥âÓâ†vW7GW&Ræ&Vf÷&U÷âÂvW7GW&RægFW%÷â’À¢Ó°¢–b†&Vf÷&RÒgFW"’æ'2‚’Âc3#£¤U4”Äôâ°¢6VÆbçG&6µöÖ—…övW7GW&RÒæöæS°¢6VÆbçG&6µöÖ—…ö6öÖÖ—EöBÒæöæS°¢6VÆbç7–æ5÷G&6µöÖ—…÷Fõ÷Æ–&6²‡G&6µö–B“°¢&WGW&ã°¢Ð¢6VÆbçG&6µöÖ—…ö6öÖÖ—EöBÒ6öÖR„–ç7FçC£¦æ÷r‚’²GW&F–öã£¦g&öÕöÖ–ÆÆ—2ƒ3S’“°¢Ð ¢fâ6öÖÖ—E÷G&6µöÖ—…övW7GW&R‚f×WB6VÆb’°¢6VÆbçG&6µöÖ—…ö6öÖÖ—EöBÒæöæS°¢ÆWB6öÖR†vW7GW&R’Ò6VÆbçG&6µöÖ—…övW7GW&RçF¶R‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB†&Vf÷&RÂgFW"’ÒÖF6‚vW7GW&Rç&ÖWFW"°¢G&6´Ö—…&ÖWFW#£¥föÇVÖRÓâ†vW7GW&Ræ&Vf÷&U÷föÇVÖUöF"ÂvW7GW&RægFW%÷föÇVÖUöF"’À¢G&6´Ö—…&ÖWFW#£¥âÓâ†vW7GW&Ræ&Vf÷&U÷âÂvW7GW&RægFW%÷â’À¢Ó°¢–b†&Vf÷&RÒgFW"’æ'2‚’Âc3#£¤U4”Äôâ°¢6VÆbç7–æ5÷G&6µöÖ—…÷Fõ÷Æ–&6²†vW7GW&RçG&6µö–B“°¢&WGW&ã°¢Ð¢ÆWB7F–öâÒÖF6‚vW7GW&Rç&ÖWFW"°¢G&6´Ö—…&ÖWFW#£¥föÇVÖRÓâFt7F–öã£¥6WEG&6µföÇVÖR°¢G&6µö–C¢vW7GW&RçG&6µö–BÀ¢föÇVÖUöF#¢vW7GW&RægFW%÷föÇVÖUöF"À¢ÒÀ¢G&6´Ö—…&ÖWFW#£¥âÓâFt7F–öã£¥6WEG&6µâ°¢G&6µö–C¢vW7GW&RçG&6µö–BÀ¢ã¢vW7GW&RægFW%÷âÀ¢ÒÀ¢Ó°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ%G&6²Ö—‚6†ævVB"“°¢Ð ¢fâG&6µöÖ—…ö6öÖÖ—Eö—5÷VæF–ær‚g6VÆbÂG&6µö–C¢G&6´–BÂ&ÖWFW#¢G&6´Ö—…&ÖWFW"’Óâ&ööÂ°¢6VÆbçG&6µöÖ—…ö6öÖÖ—EöBæ—5÷6öÖR‚¢bb6VÆbçG&6µöÖ—…övW7GW&Ræ—5÷6öÖUöæB‡ÆvW7GW&WÂ°¢vW7GW&RçG&6µö–BÓÒG&6µö–BbbvW7GW&Rç&ÖWFW"ÓÒ&ÖWFW ¢Ò¢Ð ¢fâ6ÆV%÷G&6µöÖ—…öf÷%÷&W6WB€¢f×WB6VÆbÀ¢G&6µö–C¢G&6´–BÀ¢&ÖWFW#¢G&6´Ö—…&ÖWFW"À¢F÷V&ÆUö6Æ–6³¢&ööÂÀ¢’°¢–bF÷V&ÆUö6Æ–6²bb6VÆbçG&6µöÖ—…ö6öÖÖ—Eö—5÷VæF–ær‡G&6µö–BÂ&ÖWFW"’°¢6VÆbçG&6µöÖ—…övW7GW&RÒæöæS°¢6VÆbçG&6µöÖ—…ö6öÖÖ—EöBÒæöæS°¢ÒVÇ6R°¢6VÆbæ6æ6VÅ÷G&6µöÖ—…övW7GW&R‚“°¢Ð¢Ð ¢fâ6æ6VÅ÷G&6µöÖ—…övW7GW&R‚f×WB6VÆb’°¢6VÆbçG&6µöÖ—…ö6öÖÖ—EöBÒæöæS°¢–bÆWB6öÖR…övW7GW&R’Ò6VÆbçG&6µöÖ—…övW7GW&RçF¶R‚’°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6²°¢ÆWBòÒÆ–&6²ç6WE÷G&6µöÖ—‚€¢övW7GW&RçG&6µö–BÀ¢övW7GW&Ræ&Vf÷&U÷föÇVÖUöF"À¢övW7GW&Ræ&Vf÷&U÷âÀ¢“°¢Ð¢Ð¢Ð ¢fâ&Vv–å÷G&6µöG&gB‚f×WB6VÆbÂG&6µö–C¢G&6´–BÂf–VÆC¢G&6´G&gDf–VÆB’°¢6VÆbæ7F—fU÷G&6µöG&gBÒ6öÖR‚‡G&6µö–BÂf–VÆB’“°¢6VÆbçG&6µöG&gEöW'&÷'2ç&VÖ÷fR‚b‡G&6µö–BÂf–VÆB’“°¢6VÆbç7FGW2Ò%G&6²f–VÆBVF—BVæF–æs¢VçFW"Æ–W2ÂW66RF—66&G3²&ÇW"¶VW2F†RG&gBæB6fR6öÖÖ—G2fÆ–BVF—G2"çFõö÷væVB‚“°¢Ð ¢fâ6ÆV%÷G&6µöG&gB‚f×WB6VÆbÂG&6µö–C¢G&6´–BÂf–VÆC¢G&6´G&gDf–VÆB’°¢6VÆbçG&6µöG&gEöW'&÷'2ç&VÖ÷fR‚b‡G&6µö–BÂf–VÆB’“°¢–b6VÆbæ7F—fU÷G&6µöG&gBÓÒ6öÖR‚‡G&6µö–BÂf–VÆB’’°¢6VÆbæ7F—fU÷G&6µöG&gBÒæöæS°¢Ð¢Ð ¢fâ6æ6VÅö7F—fU÷G&6µöG&gB‚f×WB6VÆb’Óâ&ööÂ°¢ÆWB6öÖR‚‡G&6µö–BÂf–VÆB’’Ò6VÆbæ7F—fU÷G&6µöG&gBçF¶R‚’VÇ6R°¢&WGW&âfÇ6S°¢Ó°¢ÖF6‚f–VÆB°¢G&6´G&gDf–VÆC£¤æÖRÓâ°¢6VÆbçG&6µöæÖUöVF—G2ç&VÖ÷fR‚gG&6µö–B“°¢Ð¢G&6´G&gDf–VÆC£¥föÇVÖRÓâ°¢6VÆbçG&6µ÷föÇVÖUöVF—G2ç&VÖ÷fR‚gG&6µö–B“°¢Ð¢G&6´G&gDf–VÆC£¥âÓâ°¢6VÆbçG&6µ÷åöVF—G2ç&VÖ÷fR‚gG&6µö–B“°¢Ð¢Ð¢6VÆbçG&6µöG&gEöW'&÷'2ç&VÖ÷fR‚b‡G&6µö–BÂf–VÆB’“°¢6VÆbç7FGW2Ò%G&6²f–VÆBG&gBF—66&FVB"çFõö÷væVB‚“°¢G'VP¢Ð ¢fâ6ÆV%÷G&6µöG&gE÷7FFR‚f×WB6VÆb’°¢6VÆbçG&6µöæÖUöVF—G2æ6ÆV"‚“°¢6VÆbæÖ–F•ö—FVÕöæÖUöVF—G2æ6ÆV"‚“°¢6VÆbæÖ–F•ö—FVÕöæÖUöW'&÷'2æ6ÆV"‚“°¢6VÆbçG&6µ÷föÇVÖUöVF—G2æ6ÆV"‚“°¢6VÆbçG&6µ÷åöVF—G2æ6ÆV"‚“°¢6VÆbæ7F—fU÷G&6µöG&gBÒæöæS°¢6VÆbçG&6µöG&gEöW'&÷'2æ6ÆV"‚“°¢Ð ¢fâ6ÆV%÷G&6µöG&gG2‚f×WB6VÆbÂG&6µö–C¢G&6´–B’°¢6VÆbçG&6µöæÖUöVF—G2ç&VÖ÷fR‚gG&6µö–B“°¢6VÆbçG&6µ÷föÇVÖUöVF—G2ç&VÖ÷fR‚gG&6µö–B“°¢6VÆbçG&6µ÷åöVF—G2ç&VÖ÷fR‚gG&6µö–B“°¢6VÆbçG&6µöG&gEöW'&÷'0¢ç&WF–â‡Â†VF—FVE÷G&6µö–BÂò’Â÷Â¦VF—FVE÷G&6µö–BÒG&6µö–B“°¢–b6VÆ`¢æ7F—fU÷G&6µöG&g@¢æ—5÷6öÖUöæB‡Â†VF—FVE÷G&6µö–BÂò—ÂVF—FVE÷G&6µö–BÓÒG&6µö–B¢°¢6VÆbæ7F—fU÷G&6µöG&gBÒæöæS°¢Ð¢Ð ¢V"‡7WW"’fâ6öÖÖ—E÷VæF–æu÷G&6µöG&gG2‚f×WB6VÆb’Óâ&ööÂ°¢ÆWB×WB7F–öç2ÒfV3£¦æWr‚“°¢ÆWB×WBf—'7EöW'&÷"ÒæöæS° ¢f÷"G&6²–â6VÆbç&ö¦V7BçG&6·2‚’°¢ÆWBG&6µö–BÒG&6²æ–B‚“°¢–bÆWB6öÖR‡FW‡B’Ò6VÆbçG&6µöæÖUöVF—G2ævWB‚gG&6µö–B’°¢ÖF6‚'6U÷G&6µöæÖUöG&gB‡FW‡B’°¢ö²†æÖR’–bæÖRÒG&6²ææÖR‚’Óâ°¢7F–öç2çW6‚„Ft7F–öã£¥6WEG&6´æÖR²G&6µö–BÂæÖRÒ“°¢Ð¢ö²…ò’Óâ·Ð¢W'"†W'&÷"’Óâ°¢f—'7EöW'&÷"Ò6öÖR‚‡G&6µö–BÂG&6´G&gDf–VÆC£¤æÖRÂW'&÷"’“°¢'&V³°¢Ð¢Ð¢Ð¢–bÆWB6öÖR‡FW‡B’Ò6VÆbçG&6µ÷föÇVÖUöVF—G2ævWB‚gG&6µö–B’°¢ÖF6‚'6U÷G&6µ÷föÇVÖUöG&gB‡FW‡B’°¢ö²‡föÇVÖUöF"’–b‡föÇVÖUöF"ÒG&6²çföÇVÖUöF"‚’’æ'2‚’ãÒc3#£¤U4”ÄôâÓâ°¢7F–öç2çW6‚„Ft7F–öã£¥6WEG&6µföÇVÖR°¢G&6µö–BÀ¢föÇVÖUöF"À¢Ò“°¢Ð¢ö²…ò’Óâ·Ð¢W'"†W'&÷"’Óâ°¢f—'7EöW'&÷"Ò6öÖR‚‡G&6µö–BÂG&6´G&gDf–VÆC£¥föÇVÖRÂW'&÷"’“°¢'&V³°¢Ð¢Ð¢Ð¢–bÆWB6öÖR‡FW‡B’Ò6VÆbçG&6µ÷åöVF—G2ævWB‚gG&6µö–B’°¢ÖF6‚'6U÷G&6µ÷åöG&gB‡FW‡B’°¢ö²‡â’–b‡âÒG&6²çâ‚’’æ'2‚’ãÒc3#£¤U4”ÄôâÓâ°¢7F–öç2çW6‚„Ft7F–öã£¥6WEG&6µâ²G&6µö–BÂâÒ“°¢Ð¢ö²…ò’Óâ·Ð¢W'"†W'&÷"’Óâ°¢f—'7EöW'&÷"Ò6öÖR‚‡G&6µö–BÂG&6´G&gDf–VÆC£¥âÂW'&÷"’“°¢'&V³°¢Ð¢Ð¢Ð¢Ð ¢ÆWB×WBf—'7EöÖ–F•ö—FVÕöW'&÷"ÒæöæS°¢f÷"—FVÒ–â6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’°¢ÆWB6öÖR‡FW‡B’Ò6VÆbæÖ–F•ö—FVÕöæÖUöVF—G2ævWB‚f—FVÒæ–B‚’’VÇ6R°¢6öçF–çVS°¢Ó°¢ÖF6‚'6UöÖ–F•ö—FVÕöæÖUöG&gB‡FW‡B’°¢ö²†æÖR’–bæÖRÒ—FVÒææÖR‚’Óâ°¢7F–öç2çW6‚„Ft7F–öã£¥6WDÖ–F”—FVÔæÖR°¢—FVÕö–C¢—FVÒæ–B‚’À¢æÖRÀ¢Ò“°¢Ð¢ö²…ò’Óâ·Ð¢W'"†W'&÷"’Óâ°¢f—'7EöÖ–F•ö—FVÕöW'&÷"Ò6öÖR‚†—FVÒæ–B‚’ÂW'&÷"’“°¢'&V³°¢Ð¢Ð¢Ð ¢–bÆWB6öÖR‚†—FVÕö–BÂW'&÷"’’Òf—'7EöÖ–F•ö—FVÕöW'&÷"°¢6VÆbæÖ–F•ö—FVÕöæÖUöW'&÷'2æ–ç6W'B†—FVÕö–BÂW'&÷"çFõö÷væVB‚’“°¢6VÆbç7FGW2Ð¢f÷&ÖB‚$6ææ÷B6fRÔ”D’—FVÒæÖS¢¶W'&÷'Ó²6÷'&V7B—B÷"6ÆV"F†RG&gB"“°¢&WGW&âfÇ6S°¢Ð ¢–bÆWB6öÖR‚‡G&6µö–BÂf–VÆBÂW'&÷"’’Òf—'7EöW'&÷"°¢6VÆbçG&6µöG&gEöW'&÷'0¢æ–ç6W'B‚‡G&6µö–BÂf–VÆB’ÂW'&÷"çFõö÷væVB‚’“°¢ÆWBG&6µöæÖRÒ6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢æÖö÷"‚'G&6²"ÂÇG&6·ÂG&6²ææÖR‚’“°¢6VÆbç7FGW2Òf÷&ÖB€¢$6ææ÷B6fS¢·G&6µöæÖWÒ·Ò(	B¶W'&÷'Ó²6÷'&V7B—B÷"&W72W66RFòF—66&B"À¢f–VÆBæÆ&VÂ‚¢“°¢&WGW&âfÇ6S°¢Ð ¢–b7F–öç2æ—5öV×G’‚’°¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢ÆWB7F–öâÒ–b7F–öç2æÆVâ‚’ÓÒ°¢7F–öç2ç÷‚’æW‡V7B‚&öæRVæF–ærG&6²VF—B7F–öâW†—7G2"¢ÒVÇ6R°¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ð¢Ó°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ%G&6²f–VÆBVF—G2Æ–VB"“°¢–b6VÆbç&Wf—6–öâÓÒ&Wf–÷W5÷&Wf—6–öâ°¢&WGW&âfÇ6S°¢Ð¢Ð¢6VÆbæ6ÆV%÷G&6µöG&gE÷7FFR‚“°¢G'VP¢Ð ¢fâ6öÖÖ—EöÖ–F•ö—FVÕöæÖR‚f×WB6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢ÆWB6öÖR‡FW‡B’Ò6VÆbæÖ–F•ö—FVÕöæÖUöVF—G2ævWB‚f—FVÕö–B’æ6ÆöæVB‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBæÖRÒÖF6‚'6UöÖ–F•ö—FVÕöæÖUöG&gB‚gFW‡B’°¢ö²†æÖR’ÓâæÖRÀ¢W'"†W'&÷"’Óâ°¢6VÆbæÖ–F•ö—FVÕöæÖUöW'&÷'2æ–ç6W'B†—FVÕö–BÂW'&÷"çFõö÷væVB‚’“°¢6VÆbç7FGW2ÒW'&÷"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB6öÖR†7W'&VçEöæÖR’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢æÖ‡Æ—FV×Â—FVÒææÖR‚’çFõö÷væVB‚’¢VÇ6R°¢6VÆbæÖ–F•ö—FVÕöæÖUöVF—G2ç&VÖ÷fR‚f—FVÕö–B“°¢6VÆbæÖ–F•ö—FVÕöæÖUöW'&÷'2ç&VÖ÷fR‚f—FVÕö–B“°¢6VÆbç7FGW2Ò$Ô”D’—FVÒæòÆöævW"W†—7G2"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢6VÆbæÖ–F•ö—FVÕöæÖUöVF—G2ç&VÖ÷fR‚f—FVÕö–B“°¢6VÆbæÖ–F•ö—FVÕöæÖUöW'&÷'2ç&VÖ÷fR‚f—FVÕö–B“°¢–bæÖRÓÒ7W'&VçEöæÖR°¢6VÆbç7FGW2Ò$Ô”D’—FVÒæÖRVæ6†ævVB"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¥6WDÖ–F”—FVÔæÖR²—FVÕö–BÂæÖRÒÀ¢$Ô”D’—FVÒ&VæÖVB"À¢“°¢Ð ¢fâ6öÖÖ—E÷G&6µ÷föÇVÖU÷FW‡B‚f×WB6VÆbÂG&6µö–C¢G&6´–B’°¢ÆWB6öÖR‡FW‡B’Ò6VÆbçG&6µ÷föÇVÖUöVF—G2ævWB‚gG&6µö–B’æ6ÆöæVB‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBfÇVRÒÖF6‚'6U÷G&6µ÷föÇVÖUöG&gB‚gFW‡B’°¢ö²‡fÇVR’ÓâfÇVRÀ¢W'"†W'&÷"’Óâ°¢6VÆbçG&6µöG&gEöW'&÷'0¢æ–ç6W'B‚‡G&6µö–BÂG&6´G&gDf–VÆC£¥föÇVÖR’ÂW'&÷"çFõö÷væVB‚’“°¢6VÆbç7FGW2ÒW'&÷"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢Ó°¢6VÆbæ6æ6VÅ÷G&6µöÖ—…övW7GW&R‚“°¢6VÆbçG&6µ÷föÇVÖUöVF—G2ç&VÖ÷fR‚gG&6µö–B“°¢6VÆbæ6ÆV%÷G&6µöG&gB‡G&6µö–BÂG&6´G&gDf–VÆC£¥föÇVÖR“°¢–b6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢æ—5÷6öÖUöæB‡ÇG&6·Â‡G&6²çföÇVÖUöF"‚’ÒfÇVR’æ'2‚’Âc3#£¤U4”Äôâ¢°¢&WGW&ã°¢Ð¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¥6WEG&6µföÇVÖR°¢G&6µö–BÀ¢föÇVÖUöF#¢fÇVRÀ¢ÒÀ¢%G&6²föÇVÖR6†ævVB"À¢“°¢Ð ¢fâ6öÖÖ—E÷G&6µ÷å÷FW‡B‚f×WB6VÆbÂG&6µö–C¢G&6´–B’°¢ÆWB6öÖR‡FW‡B’Ò6VÆbçG&6µ÷åöVF—G2ævWB‚gG&6µö–B’æ6ÆöæVB‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBfÇVRÒÖF6‚'6U÷G&6µ÷åöG&gB‚gFW‡B’°¢ö²‡fÇVR’ÓâfÇVRÀ¢W'"†W'&÷"’Óâ°¢6VÆbçG&6µöG&gEöW'&÷'0¢æ–ç6W'B‚‡G&6µö–BÂG&6´G&gDf–VÆC£¥â’ÂW'&÷"çFõö÷væVB‚’“°¢6VÆbç7FGW2ÒW'&÷"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢Ó°¢6VÆbæ6æ6VÅ÷G&6µöÖ—…övW7GW&R‚“°¢6VÆbçG&6µ÷åöVF—G2ç&VÖ÷fR‚gG&6µö–B“°¢6VÆbæ6ÆV%÷G&6µöG&gB‡G&6µö–BÂG&6´G&gDf–VÆC£¥â“°¢–b6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢æ—5÷6öÖUöæB‡ÇG&6·Â‡G&6²çâ‚’ÒfÇVR’æ'2‚’Âc3#£¤U4”Äôâ¢°¢&WGW&ã°¢Ð¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¥6WEG&6µâ°¢G&6µö–BÀ¢ã¢fÇVRÀ¢ÒÀ¢%G&6²â6†ævVB"À¢“°¢Ð ¢fâ&W6WE÷G&6µ÷föÇVÖR‚f×WB6VÆbÂG&6µö–C¢G&6´–BÂF÷V&ÆUö6Æ–6³¢&ööÂ’°¢6VÆbæ6ÆV%÷G&6µöÖ—…öf÷%÷&W6WB‡G&6µö–BÂG&6´Ö—…&ÖWFW#£¥föÇVÖRÂF÷V&ÆUö6Æ–6²“°¢ÆWBF—66&FVEöG&gBÒ6VÆbçG&6µ÷föÇVÖUöVF—G2ç&VÖ÷fR‚gG&6µö–B’æ—5÷6öÖR‚“°¢6VÆbæ6ÆV%÷G&6µöG&gB‡G&6µö–BÂG&6´G&gDf–VÆC£¥föÇVÖR“°¢–bF—66&FVEöG&gB°¢6VÆbç7FGW2Ò%G&6²föÇVÖRG&gBF—66&FVB'’&W6WB"çFõö÷væVB‚“°¢Ð¢–b6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢æ—5÷6öÖUöæB‡ÇG&6·ÂG&6²çföÇVÖUöF"‚’Òã¢°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¥6WEG&6µföÇVÖR°¢G&6µö–BÀ¢föÇVÖUöF#¢ãÀ¢ÒÀ¢%G&6²föÇVÖR&W6WB"À¢“°¢ÒVÇ6R°¢6VÆbç7–æ5÷G&6µöÖ—…÷Fõ÷Æ–&6²‡G&6µö–B“°¢Ð¢Ð ¢fâ&W6WE÷G&6µ÷â‚f×WB6VÆbÂG&6µö–C¢G&6´–BÂF÷V&ÆUö6Æ–6³¢&ööÂ’°¢6VÆbæ6ÆV%÷G&6µöÖ—…öf÷%÷&W6WB‡G&6µö–BÂG&6´Ö—…&ÖWFW#£¥âÂF÷V&ÆUö6Æ–6²“°¢ÆWBF—66&FVEöG&gBÒ6VÆbçG&6µ÷åöVF—G2ç&VÖ÷fR‚gG&6µö–B’æ—5÷6öÖR‚“°¢6VÆbæ6ÆV%÷G&6µöG&gB‡G&6µö–BÂG&6´G&gDf–VÆC£¥â“°¢–bF—66&FVEöG&gB°¢6VÆbç7FGW2Ò%G&6²âG&gBF—66&FVB'’&W6WB"çFõö÷væVB‚“°¢Ð¢–b6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢æ—5÷6öÖUöæB‡ÇG&6·ÂG&6²çâ‚’Òã¢°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¥6WEG&6µâ²G&6µö–BÂã¢ãÒÀ¢%G&6²â6VçFW&VB"À¢“°¢ÒVÇ6R°¢6VÆbç7–æ5÷G&6µöÖ—…÷Fõ÷Æ–&6²‡G&6µö–B“°¢Ð¢Ð ¢fâf–æ—6…ö—FVÕöG&r‚f×WB6VÆb’°¢ÆWB&Wf–WrÒ6VÆbçF–ÖVÆ–æRæG&u÷&Wf–Wr‚“°¢6VÆbçF–ÖVÆ–æRæ†æFÆR‡F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤VæD—FVÔG&r“°¢ÆWB6öÖR‡&Wf–Wr’Ò&Wf–WrVÇ6R°¢&WGW&ã°¢Ó°¢–b&Wf–WrçfÆ–B°¢6VÆbç7FGW2Ò$G&÷&V¦V7FVC¢—FVÒv÷VÆBÆVfRF†R&ö¦V7B&÷VæG2"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÖF6‚6VÆbæ—FVÕöG&uö7F–öâ‡&Wf–Wr’°¢ö²…6öÖR†7F–öâ’’Óâ6VÆbæÇ•ö7F–öâ†7F–öâÂ$—FV×2Ö÷fVB"’À¢ö²„æöæR’Óâ·Ð¢W'"†W'&÷"’Óâ6VÆbç7FGW2Òf÷&ÖB‚$G&÷&V¦V7FVC¢¶W'&÷'Ò"’À¢Ð¢Ð ¢fâf–æ—6…ö—FVÕ÷G&–Ò‚f×WB6VÆb’°¢ÆWB&Wf–WrÒ6VÆbçF–ÖVÆ–æRæ—FVÕ÷G&–Õ÷&Wf–Wr‚“°¢6VÆbçF–ÖVÆ–æRæ†æFÆR‡F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤VæD—FVÕG&–Ò“°¢ÆWB6öÖR‡&Wf–Wr’Ò&Wf–WrVÇ6R°¢&WGW&ã°¢Ó°¢ÖF6‚6VÆbæ—FVÕ÷G&–Õö7F–öâ‡&Wf–Wr’°¢ö²…6öÖR‚†7F–öâÂ7FGW2’’’Óâ6VÆbæÇ•ö7F–öâ†7F–öâÂ7FGW2’À¢ö²„æöæR’Óâ·Ð¢W'"†W'&÷"’Óâ6VÆbç7FGW2Òf÷&ÖB‚%G&–Ò&V¦V7FVC¢¶W'&÷'Ò"’À¢Ð¢Ð ¢fâ—FVÕ÷G&–Õ÷&Wf–Wuö—5÷fÆ–B‚g6VÆbÂ&Wf–Ws¢F–ÖVÆ–æS£¤—FVÕG&–Õ&Wf–Wr’Óâ&ööÂ°¢–b6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æç’‡Æ—FV×Â—FVÒæ–B‚’ÓÒ&Wf–Wræ—FVÕö–B¢°¢ÆWB—FVÒÒ6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ&Wf–Wræ—FVÕö–B¢æW‡V7B‚$Ô”D’—FVÒv2§W7Bf÷VæB"“°¢&WGW&â6VÆbæÖ–F•ö—FVÕ÷G&–Õö&÷VæG2†—FVÒÂ&Wf–Wr’æ—5öö²‚“°¢Ð¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ&Wf–Wræ—FVÕö–B¢VÇ6R°¢&WGW&âfÇ6S°¢Ó°¢6VÆbæVF–õö—FVÕ÷G&–Õö&÷VæG2†—FVÒÂ&Wf–Wr’æ—5öö²‚¢Ð ¢fâ—FVÕ÷G&–Õö7F–öâ€¢g6VÆbÀ¢&Wf–Ws¢F–ÖVÆ–æS£¤—FVÕG&–Õ&Wf–WrÀ¢’Óâ&W7VÇCÄ÷F–öãÂ„Ft7F–öâÂbw7FF–27G"“âÂ7G&–æsâ°¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ&Wf–Wræ—FVÕö–B¢°¢ÆWB‡7F'E÷F–6²ÂÆVæwF…÷F–6·2Â6÷W&6Uööfg6WE÷F–6·2’Ð¢6VÆbæÖ–F•ö—FVÕ÷G&–Õö&÷VæG2†—FVÒÂ&Wf–Wr“ó°¢–b—FVÒç7F'E÷F–6²‚’ÓÒ7F'E÷F–6°¢bb—FVÒæÆVæwF…÷F–6·2‚’ÓÒÆVæwF…÷F–6·0¢bb—FVÒç6÷W&6Uööfg6WE÷F–6·2‚’ÓÒ6÷W&6Uööfg6WE÷F–6·0¢°¢&WGW&âö²„æöæR“°¢Ð¢ÆWB7F–öâÒ–b&Wf–WræVFvRÓÒF–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¥7F'B°¢Ft7F–öã£¥G&–ÔÖ–F”—FVÕ7F'B°¢—FVÕö–C¢—FVÒæ–B‚’À¢7F'E÷F–6²À¢ÆVæwF…÷F–6·2À¢6÷W&6Uööfg6WE÷F–6·2À¢Ð¢ÒVÇ6R°¢Ft7F–öã£¤VF—DÖ–F”—FVÒ°¢—FVÕö–C¢—FVÒæ–B‚’À¢7F'E÷F–6²À¢ÆVæwF…÷F–6·2À¢Ð¢Ó°¢&WGW&âö²…6öÖR‚†7F–öâÂ$Ô”D’—FVÒG&–ÖÖVB"’’“°¢Ð¢ÆWB—FVÒÒ6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ&Wf–Wræ—FVÕö–B¢æöµö÷%öVÇ6R‡ÇÂf÷&ÖB‚&—FVÒ·ÒæòÆöævW"W†—7G2"Â&Wf–Wræ—FVÕö–BçfÇVR‚’’“ó°¢ÆWB6öÖR‚‡7F'E÷6×ÆRÂ6÷W&6Uööfg6WE÷6×ÆW2ÂÆVæwF…÷6×ÆW2’’Ð¢6VÆbæVF–õö—FVÕ÷G&–Õö&÷VæG2†—FVÒÂ&Wf–Wr“ð¢VÇ6R°¢&WGW&âö²„æöæR“°¢Ó°¢ö²…6öÖR‚€¢Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–C¢&Wf–Wræ—FVÕö–BÀ¢ÖVF–÷&Vc¢—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’À¢7F'E÷6×ÆRÀ¢6÷W&6Uööfg6WE÷6×ÆW2À¢ÆVæwF…÷6×ÆW2À¢ÒÀ¢$VF–ò—FVÒG&–ÖÖVB"À¢’’¢Ð ¢fâVF–õö—FVÕ÷G&–Õö&÷VæG2€¢g6VÆbÀ¢—FVÓ¢fFuö6÷&S£¤VF–ô—FVÒÀ¢&Wf–Ws¢F–ÖVÆ–æS£¤—FVÕG&–Õ&Wf–WrÀ¢’Óâ&W7VÇCÄ÷F–öãÂ‡ScBÂ“cBÂScB“âÂ7G&–æsâ°¢ÆWBöÆE÷7F'BÒ—FVÒç7F'E÷6×ÆR‚“°¢ÆWBöÆEöVæBÒöÆE÷7F'@¢æ6†V6¶VEöFB†—FVÒæÆVæwF…÷6×ÆW2‚’¢æöµö÷%öVÇ6R‡ÇÂ&VF–ò—FVÒ&ævR÷fW&fÆ÷w26×ÆRF–ÖR"çFõö÷væVB‚’“ó°¢ÆWB7F'E÷6×ÆRÒ6VÆ`¢ç&ö¦V7@¢ç6×ÆUöE÷F–6²‡&Wf–Wrç7F'E÷F–6²¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢ÆWBVæE÷6×ÆRÒ6VÆ`¢ç&ö¦V7@¢ç6×ÆUöE÷F–6²‡&Wf–WræVæE÷F–6²¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢–b7F'E÷6×ÆRãÒVæE÷6×ÆR°¢&WGW&âW'"‚$VF–ò—FV×2×W7B&VÖ–âBÆV7BöæR6×ÆRÆöær"çFõö÷væVB‚’“°¢Ð¢–b7F'E÷6×ÆRÓÒöÆE÷7F'BbbVæE÷6×ÆRÓÒöÆEöVæB°¢&WGW&âö²„æöæR“°¢Ð¢ÆWBvfVf÷&ÒÒ6VÆbæVF–õ÷vfVf÷&×2ævWB†—FVÒæÖVF–÷&Vb‚’’æöµö÷%öVÇ6R‡ÇÂ°¢$VF–ò6÷W&6R&÷VæG2&RVæf–Æ&ÆRVçF–ÂvfVf÷&Ò66ææ–ærf–æ—6†W2"çFõö÷væVB‚¢Ò“ó°¢ÆWB&ö¦V7E÷&FRÒScC£¦g&öÒ‡6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’“°¢ÆWB6÷W&6U÷&FRÒScC£¦g&öÒ‡vfVf÷&Òç6×ÆU÷&FR‚’“°¢ÆWB7F'EöFVÇFÒ“#ƒ£¦g&öÒ‡7F'E÷6×ÆR’Ò“#ƒ£¦g&öÒ†öÆE÷7F'B“°¢ÆWB6÷W&6U÷7F'EöFVÇFÒ66ÆU÷&ö¦V7E÷6×ÆW5÷Fõ÷6÷W&6Uög&ÖW5÷F÷v&E÷¦W&ò€¢7F'EöFVÇFÀ¢6÷W&6U÷&FRÀ¢&ö¦V7E÷&FRÀ¢“ó°¢ÆWB6÷W&6Uööfg6WE÷6×ÆW2Ò“#ƒ£¦g&öÒ†—FVÒç6÷W&6Uööfg6WE÷6×ÆW2‚’¢æ6†V6¶VEöFB‡6÷W&6U÷7F'EöFVÇF¢ææE÷F†Vâ‡Æöfg6WGÂ“cC£§G'•ög&öÒ†öfg6WB’æö²‚’¢æöµö÷%öVÇ6R‡ÇÂ$VF–òG&–ÒW†6VVG2F†R7W÷'FVB6–væVB6÷W&6R&ævR"çFõö÷væVB‚’“ó°¢ÆWBÆVæwF…÷6×ÆW2ÒVæE÷6×ÆRÒ7F'E÷6×ÆS°¢ö²…6öÖR‚‡7F'E÷6×ÆRÂ6÷W&6Uööfg6WE÷6×ÆW2ÂÆVæwF…÷6×ÆW2’’¢Ð ¢fâÖ–F•ö—FVÕ÷G&–Õö&÷VæG2€¢g6VÆbÀ¢—FVÓ¢fFuö6÷&S£¤Ö–F”—FVÒÀ¢&Wf–Ws¢F–ÖVÆ–æS£¤—FVÕG&–Õ&Wf–WrÀ¢’Óâ&W7VÇCÂ‡ScBÂScBÂScB’Â7G&–æsâ°¢ÆWBÆVæwF…÷F–6·2Ò&Wf–Wp¢æVæE÷F–6°¢æ6†V6¶VE÷7V"‡&Wf–Wrç7F'E÷F–6²¢æf–ÇFW"‡ÆÆVæwF‡Â¦ÆVæwF‚â¢æöµö÷%öVÇ6R‡ÇÂ$Ô”D’—FV×2×W7B&VÖ–âBÆV7BöæRF–6²Æöær"çFõö÷væVB‚’“ó°¢ÆWB6÷W&6Uööfg6WE÷F–6·2Ò–b&Wf–WræVFvRÓÒF–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¥7F'B°¢ÆWBFVÇFÒ“#ƒ£¦g&öÒ‡&Wf–Wrç7F'E÷F–6²’Ò“#ƒ£¦g&öÒ†—FVÒç7F'E÷F–6²‚’“°¢ScC£§G'•ög&öÒ†“#ƒ£¦g&öÒ†—FVÒç6÷W&6Uööfg6WE÷F–6·2‚’’²FVÇF¢æÖöW'"‡Å÷Â$Ô”D’G&–Ò6ææ÷BW‡FVæB&Vf÷&RF†R6÷W&6R6öçFVçB"çFõö÷væVB‚’“ð¢ÒVÇ6R°¢—FVÒç6÷W&6Uööfg6WE÷F–6·2‚¢Ó°¢6÷W&6Uööfg6WE÷F–6·0¢æ6†V6¶VEöFB†ÆVæwF…÷F–6·2¢æöµö÷%öVÇ6R‡ÇÂ$Ô”D’G&–ÒW†6VVG2F†R6÷W&6R6öçFVçB&ævR"çFõö÷væVB‚’“ó°¢ö²‚‡&Wf–Wrç7F'E÷F–6²ÂÆVæwF…÷F–6·2Â6÷W&6Uööfg6WE÷F–6·2’¢Ð ¢fâÆ–&6µö'W7’‚g6VÆb’Óâ&ööÂ°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢°¢6VÆbçÆ–&6µö'W7¢Ð¢5¶6fr†æ÷B†fVGW&RÒ&VF–òÖFWf–6R"’•Ð¢°¢fÇ6P¢Ð¢Ð ¢fâÆ–&6µö7F—fR‚g6VÆb’Óâ&ööÂ°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢°¢Æ–&6µ÷&WfVçG5÷&ö¦V7EöVF—G2‡6VÆbçÆ–&6²æ—5÷6öÖR‚’Â6VÆbçÆ–&6µ÷Æ––ær¢Ð¢5¶6fr†æ÷B†fVGW&RÒ&VF–òÖFWf–6R"’•Ð¢°¢fÇ6P¢Ð¢Ð ¢fâföÇVÖUöWFöÖF–öåöVF—Eö'W7’‚g6VÆb’Óâ&ööÂ°¢6VÆbç&ö¦V7Eöw&…öVF—Eö'W7’‚¢Ð ¢fâ&ö¦V7Eöw&…öVF—Eö'W7’‚g6VÆb’Óâ&ööÂ°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢ÆWB&V6÷&F–ærÐ¢6VÆbç&V6÷&F–æræ—5÷6öÖR‚’ÇÂ6VÆbç&V6÷&F–æu÷7F'F–ærÇÂ6VÆbç&V6÷&F–æu÷7F÷–æs°¢5¶6fr†æ÷B†fVGW&RÒ&VF–òÖFWf–6R"’•Ð¢ÆWB&V6÷&F–ærÒfÇ6S°¢6VÆbçÆ–&6µö'W7’‚’ÇÂ6VÆbçÆ–&6µö7F—fR‚’ÇÂ&V6÷&F–æp¢Ð ¢fâ—FVÕöG&uö7F–öâ€¢g6VÆbÀ¢&Wf–Ws¢F–ÖVÆ–æS£¤—FVÔG&u&Wf–WrÀ¢’Óâ&W7VÇCÄ÷F–öãÄFt7F–öãâÂ7G&–æsâ°¢ÆWB×WB—FVÕö–G2Ò6VÆ`¢çF–ÖVÆ–æP¢ç6VÆV7FVEö—FV×0¢æ—FW"‚¢æ6÷–VB‚¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢—FVÕö–G2ç6÷'E÷Vç7F&ÆUö'•ö¶W’‡Æ—FVÕö–GÂ—FVÕö–BçfÇVR‚’“°¢ÆWB×WB7F–öç2ÒfV3£§v—F…ö66—G’†—FVÕö–G2æÆVâ‚’¢"“°¢f÷"—FVÕö–B–â—FVÕö–G2°¢ÆWB‡6÷W&6U÷G&6µö–BÂ7F'E÷F–6²’Ò–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢€¢—FVÒçG&6µö–B‚’À¢6VÆbç&ö¦V7@¢çF–6µöE÷6×ÆR†—FVÒç7F'E÷6×ÆR‚’¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“òÀ¢¢ÒVÇ6R–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢†—FVÒçG&6µö–B‚’Â—FVÒç7F'E÷F–6²‚’¢ÒVÇ6R°¢&WGW&âW'"†f÷&ÖB‚&—FVÒ·ÒæòÆöævW"W†—7G2"Â—FVÕö–BçfÇVR‚’’“°¢Ó°¢ÆWB6÷W&6U÷G&6µö–æFW‚Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢ç÷6—F–öâ‡ÇG&6·ÂG&6²æ–B‚’ÓÒ6÷W&6U÷G&6µö–B¢æöµö÷%öVÇ6R‡ÇÂ'6÷W&6RG&6²æòÆöævW"W†—7G2"çFõö÷væVB‚’“ó°¢ÆWBF&vWE÷G&6µö–æFW‚Ð¢W6—¦S£§G'•ög&öÒ‡6÷W&6U÷G&6µö–æFW‚2“#‚²“#ƒ£¦g&öÒ‡&Wf–WrçG&6µöFVÇF’¢æÖöW'"‡Å÷Â'F&vWBG&6²—2÷WG6–FRF†R&ö¦V7B"çFõö÷væVB‚’“ó°¢ÆWBF&vWE÷G&6µö–BÒ6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢ævWB‡F&vWE÷G&6µö–æFW‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æöµö÷%öVÇ6R‡ÇÂ'F&vWBG&6²—2÷WG6–FRF†R&ö¦V7B"çFõö÷væVB‚’“ó°¢ÆWBF&vWE÷7F'E÷F–6²ÒScC£§G'•ög&öÒ†“#ƒ£¦g&öÒ‡7F'E÷F–6²’²&Wf–WræFVÇF÷F–6·2¢æÖöW'"‡Å÷Â'F&vWB÷6—F–öâ—2÷WG6–FRF†R&ö¦V7B"çFõö÷væVB‚’“ó° ¢–b&Wf–Wræ6÷’°¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢ÆWBF&vWE÷6×ÆRÒ6VÆ`¢ç&ö¦V7@¢ç6×ÆUöE÷F–6²‡F&vWE÷7F'E÷F–6²¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢7F–öç2çW6‚„Ft7F–öã£¤GWÆ–6FTVF–ô—FVÔB°¢—FVÕö–C¢—FVÒæ–B‚’À¢G&6µö–C¢F&vWE÷G&6µö–BÀ¢7F'E÷6×ÆS¢F&vWE÷6×ÆRÀ¢Ò“°¢ÒVÇ6R°¢7F–öç2çW6‚„Ft7F–öã£¤GWÆ–6FTÖ–F”—FVÕFõG&6²°¢—FVÕö–BÀ¢G&6µö–C¢F&vWE÷G&6µö–BÀ¢7F'E÷F–6³¢F&vWE÷7F'E÷F–6²À¢Ò“°¢Ð¢6öçF–çVS°¢Ð ¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢ÆWBF&vWE÷6×ÆRÒ6VÆ`¢ç&ö¦V7@¢ç6×ÆUöE÷F–6²‡F&vWE÷7F'E÷F–6²¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢–bF&vWE÷6×ÆRÒ—FVÒç7F'E÷6×ÆR‚’°¢7F–öç2çW6‚„Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–BÀ¢ÖVF–÷&Vc¢—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’À¢7F'E÷6×ÆS¢F&vWE÷6×ÆRÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢—FVÒç6÷W&6Uööfg6WE÷6×ÆW2‚’À¢ÆVæwF…÷6×ÆW3¢—FVÒæÆVæwF…÷6×ÆW2‚’À¢Ò“°¢Ð¢ÒVÇ6R–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢bbF&vWE÷7F'E÷F–6²Ò—FVÒç7F'E÷F–6²‚¢°¢7F–öç2çW6‚„Ft7F–öã£¤VF—DÖ–F”—FVÒ°¢—FVÕö–BÀ¢7F'E÷F–6³¢F&vWE÷7F'E÷F–6²À¢ÆVæwF…÷F–6·3¢—FVÒæÆVæwF…÷F–6·2‚’À¢Ò“°¢Ð ¢–bF&vWE÷G&6µö–BÒ6÷W&6U÷G&6µö–B°¢7F–öç2çW6‚„Ft7F–öã£¤Ö÷fT—FVÕFõG&6²°¢—FVÕö–BÀ¢G&6µö–C¢F&vWE÷G&6µö–BÀ¢Ò“°¢Ð¢Ð¢–b7F–öç2æ—5öV×G’‚’°¢&WGW&âö²„æöæR“°¢Ð¢ö²…6öÖR„Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ò’¢Ð ¢fâFVÆWFU÷G&6²‚f×WB6VÆbÂG&6µö–C¢G&6´–B’°¢–b6VÆ`¢çG&6µöÖ—…övW7GW&P¢æ—5÷6öÖUöæB‡ÆvW7GW&WÂvW7GW&RçG&6µö–BÓÒG&6µö–B¢°¢6VÆbæ6æ6VÅ÷G&6µöÖ—…övW7GW&R‚“°¢Ð¢6VÆbæ6ÆV%÷G&6µöG&gG2‡G&6µö–B“°¢6VÆbæÇ•ö7F–öâ„Ft7F–öã£¤FVÆWFUG&6²²G&6µö–BÒÂ%G&6²FVÆWFVB"“°¢Ð ¢fâFVÆWFUö'•öfö7W2‚f×WB6VÆbÂæõ÷&ö×F–æs¢&ööÂ’ÓâF6³ÄÖW76vSâ°¢ÖF6‚6VÆbçF–ÖVÆ–æRæFVÆWFUöfö7W2°¢F–ÖVÆ–æS£¤FVÆWFTfö7W5F&vWC£¤'&ævRÓâ6VÆbæFVÆWFU÷6VÆV7FVEö—FV×2‚’À¢F–ÖVÆ–æS£¤FVÆWFTfö7W5F&vWC£¤WFöÖF–öâÓâ°¢–bÆWB6öÖR†WfVçB’Ò6VÆbçF–ÖVÆ–æRç6VÆV7FVEöWFöÖF–öå÷ö–çEöFVÆWFUöWfVçB‚’°¢&WGW&â6VÆbçWFFR„ÖW76vS£¥F–ÖVÆ–æR†WfVçB’“°¢Ð¢Ð¢F–ÖVÆ–æS£¤FVÆWFTfö7W5F&vWC£¥G&6µæVÂÓâ°¢ÆWB6VÆV7FVE÷G&6µö–G2Ò6VÆbç6VÆV7FVE÷G&6µö–G2‚“°¢–b6VÆV7FVE÷G&6µö–G2æ—5öV×G’‚’°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢–bæõ÷&ö×F–ær°¢6VÆbæFVÆWFU÷6VÆV7FVE÷G&6·2‡6VÆV7FVE÷G&6µö–G2“°¢ÒVÇ6R°¢6VÆbçVæF–æuöfö7W6VE÷G&6µöFVÆWF–öâÒ6öÖR‡6VÆV7FVE÷G&6µö–G2“°¢Ð¢Ð¢Ð¢F6³£¦æöæR‚¢Ð ¢fâ6öæf—&Õöfö7W6VE÷G&6µöFVÆWF–öâ‚f×WB6VÆb’°¢–bÆWB6öÖR‡G&6µö–G2’Ò6VÆbçVæF–æuöfö7W6VE÷G&6µöFVÆWF–öâçF¶R‚’°¢6VÆbæFVÆWFU÷6VÆV7FVE÷G&6·2‡G&6µö–G2“°¢Ð¢Ð ¢fâFVÆWFU÷6VÆV7FVE÷G&6·2‚f×WB6VÆbÂG&6µö–G3¢fV3ÅG&6´–Câ’°¢–b6VÆbç&ö¦V7Eöw&…öVF—Eö'W7’‚’°¢6VÆbç7FGW2Ò%7F÷Æ–&6²÷"&V6÷&F–ær&Vf÷&RFVÆWF–ærG&6·2"æ–çFò‚“°¢&WGW&ã°¢Ð¢ÆWB6VÆV7FVBÒG&6µö–G0¢æ–çFõö—FW"‚¢æf–ÇFW"‡Æ–GÂ6VÆbç&ö¦V7BçG&6·2‚’æ—FW"‚’æç’‡ÇG&6·ÂG&6²æ–B‚’ÓÒ¦–B’¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢–b6VÆV7FVBæ—5öV×G’‚’°¢&WGW&ã°¢Ð ¢ÆWB×WB&VÖ–æ–ærÒ6VÆV7FVBæ6ÆöæR‚“°¢ÆWB×WB÷&FW&VBÒfV3£§v—F…ö66—G’‡&VÖ–æ–æræÆVâ‚’“°¢v†–ÆR&VÖ–æ–æræ—5öV×G’‚’°¢ÆWBæW‡BÒ6VÆbç&ö¦V7BçG&6·2‚’æ—FW"‚’æf–æEöÖ‡Æ6æF–FFWÂ°¢‡&VÖ–æ–æræ6öçF–ç2‚f6æF–FFRæ–B‚’¢bb6VÆbç&ö¦V7BçG&6·2‚’æ—FW"‚’æç’‡ÆFWVæFVçGÂ°¢FWVæFVçBæ–B‚’Ò6æF–FFRæ–B‚¢bb&VÖ–æ–æræ6öçF–ç2‚fFWVæFVçBæ–B‚’¢bb†FWVæFVçBç&VçE÷G&6²‚’ÓÒ6öÖR†6æF–FFRæ–B‚’¢ÇÂFWVæFVçBæ÷WGWE÷G&6²‚’ÓÒ6öÖR†6æF–FFRæ–B‚’¢ÇÂFWVæFVç@¢ç6VæG2‚¢æ—FW"‚¢æç’‡Ç6VæGÂ6VæBæFW7F–æF–öâ‚’ÓÒ6æF–FFRæ–B‚’’¢Ò’¢çF†Vå÷6öÖR†6æF–FFRæ–B‚’¢Ò“°¢ÆWB6öÖR‡G&6µö–B’ÒæW‡BVÇ6R°¢6VÆbç7FGW2Ò%6VÆV7FVBG&6·2†fR7–6Æ–2FVÆWF–öâFWVæFVæ7’"æ–çFò‚“°¢&WGW&ã°¢Ó°¢&VÖ–æ–ærç&VÖ÷fR‚gG&6µö–B“°¢÷&FW&VBçW6‚‡G&6µö–B“°¢Ð ¢–b6VÆ`¢çG&6µöÖ—…övW7GW&P¢æ—5÷6öÖUöæB‡ÆvW7GW&WÂ6VÆV7FVBæ6öçF–ç2‚fvW7GW&RçG&6µö–B’¢°¢6VÆbæ6æ6VÅ÷G&6µöÖ—…övW7GW&R‚“°¢Ð¢ÆWB&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢ÆWB7F–öç2Ò÷&FW&V@¢æ—FW"‚¢æ6÷–VB‚¢æÖ‡ÇG&6µö–GÂFt7F–öã£¤FVÆWFUG&6²²G&6µö–BÒ¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB7F–öâÒ–b7F–öç2æÆVâ‚’ÓÒ°¢7F–öç2æ–çFõö—FW"‚’ææW‡B‚’æW‡V7B‚&öæR6VÆV7FVBG&6²"¢ÒVÇ6R°¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ð¢Ó°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ%6VÆV7FVBG&6·2FVÆWFVB"“°¢–b6VÆbç&Wf—6–öâÒ&Wf—6–öâ°¢f÷"G&6µö–B–â6VÆV7FVB°¢6VÆbæ6ÆV%÷G&6µöG&gG2‡G&6µö–B“°¢Ð¢Ð¢Ð ¢fâ6öÖÖ—E÷G&6µöæÖR‚f×WB6VÆbÂG&6µö–C¢G&6´–B’°¢ÆWB6öÖR†æÖR’Ò6VÆbçG&6µöæÖUöVF—G2ævWB‚gG&6µö–B’æ6ÆöæVB‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBæÖRÒÖF6‚'6U÷G&6µöæÖUöG&gB‚fæÖR’°¢ö²†æÖR’ÓâæÖRÀ¢W'"†W'&÷"’Óâ°¢6VÆbçG&6µöG&gEöW'&÷'0¢æ–ç6W'B‚‡G&6µö–BÂG&6´G&gDf–VÆC£¤æÖR’ÂW'&÷"çFõö÷væVB‚’“°¢6VÆbç7FGW2ÒW'&÷"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB6öÖR†7W'&VçEöæÖR’Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢æÖ‡ÇG&6·ÂG&6²ææÖR‚’çFõö÷væVB‚’¢VÇ6R°¢6VÆbçG&6µöæÖUöVF—G2ç&VÖ÷fR‚gG&6µö–B“°¢6VÆbæ6ÆV%÷G&6µöG&gB‡G&6µö–BÂG&6´G&gDf–VÆC£¤æÖR“°¢6VÆbç7FGW2Ò%G&6²æòÆöævW"W†—7G2"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢6VÆbçG&6µöæÖUöVF—G2ç&VÖ÷fR‚gG&6µö–B“°¢6VÆbæ6ÆV%÷G&6µöG&gB‡G&6µö–BÂG&6´G&gDf–VÆC£¤æÖR“°¢–bæÖRÓÒ7W'&VçEöæÖR°¢6VÆbç7FGW2Ò%G&6²æÖRVæ6†ævVB"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢6VÆbæÇ•ö7F–öâ„Ft7F–öã£¥6WEG&6´æÖR²G&6µö–BÂæÖRÒÂ%G&6²&VæÖVB"“°¢Ð ¢fâÖ÷fU÷G&6²‚f×WB6VÆbÂG&6µö–C¢G&6´–BÂF—&V7F–öã¢“‚’°¢ÆWB6öÖR†–æFW‚’Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢ç÷6—F–öâ‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢VÇ6R°¢6VÆbç7FGW2Ò%G&6²æòÆöævW"W†—7G2"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWBF&vWEö–æFW‚ÒÖF6‚F—&V7F–öâ°¢ÓÓâƒâæ–æFW‚’ç&Wb‚’æf–æB‡Æ6æF–FFWÂ°¢6VÆbç&ö¦V7@¢æföÆFW%öFWF‚‡6VÆbç&ö¦V7BçG&6·2‚•²¦6æF–FFUÒæ–B‚’¢ÃÒ6VÆbç&ö¦V7BæföÆFW%öFWF‚‡G&6µö–B¢Ò’À¢Óâ6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢ævWB†–æFW‚²6VÆbç&ö¦V7BçG&6µ÷7V'G&VUöÆVâ‡G&6µö–B’¢æÖ‡ÆæW‡GÂ–æFW‚²6VÆbç&ö¦V7BçG&6µ÷7V'G&VUöÆVâ†æW‡Bæ–B‚’’’À¢òÓâæöæRÀ¢Ó°¢ÆWB6öÖR‡F&vWEö–æFW‚’ÒF&vWEö–æFW‚VÇ6R°¢6VÆbç7FGW2Ò%G&6²—2Ç&VG’BF†BVæBöbF†RÆ—7B"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¤Ö÷fUG&6²°¢G&6µö–BÀ¢–æFWƒ¢F&vWEö–æFW‚À¢ÒÀ¢%G&6²÷&FW"6†ævVB"À¢“°¢Ð ¢fâçVFvUöVF–õö—FVÒ‚f×WB6VÆbÂ—FVÕö–C¢—FVÔ–BÂF—&V7F–öã¢“‚ÂÖ–ÆÆ—6V6öæG3¢S3"’°¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2ç&VÖ÷fR‚f—FVÕö–B“°¢ÆWB6öÖR‚†ÖVF–÷&VbÂ6÷W&6Uööfg6WE÷6×ÆW2ÂÆVæwF…÷6×ÆW2Â7F'E÷6×ÆR’’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢æÖ‡Æ—FV×Â°¢€¢—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’À¢—FVÒç6÷W&6Uööfg6WE÷6×ÆW2‚’À¢—FVÒæÆVæwF…÷6×ÆW2‚’À¢—FVÒç7F'E÷6×ÆR‚’À¢¢Ò¢VÇ6R°¢6VÆbç7FGW2Ò$VF–ò—FVÒæòÆöævW"W†—7G2"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWBFVÇFÒ‡ScC£¦g&öÒ‡6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’’¢ScC£¦g&öÒ†Ö–ÆÆ—6V6öæG2¢òó¢æÖ‚ƒ“°¢ÆWBÖ÷fVE÷6×ÆRÒÖF6‚F—&V7F–öâ°¢ÓÓâ7F'E÷6×ÆRæ6†V6¶VE÷7V"†FVÇF’À¢Óâ7F'E÷6×ÆRæ6†V6¶VEöFB†FVÇF’À¢òÓâæöæRÀ¢Ó°¢ÆWB6öÖR‡7F'E÷6×ÆR’ÒÖ÷fVE÷6×ÆRVÇ6R°¢6VÆbç7FGW2Ò$VF–ò—FVÒ6ææ÷BÖ÷fR&W–öæBF†R6×ÆRF–ÖVÆ–æR"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–BÀ¢ÖVF–÷&VbÀ¢7F'E÷6×ÆRÀ¢6÷W&6Uööfg6WE÷6×ÆW2À¢ÆVæwF…÷6×ÆW2À¢ÒÀ¢ff÷&ÖB‚$VF–ò—FVÒÖ÷fVB'’¶Ö–ÆÆ—6V6öæG7Ò×2"’À¢“°¢Ð ¢fâ6öÖÖ—EöVF–õö—FVÕ÷7F'E÷6×ÆR‚f×WB6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢ÆWB6öÖR‡VW'’’Ò6VÆbæVF–õö—FVÕ÷7F'EöVF—G2ævWB‚f—FVÕö–B’æ6ÆöæVB‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB7F'E÷6×ÆRÒÖF6‚VW'’çG&–Ò‚’ç'6S££ÇScCâ‚’°¢ö²‡6×ÆR’Óâ6×ÆRÀ¢W'"…ò’Óâ°¢6VÆbç7FGW2Ò$VçFW"æöâÖæVvF—fR6×ÆR÷6—F–öâ"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢Ó°¢–b6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢æ—5÷6öÖUöæB‡Æ—FV×Â—FVÒç7F'E÷6×ÆR‚’ÓÒ7F'E÷6×ÆR¢°¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2ç&VÖ÷fR‚f—FVÕö–B“°¢6VÆbç7FGW2Ò$VF–ò—FVÒ÷6—F–öâ—2Væ6†ævVB"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2ç&VÖ÷fR‚f—FVÕö–B“°¢ÆWB7F–öâÒ6WEöVF–õö—FVÕ÷7F'E÷6×ÆR‚g6VÆbç&ö¦V7BÂ—FVÕö–BÂ7F'E÷6×ÆR“°¢6VÆbæÇ•öVF—B†7F–öâÂ$VF–ò—FVÒÖ÷fVBFòW†7B6×ÆR÷6—F–öâ"“°¢Ð ¢fâ6å÷VæFõö7F–öâ‚g6VÆb’Óâ&ööÂ°¢6VÆbæ÷6VÆV7F–öåö—5öæW‡E÷VæFò‚’ÇÂ6VÆbç&ö¦V7Bæ6å÷VæFò‚¢Ð ¢fâ6å÷&VFõö7F–öâ‚g6VÆb’Óâ&ööÂ°¢6VÆbæ÷6VÆV7F–öåö—5öæW‡E÷&VFò‚’ÇÂ6VÆbç&ö¦V7Bæ6å÷&VFò‚¢Ð ¢fâ÷6VÆV7F–öåö—5öæW‡E÷VæFò‚g6VÆb’Óâ&ööÂ°¢6VÆbæ—FVÕ÷6VÆV7F–öåö—5öæW‡E÷VæFò‚’ÇÂ6VÆbæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö—5öæW‡E÷VæFò‚¢Ð ¢fâ÷6VÆV7F–öåö—5öæW‡E÷&VFò‚g6VÆb’Óâ&ööÂ°¢6VÆbæ—FVÕ÷6VÆV7F–öåö—5öæW‡E÷&VFò‚’ÇÂ6VÆbæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö—5öæW‡E÷&VFò‚¢Ð ¢fâ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö—5öæW‡E÷VæFò‚g6VÆb’Óâ&ööÂ°¢ÖF6†W2€¢6VÆbæ÷VæFõö†—7F÷'’æÆ7B‚’À¢6öÖR„†—7F÷'”VçG'“£¤–æÆ–æTÖ–F•6VÆV7F–öâ††—7F÷'’’¢–b†—7F÷'’ç&ö¦V7Eö†—7F÷'•ö7W'6÷"ÓÒ6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚¢¢Ð ¢fâ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö—5öæW‡E÷&VFò‚g6VÆb’Óâ&ööÂ°¢ÖF6†W2€¢6VÆbæ÷&VFõö†—7F÷'’æÆ7B‚’À¢6öÖR„†—7F÷'”VçG'“£¤–æÆ–æTÖ–F•6VÆV7F–öâ††—7F÷'’’¢–b†—7F÷'’ç&ö¦V7Eö†—7F÷'•ö7W'6÷"ÓÒ6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚¢¢Ð ¢fâ–æÆ–æUöÖ–F•ö6÷•÷6VÆV7F–öåö—5öæW‡E÷VæFò‚g6VÆb’Óâ÷F–öãÄ–æÆ–æTÖ–F•6VÆV7F–öä†—7F÷'“â°¢ÖF6‚6VÆbæ÷VæFõö†—7F÷'’æÆ7B‚’°¢6öÖR„†—7F÷'”VçG'“£¥&ö¦V7Ev—F„–æÆ–æTÖ–F•6VÆV7F–öâ††—7F÷'’’¢–b†—7F÷'’ç&ö¦V7Eö†—7F÷'•ö7W'6÷"ÓÒ6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’Óà¢°¢6öÖR††—7F÷'’æ6ÆöæR‚’¢Ð¢òÓâæöæRÀ¢Ð¢Ð ¢fâ&ö¦V7Eö—FVÕ÷6VÆV7F–öåö—5öæW‡E÷VæFò‚g6VÆb’Óâ÷F–öãÄ—FVÕ6VÆV7F–öä†—7F÷'“â°¢ÖF6‚6VÆbæ÷VæFõö†—7F÷'’æÆ7B‚’°¢6öÖR„†—7F÷'”VçG'“£¥&ö¦V7Ev—F„—FVÕ6VÆV7F–öâ††—7F÷'’’¢–b†—7F÷'’ç&ö¦V7Eö†—7F÷'•ö7W'6÷%ögFW"ÓÒ6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’Óà¢°¢6öÖR††—7F÷'’æ6ÆöæR‚’¢Ð¢òÓâæöæRÀ¢Ð¢Ð ¢fâ&ö¦V7Eö—FVÕ÷6VÆV7F–öåö—5öæW‡E÷&VFò‚g6VÆb’Óâ÷F–öãÄ—FVÕ6VÆV7F–öä†—7F÷'“â°¢ÖF6‚6VÆbæ÷&VFõö†—7F÷'’æÆ7B‚’°¢6öÖR„†—7F÷'”VçG'“£¥&ö¦V7Ev—F„—FVÕ6VÆV7F–öâ††—7F÷'’’¢–b†—7F÷'’ç&ö¦V7Eö†—7F÷'•ö7W'6÷%ö&Vf÷&RÓÒ6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’Óà¢°¢6öÖR††—7F÷'’æ6ÆöæR‚’¢Ð¢òÓâæöæRÀ¢Ð¢Ð ¢fâ&W7F÷&U÷&ö¦V7Eö—FVÕ÷6VÆV7F–öåö†—7F÷'’€¢f×WB6VÆbÀ¢†—7F÷'“¢d—FVÕ6VÆV7F–öä†—7F÷'’À¢&VFó¢&ööÂÀ¢’°¢6VÆbçF–ÖVÆ–æRç&W7F÷&Uö—FVÕ÷6VÆV7F–öå÷7FFR†–b&VFò°¢f†—7F÷'’ægFW ¢ÒVÇ6R°¢f†—7F÷'’æ&Vf÷&P¢Ò“°¢Ð ¢fâ&W7F÷&Uö–æÆ–æUöÖ–F•÷6VÆV7F–öåö†—7F÷'’€¢f×WB6VÆbÀ¢†—7F÷'“¢d–æÆ–æTÖ–F•6VÆV7F–öä†—7F÷'’À¢&VFó¢&ööÂÀ¢’°¢ÆWB6VÆV7F–öâÒ–b&VFò°¢f†—7F÷'’ægFW ¢ÒVÇ6R°¢f†—7F÷'’æ&Vf÷&P¢Ó°¢–b6VÆV7F–öâæ—5öV×G’‚’°¢6VÆbçF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢ç&VÖ÷fR‚f†—7F÷'’æ—FVÕö–B“°¢ÒVÇ6R°¢6VÆbçF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢æ–ç6W'B††—7F÷'’æ—FVÕö–BÂ6VÆV7F–öâæ6ÆöæR‚’“°¢Ð ¢–b6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BÓÒ6öÖR††—7F÷'’æ—FVÕö–B’°¢ÆWBVF—F÷%÷6VÆV7F–öâÒ–b&VFò°¢f†—7F÷'’æVF—F÷%ögFW ¢ÒVÇ6R°¢f†—7F÷'’æVF—F÷%ö&Vf÷&P¢Ó°¢–bÆWB6öÖR†VF—F÷%÷6VÆV7F–öâ’ÒVF—F÷%÷6VÆV7F–öâ°¢6VÆbç&W7F÷&UöÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR†VF—F÷%÷6VÆV7F–öâ“°¢Ð¢Ð¢Ð ¢fâÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR‚g6VÆb’ÓâÖ–F”VF—F÷%6VÆV7F–öå7FFR°¢Ö–F”VF—F÷%6VÆV7F–öå7FFR°¢æ÷FW3¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6ÆöæR‚’À¢6öçG&öÆÆW'3¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6ÆöæR‚’À¢—F6…ö&VæG3¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6ÆöæR‚’À¢&öw&Õö6†ævW3¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6ÆöæR‚’À¢öÇ•ögFW'F÷V6†W3¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ6ÆöæR‚’À¢6†ææVÅ÷&W77W&W3¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6ÆöæR‚’À¢÷VUöWfVçG3¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2æ6ÆöæR‚’À¢WfVçEöÆ—7Eö7W'6÷#¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eö7W'6÷"À¢WfVçEöÆ—7Eöæ6†÷#¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöæ6†÷"À¢Ð¢Ð ¢fâ&W7F÷&UöÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR‚f×WB6VÆbÂ6VÆV7F–öã¢dÖ–F”VF—F÷%6VÆV7F–öå7FFR’°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò6VÆV7F–öâææ÷FW2æ6ÆöæR‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2Ò6VÆV7F–öâæ6öçG&öÆÆW'2æ6ÆöæR‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2Ò6VÆV7F–öâç—F6…ö&VæG2æ6ÆöæR‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2Ò6VÆV7F–öâç&öw&Õö6†ævW2æ6ÆöæR‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2Ò6VÆV7F–öâçöÇ•ögFW'F÷V6†W2æ6ÆöæR‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2Ò6VÆV7F–öâæ6†ææVÅ÷&W77W&W2æ6ÆöæR‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2Ò6VÆV7F–öâæ÷VUöWfVçG2æ6ÆöæR‚“°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eö7W'6÷"Ò6VÆV7F–öâæWfVçEöÆ—7Eö7W'6÷#°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöæ6†÷"Ò6VÆV7F–öâæWfVçEöÆ—7Eöæ6†÷#°¢Ð ¢fâÖ–F•öVF—F÷%÷6VÆV7FVEöWfVçG5ö6†ævVB€¢&Vf÷&S¢dÖ–F”VF—F÷%6VÆV7F–öå7FFRÀ¢gFW#¢dÖ–F”VF—F÷%6VÆV7F–öå7FFRÀ¢’Óâ&ööÂ°¢&Vf÷&Rææ÷FW2ÒgFW"ææ÷FW0¢ÇÂ&Vf÷&Ræ6öçG&öÆÆW'2ÒgFW"æ6öçG&öÆÆW'0¢ÇÂ&Vf÷&Rç—F6…ö&VæG2ÒgFW"ç—F6…ö&VæG0¢ÇÂ&Vf÷&Rç&öw&Õö6†ævW2ÒgFW"ç&öw&Õö6†ævW0¢ÇÂ&Vf÷&RçöÇ•ögFW'F÷V6†W2ÒgFW"çöÇ•ögFW'F÷V6†W0¢ÇÂ&Vf÷&Ræ6†ææVÅ÷&W77W&W2ÒgFW"æ6†ææVÅ÷&W77W&W0¢ÇÂ&Vf÷&Ræ÷VUöWfVçG2ÒgFW"æ÷VUöWfVçG0¢Ð ¢fâ&V6÷&EöÖ–F•öVF—F÷%÷6VÆV7F–öåö†—7F÷'’€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢&Vf÷&Uö–æÆ–æS¢†6…6WCÄæ÷FT–CâÀ¢&Vf÷&UöVF—F÷#¢Ö–F”VF—F÷%6VÆV7F–öå7FFRÀ¢’°¢–b6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ—5öV×G’‚’°¢6VÆbçF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öâç&VÖ÷fR‚f—FVÕö–B“°¢ÒVÇ6R°¢6VÆbçF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢æ–ç6W'B†—FVÕö–BÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6ÆöæR‚’“°¢Ð¢ÆWBgFW%ö–æÆ–æRÒ6VÆ`¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢ævWB‚f—FVÕö–B¢æ6ÆöæVB‚¢çVçw&ö÷%öFVfVÇB‚“°¢ÆWBgFW%öVF—F÷"Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR‚“°¢–b&Vf÷&Uö–æÆ–æRÓÒgFW%ö–æÆ–æRbb&Vf÷&UöVF—F÷"ÓÒgFW%öVF—F÷"°¢&WGW&ã°¢Ð ¢6VÆbç&ö¦V7BæF—66&E÷&VFõö†—7F÷'’‚“°¢6VÆbæ÷&VFõö†—7F÷'’æ6ÆV"‚“°¢6VÆbæ÷VæFõö†—7F÷'¢çW6‚„†—7F÷'”VçG'“£¤–æÆ–æTÖ–F•6VÆV7F–öâ€¢–æÆ–æTÖ–F•6VÆV7F–öä†—7F÷'’°¢—FVÕö–BÀ¢&Vf÷&S¢&Vf÷&Uö–æÆ–æRÀ¢gFW#¢gFW%ö–æÆ–æRÀ¢VF—F÷%ö&Vf÷&S¢6öÖR†&Vf÷&UöVF—F÷"’À¢VF—F÷%ögFW#¢6öÖR†gFW%öVF—F÷"’À¢&ö¦V7Eö†—7F÷'•ö7W'6÷#¢6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’À¢ÒÀ¢’“°¢Ð ¢fâÖ÷fUöÖ–F•öVF—F÷%öWfVçEöÆ—7Eö7W'6÷%÷v—F…ö†—7F÷'’€¢f×WB6VÆbÀ¢F—&V7F–öã¢Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷$F—&V7F–öâÀ¢&W6W'fU÷6VÆV7F–öã¢&ööÂÀ¢W‡FVæE÷6VÆV7F–öã¢&ööÂÀ¢’°¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢6VÆbæÖ÷fUöÖ–F•öVF—F÷%öWfVçEöÆ—7Eö7W'6÷"€¢F—&V7F–öâÀ¢&W6W'fU÷6VÆV7F–öâÀ¢W‡FVæE÷6VÆV7F–öâÀ¢“°¢&WGW&ã°¢Ó°¢ÆWB&Vf÷&UöVF—F÷"Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR‚“°¢ÆWB&Vf÷&Uö–æÆ–æRÒ6VÆ`¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢ævWB‚f—FVÕö–B¢æ6ÆöæVB‚¢çVçw&ö÷%öFVfVÇB‚“°¢6VÆbæÖ÷fUöÖ–F•öVF—F÷%öWfVçEöÆ—7Eö7W'6÷"†F—&V7F–öâÂ&W6W'fU÷6VÆV7F–öâÂW‡FVæE÷6VÆV7F–öâ“°¢ÆWBgFW%öVF—F÷"Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR‚“°¢–b6VÆc£¦Ö–F•öVF—F÷%÷6VÆV7FVEöWfVçG5ö6†ævVB‚f&Vf÷&UöVF—F÷"ÂfgFW%öVF—F÷"’°¢6VÆbç&V6÷&EöÖ–F•öVF—F÷%÷6VÆV7F–öåö†—7F÷'’†—FVÕö–BÂ&Vf÷&Uö–æÆ–æRÂ&Vf÷&UöVF—F÷"“°¢Ð¢Ð ¢fâ—FVÕ÷6VÆV7F–öåö—5öæW‡E÷VæFò‚g6VÆb’Óâ&ööÂ°¢ÖF6†W2€¢6VÆbæ÷VæFõö†—7F÷'’æÆ7B‚’À¢6öÖR„†—7F÷'”VçG'“£¤—FVÕ6VÆV7F–öâ°¢&ö¦V7Eö†—7F÷'•ö7W'6÷"À¢âà¢Ò’–b§&ö¦V7Eö†—7F÷'•ö7W'6÷"ÓÒ6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚¢¢Ð ¢fâ—FVÕ÷6VÆV7F–öåö—5öæW‡E÷&VFò‚g6VÆb’Óâ&ööÂ°¢ÖF6†W2€¢6VÆbæ÷&VFõö†—7F÷'’æÆ7B‚’À¢6öÖR„†—7F÷'”VçG'“£¤—FVÕ6VÆV7F–öâ°¢&ö¦V7Eö†—7F÷'•ö7W'6÷"À¢âà¢Ò’–b§&ö¦V7Eö†—7F÷'•ö7W'6÷"ÓÒ6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚¢¢Ð ¢fâ6ÆV%öö†—7F÷'’‚f×WB6VÆb’°¢6VÆbæ÷VæFõö†—7F÷'’æ6ÆV"‚“°¢6VÆbæ÷&VFõö†—7F÷'’æ6ÆV"‚“°¢Ð ¢fâv—F…ö—FVÕ÷6VÆV7F–öåö†—7F÷'’‚f×WB6VÆbÂÆ&VÃ¢bw7FF–27G"Â7F–öã¢–×Âfäöæ6R‚f×WB6VÆb’’°¢ÆWB&Vf÷&RÒ6VÆbçF–ÖVÆ–æRæ—FVÕ÷6VÆV7F–öå÷7FFR‚“°¢7F–öâ‡6VÆb“°¢ÆWBgFW"Ò6VÆbçF–ÖVÆ–æRæ—FVÕ÷6VÆV7F–öå÷7FFR‚“°¢–b&Vf÷&RÓÒgFW"°¢&WGW&ã°¢Ð ¢6VÆbç&ö¦V7BæF—66&E÷&VFõö†—7F÷'’‚“°¢6VÆbæ÷&VFõö†—7F÷'’æ6ÆV"‚“°¢6VÆbæ÷VæFõö†—7F÷'’çW6‚„†—7F÷'”VçG'“£¤—FVÕ6VÆV7F–öâ°¢Æ&VÂÀ¢&Vf÷&S¢&÷ƒ£¦æWr†&Vf÷&R’À¢gFW#¢&÷ƒ£¦æWr†gFW"’À¢&ö¦V7Eö†—7F÷'•ö7W'6÷#¢6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’À¢Ò“°¢Ð ¢fâv—F…ö–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö†—7F÷'’€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢æ÷FUö–G3¢fV3ÆFuö6÷&S£¤æ÷FT–CâÀ¢’°¢ÆWB&Vf÷&RÒ6VÆ`¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢ævWB‚f—FVÕö–B¢æ6ÆöæVB‚¢çVçw&ö÷%öFVfVÇB‚“°¢ÆWBVF—F÷%ö&Vf÷&RÐ¢‡6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BÓÒ6öÖR†—FVÕö–B’’çF†Vâ‡ÇÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR‚’“° ¢6VÆbæ†æFÆU÷F–ÖVÆ–æU÷f–WuöWfVçB‡F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6WD–æÆ–æTÖ–F”æ÷FU6VÆV7F–öâ°¢—FVÕö–BÀ¢æ÷FUö–G2À¢Ò“° ¢ÆWBgFW"Ò6VÆ`¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢ævWB‚f—FVÕö–B¢æ6ÆöæVB‚¢çVçw&ö÷%öFVfVÇB‚“°¢ÆWBVF—F÷%ögFW"ÒVF—F÷%ö&Vf÷&Ræ5÷&Vb‚’æÖ‡Æ&Vf÷&WÂ°¢ÆWB×WBVF—F÷%ögFW"Ò&Vf÷&Ræ6ÆöæR‚“°¢VF—F÷%ögFW"ææ÷FW2ÒgFW"æ6ÆöæR‚“°¢VF—F÷%ögFW ¢Ò“°¢–bÆWB6öÖR†VF—F÷%ögFW"’ÒVF—F÷%ögFW"æ5÷&Vb‚’°¢6VÆbç&W7F÷&UöÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR†VF—F÷%ögFW"“°¢Ð¢–b&Vf÷&RÓÒgFW"bbVF—F÷%ö&Vf÷&RÓÒVF—F÷%ögFW"°¢&WGW&ã°¢Ð ¢6VÆbç&ö¦V7BæF—66&E÷&VFõö†—7F÷'’‚“°¢6VÆbæ÷&VFõö†—7F÷'’æ6ÆV"‚“°¢6VÆbæ÷VæFõö†—7F÷'¢çW6‚„†—7F÷'”VçG'“£¤–æÆ–æTÖ–F•6VÆV7F–öâ€¢–æÆ–æTÖ–F•6VÆV7F–öä†—7F÷'’°¢—FVÕö–BÀ¢&Vf÷&RÀ¢gFW"À¢VF—F÷%ö&Vf÷&RÀ¢VF—F÷%ögFW"À¢&ö¦V7Eö†—7F÷'•ö7W'6÷#¢6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’À¢ÒÀ¢’“°¢Ð ¢fâv—F…öÖ–F•öVF—F÷%öæ÷FU÷6VÆV7F–öåö†—7F÷'’‚f×WB6VÆbÂæ÷FUö–G3¢†6…6WCÆFuö6÷&S£¤æ÷FT–Câ’°¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢6VÆbç6VÆV7EöÖ–F•öæ÷FW2†æ÷FUö–G2“°¢&WGW&ã°¢Ó°¢ÆWB&Vf÷&RÒ6VÆ`¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢ævWB‚f—FVÕö–B¢æ6ÆöæVB‚¢çVçw&ö÷%öFVfVÇB‚“°¢ÆWBVF—F÷%ö&Vf÷&RÒ6VÆbæÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR‚“° ¢6VÆbç6VÆV7EöÖ–F•öæ÷FW2†æ÷FUö–G2“°¢–b6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ—5öV×G’‚’°¢6VÆbçF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öâç&VÖ÷fR‚f—FVÕö–B“°¢ÒVÇ6R°¢6VÆbçF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢æ–ç6W'B†—FVÕö–BÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6ÆöæR‚’“°¢Ð ¢ÆWBgFW"Ò6VÆ`¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢ævWB‚f—FVÕö–B¢æ6ÆöæVB‚¢çVçw&ö÷%öFVfVÇB‚“°¢ÆWBVF—F÷%ögFW"Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR‚“°¢–b&Vf÷&RÓÒgFW"bbVF—F÷%ö&Vf÷&RÓÒVF—F÷%ögFW"°¢&WGW&ã°¢Ð ¢6VÆbç&ö¦V7BæF—66&E÷&VFõö†—7F÷'’‚“°¢6VÆbæ÷&VFõö†—7F÷'’æ6ÆV"‚“°¢6VÆbæ÷VæFõö†—7F÷'¢çW6‚„†—7F÷'”VçG'“£¤–æÆ–æTÖ–F•6VÆV7F–öâ€¢–æÆ–æTÖ–F•6VÆV7F–öä†—7F÷'’°¢—FVÕö–BÀ¢&Vf÷&RÀ¢gFW"À¢VF—F÷%ö&Vf÷&S¢6öÖR†VF—F÷%ö&Vf÷&R’À¢VF—F÷%ögFW#¢6öÖR†VF—F÷%ögFW"’À¢&ö¦V7Eö†—7F÷'•ö7W'6÷#¢6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’À¢ÒÀ¢’“°¢Ð ¢fâv—F…öÖ–F•öVF—F÷%öWfVçE÷6VÆV7F–öåö†—7F÷'’€¢f×WB6VÆbÀ¢7W'6÷#¢Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷"À¢FF—F—fS¢&ööÂÀ¢’ÓâF6³ÄÖW76vSâ°¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢&WGW&âF6³£¦æöæR‚“°¢Ó°¢ÆWB&Vf÷&UöVF—F÷"Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR‚“°¢ÆWB&Vf÷&Uö–æÆ–æRÒ6VÆ`¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢ævWB‚f—FVÕö–B¢æ6ÆöæVB‚¢çVçw&ö÷%öFVfVÇB‚“°¢ÆWBÖW76vRÒÖF6‚7W'6÷"°¢Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¤æ÷FR†æ÷FUö–B’Óâ°¢ÖW76vS£¥6VÆV7DÖ–F”æ÷FTWfVçB†æ÷FUö–BÂFF—F—fR¢Ð¢Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¤6öçG&öÆÆW"†WfVçB’Óâ°¢ÖW76vS£¥6VÆV7DÖ–F”6öçG&öÆÆW$WfVçB†—FVÕö–BÂWfVçBÂFF—F—fR¢Ð¢Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¤&æµ6VÆV7B†×6"ÂÇ6"’Óâ°¢ÖW76vS£¥6VÆV7DÖ–F”&æµ6VÆV7DWfVçB†—FVÕö–BÂ×6"ÂÇ6"ÂFF—F—fR¢Ð¢Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¥—F6„&VæB†WfVçB’Óâ°¢ÖW76vS£¥6VÆV7DÖ–F•—F6„&VæDWfVçB†—FVÕö–BÂWfVçBÂFF—F—fR¢Ð¢Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¥&öw&Ô6†ævR†WfVçB’Óâ°¢ÖW76vS£¥6VÆV7DÖ–F•&öw&Ô6†ævTWfVçB†—FVÕö–BÂWfVçBÂFF—F—fR¢Ð¢Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¥öÇ”gFW'F÷V6‚†WfVçB’Óâ°¢ÖW76vS£¥6VÆV7DÖ–F•öÇ”gFW'F÷V6„WfVçB†—FVÕö–BÂWfVçBÂFF—F—fR¢Ð¢Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¤6†ææVÅ&W77W&R†WfVçB’Óâ°¢ÖW76vS£¥6VÆV7DÖ–F”6†ææVÅ&W77W&TWfVçB†—FVÕö–BÂWfVçBÂFF—F—fR¢Ð¢Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¤÷VR†–æFW‚’Óâ°¢ÖW76vS£¥6VÆV7DÖ–F”÷VTWfVçB†—FVÕö–BÂ–æFW‚ÂFF—F—fR¢Ð¢Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¤VæBÓâÖW76vS£¥6VÆV7DÖ–F”VF—F÷$WfVçDÆ—7DVæB†FF—F—fR’À¢Ó°¢ÆWBF6²Ò6VÆbçWFFR†ÖW76vR“°¢6VÆbç&V6÷&EöÖ–F•öVF—F÷%÷6VÆV7F–öåö†—7F÷'’†—FVÕö–BÂ&Vf÷&Uö–æÆ–æRÂ&Vf÷&UöVF—F÷"“°¢F6°¢Ð ¢fâÇ•öÖ–F•öVF—F÷%öf–ÇFW%÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢÷W&F–öã¢Ö–F”VF—F÷$f–ÇFW%6VÆV7F–öä÷W&F–öâÀ¢’°¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6öÖR†ÖF6†–ær’Ò6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öWfVçE÷6VÆV7F–öâ†—FVÕö–B’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB&Vf÷&UöVF—F÷"Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7F–öå÷7FFR‚“°¢ÆWB&Vf÷&Uö–æÆ–æRÒ6VÆ`¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢ævWB‚f—FVÕö–B¢æ6ÆöæVB‚¢çVçw&ö÷%öFVfVÇB‚“° ¢ÖF6‚÷W&F–öâ°¢Ö–F”VF—F÷$f–ÇFW%6VÆV7F–öä÷W&F–öã£¥6WBÓâ°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æW‡FVæB†ÖF6†–ærææ÷FW2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'0¢æW‡FVæB†ÖF6†–æræ6öçG&öÆÆW'2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG0¢æW‡FVæB†ÖF6†–ærç—F6…ö&VæG2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW0¢æW‡FVæB†ÖF6†–ærç&öw&Õö6†ævW2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W0¢æW‡FVæB†ÖF6†–ærçöÇ•ögFW'F÷V6†W2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W0¢æW‡FVæB†ÖF6†–æræ6†ææVÅ÷&W77W&W2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æW‡FVæB†ÖF6†–æræ÷VUöWfVçG2“°¢Ð¢Ö–F”VF—F÷$f–ÇFW%6VÆV7F–öä÷W&F–öã£¤FBÓâ°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æW‡FVæB†ÖF6†–ærææ÷FW2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'0¢æW‡FVæB†ÖF6†–æræ6öçG&öÆÆW'2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG0¢æW‡FVæB†ÖF6†–ærç—F6…ö&VæG2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW0¢æW‡FVæB†ÖF6†–ærç&öw&Õö6†ævW2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W0¢æW‡FVæB†ÖF6†–ærçöÇ•ögFW'F÷V6†W2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W0¢æW‡FVæB†ÖF6†–æræ6†ææVÅ÷&W77W&W2“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æW‡FVæB†ÖF6†–æræ÷VUöWfVçG2“°¢Ð¢Ö–F”VF—F÷$f–ÇFW%6VÆV7F–öä÷W&F–öã£¥&VÖ÷fRÓâ°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW0¢ç&WF–â‡ÆWfVçGÂÖF6†–ærææ÷FW2æ6öçF–ç2†WfVçB’“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'0¢ç&WF–â‡ÆWfVçGÂÖF6†–æræ6öçG&öÆÆW'2æ6öçF–ç2†WfVçB’“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG0¢ç&WF–â‡ÆWfVçGÂÖF6†–ærç—F6…ö&VæG2æ6öçF–ç2†WfVçB’“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW0¢ç&WF–â‡ÆWfVçGÂÖF6†–ærç&öw&Õö6†ævW2æ6öçF–ç2†WfVçB’“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W0¢ç&WF–â‡ÆWfVçGÂÖF6†–ærçöÇ•ögFW'F÷V6†W2æ6öçF–ç2†WfVçB’“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W0¢ç&WF–â‡ÆWfVçGÂÖF6†–æræ6†ææVÅ÷&W77W&W2æ6öçF–ç2†WfVçB’“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢ç&WF–â‡ÆWfVçGÂÖF6†–æræ÷VUöWfVçG2æ6öçF–ç2†WfVçB’“°¢Ð¢Ð ¢6VÆbæÖ–F•öVF—F÷%öfVVF&6²ÒæöæS°¢6VÆbç&V6÷&EöÖ–F•öVF—F÷%÷6VÆV7F–öåö†—7F÷'’†—FVÕö–BÂ&Vf÷&Uö–æÆ–æRÂ&Vf÷&UöVF—F÷"“°¢Ð ¢fâ6ÆV%öÖ–F•öVF—F÷%öf–ÇFW"‚f×WB6VÆb’°¢6VÆbæÖ–F•öVF—F÷%ö6†ææVÅ÷6VÆV7F–öâÒÖ–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤ÆÄ6†ææVÇ3°¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"ÒÖ–F”VF—F÷$WfVçEG—Tf–ÇFW#£¤ÆÄWfVçG3°¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"ÒÖ–F”VF—F÷$æ÷FTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"ÒÖ–F”VF—F÷$6öçG&öÆÆW$f–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"ÒÖ–F”VF—F÷%—F6„&VæDf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"ÒÖ–F”VF—F÷%&öw&Ô6†ævTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ÒÖ–F”VF—F÷%÷6—F–öäf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåöÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…öÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…ö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUöÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öG&uö6†ææVÂÒ°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öVæ&ÆVBÒfÇ6S°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöf–ÇFW%öVæ&ÆVBÒfÇ6S°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöf–ÇFW%ö7F—fRÒfÇ6S°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2ÒSc£¤Ôƒ°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ5ö–æ—F–Æ—¦VBÒG'VS°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö–çfW'FVBÒfÇ6S°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7F–öâ‚f×WB6VÆb’°¢ÆWB†5öæ÷FW2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ—5öV×G’‚“°¢ÆWB†5ö6öçG&öÆÆW'2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ—5öV×G’‚“°¢ÆWB†5÷—F6…ö&VæG2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ—5öV×G’‚“°¢ÆWB†5÷&öw&Õö6†ævW2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ—5öV×G’‚“°¢ÆWB†5÷öÇ•ögFW'F÷V6†W2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ—5öV×G’‚“°¢ÆWB†5ö6†ææVÅ÷&W77W&W2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ—5öV×G’‚“°¢ÆWB†5ö÷VUöWfVçG2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2æ—5öV×G’‚“°¢ÆWB6VÆV7FVEö¶–æG2Ò°¢†5öæ÷FW2À¢†5ö6öçG&öÆÆW'2À¢†5÷—F6…ö&VæG2À¢†5÷&öw&Õö6†ævW2À¢†5÷öÇ•ögFW'F÷V6†W2À¢†5ö6†ææVÅ÷&W77W&W2À¢†5ö÷VUöWfVçG2À¢Ð¢æ–çFõö—FW"‚¢æf–ÇFW"‡Ç6VÆV7FVGÂ§6VÆV7FVB¢æ6÷VçB‚“°¢–b6VÆV7FVEö¶–æG2ÓÒ°¢&WGW&ã°¢Ð¢–b†5ö÷VUöWfVçG0¢bb6VÆV7FVEö¶–æG2Ò¢bb‡6VÆV7FVEö¶–æG2ÓÒ"bb††5ö6öçG&öÆÆW'2ÇÂ†5öæ÷FW2ÇÂ†5÷—F6…ö&VæG2’¢bb‡6VÆV7FVEö¶–æG2ÓÒ"bb†5÷&öw&Õö6†ævW2¢bb‡6VÆV7FVEö¶–æG2ÓÒ"bb†5÷öÇ•ögFW'F÷V6†W2¢bb‡6VÆV7FVEö¶–æG2ÓÒ"bb†5ö6†ææVÅ÷&W77W&W2¢bb‡6VÆV7FVEö¶–æG2ÓÒ2bb†5öæ÷FW2bb†5ö6öçG&öÆÆW'2¢bb‡6VÆV7FVEö¶–æG2ÓÒ2bb†5öæ÷FW2bb†5÷—F6…ö&VæG2¢bb‡6VÆV7FVEö¶–æG2ÓÒ2bb†5ö6öçG&öÆÆW'2bb†5÷—F6…ö&VæG2¢bb‡6VÆV7FVEö¶–æG2ÓÒ2bb†5ö6öçG&öÆÆW'2bb†5÷&öw&Õö6†ævW2¢bb‡6VÆV7FVEö¶–æG2ÓÒ2bb†5ö6öçG&öÆÆW'2bb†5ö6†ææVÅ÷&W77W&W2¢bb‡6VÆV7FVEö¶–æG2ÓÒ@¢bb†5ö6öçG&öÆÆW'0¢bb†5÷&öw&Õö6†ævW0¢bb†5ö6†ææVÅ÷&W77W&W2¢bb‡6VÆV7FVEö¶–æG2ÓÒ@¢bb†5ö6öçG&öÆÆW'0¢bb†5÷&öw&Õö6†ævW0¢bb†5÷öÇ•ögFW'F÷V6†W2¢bb‡6VÆV7FVEö¶–æG2ÓÒ2bb†5÷&öw&Õö6†ævW2bb†5ö6†ææVÅ÷&W77W&W2¢bb‡6VÆV7FVEö¶–æG2ÓÒ2bb†5÷&öw&Õö6†ævW2bb†5÷öÇ•ögFW'F÷V6†W2¢bb‡6VÆV7FVEö¶–æG2ÓÒ@¢bb†5÷&öw&Õö6†ævW0¢bb†5÷öÇ•ögFW'F÷V6†W0¢bb†5ö6†ææVÅ÷&W77W&W2¢°¢&WGW&ã°¢Ð¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢&WGW&ã°¢Ó°¢–b6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æç’‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢&WGW&ã°¢Ð ¢òò6†ææVÂÖ&V&–ærÖöæÇ’6VÆV7F–öç2FW&—fR7&—FW&–v—F‚F†RWfVçBÆ—7@¢òògVææVÂöfbâ$TU"7F—fFW2F†RgVææVÂf÷"æ÷FR¶÷VR6VÆV7F–öç0¢òòv—F†÷WBVæ&Æ–ær6†÷rÖöæÇ“²42¶÷VRÇ6òVæ&ÆW26†÷rÖöæÇ’à¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöf–ÇFW%ö7F—fRÒfÇ6S°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöf–ÇFW%öVæ&ÆVBÒfÇ6S° ¢–b6VÆV7FVEö¶–æG2â°¢–b†5ö÷VUöWfVçG2°¢–b†5öæ÷FW2bb†5ö6öçG&öÆÆW'2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEöæ÷FUö6öçG&öÆÆW%öæEö÷VU÷6VÆV7F–öâ€¢—FVÕö–BÀ¢“°¢ÒVÇ6R–b†5öæ÷FW2bb†5÷—F6…ö&VæG2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEöæ÷FU÷—F6…öæEö÷VU÷6VÆV7F–öâ†—FVÕö–B“°¢ÒVÇ6R–b†5ö6öçG&öÆÆW'2bb†5÷—F6…ö&VæG2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%÷—F6…öæEö÷VU÷6VÆV7F–öâ€¢—FVÕö–BÀ¢“°¢ÒVÇ6R–b†5ö6öçG&öÆÆW'2bb†5ö6†ææVÅ÷&W77W&W2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%ö6†ææVÅ÷&W77W&UöæEö÷VU÷6VÆV7F–öâ€¢—FVÕö–BÀ¢“°¢ÒVÇ6R–b†5ö6öçG&öÆÆW'2bb†5÷&öw&Õö6†ævW2bb†5ö6†ææVÅ÷&W77W&W2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%÷&öw&Õö6†ææVÅ÷&W77W&UöæEö÷VU÷6VÆV7F–öâ€¢—FVÕö–BÀ¢“°¢ÒVÇ6R–b†5ö6öçG&öÆÆW'2bb†5÷&öw&Õö6†ævW2bb†5÷öÇ•ögFW'F÷V6†W2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%÷&öw&Õ÷öÇ•ögFW'F÷V6…öæEö÷VU÷6VÆV7F–öâ€¢—FVÕö–BÀ¢“°¢ÒVÇ6R–b†5ö6öçG&öÆÆW'2bb†5÷&öw&Õö6†ævW2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%÷&öw&ÕöæEö÷VU÷6VÆV7F–öâ€¢—FVÕö–BÀ¢“°¢ÒVÇ6R–b†5÷&öw&Õö6†ævW2bb†5÷öÇ•ögFW'F÷V6†W2bb†5ö6†ææVÅ÷&W77W&W2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷&öw&Õ÷öÇ•ö6†ææVÅ÷&W77W&UöæEö÷VU÷6VÆV7F–öâ€¢—FVÕö–BÀ¢“°¢ÒVÇ6R–b†5÷&öw&Õö6†ævW2bb†5÷öÇ•ögFW'F÷V6†W2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷&öw&Õ÷öÇ•ögFW'F÷V6…öæEö÷VU÷6VÆV7F–öâ€¢—FVÕö–BÀ¢“°¢ÒVÇ6R–b†5÷&öw&Õö6†ævW2bb†5ö6†ææVÅ÷&W77W&W2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷&öw&Õö6†ææVÅ÷&W77W&UöæEö÷VU÷6VÆV7F–öâ€¢—FVÕö–BÀ¢“°¢ÒVÇ6R–b†5÷&öw&Õö6†ævW2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷&öw&ÕöæEö÷VU÷6VÆV7F–öâ†—FVÕö–B“°¢ÒVÇ6R–b†5÷öÇ•ögFW'F÷V6†W2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷öÇ•ögFW'F÷V6…öæEö÷VU÷6VÆV7F–öâ€¢—FVÕö–BÀ¢“°¢ÒVÇ6R–b†5ö6†ææVÅ÷&W77W&W2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6†ææVÅ÷&W77W&UöæEö÷VU÷6VÆV7F–öâ€¢—FVÕö–BÀ¢“°¢ÒVÇ6R–b†5öæ÷FW2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEöæ÷FUöæEö÷VU÷6VÆV7F–öâ†—FVÕö–B“°¢ÒVÇ6R–b†5÷—F6…ö&VæG2°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷—F6…öæEö÷VU÷6VÆV7F–öâ†—FVÕö–B“°¢ÒVÇ6R°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%öæEö÷VU÷6VÆV7F–öâ†—FVÕö–B“°¢Ð¢ÒVÇ6R°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6†ææVÅöWfVçE÷6VÆV7F–öâ†—FVÕö–B“°¢Ð¢&WGW&ã°¢Ð ¢ÖF6‚‚’°¢ò–b†5ö÷VUöWfVçG2Óâ6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVEö÷VUöWfVçG2‚’À¢ò–b†5öæ÷FW2Óâ6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVEöæ÷FW2‚’À¢ò–b†5ö6öçG&öÆÆW'2Óâ6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVEö6öçG&öÆÆW'2‚’À¢ò–b†5÷—F6…ö&VæG2Óâ6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVE÷—F6…ö&VæG2‚’À¢ò–b†5÷&öw&Õö6†ævW2Óâ6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVE÷&öw&Õö6†ævW2‚’À¢ò–b†5÷öÇ•ögFW'F÷V6†W2Óâ°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2‚¢Ð¢ò–b†5ö6†ææVÅ÷&W77W&W2Óâ°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVEö6†ææVÅ÷&W77W&W2‚¢Ð¢òÓâ·Ð¢Ð¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVEö÷VUöWfVçG2‚f×WB6VÆb’°¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB×WB6VÆV7FVE÷F–6·2Ò—FVÐ¢æ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²“°¢ÆWB6öÖR†f—'7E÷F–6²’Ò6VÆV7FVE÷F–6·2ææW‡B‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚“°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB‡÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’Ò6VÆV7FVE÷F–6·2æföÆB€¢€¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†f—'7E÷F–6²ÂÖV7W&U÷F–6·2’À¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR†f—'7E÷F–6²ÂÖV7W&U÷F–6·2’À¢’À¢Â‡÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’ÂF–6·Â°¢€¢÷6—F–öåöÆ÷ræÖ–â†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR‡F–6²ÂÖV7W&U÷F–6·2’’À¢÷6—F–öåö†–v‚æÖ‚†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR‡F–6²ÂÖV7W&U÷F–6·2’’À¢¢ÒÀ¢“° ¢òò$TU"w&÷W2FW‡BÖWFæB7—4W‚VæFW"öæRG—RæBWFöÖF–6ÆÇ¢òòVæ&ÆW26†÷rÖöæÇ’v†VâFW&—f–ærF†Rf–ÇFW"g&öÒ÷VR6VÆV7F–öç2à¢6VÆbæÖ–F•öVF—F÷%ö6†ææVÅ÷6VÆV7F–öâÒÖ–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤ÆÄ6†ææVÇ3°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2ÒSc£¤Ôƒ°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ5ö–æ—F–Æ—¦VBÒG'VS°¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"ÒÖ–F”VF—F÷$WfVçEG—Tf–ÇFW#£¥7—4W„ÖWF°¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"ÒÖ–F”VF—F÷$æ÷FTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"ÒÖ–F”VF—F÷$6öçG&öÆÆW$f–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"ÒÖ–F”VF—F÷%—F6„&VæDf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"ÒÖ–F”VF—F÷%&öw&Ô6†ævTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ÒÖ–F”VF—F÷%÷6—F–öäf–ÇFW"°¢Væ&ÆVC¢G'VRÀ¢&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢Æ÷s¢÷6—F–öåöÆ÷rÀ¢†–vƒ¢÷6—F–öåö†–v‚À¢&WVEöWfW'“¢Ö–F”VF—F÷$f–ÇFW%&WVC£¤ÖV7W&RÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUöÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåöÆ÷rÒf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåöÆ÷r“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v‚Òf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåö†–v‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…öÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…ö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöf–ÇFW%öVæ&ÆVBÒG'VS°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöf–ÇFW%ö7F—fRÒG'VS°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%öæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢æ6öçG&öÆÆW'2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢G'VRÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷&öw&ÕöæEö÷VU÷6VÆV7F–öâ‚f×WB6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢ç&öw&Õö6†ævW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢fÇ6RÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%÷&öw&ÕöæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢æ6öçG&öÆÆW'2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒç&öw&Õö6†ævW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢fÇ6RÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%÷&öw&Õ÷öÇ•ögFW'F÷V6…öæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢æ6öçG&öÆÆW'2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒç&öw&Õö6†ævW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒçöÇ•ögFW'F÷V6†W2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢fÇ6RÀ¢“°¢òò$TU"ÆVfW2F†RgVææVÂöfbf÷"42²&öw&Ò6†ævR²öÇ’gFW'F÷V6‚²÷VRà¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöf–ÇFW%ö7F—fRÒfÇ6S°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷&öw&Õö6†ææVÅ÷&W77W&UöæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢ç&öw&Õö6†ævW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒæ6†ææVÅ÷&W77W&W2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢G'VRÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷&öw&Õ÷öÇ•ögFW'F÷V6…öæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢ç&öw&Õö6†ævW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒçöÇ•ögFW'F÷V6†W2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢G'VRÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷&öw&Õ÷öÇ•ö6†ææVÅ÷&W77W&UöæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢ç&öw&Õö6†ævW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒçöÇ•ögFW'F÷V6†W2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ6†ææVÅ÷&W77W&W2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢fÇ6RÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷öÇ•ögFW'F÷V6…öæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢çöÇ•ögFW'F÷V6†W2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢fÇ6RÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6†ææVÅ÷&W77W&UöæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢æ6†ææVÅ÷&W77W&W2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢fÇ6RÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%ö6†ææVÅ÷&W77W&UöæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢æ6öçG&öÆÆW'2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒæ6†ææVÅ÷&W77W&W2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢fÇ6RÀ¢“°¢òò$TU"¶VW2F†RWfVçBÆ—7BgVææVÂöfbgFW"F†—2F‡&VRÖ¶–æB6WBà¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöf–ÇFW%ö7F—fRÒfÇ6S°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%÷&öw&Õö6†ææVÅ÷&W77W&UöæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢æ6öçG&öÆÆW'2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒç&öw&Õö6†ævW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ6†ææVÅ÷&W77W&W2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢fÇ6RÀ¢“°¢òò$TU"¶VW26†÷rÖöæÇ’æBF†RWfVçBÆ—7BgVææVÂöfbgFW"F†—0¢òòf÷W"Ö¶–æB6WB6VÆV7F–öâà¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöf–ÇFW%ö7F—fRÒfÇ6S°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEöæ÷FUöæEö÷VU÷6VÆV7F–öâ‚f×WB6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢ææ÷FW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6öçF–ç2‚fWfVçBæ–B‚’’¢æÖ‡ÆWfVçGÂWfVçBçF–6²‚’¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢fÇ6RÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VE÷—F6…öæEö÷VU÷6VÆV7F–öâ‚f×WB6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢ç—F6…ö&VæG2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢G'VRÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6öçG&öÆÆW%÷—F6…öæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢æ6öçG&öÆÆW'2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²¢æ6†–â€¢—FVÒç—F6…ö&VæG2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢G'VRÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEöæ÷FUö6öçG&öÆÆW%öæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢ææ÷FW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6öçF–ç2‚fWfVçBæ–B‚’’¢æÖ‡ÆWfVçGÂWfVçBçF–6²‚’¢æ6†–â€¢—FVÒæ6öçG&öÆÆW'2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢G'VRÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEöæ÷FU÷—F6…öæEö÷VU÷6VÆV7F–öâ€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷F–6·2Ò—FVÐ¢ææ÷FW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6öçF–ç2‚fWfVçBæ–B‚’’¢æÖ‡ÆWfVçGÂWfVçBçF–6²‚’¢æ6†–â€¢—FVÒç—F6…ö&VæG2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂWfVçBçF–6²’À¢¢æ6†–â€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â†–æFW‚Âò—Â°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG0¢æ6öçF–ç2‚b‚¦–æFW‚2S3"’¢Ò¢æÖ‡Â…òÂWfVçB—ÂWfVçBçF–6²’À¢¢æ6öÆÆV7B‚“°¢6VÆbç6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢—FVÕö–BÀ¢6VÆV7FVE÷F–6·2À¢G'VRÀ¢“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö÷VU÷6VÆV7F–öå÷F–6·2€¢f×WB6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢6VÆV7FVE÷F–6·3¢fV3ÇScCâÀ¢6†÷uööæÇ“¢&ööÂÀ¢’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB×WB6VÆV7FVE÷F–6·2Ò6VÆV7FVE÷F–6·2æ–çFõö—FW"‚“°¢ÆWB6öÖR†f—'7E÷F–6²’Ò6VÆV7FVE÷F–6·2ææW‡B‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚“°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB‡÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’Ò6VÆV7FVE÷F–6·2æföÆB€¢€¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†f—'7E÷F–6²ÂÖV7W&U÷F–6·2’À¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR†f—'7E÷F–6²ÂÖV7W&U÷F–6·2’À¢’À¢Â‡÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’ÂF–6·Â°¢€¢÷6—F–öåöÆ÷ræÖ–â†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR‡F–6²ÂÖV7W&U÷F–6·2’’À¢÷6—F–öåö†–v‚æÖ‚†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR‡F–6²ÂÖV7W&U÷F–6·2’’À¢¢ÒÀ¢“° ¢òò$TU"W‡æG2ÖV7W&VBæ÷FRô42õ—F6‚&VæB²÷VR6öÖ&–æF–öç2Fð¢òòÆÂWfVçBG—W2æB6†ææVÇ2æBW6W2F†V—"6†&VB÷6—F–öâ&ævRà¢òòæ÷FR²÷VR7F—fFW2F†RgVææVÂv—F†÷WB6†÷rÖöæÇ“²F†RÖV7W&V@¢òò42æB—F6‚&VæB6öÖ&–æF–öç2Væ&ÆR6†÷rÖöæÇ’à¢6VÆbæÖ–F•öVF—F÷%ö6†ææVÅ÷6VÆV7F–öâÒÖ–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤ÆÄ6†ææVÇ3°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2ÒSc£¤Ôƒ°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ5ö–æ—F–Æ—¦VBÒG'VS°¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"ÒÖ–F”VF—F÷$WfVçEG—Tf–ÇFW#£¤ÆÄWfVçG3°¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"ÒÖ–F”VF—F÷$æ÷FTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"ÒÖ–F”VF—F÷$6öçG&öÆÆW$f–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"ÒÖ–F”VF—F÷%—F6„&VæDf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"ÒÖ–F”VF—F÷%&öw&Ô6†ævTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ÒÖ–F”VF—F÷%÷6—F–öäf–ÇFW"°¢Væ&ÆVC¢G'VRÀ¢&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢Æ÷s¢÷6—F–öåöÆ÷rÀ¢†–vƒ¢÷6—F–öåö†–v‚À¢&WVEöWfW'“¢Ö–F”VF—F÷$f–ÇFW%&WVC£¤ÖV7W&RÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUöÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåöÆ÷rÒf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåöÆ÷r“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v‚Òf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåö†–v‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…öÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…ö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöf–ÇFW%öVæ&ÆVBÒ6†÷uööæÇ“°¢6VÆbæÖ–F•öVF—F÷%öWfVçEöÆ—7Eöf–ÇFW%ö7F—fRÒG'VS°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕöÖ—†VEö6†ææVÅöWfVçE÷6VÆV7F–öâ‚f×WB6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB×WB6VÆV7FVEöWfVçG2ÒfV3£¦æWr‚“°¢6VÆV7FVEöWfVçG2æW‡FVæB€¢—FVÒææ÷FW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6öçF–ç2‚fWfVçBæ–B‚’’¢æÖ‡ÆWfVçGÂ†WfVçBçF–6²‚’ÂWfVçBæ6†ææVÂ‚’’’À¢“°¢6VÆV7FVEöWfVçG2æW‡FVæB€¢—FVÒæ6öçG&öÆÆW'2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂ†WfVçBçF–6²ÂWfVçBæ6†ææVÂ’’À¢“°¢6VÆV7FVEöWfVçG2æW‡FVæB€¢—FVÒç—F6…ö&VæG2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂ†WfVçBçF–6²ÂWfVçBæ6†ææVÂ’’À¢“°¢6VÆV7FVEöWfVçG2æW‡FVæB€¢—FVÒç&öw&Õö6†ævW2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂ†WfVçBçF–6²ÂWfVçBæ6†ææVÂ’’À¢“°¢6VÆV7FVEöWfVçG2æW‡FVæB€¢—FVÒçöÇ•ögFW'F÷V6†W2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂ†WfVçBçF–6²ÂWfVçBæ6†ææVÂ’’À¢“°¢6VÆV7FVEöWfVçG2æW‡FVæB€¢—FVÒæ6†ææVÅ÷&W77W&W2‚¢æ—FW"‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6öçF–ç2†WfVçB’¢æÖ‡ÆWfVçGÂ†WfVçBçF–6²ÂWfVçBæ6†ææVÂ’’À¢“°¢ÆWB6öÖR‚†f—'7E÷F–6²Âf—'7Eö6†ææVÂ’’Ò6VÆV7FVEöWfVçG0¢æ—FW"‚¢æ6÷–VB‚¢æÖ–åö'•ö¶W’‡Â‡F–6²Â6†ææVÂ—Â‚§F–6²Â¦6†ææVÂ’¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚“°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB†6†ææVÇ2Â÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’Ò6VÆV7FVEöWfVçG2æ—FW"‚’æföÆB€¢€¢÷SbÀ¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†f—'7E÷F–6²ÂÖV7W&U÷F–6·2’À¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR†f—'7E÷F–6²ÂÖV7W&U÷F–6·2’À¢’À¢Â†6†ææVÇ2Â÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’Â‡F–6²Â6†ææVÂ—Â°¢€¢6†ææVÇ2Âƒ÷SbÃÂ‚¦6†ææVÂ’æÖ–âƒR’’À¢÷6—F–öåöÆ÷ræÖ–â†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR‚§F–6²ÂÖV7W&U÷F–6·2’’À¢÷6—F–öåö†–v‚æÖ‚†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR‚§F–6²ÂÖV7W&U÷F–6·2’’À¢¢ÒÀ¢“°¢–b6†ææVÇ2ÓÒ°¢&WGW&ã°¢Ð ¢6VÆbæÖ–F•öVF—F÷%ö6†ææVÅ÷6VÆV7F–öâÒ–b6†ææVÇ2æ6÷VçEööæW2‚’ÓÒ°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤6†ææVÂ†6†ææVÇ2çG&–Æ–æu÷¦W&÷2‚’2S‚¢ÒVÇ6R°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤×VÇF–6†ææVÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2Ò6†ææVÇ3°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ5ö–æ—F–Æ—¦VBÒG'VS°¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"ÒÖ–F”VF—F÷$WfVçEG—Tf–ÇFW#£¤ÆÄWfVçG3°¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"ÒÖ–F”VF—F÷$æ÷FTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"ÒÖ–F”VF—F÷$6öçG&öÆÆW$f–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"ÒÖ–F”VF—F÷%—F6„&VæDf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"ÒÖ–F”VF—F÷%&öw&Ô6†ævTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ÒÖ–F”VF—F÷%÷6—F–öäf–ÇFW"°¢Væ&ÆVC¢G'VRÀ¢&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢Æ÷s¢÷6—F–öåöÆ÷rÀ¢†–vƒ¢÷6—F–öåö†–v‚À¢&WVEöWfW'“¢Ö–F”VF—F÷$f–ÇFW%&WVC£¤ÖV7W&RÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUöÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåöÆ÷rÒf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåöÆ÷r“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v‚Òf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåö†–v‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…öÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…ö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öG&uö6†ææVÂÒf—'7Eö6†ææVÂæÖ–âƒR“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVEöæ÷FW2‚f×WB6VÆb’°¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚“°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB×WB6VÆV7FVEöæ÷FW2Ò—FVÐ¢ææ÷FW2‚¢æ—FW"‚¢æf–ÇFW"‡Ææ÷FWÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6öçF–ç2‚fæ÷FRæ–B‚’’“°¢ÆWB6öÖR†f—'7Eöæ÷FR’Ò6VÆV7FVEöæ÷FW2ææW‡B‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBf—'7E÷÷6—F–öâÒÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†f—'7Eöæ÷FRçF–6²‚’ÂÖV7W&U÷F–6·2“°¢ÆWBf—'7E÷÷6—F–öåö†–v‚Ð¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR†f—'7Eöæ÷FRçF–6²‚’ÂÖV7W&U÷F–6·2“°¢ÆWB€¢6†ææVÇ2À¢—F6…öÆ÷rÀ¢—F6…ö†–v‚À¢fVÆö6—G•öÆ÷rÀ¢fVÆö6—G•ö†–v‚À¢÷6—F–öåöÆ÷rÀ¢÷6—F–öåö†–v‚À¢ÆVæwF…öÆ÷u÷F–6·2À¢ÆVæwF…ö†–v…÷F–6·2À¢’Ò6VÆV7FVEöæ÷FW2æföÆB€¢€¢÷SbÃÂf—'7Eöæ÷FRæ6†ææVÂ‚’æÖ–âƒR’À¢f—'7Eöæ÷FRç—F6‚‚’À¢f—'7Eöæ÷FRç—F6‚‚’À¢f—'7Eöæ÷FRçfVÆö6—G’‚’À¢f—'7Eöæ÷FRçfVÆö6—G’‚’À¢f—'7E÷÷6—F–öâÀ¢f—'7E÷÷6—F–öåö†–v‚À¢f—'7Eöæ÷FRæGW&F–öâ‚’À¢f—'7Eöæ÷FRæGW&F–öâ‚’À¢’À¢Â€¢6†ææVÇ2À¢—F6…öÆ÷rÀ¢—F6…ö†–v‚À¢fVÆö6—G•öÆ÷rÀ¢fVÆö6—G•ö†–v‚À¢÷6—F–öåöÆ÷rÀ¢†–v†W7E÷÷6—F–öâÀ¢ÆVæwF…öÆ÷u÷F–6·2À¢ÆVæwF…ö†–v…÷F–6·2À¢’À¢æ÷FWÂ°¢ÆWB÷6—F–öâÒÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†æ÷FRçF–6²‚’ÂÖV7W&U÷F–6·2“°¢ÆWBWfVçE÷÷6—F–öåö†–v‚Ð¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR†æ÷FRçF–6²‚’ÂÖV7W&U÷F–6·2“°¢€¢6†ææVÇ2Âƒ÷SbÃÂæ÷FRæ6†ææVÂ‚’æÖ–âƒR’’À¢—F6…öÆ÷ræÖ–â†æ÷FRç—F6‚‚’’À¢—F6…ö†–v‚æÖ‚†æ÷FRç—F6‚‚’’À¢fVÆö6—G•öÆ÷ræÖ–â†æ÷FRçfVÆö6—G’‚’’À¢fVÆö6—G•ö†–v‚æÖ‚†æ÷FRçfVÆö6—G’‚’’À¢÷6—F–öåöÆ÷ræÖ–â‡÷6—F–öâ’À¢†–v†W7E÷÷6—F–öâæÖ‚†WfVçE÷÷6—F–öåö†–v‚’À¢ÆVæwF…öÆ÷u÷F–6·2æÖ–â†æ÷FRæGW&F–öâ‚’’À¢ÆVæwF…ö†–v…÷F–6·2æÖ‚†æ÷FRæGW&F–öâ‚’’À¢¢ÒÀ¢“°¢–b6†ææVÇ2ÓÒ°¢&WGW&ã°¢Ð ¢6VÆbæÖ–F•öVF—F÷%ö6†ææVÅ÷6VÆV7F–öâÒ–b6†ææVÇ2æ6÷VçEööæW2‚’ÓÒ°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤6†ææVÂ†6†ææVÇ2çG&–Æ–æu÷¦W&÷2‚’2S‚¢ÒVÇ6R°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤×VÇF–6†ææVÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2Ò6†ææVÇ3°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ5ö–æ—F–Æ—¦VBÒG'VS°¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"ÒÖ–F”VF—F÷$WfVçEG—Tf–ÇFW#£¤æ÷FW3°¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"ÒÖ–F”VF—F÷$æ÷FTf–ÇFW"°¢—F6…öVæ&ÆVC¢G'VRÀ¢—F6…÷&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢—F6…öÆ÷rÀ¢—F6…ö†–v‚À¢fVÆö6—G•öVæ&ÆVC¢G'VRÀ¢fVÆö6—G•÷&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢fVÆö6—G•öÆ÷rÀ¢fVÆö6—G•ö†–v‚À¢ÆVæwF…öVæ&ÆVC¢G'VRÀ¢ÆVæwF…÷&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢ÆVæwF…öÆ÷u÷F–6·2À¢ÆVæwF…ö†–v…÷F–6·2À¢Ó°¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"ÒÖ–F”VF—F÷$6öçG&öÆÆW$f–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"ÒÖ–F”VF—F÷%—F6„&VæDf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"ÒÖ–F”VF—F÷%&öw&Ô6†ævTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ÒÖ–F”VF—F÷%÷6—F–öäf–ÇFW"°¢Væ&ÆVC¢G'VRÀ¢&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢Æ÷s¢÷6—F–öåöÆ÷rÀ¢†–vƒ¢÷6—F–öåö†–v‚À¢&WVEöWfW'“¢Ö–F”VF—F÷$f–ÇFW%&WVC£¤ÖV7W&RÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUöÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåöÆ÷rÒf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåöÆ÷r“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v‚Òf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåö†–v‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…öÆ÷rÐ¢f÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%öÆVæwF‚†ÆVæwF…öÆ÷u÷F–6·2Â“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…ö†–v‚Ð¢f÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%öÆVæwF‚†ÆVæwF…ö†–v…÷F–6·2Â“°¢6VÆbæÖ–F•öVF—F÷%öG&uö6†ææVÂÒ°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVE÷—F6…ö&VæG2‚f×WB6VÆb’°¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVEö&VæG2Ò—FVÐ¢ç—F6…ö&VæG2‚¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6öçF–ç2†WfVçB’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6öÖR†f—'7Eö&VæB’Ò6VÆV7FVEö&VæG2æf—'7B‚’æ6÷–VB‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚“°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB†6†ææVÇ2ÂfÇVUöÆ÷rÂfÇVUö†–v‚Â÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’Ð¢6VÆV7FVEö&VæG2æ—FW"‚’æföÆB€¢€¢÷SbÃÂf—'7Eö&VæBæ6†ææVÂæÖ–âƒR’À¢f—'7Eö&VæBçfÇVRÀ¢f—'7Eö&VæBçfÇVRÀ¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†f—'7Eö&VæBçF–6²ÂÖV7W&U÷F–6·2’À¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR†f—'7Eö&VæBçF–6²ÂÖV7W&U÷F–6·2’À¢’À¢Â†6†ææVÇ2ÂfÇVUöÆ÷rÂfÇVUö†–v‚Â÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’ÂWfVçGÂ°¢€¢6†ææVÇ2Âƒ÷SbÃÂWfVçBæ6†ææVÂæÖ–âƒR’’À¢fÇVUöÆ÷ræÖ–â†WfVçBçfÇVR’À¢fÇVUö†–v‚æÖ‚†WfVçBçfÇVR’À¢÷6—F–öåöÆ÷p¢æÖ–â†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†WfVçBçF–6²ÂÖV7W&U÷F–6·2’’À¢÷6—F–öåö†–v‚æÖ‚†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR€¢WfVçBçF–6²À¢ÖV7W&U÷F–6·2À¢’’À¢¢ÒÀ¢“°¢–b6†ææVÇ2ÓÒ°¢&WGW&ã°¢Ð ¢ÆWBfÇVUöÆ÷rÒfÇVUöÆ÷r2“3"Ò…ó“#°¢ÆWBfÇVUö†–v‚ÒfÇVUö†–v‚2“3"Ò…ó“#°¢6VÆbæÖ–F•öVF—F÷%ö6†ææVÅ÷6VÆV7F–öâÒ–b6†ææVÇ2æ6÷VçEööæW2‚’ÓÒ°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤6†ææVÂ†6†ææVÇ2çG&–Æ–æu÷¦W&÷2‚’2S‚¢ÒVÇ6R°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤×VÇF–6†ææVÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2Ò6†ææVÇ3°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ5ö–æ—F–Æ—¦VBÒG'VS°¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"ÒÖ–F”VF—F÷$WfVçEG—Tf–ÇFW#£¥—F6ƒ°¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"ÒÖ–F”VF—F÷$æ÷FTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"ÒÖ–F”VF—F÷$6öçG&öÆÆW$f–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"ÒÖ–F”VF—F÷%&öw&Ô6†ævTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"ÒÖ–F”VF—F÷%—F6„&VæDf–ÇFW"°¢Væ&ÆVC¢G'VRÀ¢&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢Æ÷s¢fÇVUöÆ÷r2“bÀ¢†–vƒ¢fÇVUö†–v‚2“bÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ÒÖ–F”VF—F÷%÷6—F–öäf–ÇFW"°¢Væ&ÆVC¢G'VRÀ¢&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢Æ÷s¢÷6—F–öåöÆ÷rÀ¢†–vƒ¢÷6—F–öåö†–v‚À¢&WVEöWfW'“¢Ö–F”VF—F÷$f–ÇFW%&WVC£¤ÖV7W&RÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUöÆ÷rÒfÇVUöÆ÷rçFõ÷7G&–ær‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUö†–v‚ÒfÇVUö†–v‚çFõ÷7G&–ær‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåöÆ÷rÒf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåöÆ÷r“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v‚Òf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåö†–v‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…öÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…ö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öG&uö6†ææVÂÒf—'7Eö&VæBæ6†ææVÂæÖ–âƒR“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVE÷&öw&Õö6†ævW2‚f×WB6VÆb’°¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷&öw&Õö6†ævW2Ò—FVÐ¢ç&öw&Õö6†ævW2‚¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6öçF–ç2†WfVçB’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6öÖR†f—'7E÷&öw&Õö6†ævR’Ò6VÆV7FVE÷&öw&Õö6†ævW2æf—'7B‚’æ6÷–VB‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚“°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB†6†ææVÇ2Â÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’Ò6VÆV7FVE÷&öw&Õö6†ævW2æ—FW"‚’æföÆB€¢€¢÷SbÃÂf—'7E÷&öw&Õö6†ævRæ6†ææVÂæÖ–âƒR’À¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†f—'7E÷&öw&Õö6†ævRçF–6²ÂÖV7W&U÷F–6·2’À¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR†f—'7E÷&öw&Õö6†ævRçF–6²ÂÖV7W&U÷F–6·2’À¢’À¢Â†6†ææVÇ2Â÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’ÂWfVçGÂ°¢€¢6†ææVÇ2Âƒ÷SbÃÂWfVçBæ6†ææVÂæÖ–âƒR’’À¢÷6—F–öåöÆ÷ræÖ–â†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†WfVçBçF–6²ÂÖV7W&U÷F–6·2’’À¢÷6—F–öåö†–v‚æÖ‚†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR€¢WfVçBçF–6²À¢ÖV7W&U÷F–6·2À¢’’À¢¢ÒÀ¢“°¢–b6†ææVÇ2ÓÒ°¢&WGW&ã°¢Ð ¢6VÆbæÖ–F•öVF—F÷%ö6†ææVÅ÷6VÆV7F–öâÒ–b6†ææVÇ2æ6÷VçEööæW2‚’ÓÒ°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤6†ææVÂ†6†ææVÇ2çG&–Æ–æu÷¦W&÷2‚’2S‚¢ÒVÇ6R°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤×VÇF–6†ææVÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2Ò6†ææVÇ3°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ5ö–æ—F–Æ—¦VBÒG'VS°¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"ÒÖ–F”VF—F÷$WfVçEG—Tf–ÇFW#£¥&öw&Ô6†ævS°¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"ÒÖ–F”VF—F÷$æ÷FTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"ÒÖ–F”VF—F÷$6öçG&öÆÆW$f–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"ÒÖ–F”VF—F÷%—F6„&VæDf–ÇFW#£¦FVfVÇB‚“°¢òò$TU"ÆVfW2F†R&öw&ÒfÇVR&ævRVæ6†V6¶VBv†VâFW&—f–ærg&öÒ6VÆV7F–öâà¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"ÒÖ–F”VF—F÷%&öw&Ô6†ævTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ÒÖ–F”VF—F÷%÷6—F–öäf–ÇFW"°¢Væ&ÆVC¢G'VRÀ¢&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢Æ÷s¢÷6—F–öåöÆ÷rÀ¢†–vƒ¢÷6—F–öåö†–v‚À¢&WVEöWfW'“¢Ö–F”VF—F÷$f–ÇFW%&WVC£¤ÖV7W&RÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUöÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåöÆ÷rÒf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåöÆ÷r“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v‚Òf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåö†–v‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…öÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…ö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öG&uö6†ææVÂÒf—'7E÷&öw&Õö6†ævRæ6†ææVÂæÖ–âƒR“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2‚f×WB6VÆb’°¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVEögFW'F÷V6†W2Ò—FVÐ¢çöÇ•ögFW'F÷V6†W2‚¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ6öçF–ç2†WfVçB’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6öÖR†f—'7EögFW'F÷V6‚’Ò6VÆV7FVEögFW'F÷V6†W2æf—'7B‚’æ6÷–VB‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB€¢6†ææVÇ2À¢æ÷FUöÆ÷rÀ¢æ÷FUö†–v‚À¢&W77W&UöÆ÷rÀ¢&W77W&Uö†–v‚À¢÷6—F–öåöÆ÷rÀ¢÷6—F–öåö†–v‚À¢’Ò6VÆV7FVEögFW'F÷V6†W2æ—FW"‚’æföÆB€¢€¢÷SbÃÂf—'7EögFW'F÷V6‚æ6†ææVÂæÖ–âƒR’À¢f—'7EögFW'F÷V6‚ææ÷FRÀ¢f—'7EögFW'F÷V6‚ææ÷FRÀ¢f—'7EögFW'F÷V6‚çfÇVRÀ¢f—'7EögFW'F÷V6‚çfÇVRÀ¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†f—'7EögFW'F÷V6‚çF–6²ÂÖV7W&U÷F–6·2’À¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR†f—'7EögFW'F÷V6‚çF–6²ÂÖV7W&U÷F–6·2’À¢’À¢Â€¢6†ææVÇ2À¢æ÷FUöÆ÷rÀ¢æ÷FUö†–v‚À¢&W77W&UöÆ÷rÀ¢&W77W&Uö†–v‚À¢÷6—F–öåöÆ÷rÀ¢÷6—F–öåö†–v‚À¢’À¢WfVçGÂ°¢€¢6†ææVÇ2Âƒ÷SbÃÂWfVçBæ6†ææVÂæÖ–âƒR’’À¢æ÷FUöÆ÷ræÖ–â†WfVçBææ÷FR’À¢æ÷FUö†–v‚æÖ‚†WfVçBææ÷FR’À¢&W77W&UöÆ÷ræÖ–â†WfVçBçfÇVR’À¢&W77W&Uö†–v‚æÖ‚†WfVçBçfÇVR’À¢÷6—F–öåöÆ÷ræÖ–â†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†WfVçBçF–6²ÂÖV7W&U÷F–6·2’’À¢÷6—F–öåö†–v‚æÖ‚†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR€¢WfVçBçF–6²À¢ÖV7W&U÷F–6·2À¢’’À¢¢ÒÀ¢“°¢–b6†ææVÇ2ÓÒ°¢&WGW&ã°¢Ð ¢6VÆbæÖ–F•öVF—F÷%ö6†ææVÅ÷6VÆV7F–öâÒ–b6†ææVÇ2æ6÷VçEööæW2‚’ÓÒ°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤6†ææVÂ†6†ææVÇ2çG&–Æ–æu÷¦W&÷2‚’2S‚¢ÒVÇ6R°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤×VÇF–6†ææVÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2Ò6†ææVÇ3°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ5ö–æ—F–Æ—¦VBÒG'VS°¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"ÒÖ–F”VF—F÷$WfVçEG—Tf–ÇFW#£¥öÇ”gFW'F÷V6ƒ°¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"ÒÖ–F”VF—F÷$æ÷FTf–ÇFW"°¢—F6…öVæ&ÆVC¢G'VRÀ¢—F6…÷&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢—F6…öÆ÷s¢æ÷FUöÆ÷rÀ¢—F6…ö†–vƒ¢æ÷FUö†–v‚À¢fVÆö6—G•öVæ&ÆVC¢G'VRÀ¢fVÆö6—G•÷&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢fVÆö6—G•öÆ÷s¢&W77W&UöÆ÷rÀ¢fVÆö6—G•ö†–vƒ¢&W77W&Uö†–v‚À¢âäÖ–F”VF—F÷$æ÷FTf–ÇFW#£¦FVfVÇB‚¢Ó°¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"ÒÖ–F”VF—F÷$6öçG&öÆÆW$f–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"ÒÖ–F”VF—F÷%—F6„&VæDf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"ÒÖ–F”VF—F÷%&öw&Ô6†ævTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ÒÖ–F”VF—F÷%÷6—F–öäf–ÇFW"°¢Væ&ÆVC¢G'VRÀ¢&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢Æ÷s¢÷6—F–öåöÆ÷rÀ¢†–vƒ¢÷6—F–öåö†–v‚À¢&WVEöWfW'“¢Ö–F”VF—F÷$f–ÇFW%&WVC£¤ÖV7W&RÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUöÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåöÆ÷rÒf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåöÆ÷r“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v‚Òf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåö†–v‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…öÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…ö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öG&uö6†ææVÂÒf—'7EögFW'F÷V6‚æ6†ææVÂæÖ–âƒR“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVEö6†ææVÅ÷&W77W&W2‚f×WB6VÆb’°¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVE÷&W77W&W2Ò—FVÐ¢æ6†ææVÅ÷&W77W&W2‚¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6öçF–ç2†WfVçB’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6öÖR†f—'7E÷&W77W&R’Ò6VÆV7FVE÷&W77W&W2æf—'7B‚’æ6÷–VB‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB†6†ææVÇ2Â÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’Ò6VÆV7FVE÷&W77W&W2æ—FW"‚’æföÆB€¢€¢÷SbÃÂf—'7E÷&W77W&Ræ6†ææVÂæÖ–âƒR’À¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†f—'7E÷&W77W&RçF–6²ÂÖV7W&U÷F–6·2’À¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR†f—'7E÷&W77W&RçF–6²ÂÖV7W&U÷F–6·2’À¢’À¢Â†6†ææVÇ2Â÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’ÂWfVçGÂ°¢€¢6†ææVÇ2Âƒ÷SbÃÂWfVçBæ6†ææVÂæÖ–âƒR’’À¢÷6—F–öåöÆ÷ræÖ–â†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†WfVçBçF–6²ÂÖV7W&U÷F–6·2’’À¢÷6—F–öåö†–v‚æÖ‚†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR€¢WfVçBçF–6²À¢ÖV7W&U÷F–6·2À¢’’À¢¢ÒÀ¢“°¢–b6†ææVÇ2ÓÒ°¢&WGW&ã°¢Ð ¢6VÆbæÖ–F•öVF—F÷%ö6†ææVÅ÷6VÆV7F–öâÒ–b6†ææVÇ2æ6÷VçEööæW2‚’ÓÒ°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤6†ææVÂ†6†ææVÇ2çG&–Æ–æu÷¦W&÷2‚’2S‚¢ÒVÇ6R°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤×VÇF–6†ææVÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2Ò6†ææVÇ3°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ5ö–æ—F–Æ—¦VBÒG'VS°¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"ÒÖ–F”VF—F÷$WfVçEG—Tf–ÇFW#£¤6†ææVÅ&W77W&S°¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"ÒÖ–F”VF—F÷$æ÷FTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"ÒÖ–F”VF—F÷$6öçG&öÆÆW$f–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"ÒÖ–F”VF—F÷%—F6„&VæDf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"ÒÖ–F”VF—F÷%&öw&Ô6†ævTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ÒÖ–F”VF—F÷%÷6—F–öäf–ÇFW"°¢Væ&ÆVC¢G'VRÀ¢&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢Æ÷s¢÷6—F–öåöÆ÷rÀ¢†–vƒ¢÷6—F–öåö†–v‚À¢&WVEöWfW'“¢Ö–F”VF—F÷$f–ÇFW%&WVC£¤ÖV7W&RÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUöÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåöÆ÷rÒf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåöÆ÷r“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v‚Òf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåö†–v‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…öÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…ö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öG&uö6†ææVÂÒf—'7E÷&W77W&Ræ6†ææVÂæÖ–âƒR“°¢Ð ¢fâ6WEöÖ–F•öVF—F÷%öf–ÇFW%ög&öÕ÷6VÆV7FVEö6öçG&öÆÆW'2‚f×WB6VÆb’°¢ÆWB6öÖR†—FVÕö–B’Ò6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BVÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚“°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB6VÆV7FVEö6öçG&öÆÆW'2Ò—FVÐ¢æ6öçG&öÆÆW'2‚¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6öçF–ç2†WfVçB’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6öÖR†f—'7Eö6öçG&öÆÆW"’Ò6VÆV7FVEö6öçG&öÆÆW'2æf—'7B‚’æ6÷–VB‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6öçG&öÆÆW%öVæ&ÆVBÒ6VÆV7FVEö6öçG&öÆÆW'0¢æ—FW"‚¢æç’‡ÆWfVçGÂWfVçBæ6öçG&öÆÆW"Òf—'7Eö6öçG&öÆÆW"æ6öçG&öÆÆW"“° ¢ÆWB†6†ææVÇ2ÂfÇVUöÆ÷rÂfÇVUö†–v‚Â÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’Ð¢6VÆV7FVEö6öçG&öÆÆW'2æ—FW"‚’æföÆB€¢€¢÷SbÃÂf—'7Eö6öçG&öÆÆW"æ6†ææVÂæÖ–âƒR’À¢f—'7Eö6öçG&öÆÆW"çfÇVRÀ¢f—'7Eö6öçG&öÆÆW"çfÇVRÀ¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†f—'7Eö6öçG&öÆÆW"çF–6²ÂÖV7W&U÷F–6·2’À¢Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR†f—'7Eö6öçG&öÆÆW"çF–6²ÂÖV7W&U÷F–6·2’À¢’À¢Â†6†ææVÇ2ÂfÇVUöÆ÷rÂfÇVUö†–v‚Â÷6—F–öåöÆ÷rÂ÷6—F–öåö†–v‚’ÂWfVçGÂ°¢€¢6†ææVÇ2Âƒ÷SbÃÂWfVçBæ6†ææVÂæÖ–âƒR’’À¢fÇVUöÆ÷ræÖ–â†WfVçBçfÇVR’À¢fÇVUö†–v‚æÖ‚†WfVçBçfÇVR’À¢÷6—F–öåöÆ÷p¢æÖ–â†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†WfVçBçF–6²ÂÖV7W&U÷F–6·2’’À¢÷6—F–öåö†–v‚æÖ‚†Ö–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v…÷fÇVR€¢WfVçBçF–6²À¢ÖV7W&U÷F–6·2À¢’’À¢¢ÒÀ¢“°¢–b6†ææVÇ2ÓÒ°¢&WGW&ã°¢Ð ¢6VÆbæÖ–F•öVF—F÷%ö6†ææVÅ÷6VÆV7F–öâÒ–b6†ææVÇ2æ6÷VçEööæW2‚’ÓÒ°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤6†ææVÂ†6†ææVÇ2çG&–Æ–æu÷¦W&÷2‚’2S‚¢ÒVÇ6R°¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤×VÇF–6†ææVÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2Ò6†ææVÇ3°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ5ö–æ—F–Æ—¦VBÒG'VS°¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"ÒÖ–F”VF—F÷$WfVçEG—Tf–ÇFW#£¤6öçG&öÄ6†ævS°¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"ÒÖ–F”VF—F÷$æ÷FTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"ÒÖ–F”VF—F÷%—F6„&VæDf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"ÒÖ–F”VF—F÷%&öw&Ô6†ævTf–ÇFW#£¦FVfVÇB‚“°¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"ÒÖ–F”VF—F÷$6öçG&öÆÆW$f–ÇFW"°¢6öçG&öÆÆW%öVæ&ÆVBÀ¢6öçG&öÆÆW#¢–b6öçG&öÆÆW%öVæ&ÆVB°¢f—'7Eö6öçG&öÆÆW"æ6öçG&öÆÆW ¢ÒVÇ6R°¢ ¢ÒÀ¢fÇVUöVæ&ÆVC¢G'VRÀ¢fÇVU÷&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢fÇVUöÆ÷rÀ¢fÇVUö†–v‚À¢Ó°¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ÒÖ–F”VF—F÷%÷6—F–öäf–ÇFW"°¢Væ&ÆVC¢G'VRÀ¢&ævUö–æ—F–Æ—¦VC¢G'VRÀ¢Æ÷s¢÷6—F–öåöÆ÷rÀ¢†–vƒ¢÷6—F–öåö†–v‚À¢&WVEöWfW'“¢Ö–F”VF—F÷$f–ÇFW%&WVC£¤ÖV7W&RÀ¢Ó°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUöÆ÷rÒfÇVUöÆ÷rçFõ÷7G&–ær‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷fÇVUö†–v‚ÒfÇVUö†–v‚çFõ÷7G&–ær‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåöÆ÷rÒf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåöÆ÷r“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öåö†–v‚Òf÷&ÖEöÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öâ‡÷6—F–öåö†–v‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…öÆ÷ræ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÆVæwF…ö†–v‚æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%öG&uö6†ææVÂÒ°¢Ð ¢fâÖ–F•öVF—F÷%öf–ÇFW%öWfVçE÷6VÆV7F–öâ€¢g6VÆbÀ¢—FVÕö–C¢—FVÔ–BÀ¢’Óâ÷F–öãÄÖ–F”VF—F÷%6VÆV7F–öå7FFSâ°¢ÆWB—FVÒÒ6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B“ó°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB×WB6VÆV7F–öâÒÖ–F”VF—F÷%6VÆV7F–öå7FFS£¦FVfVÇB‚“°¢6VÆV7F–öâææ÷FW2æW‡FVæB€¢—FVÒææ÷FW2‚¢æ—FW"‚¢æf–ÇFW"‡Ææ÷FWÂ6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÖF6†W2†—FVÒÂæ÷FR’¢æÖ‡Ææ÷FWÂæ÷FRæ–B‚’’À¢“°¢6VÆV7F–öâæ6öçG&öÆÆW'2æW‡FVæB€¢—FVÒæ6öçG&öÆÆW'2‚¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6öçG&öÆÆW%öÖF6†W2†—FVÒÂ¦WfVçB’’À¢“°¢6VÆV7F–öâç—F6…ö&VæG2æW‡FVæB€¢—FVÒç—F6…ö&VæG2‚¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷—F6…ö&VæEöÖF6†W2†—FVÒÂ¦WfVçB’’À¢“°¢6VÆV7F–öâç&öw&Õö6†ævW2æW‡FVæB€¢—FVÒç&öw&Õö6†ævW2‚¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷&öw&Õö6†ævUöÖF6†W2†—FVÒÂ¦WfVçB’’À¢“°¢6VÆV7F–öâçöÇ•ögFW'F÷V6†W2æW‡FVæB€¢—FVÒçöÇ•ögFW'F÷V6†W2‚¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%öf–ÇFW%÷öÇ•ögFW'F÷V6…öÖF6†W2†—FVÒÂ¦WfVçB’’À¢“°¢6VÆV7F–öâæ6†ææVÅ÷&W77W&W2æW‡FVæB€¢—FVÒæ6†ææVÅ÷&W77W&W2‚¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡ÆWfVçGÂ6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÅ÷&W77W&UöÖF6†W2†—FVÒÂ¦WfVçB’’À¢“°¢6VÆV7F–öâæ÷VUöWfVçG2æW‡FVæB€¢—FVÒæ÷VUöWfVçG2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æf–ÇFW"‡Â…òÂWfVçB—Â°¢ÆWBWfVçEö¶–æBÒÖF6‚fWfVçBæ¶–æB°¢Ö–F”÷VTWfVçD¶–æC£¥7—7FVÔW†6ÇW6—fR²ââÒÓâÖ–F”VF—F÷$WfVçD¶–æC£¥7—4W‚À¢Ö–F”÷VTWfVçD¶–æC£¤ÖWF²ââÒÓâÖ–F”VF—F÷$WfVçD¶–æC£¥FW‡BÀ¢Ó°¢ÆWB÷6—F–öâÒÖ–F•öVF—F÷%öf–ÇFW%÷&WVE÷÷6—F–öå÷fÇVR€¢WfVçBçF–6²À¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢ÖV7W&U÷F–6·2À¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ç&WVEöWfW'’À¢“°¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%öWfVçEöÖF6†W2„æöæRÂWfVçEö¶–æBÂ÷6—F–öâ¢Ò¢æÖ‡Â†–æFW‚Âò—Â–æFW‚2S3"’À¢“°¢6öÖR‡6VÆV7F–öâ¢Ð ¢fâÖ–F•öVF—F÷%öf–ÇFW%öWfVçEöÖF6†W2€¢g6VÆbÀ¢6†ææVÃ¢÷F–öãÇSƒâÀ¢WfVçEö¶–æC¢Ö–F”VF—F÷$WfVçD¶–æBÀ¢÷6—F–öåö–å÷&WVC¢S3"À¢’Óâ&ööÂ°¢Ö–F”VF—F÷$f–ÇFW%7FFR°¢6†ææVÅ÷6VÆV7F–öã¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤ÆÄ6†ææVÇ2À¢6†ææVÅöf–ÇFW%öVæ&ÆVC¢fÇ6RÀ¢WfVçEöf–ÇFW%öVæ&ÆVC¢G'VRÀ¢6†ææVÇ3¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2À¢–çfW'FVC¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö–çfW'FVBÀ¢WfVçE÷G—S¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"À¢æ÷FS¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"À¢6öçG&öÆÆW#¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"À¢—F6…ö&VæC¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"À¢&öw&Õö6†ævS¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"À¢÷6—F–öã¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"À¢Ð¢æWfVçE÷f—6–&ÆUöB†6†ææVÂÂWfVçEö¶–æBÂ÷6—F–öåö–å÷&WVB¢Ð ¢fâÖ–F•öVF—F÷%öf–ÇFW%ö6öçG&öÆÆW%öÖF6†W2€¢g6VÆbÀ¢—FVÓ¢dÖ–F”—FVÒÀ¢WfVçC¢Ö–F”6öçG&öÆÆW$FFÀ¢’Óâ&ööÂ°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB÷6—F–öâÒÖ–F•öVF—F÷%öf–ÇFW%÷&WVE÷÷6—F–öå÷fÇVR€¢WfVçBçF–6²À¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢ÖV7W&U÷F–6·2À¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ç&WVEöWfW'’À¢“°¢Ö–F”VF—F÷$f–ÇFW%7FFR°¢6†ææVÅ÷6VÆV7F–öã¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤ÆÄ6†ææVÇ2À¢6†ææVÅöf–ÇFW%öVæ&ÆVC¢fÇ6RÀ¢WfVçEöf–ÇFW%öVæ&ÆVC¢G'VRÀ¢6†ææVÇ3¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2À¢–çfW'FVC¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö–çfW'FVBÀ¢WfVçE÷G—S¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"À¢æ÷FS¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"À¢6öçG&öÆÆW#¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"À¢—F6…ö&VæC¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"À¢&öw&Õö6†ævS¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"À¢÷6—F–öã¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"À¢Ð¢æ6öçG&öÆÆW%÷f—6–&ÆUöB†WfVçBæ6†ææVÂÂWfVçBæ6öçG&öÆÆW"ÂWfVçBçfÇVRÂ÷6—F–öâ¢Ð ¢fâÖ–F•öVF—F÷%öf–ÇFW%÷—F6…ö&VæEöÖF6†W2€¢g6VÆbÀ¢—FVÓ¢dÖ–F”—FVÒÀ¢WfVçC¢Ö–F•—F6„&VæDFFÀ¢’Óâ&ööÂ°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB÷6—F–öâÒÖ–F•öVF—F÷%öf–ÇFW%÷÷6—F–öå÷fÇVR†WfVçBçF–6²ÂÖV7W&U÷F–6·2“°¢Ö–F”VF—F÷$f–ÇFW%7FFR°¢6†ææVÅ÷6VÆV7F–öã¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤ÆÄ6†ææVÇ2À¢6†ææVÅöf–ÇFW%öVæ&ÆVC¢fÇ6RÀ¢WfVçEöf–ÇFW%öVæ&ÆVC¢G'VRÀ¢6†ææVÇ3¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2À¢–çfW'FVC¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö–çfW'FVBÀ¢WfVçE÷G—S¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"À¢æ÷FS¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"À¢6öçG&öÆÆW#¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"À¢—F6…ö&VæC¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"À¢&öw&Õö6†ævS¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"À¢÷6—F–öã¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"À¢Ð¢ç—F6…ö&VæE÷f—6–&ÆUöB†WfVçBæ6†ææVÂÂWfVçBçfÇVRÂ÷6—F–öâ¢Ð ¢fâÖ–F•öVF—F÷%öf–ÇFW%÷&öw&Õö6†ævUöÖF6†W2€¢g6VÆbÀ¢—FVÓ¢dÖ–F”—FVÒÀ¢WfVçC¢Ö–F•&öw&Ô6†ævTFFÀ¢’Óâ&ööÂ°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB÷6—F–öâÒÖ–F•öVF—F÷%öf–ÇFW%÷&WVE÷÷6—F–öå÷fÇVR€¢WfVçBçF–6²À¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢ÖV7W&U÷F–6·2À¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ç&WVEöWfW'’À¢“°¢Ö–F”VF—F÷$f–ÇFW%7FFR°¢6†ææVÅ÷6VÆV7F–öã¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤ÆÄ6†ææVÇ2À¢6†ææVÅöf–ÇFW%öVæ&ÆVC¢fÇ6RÀ¢WfVçEöf–ÇFW%öVæ&ÆVC¢G'VRÀ¢6†ææVÇ3¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2À¢–çfW'FVC¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö–çfW'FVBÀ¢WfVçE÷G—S¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"À¢æ÷FS¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"À¢6öçG&öÆÆW#¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"À¢—F6…ö&VæC¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"À¢&öw&Õö6†ævS¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"À¢÷6—F–öã¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"À¢Ð¢ç&öw&Õö6†ævU÷f—6–&ÆUöB†WfVçBæ6†ææVÂÂWfVçBç&öw&ÒÂ÷6—F–öâ¢Ð ¢fâÖ–F•öVF—F÷%öf–ÇFW%÷öÇ•ögFW'F÷V6…öÖF6†W2€¢g6VÆbÀ¢—FVÓ¢dÖ–F”—FVÒÀ¢WfVçC¢Ö–F•öÇ”gFW'F÷V6„FFÀ¢’Óâ&ööÂ°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB÷6—F–öâÒÖ–F•öVF—F÷%öf–ÇFW%÷&WVE÷÷6—F–öå÷fÇVR€¢WfVçBçF–6²À¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢ÖV7W&U÷F–6·2À¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ç&WVEöWfW'’À¢“°¢Ö–F”VF—F÷$f–ÇFW%7FFR°¢6†ææVÅ÷6VÆV7F–öã¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤ÆÄ6†ææVÇ2À¢6†ææVÅöf–ÇFW%öVæ&ÆVC¢fÇ6RÀ¢WfVçEöf–ÇFW%öVæ&ÆVC¢G'VRÀ¢6†ææVÇ3¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2À¢–çfW'FVC¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö–çfW'FVBÀ¢WfVçE÷G—S¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"À¢æ÷FS¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"À¢6öçG&öÆÆW#¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"À¢—F6…ö&VæC¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"À¢&öw&Õö6†ævS¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"À¢÷6—F–öã¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"À¢Ð¢çöÇ•ögFW'F÷V6…÷f—6–&ÆUöB†WfVçBæ6†ææVÂÂWfVçBææ÷FRÂWfVçBçfÇVRÂ÷6—F–öâ¢Ð ¢fâÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÅ÷&W77W&UöÖF6†W2€¢g6VÆbÀ¢—FVÓ¢dÖ–F”—FVÒÀ¢WfVçC¢Ö–F”6†ææVÅ&W77W&TFFÀ¢’Óâ&ööÂ°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢ÆWB÷6—F–öâÒÖ–F•öVF—F÷%öf–ÇFW%÷&WVE÷÷6—F–öå÷fÇVR€¢WfVçBçF–6²À¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢ÖV7W&U÷F–6·2À¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ç&WVEöWfW'’À¢“°¢Ö–F”VF—F÷$f–ÇFW%7FFR°¢6†ææVÅ÷6VÆV7F–öã¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤ÆÄ6†ææVÇ2À¢6†ææVÅöf–ÇFW%öVæ&ÆVC¢fÇ6RÀ¢WfVçEöf–ÇFW%öVæ&ÆVC¢G'VRÀ¢6†ææVÇ3¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2À¢–çfW'FVC¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö–çfW'FVBÀ¢WfVçE÷G—S¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"À¢æ÷FS¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"À¢6öçG&öÆÆW#¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"À¢—F6…ö&VæC¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"À¢&öw&Õö6†ævS¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"À¢÷6—F–öã¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"À¢Ð¢æ6†ææVÅ÷&W77W&U÷f—6–&ÆUöB†WfVçBæ6†ææVÂÂWfVçBçfÇVRÂ÷6—F–öâ¢Ð ¢fâÖ–F•öVF—F÷%öf–ÇFW%öæ÷FUöÖF6†W2€¢g6VÆbÀ¢—FVÓ¢dÖ–F”—FVÒÀ¢æ÷FS¢fFuö6÷&S£¤Ö–F”æ÷FRÀ¢’Óâ&ööÂ°¢ÆWBÖV7W&U÷F–6·2ÒÖ–F•öVF—F÷%öÖV7W&UöÆVæwF…÷F–6·2€¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢6VÆbç&ö¦V7BçF–ÖU÷6–væGW&UöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢“°¢Ö–F”VF—F÷$f–ÇFW%7FFR°¢6†ææVÅ÷6VÆV7F–öã¢Ö–F”VF—F÷$6†ææVÅ6VÆV7F–öã£¤ÆÄ6†ææVÇ2À¢6†ææVÅöf–ÇFW%öVæ&ÆVC¢fÇ6RÀ¢WfVçEöf–ÇFW%öVæ&ÆVC¢fÇ6RÀ¢6†ææVÇ3¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö6†ææVÇ2À¢–çfW'FVC¢6VÆbæÖ–F•öVF—F÷%öf–ÇFW%ö–çfW'FVBÀ¢WfVçE÷G—S¢6VÆbæÖ–F•öVF—F÷%öWfVçE÷G—Uöf–ÇFW"À¢æ÷FS¢6VÆbæÖ–F•öVF—F÷%öæ÷FUöf–ÇFW"À¢6öçG&öÆÆW#¢6VÆbæÖ–F•öVF—F÷%ö6öçG&öÆÆW%öf–ÇFW"À¢—F6…ö&VæC¢6VÆbæÖ–F•öVF—F÷%÷—F6…ö&VæEöf–ÇFW"À¢&öw&Õö6†ævS¢6VÆbæÖ–F•öVF—F÷%÷&öw&Õö6†ævUöf–ÇFW"À¢÷6—F–öã¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"À¢Ð¢ææ÷FU÷76W5öWfVçEöf–ÇFW"€¢æ÷FRæ6†ææVÂ‚’À¢Ö–F•öVF—F÷%öæ÷FUöf–ÇFW%öWfVçB€¢æ÷FRÀ¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’À¢ÖV7W&U÷F–6·2À¢6VÆbæÖ–F•öVF—F÷%÷÷6—F–öåöf–ÇFW"ç&WVEöWfW'’À¢’À¢¢Ð ¢fâ6VÆV7EöÖ–F•öæ÷FW2‚f×WB6VÆbÂæ÷FUö–G3¢†6…6WCÆFuö6÷&S£¤æ÷FT–Câ’°¢6VÆbæÖ–F•öVF—F÷%öfVVF&6²ÒæöæS°¢–b6VÆbæ¶W–&ö&EöÖöF–f–W'2æ6öÖÖæB‚’bb6VÆbæ¶W–&ö&EöÖöF–f–W'2æ6öçG&öÂ‚’°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6ÆV"‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2æ6ÆV"‚“°¢Ð¢ÆWBf—6–&ÆUöæ÷FUö–G2Ò6VÆbçf—6–&ÆUöÖ–F•öæ÷FUö–G2‚“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Òæ÷FUö–G0¢æ–çFõö—FW"‚¢æf–ÇFW"‡Ææ÷FUö–GÂf—6–&ÆUöæ÷FUö–G2æ6öçF–ç2†æ÷FUö–B’¢æ6öÆÆV7B‚“°¢Ð ¢fâVæFõö–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö–eöæW‡B‚f×WB6VÆb’Óâ&ööÂ°¢–b6VÆbæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö—5öæW‡E÷VæFò‚’°¢&WGW&âfÇ6S°¢Ð¢ÆWB6öÖR„†—7F÷'”VçG'“£¤–æÆ–æTÖ–F•6VÆV7F–öâ††—7F÷'’’’Ò6VÆbæ÷VæFõö†—7F÷'’ç÷‚¢VÇ6R°¢&WGW&âfÇ6S°¢Ó°¢6VÆbç&W7F÷&Uö–æÆ–æUöÖ–F•÷6VÆV7F–öåö†—7F÷'’‚f†—7F÷'’ÂfÇ6R“°¢6VÆbæ÷&VFõö†—7F÷'¢çW6‚„†—7F÷'”VçG'“£¤–æÆ–æTÖ–F•6VÆV7F–öâ††—7F÷'’’“°¢6VÆbç7FGW2Ò%VæF–BÔ”D’æ÷FR6VÆV7F–öâ"çFõö÷væVB‚“°¢G'VP¢Ð ¢fâ&VFõö–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö–eöæW‡B‚f×WB6VÆb’Óâ&ööÂ°¢–b6VÆbæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö—5öæW‡E÷&VFò‚’°¢&WGW&âfÇ6S°¢Ð¢ÆWB6öÖR„†—7F÷'”VçG'“£¤–æÆ–æTÖ–F•6VÆV7F–öâ††—7F÷'’’’Ò6VÆbæ÷&VFõö†—7F÷'’ç÷‚¢VÇ6R°¢&WGW&âfÇ6S°¢Ó°¢6VÆbç&W7F÷&Uö–æÆ–æUöÖ–F•÷6VÆV7F–öåö†—7F÷'’‚f†—7F÷'’ÂG'VR“°¢6VÆbæ÷VæFõö†—7F÷'¢çW6‚„†—7F÷'”VçG'“£¤–æÆ–æTÖ–F•6VÆV7F–öâ††—7F÷'’’“°¢6VÆbç7FGW2Ò%&VF–BÔ”D’æ÷FR6VÆV7F–öâ"çFõö÷væVB‚“°¢G'VP¢Ð ¢fâVæFõö—FVÕ÷6VÆV7F–öåö–eöæW‡B‚f×WB6VÆb’Óâ&ööÂ°¢–b6VÆbæ—FVÕ÷6VÆV7F–öåö—5öæW‡E÷VæFò‚’°¢&WGW&âfÇ6S°¢Ð¢ÆWB6öÖR„†—7F÷'”VçG'“£¤—FVÕ6VÆV7F–öâ°¢Æ&VÂÀ¢&Vf÷&RÀ¢gFW"À¢&ö¦V7Eö†—7F÷'•ö7W'6÷"À¢Ò’Ò6VÆbæ÷VæFõö†—7F÷'’ç÷‚¢VÇ6R°¢&WGW&âfÇ6S°¢Ó°¢6VÆbçF–ÖVÆ–æRç&W7F÷&Uö—FVÕ÷6VÆV7F–öå÷7FFR‚f&Vf÷&R“°¢6VÆbæ÷&VFõö†—7F÷'’çW6‚„†—7F÷'”VçG'“£¤—FVÕ6VÆV7F–öâ°¢Æ&VÂÀ¢&Vf÷&RÀ¢gFW"À¢&ö¦V7Eö†—7F÷'•ö7W'6÷"À¢Ò“°¢6VÆbç7FGW2Òf÷&ÖB‚%VæF–B¶Æ&VÇÒ"“°¢G'VP¢Ð ¢fâ&VFõö—FVÕ÷6VÆV7F–öåö–eöæW‡B‚f×WB6VÆb’Óâ&ööÂ°¢–b6VÆbæ—FVÕ÷6VÆV7F–öåö—5öæW‡E÷&VFò‚’°¢&WGW&âfÇ6S°¢Ð¢ÆWB6öÖR„†—7F÷'”VçG'“£¤—FVÕ6VÆV7F–öâ°¢Æ&VÂÀ¢&Vf÷&RÀ¢gFW"À¢&ö¦V7Eö†—7F÷'•ö7W'6÷"À¢Ò’Ò6VÆbæ÷&VFõö†—7F÷'’ç÷‚¢VÇ6R°¢&WGW&âfÇ6S°¢Ó°¢6VÆbçF–ÖVÆ–æRç&W7F÷&Uö—FVÕ÷6VÆV7F–öå÷7FFR‚fgFW"“°¢6VÆbæ÷VæFõö†—7F÷'’çW6‚„†—7F÷'”VçG'“£¤—FVÕ6VÆV7F–öâ°¢Æ&VÂÀ¢&Vf÷&RÀ¢gFW"À¢&ö¦V7Eö†—7F÷'•ö7W'6÷"À¢Ò“°¢6VÆbç7FGW2Òf÷&ÖB‚%&VF–B¶Æ&VÇÒ"“°¢G'VP¢Ð ¢fâVæFò‚f×WB6VÆb’°¢–b6VÆbçVæFõö–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö–eöæW‡B‚’°¢&WGW&ã°¢Ð¢–b6VÆbçVæFõö—FVÕ÷6VÆV7F–öåö–eöæW‡B‚’°¢&WGW&ã°¢Ð¢ÆWB–æÆ–æUöæ÷FU÷6VÆV7F–öåö†—7F÷'’Ò6VÆbæ–æÆ–æUöÖ–F•ö6÷•÷6VÆV7F–öåö—5öæW‡E÷VæFò‚“°¢ÆWB&ö¦V7Eö—FVÕ÷6VÆV7F–öåö†—7F÷'’Ò6VÆbç&ö¦V7Eö—FVÕ÷6VÆV7F–öåö—5öæW‡E÷VæFò‚“°¢ÆWB6VÆV7FVEög…ö–ç7Fæ6Uö–BÒ6VÆbç6VÆV7FVEög…ö6†–åö–ç7Fæ6Uö–B‚“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢ÆWBFV×õö&Vf÷&RÒ6VÆbç&ö¦V7BçFV×õ÷ö–çG2‚’æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢ÆWBg…öWFöÖF–öåö&Vf÷&RÒg…÷&ÖWFW%öWFöÖF–öå÷66†VGVÆR‚g6VÆbç&ö¦V7B“°¢–bÆWB6öÖR‡&ÖWFW%ö–B’Ò6VÆ`¢æg…÷&ÖWFW%övW7GW&P¢æ5÷&Vb‚¢æÖ‡ÆvW7GW&WÂvW7GW&Rç&ÖWFW%ö–B¢°¢6VÆbæVæEög…÷&ÖWFW%övW7GW&R‡&ÖWFW%ö–B“°¢Ð¢–b6VÆbæg…÷&ÖWFW%öVæE÷&WVW7FVBÇÂ6VÆbçVæF–æuög…÷&ÖWFW%÷7–æ2æ—5÷6öÖR‚’°¢6VÆbç7FGW2Ò%v—F–ærf÷"F†R7F—fR4Ä&ÖWFW"WFFRFòf–æ—6‚"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–b6VÆbæg…öWFöÖF–öå÷w&—FU÷F&vWBæ—5÷6öÖR‚’°¢6VÆbçVæF–æuög…öWFöÖF–öåö†—7F÷'’Ò6öÖR„g„WFöÖF–öä†—7F÷'”7F–öã£¥VæFò“°¢6VÆbæf–æ—6…ög…öWFöÖF–öå÷w&—FR‚“°¢–b6VÆbæg…öWFöÖF–öå÷w&—FU÷F&vWBæ—5÷6öÖR‚’°¢6VÆbç7FGW2Ò%v—F–ærf÷"F†Re‚WFöÖF–öâF¶RFòf–æ—6‚&Vf÷&RVæFò"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢6VÆbçVæF–æuög…öWFöÖF–öåö†—7F÷'’ÒæöæS°¢Ð¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2æ6ÆV"‚“°¢ÆWB6VÆV7FVEö6öçG&öÆÆW'2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6ÆöæR‚“°¢ÆWB6VÆV7FVE÷—F6…ö&VæG2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6ÆöæR‚“°¢ÆWB6VÆV7FVE÷&öw&Õö6†ævW2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6ÆöæR‚“°¢ÆWB6VÆV7FVE÷öÇ•ögFW'F÷V6†W2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ6ÆöæR‚“°¢ÆWB6VÆV7FVEö6†ææVÅ÷&W77W&W2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6ÆöæR‚“°¢ÆWB6VÆV7FVEö÷VUöWfVçG2Ò6VÆbç6VÆV7FVEöÖ–F•ö÷VUöWfVçEöFF‚“°¢ÆWBÖ–F•öVF—F÷%ö—FVÕö–BÒ6VÆbæÖ–F•öVF—F÷%ö—FVÕö–C°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢ÆWB6×ÆU÷&FUö&Vf÷&RÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚“°¢6VÆbç7FGW2ÒÖF6‚6VÆbç&ö¦V7BçVæFò‚’°¢ö²‡G'VR’Óâ°¢–bÆWB6öÖR†—FVÕö–B’ÒÖ–F•öVF—F÷%ö—FVÕö–@¢bbÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2Ò6VÆc£§&VÖ÷6VÆV7FVEö6öçG&öÆÆW%öWfVçG2€¢g6VÆV7FVEö6öçG&öÆÆW'2À¢—FVÒæ6öçG&öÆÆW'2‚’À¢“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2Ò6VÆc£§&VÖ÷6VÆV7FVE÷—F6…ö&VæEöWfVçG2€¢g6VÆV7FVE÷—F6…ö&VæG2À¢—FVÒç—F6…ö&VæG2‚’À¢“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2Ð¢6VÆc£§&VÖ÷6VÆV7FVEöÖ–F•÷ö–çEöWfVçG2€¢g6VÆV7FVE÷&öw&Õö6†ævW2À¢—FVÒç&öw&Õö6†ævW2‚’À¢ÆÆVgBÂ&–v‡GÂ°¢ÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÂbbÆVgBç&öw&ÒÓÒ&–v‡Bç&öw&Ð¢ÒÀ¢ÆÆVgBÂ&–v‡GÂÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÂÀ¢ÆWfVçGÂWfVçBçF–6²À¢“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2Ð¢6VÆc£§&VÖ÷6VÆV7FVEöÖ–F•÷ö–çEöWfVçG2€¢g6VÆV7FVE÷öÇ•ögFW'F÷V6†W2À¢—FVÒçöÇ•ögFW'F÷V6†W2‚’À¢ÆÆVgBÂ&–v‡GÂ°¢ÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÀ¢bbÆVgBææ÷FRÓÒ&–v‡Bææ÷FP¢bbÆVgBçfÇVRÓÒ&–v‡BçfÇVP¢ÒÀ¢ÆÆVgBÂ&–v‡GÂÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÂbbÆVgBææ÷FRÓÒ&–v‡Bææ÷FRÀ¢ÆWfVçGÂWfVçBçF–6²À¢“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2Ð¢6VÆc£§&VÖ÷6VÆV7FVEöÖ–F•÷ö–çEöWfVçG2€¢g6VÆV7FVEö6†ææVÅ÷&W77W&W2À¢—FVÒæ6†ææVÅ÷&W77W&W2‚’À¢ÆÆVgBÂ&–v‡GÂ°¢ÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÂbbÆVgBçfÇVRÓÒ&–v‡BçfÇVP¢ÒÀ¢ÆÆVgBÂ&–v‡GÂÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÂÀ¢ÆWfVçGÂWfVçBçF–6²À¢“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2Ð¢6VÆc£§&VÖ÷6VÆV7FVEöÖ–F•ö÷VUöWfVçG2€¢g6VÆV7FVEö÷VUöWfVçG2À¢—FVÒæ÷VUöWfVçG2‚’À¢“°¢Ð¢ÆWB&ö¦V7Eö†—7F÷'•öVçG'’Ò–b–æÆ–æUöæ÷FU÷6VÆV7F–öåö†—7F÷'’æ—5÷6öÖR‚¢ÇÂ&ö¦V7Eö—FVÕ÷6VÆV7F–öåö†—7F÷'’æ—5÷6öÖR‚¢ÇÂÖF6†W2‡6VÆbæ÷VæFõö†—7F÷'’æÆ7B‚’Â6öÖR„†—7F÷'”VçG'“£¥&ö¦V7B’¢°¢6VÆbæ÷VæFõö†—7F÷'’ç÷‚¢ÒVÇ6R°¢æöæP¢Ð¢çVçw&ö÷"„†—7F÷'”VçG'“£¥&ö¦V7B“°¢6VÆbæ÷&VFõö†—7F÷'’çW6‚‡&ö¦V7Eö†—7F÷'•öVçG'’“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–b6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’Ò6×ÆU÷&FUö&Vf÷&R°¢ÆWB6×ÆU÷&FUögFW"Ò6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚“°¢6VÆbçÆ–†VE÷6×ÆRÒ&W66ÆU÷G&ç7÷'E÷6×ÆR€¢6VÆbçÆ–†VE÷6×ÆRÀ¢6×ÆU÷&FUö&Vf÷&RÀ¢6×ÆU÷&FUögFW"À¢“°¢6VÆbçÆ–&6µ÷7F'E÷6×ÆRÒ&W66ÆU÷G&ç7÷'E÷6×ÆR€¢6VÆbçÆ–&6µ÷7F'E÷6×ÆRÀ¢6×ÆU÷&FUö&Vf÷&RÀ¢6×ÆU÷&FUögFW"À¢“°¢Ð¢6VÆbç&W7F÷&Uög…ö6†–å÷6VÆV7F–öâ‡6VÆV7FVEög…ö–ç7Fæ6Uö–B“°¢6VÆbæÖ–F•öæ÷FUö6Æ—&ö&BæÆ7E÷7FRÒæöæS°¢6VÆbç&Wf—6–öâÒ6VÆbç&Wf—6–öâçw&–æuöFBƒ“°¢6VÆbçF–ÖVÆ–æRç&V'V–ÆB‚g6VÆbç&ö¦V7B“°¢6VÆbç&Vg&W6…ö—FVÕ÷&÷W'F–W5÷F¶U÷6VÆV7F–öâ‚“°¢–bÆWB6öÖR††—7F÷'’’Ò&ö¦V7Eö—FVÕ÷6VÆV7F–öåö†—7F÷'’æ5÷&Vb‚’°¢6VÆbç&W7F÷&U÷&ö¦V7Eö—FVÕ÷6VÆV7F–öåö†—7F÷'’††—7F÷'’ÂfÇ6R“°¢Ð¢–bÆWB6öÖR††—7F÷'’’Ò–æÆ–æUöæ÷FU÷6VÆV7F–öåö†—7F÷'’æ5÷&Vb‚’°¢6VÆbç&W7F÷&Uö–æÆ–æUöÖ–F•÷6VÆV7F–öåö†—7F÷'’††—7F÷'’ÂfÇ6R“°¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢°¢6VÆbçÆ–&6µöw&…öF—'G’ÒG'VS°¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–b6VÆbç&ö¦V7BçFV×õ÷ö–çG2‚’ææR‡FV×õö&Vf÷&R’°¢6VÆbçÆ–&6µöw&…öF—'G’ÒG'VS°¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bg…öWFöÖF–öåö&Vf÷&RÒg…÷&ÖWFW%öWFöÖF–öå÷66†VGVÆR‚g6VÆbç&ö¦V7B’°¢6VÆbçÆ–&6µöw&…öF—'G’ÒG'VS°¢Ð¢6VÆbç7–æ5ög…÷&ÖWFW%ö66†Uög&öÕ÷&ö¦V7B‚“°¢$7F–öâVæFöæR"çFõö÷væVB‚¢Ð¢ö²†fÇ6R’Óâ$æ÷F†–ærFòVæFò"çFõö÷væVB‚’À¢W'"†W'&÷"’Óâf÷&ÖB‚%VæFòf–ÆVC¢¶W'&÷'Ò"’À¢Ó°¢Ð ¢fâ&VFò‚f×WB6VÆb’°¢–b6VÆbç&VFõö–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö–eöæW‡B‚’°¢&WGW&ã°¢Ð¢–b6VÆbç&VFõö—FVÕ÷6VÆV7F–öåö–eöæW‡B‚’°¢&WGW&ã°¢Ð¢ÆWB–æÆ–æUöæ÷FU÷6VÆV7F–öåö†—7F÷'’ÒÖF6‚6VÆbæ÷&VFõö†—7F÷'’æÆ7B‚’°¢6öÖR„†—7F÷'”VçG'“£¥&ö¦V7Ev—F„–æÆ–æTÖ–F•6VÆV7F–öâ††—7F÷'’’’Óâ6öÖR††—7F÷'’æ6ÆöæR‚’’À¢òÓâæöæRÀ¢Ó°¢ÆWB&ö¦V7Eö—FVÕ÷6VÆV7F–öåö†—7F÷'’Ò6VÆbç&ö¦V7Eö—FVÕ÷6VÆV7F–öåö—5öæW‡E÷&VFò‚“°¢ÆWB6VÆV7FVEög…ö–ç7Fæ6Uö–BÒ6VÆbç6VÆV7FVEög…ö6†–åö–ç7Fæ6Uö–B‚“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢ÆWBFV×õö&Vf÷&RÒ6VÆbç&ö¦V7BçFV×õ÷ö–çG2‚’æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢ÆWBg…öWFöÖF–öåö&Vf÷&RÒg…÷&ÖWFW%öWFöÖF–öå÷66†VGVÆR‚g6VÆbç&ö¦V7B“°¢–bÆWB6öÖR‡&ÖWFW%ö–B’Ò6VÆ`¢æg…÷&ÖWFW%övW7GW&P¢æ5÷&Vb‚¢æÖ‡ÆvW7GW&WÂvW7GW&Rç&ÖWFW%ö–B¢°¢6VÆbæVæEög…÷&ÖWFW%övW7GW&R‡&ÖWFW%ö–B“°¢Ð¢–b6VÆbæg…÷&ÖWFW%öVæE÷&WVW7FVBÇÂ6VÆbçVæF–æuög…÷&ÖWFW%÷7–æ2æ—5÷6öÖR‚’°¢6VÆbç7FGW2Ò%v—F–ærf÷"F†R7F—fR4Ä&ÖWFW"WFFRFòf–æ—6‚"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–b6VÆbæg…öWFöÖF–öå÷w&—FU÷F&vWBæ—5÷6öÖR‚’°¢6VÆbçVæF–æuög…öWFöÖF–öåö†—7F÷'’Ò6öÖR„g„WFöÖF–öä†—7F÷'”7F–öã£¥&VFò“°¢6VÆbæf–æ—6…ög…öWFöÖF–öå÷w&—FR‚“°¢–b6VÆbæg…öWFöÖF–öå÷w&—FU÷F&vWBæ—5÷6öÖR‚’°¢6VÆbç7FGW2Ò%v—F–ærf÷"F†Re‚WFöÖF–öâF¶RFòf–æ—6‚&Vf÷&R&VFò"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢6VÆbçVæF–æuög…öWFöÖF–öåö†—7F÷'’ÒæöæS°¢Ð¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2æ6ÆV"‚“°¢ÆWB6VÆV7FVEö6öçG&öÆÆW'2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6ÆöæR‚“°¢ÆWB6VÆV7FVE÷—F6…ö&VæG2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2æ6ÆöæR‚“°¢ÆWB6VÆV7FVE÷&öw&Õö6†ævW2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2æ6ÆöæR‚“°¢ÆWB6VÆV7FVE÷öÇ•ögFW'F÷V6†W2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2æ6ÆöæR‚“°¢ÆWB6VÆV7FVEö6†ææVÅ÷&W77W&W2Ò6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2æ6ÆöæR‚“°¢ÆWB6VÆV7FVEö÷VUöWfVçG2Ò6VÆbç6VÆV7FVEöÖ–F•ö÷VUöWfVçEöFF‚“°¢ÆWBÖ–F•öVF—F÷%ö—FVÕö–BÒ6VÆbæÖ–F•öVF—F÷%ö—FVÕö–C°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢ÆWB6×ÆU÷&FUö&Vf÷&RÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚“°¢6VÆbç7FGW2ÒÖF6‚6VÆbç&ö¦V7Bç&VFò‚’°¢ö²‡G'VR’Óâ°¢–bÆWB6öÖR†—FVÕö–B’ÒÖ–F•öVF—F÷%ö—FVÕö–@¢bbÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2Ò6VÆc£§&VÖ÷6VÆV7FVEö6öçG&öÆÆW%öWfVçG2€¢g6VÆV7FVEö6öçG&öÆÆW'2À¢—FVÒæ6öçG&öÆÆW'2‚’À¢“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2Ò6VÆc£§&VÖ÷6VÆV7FVE÷—F6…ö&VæEöWfVçG2€¢g6VÆV7FVE÷—F6…ö&VæG2À¢—FVÒç—F6…ö&VæG2‚’À¢“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2Ð¢6VÆc£§&VÖ÷6VÆV7FVEöÖ–F•÷ö–çEöWfVçG2€¢g6VÆV7FVE÷&öw&Õö6†ævW2À¢—FVÒç&öw&Õö6†ævW2‚’À¢ÆÆVgBÂ&–v‡GÂ°¢ÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÂbbÆVgBç&öw&ÒÓÒ&–v‡Bç&öw&Ð¢ÒÀ¢ÆÆVgBÂ&–v‡GÂÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÂÀ¢ÆWfVçGÂWfVçBçF–6²À¢“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2Ð¢6VÆc£§&VÖ÷6VÆV7FVEöÖ–F•÷ö–çEöWfVçG2€¢g6VÆV7FVE÷öÇ•ögFW'F÷V6†W2À¢—FVÒçöÇ•ögFW'F÷V6†W2‚’À¢ÆÆVgBÂ&–v‡GÂ°¢ÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÀ¢bbÆVgBææ÷FRÓÒ&–v‡Bææ÷FP¢bbÆVgBçfÇVRÓÒ&–v‡BçfÇVP¢ÒÀ¢ÆÆVgBÂ&–v‡GÂÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÂbbÆVgBææ÷FRÓÒ&–v‡Bææ÷FRÀ¢ÆWfVçGÂWfVçBçF–6²À¢“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2Ð¢6VÆc£§&VÖ÷6VÆV7FVEöÖ–F•÷ö–çEöWfVçG2€¢g6VÆV7FVEö6†ææVÅ÷&W77W&W2À¢—FVÒæ6†ææVÅ÷&W77W&W2‚’À¢ÆÆVgBÂ&–v‡GÂ°¢ÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÂbbÆVgBçfÇVRÓÒ&–v‡BçfÇVP¢ÒÀ¢ÆÆVgBÂ&–v‡GÂÆVgBæ6†ææVÂÓÒ&–v‡Bæ6†ææVÂÀ¢ÆWfVçGÂWfVçBçF–6²À¢“°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2Ð¢6VÆc£§&VÖ÷6VÆV7FVEöÖ–F•ö÷VUöWfVçG2€¢g6VÆV7FVEö÷VUöWfVçG2À¢—FVÒæ÷VUöWfVçG2‚’À¢“°¢Ð¢ÆWB&ö¦V7Eö†—7F÷'•öVçG'’Ò–b–æÆ–æUöæ÷FU÷6VÆV7F–öåö†—7F÷'’æ—5÷6öÖR‚¢ÇÂ&ö¦V7Eö—FVÕ÷6VÆV7F–öåö†—7F÷'’æ—5÷6öÖR‚¢ÇÂÖF6†W2‡6VÆbæ÷&VFõö†—7F÷'’æÆ7B‚’Â6öÖR„†—7F÷'”VçG'“£¥&ö¦V7B’¢°¢6VÆbæ÷&VFõö†—7F÷'’ç÷‚¢ÒVÇ6R°¢æöæP¢Ð¢çVçw&ö÷"„†—7F÷'”VçG'“£¥&ö¦V7B“°¢6VÆbæ÷VæFõö†—7F÷'’çW6‚‡&ö¦V7Eö†—7F÷'•öVçG'’“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–b6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’Ò6×ÆU÷&FUö&Vf÷&R°¢ÆWB6×ÆU÷&FUögFW"Ò6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚“°¢6VÆbçÆ–†VE÷6×ÆRÒ&W66ÆU÷G&ç7÷'E÷6×ÆR€¢6VÆbçÆ–†VE÷6×ÆRÀ¢6×ÆU÷&FUö&Vf÷&RÀ¢6×ÆU÷&FUögFW"À¢“°¢6VÆbçÆ–&6µ÷7F'E÷6×ÆRÒ&W66ÆU÷G&ç7÷'E÷6×ÆR€¢6VÆbçÆ–&6µ÷7F'E÷6×ÆRÀ¢6×ÆU÷&FUö&Vf÷&RÀ¢6×ÆU÷&FUögFW"À¢“°¢Ð¢6VÆbç&W7F÷&Uög…ö6†–å÷6VÆV7F–öâ‡6VÆV7FVEög…ö–ç7Fæ6Uö–B“°¢6VÆbæÖ–F•öæ÷FUö6Æ—&ö&BæÆ7E÷7FRÐ¢6VÆbæÖ–F•öVF—F÷%ö—FVÕö–BææE÷F†Vâ‡Æ—FVÕö–GÂ°¢6VÆbç6VÆV7FVEö6Æ—&ö&E÷7FU÷7F'B†—FVÕö–B¢æÖ‡Ç7F'E÷F–6·Â†—FVÕö–BÂ7F'E÷F–6²’¢Ò“°¢6VÆbç&Wf—6–öâÒ6VÆbç&Wf—6–öâçw&–æuöFBƒ“°¢6VÆbçF–ÖVÆ–æRç&V'V–ÆB‚g6VÆbç&ö¦V7B“°¢6VÆbç&Vg&W6…ö—FVÕ÷&÷W'F–W5÷F¶U÷6VÆV7F–öâ‚“°¢–bÆWB6öÖR††—7F÷'’’Ò&ö¦V7Eö—FVÕ÷6VÆV7F–öåö†—7F÷'’æ5÷&Vb‚’°¢6VÆbç&W7F÷&U÷&ö¦V7Eö—FVÕ÷6VÆV7F–öåö†—7F÷'’††—7F÷'’ÂG'VR“°¢Ð¢–bÆWB6öÖR††—7F÷'’’Ò–æÆ–æUöæ÷FU÷6VÆV7F–öåö†—7F÷'’æ5÷&Vb‚’°¢6VÆbç&W7F÷&Uö–æÆ–æUöÖ–F•÷6VÆV7F–öåö†—7F÷'’††—7F÷'’ÂG'VR“°¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢°¢6VÆbçÆ–&6µöw&…öF—'G’ÒG'VS°¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–b6VÆbç&ö¦V7BçFV×õ÷ö–çG2‚’ææR‡FV×õö&Vf÷&R’°¢6VÆbçÆ–&6µöw&…öF—'G’ÒG'VS°¢Ð¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bg…öWFöÖF–öåö&Vf÷&RÒg…÷&ÖWFW%öWFöÖF–öå÷66†VGVÆR‚g6VÆbç&ö¦V7B’°¢6VÆbçÆ–&6µöw&…öF—'G’ÒG'VS°¢Ð¢6VÆbç7–æ5ög…÷&ÖWFW%ö66†Uög&öÕ÷&ö¦V7B‚“°¢$7F–öâ&VFöæR"çFõö÷væVB‚¢Ð¢ö²†fÇ6R’Óâ$æ÷F†–ærFò&VFò"çFõö÷væVB‚’À¢W'"†W'&÷"’Óâf÷&ÖB‚%&VFòf–ÆVC¢¶W'&÷'Ò"’À¢Ó°¢Ð ¢fâ6VÆV7FVEö6Æ—&ö&E÷7FU÷7F'B‚g6VÆbÂ—FVÕö–C¢—FVÔ–B’Óâ÷F–öãÇScCâ°¢–b6VÆbæÖ–F•öæ÷FUö6Æ—&ö&Bææ÷FW2æ—5öV×G’‚’ÇÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ—5öV×G’‚’°¢&WGW&âæöæS°¢Ð¢ÆWB—FVÒÒ6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B“ó°¢ÆWB×WB6VÆV7FVBÒ—FVÐ¢ææ÷FW2‚¢æ—FW"‚¢æf–ÇFW"‡Ææ÷FWÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6öçF–ç2‚fæ÷FRæ–B‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–b6VÆV7FVBæÆVâ‚’Ò6VÆbæÖ–F•öæ÷FUö6Æ—&ö&Bææ÷FW2æÆVâ‚’°¢&WGW&âæöæS°¢Ð¢6VÆV7FVBç6÷'Eö'•ö¶W’‡Ææ÷FWÂ†æ÷FRçF–6²‚’Âæ÷FRæ6†ææVÂ‚’Âæ÷FRç—F6‚‚’’“°¢ÆWB7F'E÷F–6²Ò6VÆV7FVBæ—FW"‚’æÖ‡Ææ÷FWÂæ÷FRçF–6²‚’’æÖ–â‚“ó°¢ÆWBÖF6†W5ö6Æ—&ö&BÐ¢6VÆV7FV@¢æ—FW"‚¢ç¦—‚g6VÆbæÖ–F•öæ÷FUö6Æ—&ö&Bææ÷FW2¢æÆÂ‡Â†æ÷FRÂ6÷–VB—Â°¢æ÷FRçF–6²‚’Ò7F'E÷F–6²ÓÒ6÷–VBçF–6°¢bbæ÷FRæ6†ææVÂ‚’ÓÒ6÷–VBæ6†ææVÀ¢bbæ÷FRç—F6‚‚’ÓÒ6÷–VBç—F6€¢bbæ÷FRæGW&F–öâ‚’ÓÒ6÷–VBæGW&F–öà¢bbæ÷FRçfVÆö6—G’‚’ÓÒ6÷–VBçfVÆö6—G¢Ò“°¢ÖF6†W5ö6Æ—&ö&BçF†Vå÷6öÖR‡7F'E÷F–6²ç6GW&F–æu÷7V"†—FVÒç6÷W&6Uööfg6WE÷F–6·2‚’’¢Ð ¢fâGWÆ–6FU÷6VÆV7FVEöÖ–F•öæ÷FW2‚f×WB6VÆbÂ—FVÕö–C¢—FVÔ–B’°¢ÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6÷W&6U÷7F'E÷F–6²Ò—FVÒç6÷W&6Uööfg6WE÷F–6·2‚“°¢ÆWB—FVÕöVæE÷F–6²Ò6÷W&6U÷7F'E÷F–6²ç6GW&F–æuöFB†—FVÒæÆVæwF…÷F–6·2‚’“°¢ÆWB—FVÕöÆVæwF‚Ò—FVÒæÆVæwF…÷F–6·2‚“°¢ÆWB6VÆV7FVBÒ—FVÐ¢ææ÷FW2‚¢æ—FW"‚¢æf–ÇFW"‡Ææ÷FWÂ6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6öçF–ç2‚fæ÷FRæ–B‚’’¢æÖ‡Ææ÷FWÂ°¢€¢æ÷FRæ–B‚’À¢Fuö6÷&S£¤Ö–F”æ÷FTFF°¢6†ææVÃ¢æ÷FRæ6†ææVÂ‚’À¢—F6ƒ¢æ÷FRç—F6‚‚’À¢F–6³¢æ÷FRçF–6²‚’À¢GW&F–öã¢æ÷FRæGW&F–öâ‚’À¢fVÆö6—G“¢æ÷FRçfVÆö6—G’‚’À¢öfe÷fVÆö6—G“¢æ÷FRæöfe÷fVÆö6—G’‚’À¢ÒÀ¢¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6öÖR‡6÷W&6U÷7F'B’Ò6VÆV7FVBæ—FW"‚’æÖ‡Â…òÂæ÷FR—Âæ÷FRçF–6²’æÖ–â‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6÷W&6UöVæBÒ6VÆV7FV@¢æ—FW"‚¢æÖ‡Â…òÂæ÷FR—Âæ÷FRçF–6²ç6GW&F–æuöFB†æ÷FRæGW&F–öâ’¢æÖ‚‚¢çVçw&ö÷"‡6÷W&6U÷7F'B“°¢ÆWB7âÒ6÷W&6UöVæBç6GW&F–æu÷7V"‡6÷W&6U÷7F'B“°¢ÆWBw&–BÒ6VÆbæÖ–F•öVF—F÷%÷6æö–çFW'fÂ‚“°¢ÆWBF&vWBÒ6æ÷F–6µ÷W‡6÷W&6UöVæBÂw&–B“°¢ÆWBGWÆ–6FW2Ò6VÆV7FV@¢æ—FW"‚¢æÖ‡Â…òÂæ÷FR—ÂFuö6÷&S£¤Ö–F”æ÷FTFF°¢F–6³¢F&vWBç6GW&F–æuöFB†æ÷FRçF–6²ç6GW&F–æu÷7V"‡6÷W&6U÷7F'B’’À¢ââ¦æ÷FP¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–bGWÆ–6FW2æ—FW"‚’æç’‡Ææ÷FWÂ°¢æ÷FRçF–6²Â6÷W&6U÷7F'E÷F–6²ÇÂæ÷FRçF–6²ç6GW&F–æuöFB†æ÷FRæGW&F–öâ’â—FVÕöVæE÷F–6°¢Ò’°¢ÆWBfVVF&6²Ò$GWÆ–6FR&V¦V7FVC¢æ÷FW2v÷VÆBW‡FVæB&W–öæBF†RÔ”D’—FVÒ#°¢6VÆbç7FGW2ÒfVVF&6²çFõö÷væVB‚“°¢6VÆbæÖ–F•öVF—F÷%öfVVF&6²Ò6öÖR†fVVF&6²çFõö÷væVB‚’“°¢&WGW&ã°¢Ð ¢ÆWBöÆEö–G2Ò—FVÐ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂæ÷FRæ–B‚’¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢ÆWB&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢6VÆbæÖ–F•öVF—F÷%öfVVF&6²ÒæöæS°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢GWÆ–6FW2À¢ÒÀ¢$Ô”D’æ÷FW2GWÆ–6FVB"À¢“°¢–b6VÆbç&Wf—6–öâÒ&Wf—6–öâ°¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢6VÆbæÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò—FVÐ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂæ÷FRæ–B‚’¢æf–ÇFW"‡Æ–GÂöÆEö–G2æ6öçF–ç2†–B’¢æ6öÆÆV7B‚“°¢Ð¢6VÆbæÖ–F•öæ÷FUö6Æ—&ö&BæÆ7E÷7FRÒæöæS°¢6VÆbæÖ–F•öVF—F÷%öVF—Eö7W'6÷%÷F–6²Ò6öÖR€¢6æ÷F–6µ÷W‡F&vWBç6GW&F–æuöFB‡7â’Âw&–B¢ç6GW&F–æu÷7V"‡6÷W&6U÷7F'E÷F–6²¢æÖ–â†—FVÕöÆVæwF‚’À¢“°¢Ð¢Ð ¢fâÖ–F•öVF—F÷%÷6æö–çFW'fÂ‚g6VÆb’ÓâScB°¢–b6VÆbçF–ÖVÆ–æRç6æöVæ&ÆVB°¢6VÆbçF–ÖVÆ–æP¢æÖ–F•öVF—F÷%÷6æöw&–@¢çF–6µö–çFW'fÂ‡6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’¢çVçw&ö÷"ƒ¢ÒVÇ6R°¢¢Ð¢Ð ¢fâÖ–F•öVF—F÷%÷7FU÷F&vWE÷F–6²‚g6VÆbÂ—FVÕö–C¢÷F–öãÄ—FVÔ–Câ’ÓâScB°¢ÆWBw&–BÒ6VÆbæÖ–F•öVF—F÷%÷6æö–çFW'fÂ‚“°¢–bÆWB6öÖR‚…òÂ&Wf–÷W2’’Ò6VÆ`¢æÖ–F•öæ÷FUö6Æ—&ö&@¢æÆ7E÷7FP¢æf–ÇFW"‡Â†Æ7Eö—FVÒÂò—Â6öÖR‚¦Æ7Eö—FVÒ’ÓÒ—FVÕö–B¢°¢&WGW&â6æ÷F–6µ÷W€¢&Wf–÷W2ç6GW&F–æuöFB‡6VÆbæÖ–F•öæ÷FUö6Æ—&ö&Bç7å÷F–6·2’À¢w&–BÀ¢“°¢Ð¢6VÆbæÖ–F•öVF—F÷%öVF—Eö7W'6÷%÷F–6°¢æ÷"‡6VÆbæÖ–F•öæ÷FUö6Æ—&ö&BæFVfVÇE÷7FU÷F–6²¢æÖö÷"ƒÂÇF–6·ÂF–6²ç6GW&F–æuöFB†w&–Bò"’òw&–B¢w&–B¢Ð ¢fâ÷VåöÖ–åöÖVçR‚f×WB6VÆbÂÖVçS¢Ö–äÖVçR’ÓâF6³ÄÖW76vSâ°¢6VÆbæ7F—fUöÖVçRÒ6öÖR†ÖVçR“°¢6VÆbç&V6VçE÷&ö¦V7G5öÖVçUö÷VâÒfÇ6S°¢6VÆbç&ö¦V7E÷FV×ÆFW5öÖVçUö÷VâÒfÇ6S°¢6VÆbæÖVçU÷6VÆV7FVEö6öÖÖæBÒ6VÆbæÖVçUö6öÖÖæEö–G2†ÖVçR’æf—'7B‚’æ6÷–VB‚“°¢6VÆbæ7F–öåöÖVçU÷67&öÆÅööfg6WBÒã°¢–bÖVçRÓÒÖ–äÖVçS£¤7F–öç2°¢F6³£¦&F6‚…°¢–6VC£§v–FvWC£¦÷W&F–öã£¦fö7W2†ÖW76vW3£¦7F–öå÷6V&6…ö–çWEö–B‚’’À¢67&öÆÅ÷v–FvWE÷Fò‚&FrÖ7F–öç2ÖÖVçR×&W7VÇG2"Âã’À¢Ò¢ÒVÇ6R°¢F6³£¦æöæR‚¢Ð¢Ð ¢fâÖVçUö6öÖÖæEö–G2‚g6VÆbÂÖVçS¢Ö–äÖVçR’ÓâfV3Ä6öÖÖæD–Câ°¢ÆWBVçG&–W2Ò–bÖVçRÓÒÖ–äÖVçS£¤7F–öç2°¢6öÖÖæG3£¦ÖF6†–æuö7F–öç5öÖVçR‡6VÆbÂg6VÆbæ7F–öå÷VW'’¢ÒVÇ6R°¢6öÖÖæG3£¦f÷%öÖVçR‡6VÆbÂÖVçR¢Ó°¢VçG&–W0¢æ–çFõö—FW"‚¢æf–ÇFW"‡ÆVçG'—ÂVçG'’æVæ&ÆVB¢æÖ‡ÆVçG'—ÂVçG'’æ–B¢æ6öÆÆV7B‚¢Ð ¢fâæf–vFUöÖ–åöÖVçR‚f×WB6VÆbÂæf–vF–öã¢ÖVçTæf–vF–öâ’ÓâF6³ÄÖW76vSâ°¢6öç7BÔTåU3¢´Ö–äÖVçS²•ÒÒ°¢Ö–äÖVçS£¤f–ÆRÀ¢Ö–äÖVçS£¤VF—BÀ¢Ö–äÖVçS£¥f–WrÀ¢Ö–äÖVçS£¤–ç6W'BÀ¢Ö–äÖVçS£¤—FVÒÀ¢Ö–äÖVçS£¥G&6²À¢Ö–äÖVçS£¤÷F–öç2À¢Ö–äÖVçS£¤7F–öç2À¢Ö–äÖVçS£¤†VÇÀ¢Ó°¢ÖF6‚æf–vF–öâ°¢ÖVçTæf–vF–öã£¤÷VäÖVçR†ÖVçR’Óâ6VÆbæ÷VåöÖ–åöÖVçR†ÖVçR’À¢ÖVçTæf–vF–öã£¤æW‡DÖVçRÂÖVçTæf–vF–öã£¥&Wf–÷W4ÖVçRÓâ°¢ÆWBöå÷&V6VçE÷&ö¦V7G5÷&VçBÐ¢6VÆbæÖVçU÷6VÆV7FVEö6öÖÖæBÓÒ6öÖR„6öÖÖæD–C£¤÷Vå&V6VçE&ö¦V7G2“°¢ÆWBöå÷&ö¦V7E÷FV×ÆFW5÷&VçBÐ¢6VÆbæÖVçU÷6VÆV7FVEö6öÖÖæBÓÒ6öÖR„6öÖÖæD–C£¤÷Vå&ö¦V7EFV×ÆFW2“°¢ÆWBVçFW%÷&V6VçE÷&ö¦V7G2Ð¢æf–vF–öâÓÒÖVçTæf–vF–öã£¤æW‡DÖVçRbböå÷&V6VçE÷&ö¦V7G5÷&VçC°¢ÆWBVçFW%÷&ö¦V7E÷FV×ÆFW2Ð¢æf–vF–öâÓÒÖVçTæf–vF–öã£¤æW‡DÖVçRbböå÷&ö¦V7E÷FV×ÆFW5÷&VçC°¢ÆWBÆVfU÷&V6VçE÷&ö¦V7G2Ð¢æf–vF–öâÓÒÖVçTæf–vF–öã£¥&Wf–÷W4ÖVçRbb6VÆbç&V6VçE÷&ö¦V7G5öÖVçUö÷Vã°¢ÆWBÆVfU÷&ö¦V7E÷FV×ÆFW2Ð¢æf–vF–öâÓÒÖVçTæf–vF–öã£¥&Wf–÷W4ÖVçRbb6VÆbç&ö¦V7E÷FV×ÆFW5öÖVçUö÷Vã°¢–b6VÆbæ7F—fUöÖVçRÓÒ6öÖR„Ö–äÖVçS£¤f–ÆR¢bb†VçFW%÷&V6VçE÷&ö¦V7G0¢ÇÂVçFW%÷&ö¦V7E÷FV×ÆFW0¢ÇÂÆVfU÷&V6VçE÷&ö¦V7G0¢ÇÂÆVfU÷&ö¦V7E÷FV×ÆFW2¢°¢6VÆbç&V6VçE÷&ö¦V7G5öÖVçUö÷VâÒVçFW%÷&V6VçE÷&ö¦V7G3°¢6VÆbç&ö¦V7E÷FV×ÆFW5öÖVçUö÷VâÒVçFW%÷&ö¦V7E÷FV×ÆFW3°¢6VÆbæÖVçU÷6VÆV7FVEö6öÖÖæBÐ¢6öÖR†–bVçFW%÷&ö¦V7E÷FV×ÆFW2ÇÂÆVfU÷&ö¦V7E÷FV×ÆFW2°¢6öÖÖæD–C£¤÷Vå&ö¦V7EFV×ÆFW0¢ÒVÇ6R°¢6öÖÖæD–C£¤÷Vå&V6VçE&ö¦V7G0¢Ò“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ÆWBF—&V7F–öâÒ–bæf–vF–öâÓÒÖVçTæf–vF–öã£¤æW‡DÖVçR°¢ö—6—¦P¢ÒVÇ6R°¢Óö—6—¦P¢Ó°¢ÆWB7W'&VçBÒ6VÆ`¢æ7F—fUöÖVçP¢ææE÷F†Vâ‡Æ7F—fWÂÔTåU2æ—FW"‚’ç÷6—F–öâ‡ÆÖVçWÂ¦ÖVçRÓÒ7F—fR’¢çVçw&ö÷"ƒ“°¢ÆWBæW‡BÒ–bF—&V7F–öââ°¢†7W'&VçB²’RÔTåU2æÆVâ‚¢ÒVÇ6R°¢†7W'&VçB²ÔTåU2æÆVâ‚’Ò’RÔTåU2æÆVâ‚¢Ó°¢6VÆbæ÷VåöÖ–åöÖVçR„ÔTåU5¶æW‡EÒ¢Ð¢ÖVçTæf–vF–öã£¤f—'7D6öÖÖæ@¢ÂÖVçTæf–vF–öã£¤Æ7D6öÖÖæ@¢ÂÖVçTæf–vF–öã£¤æW‡D6öÖÖæ@¢ÂÖVçTæf–vF–öã£¥&Wf–÷W46öÖÖæBÓâ°¢ÆWB6öÖR†ÖVçR’Ò6VÆbæ7F—fUöÖVçRVÇ6R°¢&WGW&âF6³£¦æöæR‚“°¢Ó°¢–bÖVçRÓÒÖ–äÖVçS£¤f–ÆP¢bb‡6VÆbç&V6VçE÷&ö¦V7G5öÖVçUö÷VâÇÂ6VÆbç&ö¦V7E÷FV×ÆFW5öÖVçUö÷Vâ¢°¢ÆWB&VçEö6öÖÖæBÒ–b6VÆbç&ö¦V7E÷FV×ÆFW5öÖVçUö÷Vâ°¢6öÖÖæD–C£¤÷Vå&ö¦V7EFV×ÆFW0¢ÒVÇ6R°¢6öÖÖæD–C£¤÷Vå&V6VçE&ö¦V7G0¢Ó°¢ÆWB7V&ÖVçUö6öÖÖæG2Ò–b6VÆbç&ö¦V7E÷FV×ÆFW5öÖVçUö÷Vâ°¢6öÖÖæG3£§&ö¦V7E÷FV×ÆFUöVçG&–W2‡6VÆb¢ÒVÇ6R°¢6öÖÖæG3£§&V6VçE÷&ö¦V7EöVçG&–W2‡6VÆb¢Ð¢æ–çFõö—FW"‚¢æf–ÇFW"‡ÆVçG'—ÂVçG'’æVæ&ÆVB¢æÖ‡ÆVçG'—ÂVçG'’æ–B¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–b7V&ÖVçUö6öÖÖæG2æ—5öV×G’‚’°¢6VÆbæÖVçU÷6VÆV7FVEö6öÖÖæBÒ6öÖR‡&VçEö6öÖÖæB“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ÆWB7W'&VçBÒ6VÆbæÖVçU÷6VÆV7FVEö6öÖÖæBææE÷F†Vâ‡Ç6VÆV7FVGÂ°¢7V&ÖVçUö6öÖÖæG0¢æ—FW"‚¢ç÷6—F–öâ‡Æ6öÖÖæGÂ¦6öÖÖæBÓÒ6VÆV7FVB¢Ò“°¢ÆWBæW‡BÒÖVçUö6öÖÖæEö–æFW‚†7W'&VçBÂ7V&ÖVçUö6öÖÖæG2æÆVâ‚’Âæf–vF–öâ“°¢6VÆbæÖVçU÷6VÆV7FVEö6öÖÖæBÒ6öÖR‡7V&ÖVçUö6öÖÖæG5¶æW‡EÒ“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ÆWB6öÖÖæG2Ò6VÆbæÖVçUö6öÖÖæEö–G2†ÖVçR“°¢–b6öÖÖæG2æ—5öV×G’‚’°¢6VÆbæÖVçU÷6VÆV7FVEö6öÖÖæBÒæöæS°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ÆWB7W'&VçBÒ6VÆ`¢æÖVçU÷6VÆV7FVEö6öÖÖæ@¢ææE÷F†Vâ‡Ç6VÆV7FVGÂ6öÖÖæG2æ—FW"‚’ç÷6—F–öâ‡Æ6öÖÖæGÂ¦6öÖÖæBÓÒ6VÆV7FVB’“°¢ÆWBæW‡BÒÖVçUö6öÖÖæEö–æFW‚†7W'&VçBÂ6öÖÖæG2æÆVâ‚’Âæf–vF–öâ“°¢6VÆbæÖVçU÷6VÆV7FVEö6öÖÖæBÒ6öÖR†6öÖÖæG5¶æW‡EÒ“°¢–bÖVçRÓÒÖ–äÖVçS£¤7F–öç2°¢67&öÆÅö7F–öç5öÖVçU÷6VÆV7F–öâ‡6VÆbÂ6öÖÖæG5¶æW‡EÒ¢ÒVÇ6R°¢F6³£¦æöæR‚¢Ð¢Ð¢ÖVçTæf–vF–öã£¤7F—fFRÓâ°¢–bÆWB6öÖR†6öÖÖæB’Ò6VÆ`¢æÖVçU÷6VÆV7FVEö6öÖÖæ@¢æf–ÇFW"‡Æ6öÖÖæGÂ6öÖÖæG3£¦—5öVæ&ÆVB‡6VÆbÂ¦6öÖÖæB’¢°¢6VÆbçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†6öÖÖæB’¢ÒVÇ6R–b6VÆbæ7F—fUöÖVçRÓÒ6öÖR„Ö–äÖVçS£¤7F–öç2’°¢6VÆbç'Våö7F–öå÷VW'’‚¢ÒVÇ6R°¢F6³£¦æöæR‚¢Ð¢Ð¢Ð¢Ð ¢fâ'Våö7F–öå÷VW'’‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢ÆWB6VÆV7FVBÒ‡6VÆbæ7F—fUöÖVçRÓÒ6öÖR„Ö–äÖVçS£¤7F–öç2’¢çF†Vå÷6öÖR‡6VÆbæÖVçU÷6VÆV7FVEö6öÖÖæB¢æfÆGFVâ‚“°¢–bÆWB6öÖR†6öÖÖæB’Ò6VÆV7FV@¢æf–ÇFW"‡Æ6öÖÖæGÂ6öÖÖæG3£¦—5öVæ&ÆVB‡6VÆbÂ¦6öÖÖæB’¢æ÷%öVÇ6R‡ÇÂ6öÖÖæG3£¦f–æB‡6VÆbÂg6VÆbæ7F–öå÷VW'’’¢°¢6VÆbçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†6öÖÖæB’¢ÒVÇ6R°¢6VÆbç7FGW2Ð¢%Væ¶æ÷vâ7F–öââ6V&6‚f÷"6öÖÖæB÷"6†ö÷6RöæRg&öÒF†R7F–öç2ÖVçRâ ¢çFõö÷væVB‚“°¢F6³£¦æöæR‚¢Ð¢Ð ¢fâFEö7F–öåöÖ7&õ÷7FW‚f×WB6VÆb’°¢–b6VÆbæ7F–öåöÖ7&õ÷7FW2æÆVâ‚’ãÒ7F–öåöÖ7&÷3£¤Ô…ôÔ5$õõ5DU2°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Òf÷&ÖB€¢$Ö7&ò6â6öçF–âBÖ÷7B·Ò7FW2"À¢7F–öåöÖ7&÷3£¤Ô…ôÔ5$õõ5DU0¢“°¢&WGW&ã°¢Ð¢ÆWB6öÖR‡7FW’Ò6VÆbæ7F–öåöÖ7&õ÷7FWæ6ÆöæR‚’VÇ6R°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Ò$6†ö÷6R7W÷'FVB7F–öâf—'7B"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢–b6öÖÖæG3£¦Ö7&õ÷7FWö–G2‚’æ6öçF–ç2‚g7FW’°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Ò%F†B7F–öâ6ææ÷B&RW6VB–âÖ7&ò"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢6VÆbæ7F–öåöÖ7&õ÷7FW2çW6‚‡7FW“°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²æ6ÆV"‚“°¢Ð ¢fâVF—Eö7F–öåöÖ7&ò‚f×WB6VÆbÂ–C¢ScB’°¢–bÆWB6öÖR†7F–öåöÖ7&ò’Ò6VÆ`¢æ7F–öåöÖ7&÷0¢æ—FW"‚¢æf–æB‡Æ7F–öåöÖ7&÷Â7F–öåöÖ7&òæ–BÓÒ–B¢°¢6VÆbæ7F–öåöÖ7&õöVF—F–æuö–BÒ6öÖR†–B“°¢6VÆbæ7F–öåöÖ7&õöæÖRÒ7F–öåöÖ7&òææÖRæ6ÆöæR‚“°¢6VÆbæ7F–öåöÖ7&õ÷7FW2Ò7F–öåöÖ7&òç7FW2æ6ÆöæR‚“°¢6VÆbæ7F–öåöÖ7&õ÷7FWÒ6öÖÖæG3£¦Ö7&õ÷7FWö6†ö–6W2‚¢æf—'7B‚¢æÖ‡Æ6†ö–6WÂ6†ö–6Ræ–Bæ6ÆöæR‚’“°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²æ6ÆV"‚“°¢Ð¢Ð ¢fâ6fUö7F–öåöÖ7&ò‚f×WB6VÆb’°¢–b6VÆbæ7F–öåöÖ7&õö6öæf–uöW'&÷"æ—5÷6öÖR‚’°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Ð¢$f—‚F†R7F–öâÖ7&ò6öæf–rf–ÆR&Vf÷&R6f–ær"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB–BÒÖF6‚6VÆbæ7F–öåöÖ7&õöVF—F–æuö–B°¢6öÖR†–B’Óâ–BÀ¢æöæRÓâÖF6‚6VÆ`¢æ7F–öåöÖ7&÷0¢æ—FW"‚¢æÖ‡Æ7F–öåöÖ7&÷Â7F–öåöÖ7&òæ–B¢æÖ‚‚¢çVçw&ö÷"ƒ¢æ6†V6¶VEöFBƒ¢°¢6öÖR†–B’Óâ–BÀ¢æöæRÓâ°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Ò$æòÖ÷&RÖ7&ò”G2&Rf–Æ&ÆR"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÒÀ¢Ó°¢ÆWB7F–öåöÖ7&òÒ7F–öåöÖ7&÷3£¤7F–öäÖ7&ò°¢–BÀ¢æÖS¢6VÆbæ7F–öåöÖ7&õöæÖRçG&–Ò‚’çFõö÷væVB‚’À¢7FW3¢6VÆbæ7F–öåöÖ7&õ÷7FW2æ6ÆöæR‚’À¢Ó°¢ÆWB×WB6æF–FFRÒ6VÆbæ7F–öåöÖ7&÷2æ6ÆöæR‚“°¢–bÆWB6öÖR†–æFW‚’Ò6æF–FFRæ—FW"‚’ç÷6—F–öâ‡ÆW†—7F–æwÂW†—7F–æræ–BÓÒ–B’°¢6æF–FFU¶–æFW…ÒÒ7F–öåöÖ7&ó°¢ÒVÇ6R°¢6æF–FFRçW6‚†7F–öåöÖ7&ò“°¢Ð¢ÆWB6æF–FFRÒÖF6‚6öÖÖæG3£§fÆ–FFUö7F–öåöÖ7&÷2†6æF–FFR’°¢ö²†6æF–FFR’Óâ6æF–FFRÀ¢W'"†W'&÷"’Óâ°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Òf÷&ÖB‚$Ö7&òv2æ÷B6fVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÖF6‚7F–öåöÖ7&÷3£§6fR‚f6æF–FFR’°¢ö²‚‚’’Óâ°¢6VÆbæ7F–öåöÖ7&÷2Ò6æF–FFS°¢6VÆbæ7F–öåöÖ7&õöVF—F–æuö–BÒ6öÖR†–B“°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Ò$Ö7&ò6fVB"çFõö÷væVB‚“°¢6VÆbç7FGW2Ò$7F–öâÖ7&ò6fVB"çFõö÷væVB‚“°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Òf÷&ÖB‚$Ö7&ò6÷VÆBæ÷B&R6fVC¢¶W'&÷'Ò"“°¢Ð¢Ð¢Ð ¢fâFVÆWFUö7F–öåöÖ7&ò‚f×WB6VÆbÂ–C¢ScB’°¢–b6VÆbæ7F–öåöÖ7&õö6öæf–uöW'&÷"æ—5÷6öÖR‚’°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Ð¢$f—‚F†R7F–öâÖ7&ò6öæf–rf–ÆR&Vf÷&RVF—F–ær"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢–b6VÆ`¢æ7F–öåöÖ7&÷0¢æ—FW"‚¢æç’‡Æ7F–öåöÖ7&÷Â7F–öåöÖ7&òæ–BÓÒ–B¢°¢&WGW&ã°¢Ð¢ÆWB×WB6æF–FFRÒ6VÆbæ7F–öåöÖ7&÷2æ6ÆöæR‚“°¢6æF–FFRç&WF–â‡Æ7F–öåöÖ7&÷Â7F–öåöÖ7&òæ–BÒ–B“°¢ÆWB×WB6fVEö&–æF–æw2ÒÖF6‚¶W–&ö&Eö6öæf–s£¦ÆöB‚’°¢ö²†&–æF–æw2’Óâ&–æF–æw2À¢W'"†W'&÷"’Óâ°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Ð¢f÷&ÖB‚%6†÷'F7WB6öæf–r6÷VÆBæ÷B&RÆöFVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢6fVEö&–æF–æw2ç&VÖ÷fR‚f6öÖÖæG3£¦Ö7&õö–B†–B’“°¢ÆWB6fVEö&–æF–æw2Ð¢ÖF6‚6öÖÖæG3£§fÆ–FFUö&–æF–æw5÷v—F…öÖ7&÷2‚g6fVEö&–æF–æw2Âf6æF–FFR’°¢ö²†&–æF–æw2’Óâ&–æF–æw2À¢W'"†W'&÷"’Óâ°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Òf÷&ÖB‚$Ö7&ò6÷VÆBæ÷B&RFVÆWFVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB×WB7FvVEö&–æF–æw2Ò6VÆbç6†÷'F7WEö&–æF–æuöVF—G2æ6ÆöæR‚“°¢7FvVEö&–æF–æw2ç&VÖ÷fR‚f6öÖÖæG3£¦Ö7&õö–B†–B’“°¢ÆWB7FvVEö&–æF–æw2Ð¢ÖF6‚6öÖÖæG3£§fÆ–FFUö&–æF–æw5÷v—F…öÖ7&÷2‚g7FvVEö&–æF–æw2Âf6æF–FFR’°¢ö²†&–æF–æw2’Óâ&–æF–æw2À¢W'"†W'&÷"’Óâ°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Òf÷&ÖB‚$Ö7&ò6÷VÆBæ÷B&RFVÆWFVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢–bÆWBW'"†W'&÷"’Ò7F–öåöÖ7&÷3£§6fR‚f6æF–FFR’°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Òf÷&ÖB‚$Ö7&ò6÷VÆBæ÷B&RFVÆWFVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢–bÆWBW'"†W'&÷"’Ò¶W–&ö&Eö6öæf–s£§6fR‚g6fVEö&–æF–æw2’°¢ÆWB&öÆÆ&6²Ò7F–öåöÖ7&÷3£§6fR‚g6VÆbæ7F–öåöÖ7&÷2“°¢6VÆbæ7F–öåöÖ7&õöfVVF&6²ÒÖF6‚&öÆÆ&6²°¢ö²‚‚’’Óâf÷&ÖB‚$Ö7&ò6÷VÆBæ÷B&RFVÆWFVC¢6†÷'F7WB6fRf–ÆVC¢¶W'&÷'Ò"’À¢W'"‡&öÆÆ&6µöW'&÷"’Óâf÷&ÖB€¢$Ö7&òFVÆWF–öâ'F–ÆÇ’f–ÆVC¢6†÷'F7WB6fRf–ÆVC¢¶W'&÷'Ó²Ö7&ò&öÆÆ&6²f–ÆVC¢·&öÆÆ&6µöW'&÷'Ò ¢’À¢Ó°¢&WGW&ã°¢Ð¢ÆWB×WB7F—fUö&–æF–æw2Ò6VÆ`¢ç6†÷'F7WEö&–æF–æw0¢ç&VB‚¢çVçw&ö÷%öVÇ6R‡7FC£§7–æ3£¥ö—6öäW'&÷#£¦–çFõö–ææW"¢æ6ÆöæR‚“°¢7F—fUö&–æF–æw2ç&VÖ÷fR‚f6öÖÖæG3£¦Ö7&õö–B†–B’“°¢6VÆbæ7F–öåöÖ7&÷2Ò6æF–FFS°¢6VÆbç6†÷'F7WEö&–æF–æuöVF—G2Ò7FvVEö&–æF–æw3°¢6VÆbç6†÷'F7WEöFVfVÇG5÷&W7F÷&V@¢ç&VÖ÷fR‚f6öÖÖæG3£¦Ö7&õö–B†–B’“°¢§6VÆ`¢ç6†÷'F7WEö&–æF–æw0¢çw&—FR‚¢çVçw&ö÷%öVÇ6R‡7FC£§7–æ3£¥ö—6öäW'&÷#£¦–çFõö–ææW"’Ò7F—fUö&–æF–æw3°¢–b6VÆbæ7F–öåöÖ7&õöVF—F–æuö–BÓÒ6öÖR†–B’°¢6VÆbæ7F–öåöÖ7&õöVF—F–æuö–BÒæöæS°¢6VÆbæ7F–öåöÖ7&õöæÖRæ6ÆV"‚“°¢6VÆbæ7F–öåöÖ7&õ÷7FW2æ6ÆV"‚“°¢Ð¢6VÆbæ7F–öåöÖ7&õöfVVF&6²Ò$Ö7&òFVÆWFVB"çFõö÷væVB‚“°¢6VÆbç7FGW2Ò$7F–öâÖ7&òFVÆWFVB"çFõö÷væVB‚“°¢Ð ¢fâ6fU÷6†÷'F7WEö&–æF–æw2‚f×WB6VÆb’°¢ÆWB&–æF–æw2ÒÖF6‚6öÖÖæG3£§fÆ–FFUö&–æF–æw5÷v—F…öÖ7&÷2€¢g6VÆbç6†÷'F7WEö&–æF–æuöVF—G2À¢g6VÆbæ7F–öåöÖ7&÷2À¢’°¢ö²†&–æF–æw2’Óâ&–æF–æw2À¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%6†÷'F7WB&–æF–æw2æ÷B6fVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÖF6‚¶W–&ö&Eö6öæf–s£§6fR‚f&–æF–æw2’°¢ö²‚‚’’Óâ°¢6VÆbç6†÷'F7WEö6GW&Uö–BÒæöæS°¢6VÆbç6†÷'F7WEöFVfVÇG5÷&W7F÷&VBæ6ÆV"‚“°¢§6VÆ`¢ç6†÷'F7WEö&–æF–æw0¢çw&—FR‚¢çVçw&ö÷%öVÇ6R‡7FC£§7–æ3£¥ö—6öäW'&÷#£¦–çFõö–ææW"’Ò&–æF–æw2æ6ÆöæR‚“°¢6VÆbç6†÷'F7WEö&–æF–æuöVF—G2Ò&–æF–æw3°¢6VÆbç7FGW2Ò$¶W–&ö&B6†÷'F7WG26fVB"çFõö÷væVB‚“°¢6VÆbç6†÷'F7WEöVF—F÷%öfVVF&6²Ò$¶W–&ö&B6†÷'F7WG26fVB"çFõö÷væVB‚“°¢Ð¢W'"†W'&÷"’Óâ6VÆbç7FGW2Òf÷&ÖB‚$¶W–&ö&B6†÷'F7WG26÷VÆBæ÷B&R6fVC¢¶W'&÷'Ò"’À¢Ð¢Ð ¢fâ6WEöÖ7FW%ö÷WGWEö6V–Æ–æuöF&g2‚f×WB6VÆbÂ6V–Æ–æs¢FuöVæv–æS£¤Ö7FW$÷WGWD6V–Æ–ær’°¢ÆWB6WGF–æw2ÒVF–õö6öæf–s£¤VF–õ6WGF–æw2°¢Ö7FW%ö÷WGWEö6V–Æ–æs¢6V–Æ–ærÀ¢âç6VÆbæVF–õ÷6WGF–æw2æ6ÆöæR‚¢Ó°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢–bÆWB6öÖR‡Æ–&6²’Òg6VÆbçÆ–&6²°¢Æ–&6²ç6WEöÖ7FW%ö÷WGWEö6V–Æ–æuöF&g2†6V–Æ–ær“°¢Ð¢6VÆbæVF–õ÷6WGF–æw2Ò6WGF–æw2æ6ÆöæR‚“°¢ÖF6‚VF–õö6öæf–s£§6fR‚g6WGF–æw2’°¢ö²‚‚’’Óâ°¢6VÆbæVF–õ÷6WGF–æw5öfVVF&6²Ð¢f÷&ÖB‚$Ö7FW"6×ÆR×V²6V–Æ–ær6WBFò¶6V–Æ–æwÒ"“°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbæVF–õ÷6WGF–æw5öfVVF&6²Ð¢f÷&ÖB‚$6V–Æ–ær—27F—fRf÷"F†—26W76–öâ'WB6÷VÆBæ÷B&R6fVC¢¶W'&÷'Ò"“°¢Ð¢Ð¢Ð ¢fâÇ•÷&V6÷&F–æuööfg6WB‚f×WB6VÆb’°¢ÆWBfÇVRÒ6VÆ`¢æVF–õ÷&V6÷&F–æuööfg6WE÷VW'¢æ5öFW&Vb‚¢çVçw&ö÷"‚#ã"“°¢ÆWB6öÖR†öfg6WE÷W2’ÒVF–õö6öæf–s£§'6U÷&V6÷&F–æuööfg6WEö×2‡fÇVR’VÇ6R°¢6VÆbæVF–õ÷6WGF–æw5öfVVF&6²Ð¢$VçFW"&V6÷&F–æröfg6WBg&öÒÓSFòS×2Âv—F‚WFò2FV6–ÖÂÆ6W2 ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWB6WGF–æw2ÒVF–õö6öæf–s£¤VF–õ6WGF–æw2°¢&V6÷&F–æuööfg6WE÷W3¢öfg6WE÷W2À¢âç6VÆbæVF–õ÷6WGF–æw2æ6ÆöæR‚¢Ó°¢6VÆbæVF–õ÷6WGF–æw2Ò6WGF–æw2æ6ÆöæR‚“°¢6VÆbæVF–õ÷&V6÷&F–æuööfg6WE÷VW'’Ð¢6öÖR†VF–õö6öæf–s£¦f÷&ÖE÷&V6÷&F–æuööfg6WEö×2†öfg6WE÷W2’“°¢ÖF6‚VF–õö6öæf–s£§6fR‚g6WGF–æw2’°¢ö²‚‚’’Óâ°¢6VÆbæVF–õ÷6WGF–æw5öfVVF&6²Òf÷&ÖB€¢%&V6÷&F–ærÆ6VÖVçBöfg6WB6WBFò·Ò×3²÷6—F—fRfÇVW2Ö÷fR—FV×2ÆFW""À¢VF–õö6öæf–s£¦f÷&ÖE÷&V6÷&F–æuööfg6WEö×2†öfg6WE÷W2¢“°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbæVF–õ÷6WGF–æw5öfVVF&6²Òf÷&ÖB€¢%&V6÷&F–ærÆ6VÖVçBöfg6WB—27F—fRf÷"F†—26W76–öâ'WB6÷VÆBæ÷B&R6fVC¢¶W'&÷'Ò ¢“°¢Ð¢Ð¢Ð ¢fâ&W6WE÷6†÷'F7WEö&–æF–æw2‚f×WB6VÆb’°¢ÖF6‚¶W–&ö&Eö6öæf–s£§&W6WB‚’°¢ö²‚‚’’Óâ°¢6VÆbç6†÷'F7WEö&–æF–æuöVF—G2æ6ÆV"‚“°¢6VÆbç6†÷'F7WEöFVfVÇG5÷&W7F÷&VBæ6ÆV"‚“°¢6VÆbç6†÷'F7WEö6GW&Uö–BÒæöæS°¢6VÆbç6†÷'F7WEö&–æF–æw0¢çw&—FR‚¢çVçw&ö÷%öVÇ6R‡7FC£§7–æ3£¥ö—6öäW'&÷#£¦–çFõö–ææW"¢æ6ÆV"‚“°¢6VÆbç7FGW2Ò$¶W–&ö&B6†÷'F7WG2&W7F÷&VBFòFVfVÇG2"çFõö÷væVB‚“°¢6VÆbç6†÷'F7WEöVF—F÷%öfVVF&6²Ð¢$¶W–&ö&B6†÷'F7WG2&W7F÷&VBFòFVfVÇG2"çFõö÷væVB‚“°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚$¶W–&ö&B6†÷'F7WG26÷VÆBæ÷B&R&W6WC¢¶W'&÷'Ò"“°¢Ð¢Ð¢Ð ¢fâ6VÆV7FVE÷G&6µö–B‚g6VÆb’Óâ÷F–öãÅG&6´–Câ°¢6VÆbçF–ÖVÆ–æRç6VÆV7FVE÷G&6²æf–ÇFW"‡ÇG&6µö–GÂ°¢6VÆbç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æç’‡ÇG&6·ÂG&6²æ–B‚’ÓÒ§G&6µö–B¢Ò¢Ð ¢5¶6fr†ÆÂ†fVGW&RÒ&VF–òÖFWf–6R"ÂF&vWEö÷2Ò&æG&ö–B"’•Ð¢fâ6VÆV7FVEöÖ–F•ö–çWE÷G&6µö–B‚g6VÆb’Óâ÷F–öãÅG&6´–Câ°¢ÆWBG&6µö–BÒ6VÆbç6VÆV7FVE÷G&6µö–B‚“ó°¢6VÆbç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–BbbG&6²æ–ç7G'VÖVçB‚’æ—5÷6öÖR‚’¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢Ð ¢fâ&Vv–å÷G&6µöæÖUöVF—B‚f×WB6VÆbÂG&6µö–C¢G&6´–B’ÓâF6³ÄÖW76vSâ°¢ÆWB6öÖR‡G&6²’Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢VÇ6R°¢&WGW&âF6³£¦æöæR‚“°¢Ó°¢6VÆbçG&6µöæÖUöVF—G0¢æ–ç6W'B‡G&6µö–BÂG&6²ææÖR‚’çFõö÷væVB‚’“°¢6VÆbæ&Vv–å÷G&6µöG&gB‡G&6µö–BÂG&6´G&gDf–VÆC£¤æÖR“°¢6VÆbçF–ÖVÆ–æRæFVÆWFUöfö7W2ÒF–ÖVÆ–æS£¤FVÆWFTfö7W5F&vWC£¥G&6µæVÃ°¢6VÆbçF–ÖVÆ–æRç6VÆV7E÷G&6µööæÇ’‡G&6µö–B“°¢ÆWB–çWEö–BÒÖW76vW3£§G&6µöæÖUö–çWEö–B‡G&6µö–B“°¢F6³£¦&F6‚…°¢–6VC£§v–FvWC£¦÷W&F–öã£¦fö7W2†–çWEö–Bæ6ÆöæR‚’’À¢–6VC£§v–FvWC£¦÷W&F–öã£¦Ö÷fUö7W'6÷%÷FõöVæB†–çWEö–B’À¢Ò¢Ð ¢V"‡7WW"’fâ6å÷7Æ—E÷6VÆV7FVEö—FV×5öE÷F–ÖU÷6VÆV7F–öâ‚g6VÆb’Óâ&ööÂ°¢–b6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâæ—5÷6öÖR‚’°¢–b6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’°¢6VÆbæÆÅö—FV×5÷F–ÖU÷6VÆV7F–öå÷7Æ—Eö7F–öâ„æöæR¢æ—5ööµöæB‡Æ7F–öçÂ7F–öâæ—5÷6öÖR‚’¢ÒVÇ6R°¢6VÆbç6VÆV7FVEö—FVÕ÷7Æ—Eö7F–öâ‡G'VRÂæöæR¢æ—5ööµöæB‡Æ7F–öçÂ7F–öâæ—5÷6öÖR‚’¢Ð¢ÒVÇ6R°¢6VÆbæÆÅö—FV×5÷&¦÷%ö&V÷7Æ—Eö7F–öâ„æöæR¢æ—5ööµöæB‡Æ7F–öçÂ7F–öâæ—5÷6öÖR‚’¢Ð¢Ð ¢fâ6VÆV7FVEö—FVÕ÷7Æ—Eö7F–öâ€¢g6VÆbÀ¢W6U÷F–ÖU÷6VÆV7F–öã¢&ööÂÀ¢6÷W&6U÷6×ÆU÷&FW3¢÷F–öãÂd†6„ÖÅ7G&–ærÂS3#ãâÀ¢’Óâ&W7VÇCÄ÷F–öãÄFt7F–öãâÂ7G&–æsâ°¢ÆWBF–6µö&÷VæF&–W2Ò–bW6U÷F–ÖU÷6VÆV7F–öâ°¢ÆWB6öÖR‡6VÆV7F–öâ’Ò6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâVÇ6R°¢&WGW&âö²„æöæR“°¢Ó°¢fV2·6VÆV7F–öâç7F'E÷F–6²Â6VÆV7F–öâæVæE÷F–6µÐ¢ÒVÇ6R°¢fV2·6VÆbçF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6µÐ¢Ó°¢ÆWB7W'6÷%÷6×ÆRÒ–bW6U÷F–ÖU÷6VÆV7F–öâ°¢æöæP¢ÒVÇ6R°¢6öÖR€¢6VÆbçF–ÖVÆ–æP¢æVF—Eö7W'6÷%÷6×ÆR‚g6VÆbç&ö¦V7B¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“òÀ¢¢Ó°¢ÆWB×WB6VÆV7FVEö—FVÕö–G2Ò6VÆ`¢çF–ÖVÆ–æP¢ç6VÆV7FVEö—FV×0¢æ—FW"‚¢æ6÷–VB‚¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢6VÆV7FVEö—FVÕö–G2ç6÷'E÷Vç7F&ÆUö'•ö¶W’‡Æ—FVÕö–GÂ—FVÕö–BçfÇVR‚’“° ¢ÆWB×WB7F–öç2ÒfV3£¦æWr‚“°¢f÷"—FVÕö–B–â6VÆV7FVEö—FVÕö–G2°¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢ÆWB6÷W&6U÷6×ÆU÷&FRÒ6÷W&6U÷6×ÆU÷&FW0¢ææE÷F†Vâ‡Ç&FW7Â&FW2ævWB†—FVÒæÖVF–÷&Vb‚’’æ6÷–VB‚’¢çVçw&ö÷%öVÇ6R‡ÇÂ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’“°¢–bÆWB6öÖR‡6×ÆR’Ò7W'6÷%÷6×ÆR°¢7Æ—EöVF–õö—FVÕö7F–öç5öE÷6×ÆW2€¢g6VÆbç&ö¦V7BÀ¢—FVÒÀ¢e·6×ÆUÒÀ¢6÷W&6U÷6×ÆU÷&FRÀ¢f×WB7F–öç2À¢“ó°¢ÒVÇ6R°¢7Æ—EöVF–õö—FVÕö7F–öç2€¢g6VÆbç&ö¦V7BÀ¢—FVÒÀ¢gF–6µö&÷VæF&–W2À¢6÷W&6U÷6×ÆU÷&FRÀ¢f×WB7F–öç2À¢“ó°¢Ð¢ÒVÇ6R–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢7Æ—EöÖ–F•ö—FVÕö7F–öç2†—FVÒÂgF–6µö&÷VæF&–W2Âf×WB7F–öç2“ó°¢Ð¢Ð¢–b7F–öç2æ—5öV×G’‚’°¢ö²„æöæR¢ÒVÇ6R°¢ö²…6öÖR„Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ò’¢Ð¢Ð ¢fâ6VÆV7FVEöVF–õ÷6÷W&6U÷6×ÆU÷&FW2€¢g6VÆbÀ¢W6U÷F–ÖU÷6VÆV7F–öã¢&ööÂÀ¢’Óâ&W7VÇCÄ†6„ÖÅ7G&–ærÂS3#âÂ7G&–æsâ°¢ÆWB&÷VæF&–W2Ò–bW6U÷F–ÖU÷6VÆV7F–öâ°¢ÆWB6öÖR‡6VÆV7F–öâ’Ò6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâVÇ6R°¢&WGW&âö²„†6„Ö£¦æWr‚’“°¢Ó°¢·6VÆV7F–öâç7F'E÷F–6²Â6VÆV7F–öâæVæE÷F–6µÐ¢æ–çFõö—FW"‚¢æf–ÇFW%öÖ‡ÇF–6·Â6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²‡F–6²’æö²‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚¢ÒVÇ6R°¢fV2°¢6VÆbçF–ÖVÆ–æP¢æVF—Eö7W'6÷%÷6×ÆR‚g6VÆbç&ö¦V7B¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“òÀ¢Ð¢Ó°¢ÆWBÖVF–÷&Vg2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚f—FVÒæ–B‚’’¢æf–ÇFW"‡Æ—FV×Â°¢&÷VæF&–W0¢æ—FW"‚¢æç’‡Ç6×ÆWÂ—FVÒç7F'E÷6×ÆR‚’Â§6×ÆRbb§6×ÆRÂ—FVÒæVæE÷6×ÆR‚’¢Ò¢æÖ‡Æ—FV×Â—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢6VÆbæVF–õ÷6÷W&6U÷6×ÆU÷&FW2†ÖVF–÷&Vg2¢Ð ¢fâVF–õ÷6÷W&6U÷6×ÆU÷&FW2€¢g6VÆbÀ¢ÖVF–÷&Vg3¢–×Â–çFô—FW&F÷#Ä—FVÒÒ7G&–æsâÀ¢’Óâ&W7VÇCÄ†6„ÖÅ7G&–ærÂS3#âÂ7G&–æsâ°¢ÆWB×WBÖVF–÷&Vg2ÒÖVF–÷&Vg2æ–çFõö—FW"‚’æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÖVF–÷&Vg2ç6÷'E÷Vç7F&ÆR‚“°¢ÖVF–÷&Vg2æFVGW‚“°¢–bÖVF–÷&Vg2æ—5öV×G’‚’°¢&WGW&âö²„†6„Ö£¦æWr‚’“°¢Ð ¢ÆWBF‚Ò6VÆ`¢æÖVF–÷7F÷&U÷F‚‚¢æf–ÇFW"‡ÇF‡ÂF‚æ—5öf–ÆR‚’¢æöµö÷%öVÇ6R‡ÇÂ&VF–òÖVF–7F÷&R—2Væf–Æ&ÆR"çFõö÷væVB‚’“ó°¢ÆWB7F÷&RÒ&ö¦V7E7F÷&S£¦÷Vâ‚gF‚’æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢ÆWB×WB&FW2Ò†6„Ö£§v—F…ö66—G’†ÖVF–÷&Vg2æÆVâ‚’“°¢ÆWB×WBÖWFFF÷&W7VÇBÒö²‚‚’“°¢f÷"ÖVF–÷&Vb–âÖVF–÷&Vg2°¢ÖF6‚7F÷&RæVF–õö76WEöÖWFFF‚fÖVF–÷&Vb’°¢ö²…6öÖR†ÖWFFF’’ÓâÖF6‚ÖWFFFç6×ÆU÷&FRæf–ÇFW"‡Ç&FWÂ§&FRâ’°¢6öÖR‡&FR’Óâ°¢&FW2æ–ç6W'B†ÖVF–÷&VbÂ&FR“°¢Ð¢æöæRÓâ°¢ÖWFFF÷&W7VÇBÐ¢W'"†f÷&ÖB‚'6÷W&6R6×ÆR&FR—2Væf–Æ&ÆRf÷"¶ÖVF–÷&VgÒ"’“°¢'&V³°¢Ð¢ÒÀ¢ö²„æöæR’Óâ°¢ÖWFFF÷&W7VÇBÐ¢W'"†f÷&ÖB‚'6÷W&6RÖWFFF—2Væf–Æ&ÆRf÷"¶ÖVF–÷&VgÒ"’“°¢'&V³°¢Ð¢W'"†W'&÷"’Óâ°¢ÖWFFF÷&W7VÇBÒW'"†W'&÷"çFõ÷7G&–ær‚’“°¢'&V³°¢Ð¢Ð¢Ð¢ÆWB6Æ÷6U÷&W7VÇBÒ7F÷&Ræ6Æ÷6R‚’æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“°¢ÖWFFF÷&W7VÇCó°¢6Æ÷6U÷&W7VÇCó°¢ö²‡&FW2¢Ð ¢fâ7F'EöVF–õ÷vfVf÷&Õ÷66â‚f×WB6VÆbÂ&W6WC¢&ööÂ’°¢–bÆWB6öÖR‡v÷&¶W"’Ò6VÆbæVF–õ÷vfVf÷&Õ÷v÷&¶W"çF¶R‚’°¢v÷&¶W"æ6æ6VÂ‚“°¢Ð¢–b&W6WB°¢6VÆbæVF–õ÷vfVf÷&×2æ6ÆV"‚“°¢6VÆbçF–ÖVÆ–æP¢ç6WEöVF–õ÷vfVf÷&×2‚g6VÆbç&ö¦V7BÂ†6„Ö£¦æWr‚’“°¢Ð¢ÆWB6öÖR‡F‚’Ò6VÆbæÖVF–÷7F÷&U÷F‚‚’æf–ÇFW"‡ÇF‡ÂF‚æ—5öf–ÆR‚’’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBÖVF–÷&Vg2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’¢æf–ÇFW"‡ÆÖVF–÷&VgÂ6VÆbæVF–õ÷vfVf÷&×2æ6öçF–ç5ö¶W’†ÖVF–÷&Vb’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–bÖVF–÷&Vg2æ—5öV×G’‚’°¢&WGW&ã°¢Ð¢ÖF6‚VF–õvfVf÷&Õv÷&¶W#£§7F'B‡F‚çFõö÷væVB‚’ÂÖVF–÷&Vg2’°¢ö²‡v÷&¶W"’Óâ6VÆbæVF–õ÷vfVf÷&Õ÷v÷&¶W"Ò6öÖR‡v÷&¶W"’À¢W'"†W'&÷"’Óâ6VÆbç7FGW2Òf÷&ÖB‚%vfVf÷&Ò66â6÷VÆBæ÷B7F'C¢¶W'&÷'Ò"’À¢Ð¢Ð ¢fâWFFUöVF–õ÷vfVf÷&×2‚f×WB6VÆb’°¢ÆWB6öÖR‡v÷&¶W"’Òg6VÆbæVF–õ÷vfVf÷&Õ÷v÷&¶W"VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB&W7VÇG2Òv÷&¶W"ç&W7VÇG2‚“°¢ÆWBf–æ—6†VBÒv÷&¶W"æ—5öf–æ—6†VB‚“°¢ÆWB×WB6†ævVBÒfÇ6S°¢f÷"VF–õvfVf÷&Õ&W7VÇB°¢ÖVF–÷&VbÀ¢vfVf÷&ÒÀ¢66†Uö†—C¢òÀ¢Ò–â&W7VÇG0¢°¢ÖF6‚vfVf÷&Ò°¢ö²‡vfVf÷&Ò’Óâ°¢6VÆbæVF–õ÷vfVf÷&×2æ–ç6W'B†ÖVF–÷&VbÂvfVf÷&Ò“°¢6†ævVBÒG'VS°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%vfVf÷&ÒVæf–Æ&ÆS¢¶W'&÷'Ò"“°¢Ð¢Ð¢Ð¢–b6†ævVB°¢6VÆbçF–ÖVÆ–æP¢ç6WEöVF–õ÷vfVf÷&×2‚g6VÆbç&ö¦V7BÂ6VÆbæVF–õ÷vfVf÷&×2æ6ÆöæR‚’“°¢Ð¢–bf–æ—6†VB°¢ÆWBv÷&¶W"Ò6VÆ`¢æVF–õ÷vfVf÷&Õ÷v÷&¶W ¢çF¶R‚¢æW‡V7B‚&f–æ—6†VBvfVf÷&Òv÷&¶W"—2&W6VçB"“°¢–bÆWBW'"†W'&÷"’Òv÷&¶W"æ¦ö–â‚’°¢6VÆbç7FGW2Òf÷&ÖB‚%vfVf÷&Ò66âf–ÆVC¢¶W'&÷'Ò"“°¢Ð¢Ð¢Ð ¢fâ7Æ—E÷6VÆV7FVEö—FV×2‚f×WB6VÆbÂW6U÷F–ÖU÷6VÆV7F–öã¢&ööÂ’°¢–bW6U÷F–ÖU÷6VÆV7F–öâ°¢–bÆWB6öÖR‡6VÆV7F–öâ’Ò6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâ°¢–b6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’°¢6VÆbç7Æ—EöÆÅö—FV×5öE÷F–ÖU÷6VÆV7F–öâ‡6VÆV7F–öâ“°¢&WGW&ã°¢Ð¢ÒVÇ6R°¢6VÆbç7Æ—Eö—FV×5ö–å÷&¦÷%ö&V2‚“°¢&WGW&ã°¢Ð¢Ð¢–bW6U÷F–ÖU÷6VÆV7F–öâbb6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’°¢6VÆbç7Æ—EöÆÅö—FV×5öEö7W'6÷"‚“°¢&WGW&ã°¢Ð¢ÆWB6÷W&6U÷6×ÆU÷&FW2ÒÖF6‚6VÆbç6VÆV7FVEöVF–õ÷6÷W&6U÷6×ÆU÷&FW2‡W6U÷F–ÖU÷6VÆV7F–öâ¢°¢ö²‡&FW2’Óâ&FW2À¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB7F–öâÐ¢ÖF6‚6VÆbç6VÆV7FVEö—FVÕ÷7Æ—Eö7F–öâ‡W6U÷F–ÖU÷6VÆV7F–öâÂ6öÖR‚g6÷W&6U÷6×ÆU÷&FW2’’°¢ö²…6öÖR†7F–öâ’’Óâ7F–öâÀ¢ö²„æöæR’Óâ°¢6VÆbç7FGW2Ò$æò6VÆV7FVB—FVÒ7&÷76W2F†R7Æ—Bö–çB"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWBF–ÖU÷6VÆV7F–öå÷&ævRÒ–bW6U÷F–ÖU÷6VÆV7F–öâ°¢ÆWB6öÖR‡6VÆV7F–öâ’Ò6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâVÇ6R°¢6VÆbç7FGW2Ò$æòF–ÖR6VÆV7F–öâFò7Æ—B"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWB7F'E÷6×ÆRÒÖF6‚6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²‡6VÆV7F–öâç7F'E÷F–6²’°¢ö²‡6×ÆR’Óâ6×ÆRÀ¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWBVæE÷6×ÆRÒÖF6‚6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²‡6VÆV7F–öâæVæE÷F–6²’°¢ö²‡6×ÆR’Óâ6×ÆRÀ¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢6öÖR‚‡6VÆV7F–öâÂ7F'E÷6×ÆRÂVæE÷6×ÆR’¢ÒVÇ6R°¢æöæP¢Ó°¢ÆWB&Wf–÷W6Ç•÷6VÆV7FVEö–G2Ò6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6ÆöæR‚“°¢ÆWB&Wf–÷W5ö–G2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â—FVÒæ–B‚’¢æ6†–â‡6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æÖ‡Æ—FV×Â—FVÒæ–B‚’’¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ%6VÆV7FVB—FV×27Æ—B"“°¢–b6VÆbç&Wf—6–öâÓÒ&Wf–÷W5÷&Wf—6–öâ°¢&WGW&ã°¢Ð¢ÆWBæWuö–G2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â—FVÒæ–B‚’¢æ6†–â‡6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æÖ‡Æ—FV×Â—FVÒæ–B‚’’¢æf–ÇFW"‡Æ—FVÕö–GÂ&Wf–÷W5ö–G2æ6öçF–ç2†—FVÕö–B’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–bÆWB6öÖR‚‡6VÆV7F–öâÂ7F'E÷6×ÆRÂVæE÷6×ÆR’’ÒF–ÖU÷6VÆV7F–öå÷&ævR°¢ÆWB6æF–FFW2Ò&Wf–÷W6Ç•÷6VÆV7FVEö–G0¢æ–çFõö—FW"‚¢æ6†–â†æWuö–G2¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢ÆWB6VÆV7FVBÒ6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â°¢6æF–FFW2æ6öçF–ç2‚f—FVÒæ–B‚’¢bb7F'E÷6×ÆRÃÒ—FVÒç7F'E÷6×ÆR‚¢bb—FVÒæVæE÷6×ÆR‚’ÃÒVæE÷6×ÆP¢Ò¢æÖ‡Æ—FV×Â†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢æ6†–â‡6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æf–ÇFW%öÖ‡Æ—FV×Â°¢ÆWB—FVÕöVæBÒ—FVÒç7F'E÷F–6²‚’æ6†V6¶VEöFB†—FVÒæÆVæwF…÷F–6·2‚’“ó°¢†6æF–FFW2æ6öçF–ç2‚f—FVÒæ–B‚’¢bb6VÆV7F–öâç7F'E÷F–6²ÃÒ—FVÒç7F'E÷F–6²‚¢bb—FVÕöVæBÃÒ6VÆV7F–öâæVæE÷F–6²¢çF†Vå÷6öÖR‚†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢Ò’¢æ6öÆÆV7B‚“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢ÒVÇ6R°¢ÆWB6VÆV7FVBÒ6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×ÂæWuö–G2æ6öçF–ç2‚f—FVÒæ–B‚’’¢æÖ‡Æ—FV×Â†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢æ6†–â‡6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æf–ÇFW%öÖ‡Æ—FV×Â°¢æWuö–G0¢æ6öçF–ç2‚f—FVÒæ–B‚’¢çF†Vå÷6öÖR‚†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢Ò’¢æ6öÆÆV7B‚“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢Ð¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2æ6ÆV"‚“°¢Ð ¢fâÆÅö—FV×5÷F–ÖU÷6VÆV7F–öå÷7Æ—Eö7F–öâ€¢g6VÆbÀ¢6÷W&6U÷6×ÆU÷&FW3¢÷F–öãÂd†6„ÖÅ7G&–ærÂS3#ãâÀ¢’Óâ&W7VÇCÄ÷F–öãÄFt7F–öãâÂ7G&–æsâ°¢ÆWB6öÖR‡6VÆV7F–öâ’Ò6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâVÇ6R°¢&WGW&âö²„æöæR“°¢Ó°¢ÆWB&÷VæF&–W2Ò·6VÆV7F–öâç7F'E÷F–6²Â6VÆV7F–öâæVæE÷F–6µÓ°¢ÆWB×WB7F–öç2ÒfV3£¦æWr‚“°¢f÷"—FVÒ–â6VÆbç&ö¦V7BæVF–õö—FV×2‚’°¢ÆWB6÷W&6U÷6×ÆU÷&FRÒ6÷W&6U÷6×ÆU÷&FW0¢ææE÷F†Vâ‡Ç&FW7Â&FW2ævWB†—FVÒæÖVF–÷&Vb‚’’æ6÷–VB‚’¢çVçw&ö÷%öVÇ6R‡ÇÂ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’“°¢7Æ—EöVF–õö—FVÕö7F–öç2€¢g6VÆbç&ö¦V7BÀ¢—FVÒÀ¢f&÷VæF&–W2À¢6÷W&6U÷6×ÆU÷&FRÀ¢f×WB7F–öç2À¢“ó°¢Ð¢f÷"—FVÒ–â6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’°¢7Æ—EöÖ–F•ö—FVÕö7F–öç2†—FVÒÂf&÷VæF&–W2Âf×WB7F–öç2“ó°¢Ð¢–b7F–öç2æ—5öV×G’‚’°¢ö²„æöæR¢ÒVÇ6R°¢ö²…6öÖR„Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ò’¢Ð¢Ð ¢fâÆÅö—FV×5÷&¦÷%ö&V÷7Æ—Eö7F–öâ€¢g6VÆbÀ¢6÷W&6U÷6×ÆU÷&FW3¢÷F–öãÂd†6„ÖÅ7G&–ærÂS3#ãâÀ¢’Óâ&W7VÇCÄ÷F–öãÂ„Ft7F–öâÂG&6µ&¦÷$&V2“âÂ7G&–æsâ°¢ÆWBG&6µö&V2Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–ÇFW"‡ÇG&6·ÂG&6²ç&¦÷%öVF—Eö&V2‚’æ—5öV×G’‚’¢æÖ‡ÇG&6·Â‡G&6²æ–B‚’ÂG&6²ç&¦÷%öVF—Eö&V2‚’çFõ÷fV2‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–bG&6µö&V2æ—5öV×G’‚’°¢&WGW&âö²„æöæR“°¢Ð¢ÆWB&V5ö'•÷G&6²ÒG&6µö&V2æ—FW"‚’æ6ÆöæVB‚’æ6öÆÆV7C££Ä†6„ÖÅòÂóãâ‚“°¢ÆWB×WB7F–öç2ÒfV3£¦æWr‚“°¢f÷"—FVÒ–â6VÆbç&ö¦V7BæVF–õö—FV×2‚’°¢ÆWB6öÖR†&V2’Ò&V5ö'•÷G&6²ævWB‚f—FVÒçG&6µö–B‚’’VÇ6R°¢6öçF–çVS°¢Ó°¢ÆWB&÷VæF&–W2Ò&V0¢æ—FW"‚¢æfÆEöÖ‡Æ&VÂ¶&Vç7F'E÷6×ÆR‚’Â&VæVæE÷6×ÆR‚•Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6÷W&6U÷6×ÆU÷&FRÒ6÷W&6U÷6×ÆU÷&FW0¢ææE÷F†Vâ‡Ç&FW7Â&FW2ævWB†—FVÒæÖVF–÷&Vb‚’’æ6÷–VB‚’¢çVçw&ö÷%öVÇ6R‡ÇÂ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’“°¢7Æ—EöVF–õö—FVÕö7F–öç5öE÷6×ÆW2€¢g6VÆbç&ö¦V7BÀ¢—FVÒÀ¢f&÷VæF&–W2À¢6÷W&6U÷6×ÆU÷&FRÀ¢f×WB7F–öç2À¢“ó°¢Ð¢f÷"—FVÒ–â6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’°¢ÆWB6öÖR†&V2’Ò&V5ö'•÷G&6²ævWB‚f—FVÒçG&6µö–B‚’’VÇ6R°¢6öçF–çVS°¢Ó°¢ÆWB&÷VæF&–W2Ò&V0¢æ—FW"‚¢æfÆEöÖ‡Æ&VÂ¶&Vç7F'E÷6×ÆR‚’Â&VæVæE÷6×ÆR‚•Ò¢æÖ‡Ç6×ÆWÂ°¢6VÆbç&ö¦V7@¢çF–6µöE÷6×ÆR‡6×ÆR¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’¢Ò¢æ6öÆÆV7C££Å&W7VÇCÅfV3ÅóâÂóãâ‚“ó°¢7Æ—EöÖ–F•ö—FVÕö7F–öç2†—FVÒÂf&÷VæF&–W2Âf×WB7F–öç2“ó°¢Ð¢7F–öç2çW6‚„Ft7F–öã£¥6WEG&6µ&¦÷$VF—D&V2°¢G&6µö&V3¢G&6µö&V0¢æ—FW"‚¢æÖ‡Â‡G&6µö–BÂò—Â‚§G&6µö–BÂfV3£¦æWr‚’’¢æ6öÆÆV7B‚’À¢Ò“°¢ö²…6öÖR‚€¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢ÒÀ¢G&6µö&V2À¢’’¢Ð ¢fâ7Æ—EöÆÅö—FV×5öE÷F–ÖU÷6VÆV7F–öâ‚f×WB6VÆbÂ6VÆV7F–öã¢F–ÖVÆ–æS£¥F–ÖU6VÆV7F–öâ’°¢ÆWB7F'E÷6×ÆRÒÖF6‚6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²‡6VÆV7F–öâç7F'E÷F–6²’°¢ö²‡6×ÆR’Óâ6×ÆRÀ¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWBVæE÷6×ÆRÒÖF6‚6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²‡6VÆV7F–öâæVæE÷F–6²’°¢ö²‡6×ÆR’Óâ6×ÆRÀ¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWBÖVF–÷&Vg2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â°¢·6VÆV7F–öâç7F'E÷F–6²Â6VÆV7F–öâæVæE÷F–6µÐ¢æ—FW"‚¢æç’‡ÇF–6·Â°¢6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²‚§F–6²’æ—5ööµöæB‡Ç6×ÆWÂ°¢—FVÒç7F'E÷6×ÆR‚’Â6×ÆRbb6×ÆRÂ—FVÒæVæE÷6×ÆR‚¢Ò¢Ò¢Ò¢æÖ‡Æ—FV×Â—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6÷W&6U÷6×ÆU÷&FW2ÒÖF6‚6VÆbæVF–õ÷6÷W&6U÷6×ÆU÷&FW2†ÖVF–÷&Vg2’°¢ö²‡&FW2’Óâ&FW2À¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB7F–öâÒÖF6‚6VÆbæÆÅö—FV×5÷F–ÖU÷6VÆV7F–öå÷7Æ—Eö7F–öâ…6öÖR‚g6÷W&6U÷6×ÆU÷&FW2’’°¢ö²…6öÖR†7F–öâ’’Óâ7F–öâÀ¢ö²„æöæR’Óâ°¢6VÆbç7FGW2Ò$æò—FVÒ7&÷76W2F†RF–ÖR6VÆV7F–öâ"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ$—FV×27Æ—BBF–ÖR6VÆV7F–öâ"“°¢–b6VÆbç&Wf—6–öâÓÒ&Wf–÷W5÷&Wf—6–öâ°¢&WGW&ã°¢Ð¢ÆWB×WB6VÆV7FVBÒ6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â7F'E÷6×ÆRÃÒ—FVÒç7F'E÷6×ÆR‚’bb—FVÒæVæE÷6×ÆR‚’ÃÒVæE÷6×ÆR¢æÖ‡Æ—FV×Â†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢6VÆV7FVBæW‡FVæB‡6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æf–ÇFW%öÖ‡Æ—FV×Â°¢ÆWBVæE÷F–6²Ò—FVÒç7F'E÷F–6²‚’æ6†V6¶VEöFB†—FVÒæÆVæwF…÷F–6·2‚’“ó°¢‡6VÆV7F–öâç7F'E÷F–6²ÃÒ—FVÒç7F'E÷F–6²‚’bbVæE÷F–6²ÃÒ6VÆV7F–öâæVæE÷F–6²¢çF†Vå÷6öÖR‚†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢Ò’“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢Ð ¢fâ7Æ—EöÆÅö—FV×5öEö7W'6÷"‚f×WB6VÆb’°¢ÆWB7W'6÷%÷6×ÆRÒÖF6‚6VÆbçF–ÖVÆ–æRæVF—Eö7W'6÷%÷6×ÆR‚g6VÆbç&ö¦V7B’°¢ö²‡6×ÆR’Óâ6×ÆRÀ¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWBÖVF–÷&Vg2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â—FVÒç7F'E÷6×ÆR‚’Â7W'6÷%÷6×ÆRbb7W'6÷%÷6×ÆRÂ—FVÒæVæE÷6×ÆR‚’¢æÖ‡Æ—FV×Â—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6÷W&6U÷6×ÆU÷&FW2ÒÖF6‚6VÆbæVF–õ÷6÷W&6U÷6×ÆU÷&FW2†ÖVF–÷&Vg2’°¢ö²‡&FW2’Óâ&FW2À¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB7F–öâÒÖF6‚6VÆbæÆÅö—FV×5ö7W'6÷%÷7Æ—Eö7F–öâ…6öÖR‚g6÷W&6U÷6×ÆU÷&FW2’’°¢ö²…6öÖR†7F–öâ’’Óâ7F–öâÀ¢ö²„æöæR’Óâ°¢6VÆbç7FGW2Ò$æò—FVÒ7&÷76W2F†R7Æ—Bö–çB"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB&Wf–÷W5ö–G2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â—FVÒæ–B‚’¢æ6†–â‡6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æÖ‡Æ—FV×Â—FVÒæ–B‚’’¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ$—FV×27Æ—BBVF—B7W'6÷""“°¢–b6VÆbç&Wf—6–öâÓÒ&Wf–÷W5÷&Wf—6–öâ°¢&WGW&ã°¢Ð¢ÆWB6VÆV7FVBÒ6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â&Wf–÷W5ö–G2æ6öçF–ç2‚f—FVÒæ–B‚’’¢æÖ‡Æ—FV×Â†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢æ6†–â‡6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æf–ÇFW%öÖ‡Æ—FV×Â°¢‚&Wf–÷W5ö–G2æ6öçF–ç2‚f—FVÒæ–B‚’’’çF†Vå÷6öÖR‚†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢Ò’¢æ6öÆÆV7B‚“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2æ6ÆV"‚“°¢Ð ¢fâÆÅö—FV×5ö7W'6÷%÷7Æ—Eö7F–öâ€¢g6VÆbÀ¢6÷W&6U÷6×ÆU÷&FW3¢÷F–öãÂd†6„ÖÅ7G&–ærÂS3#ãâÀ¢’Óâ&W7VÇCÄ÷F–öãÄFt7F–öãâÂ7G&–æsâ°¢ÆWBF–6µö&÷VæF&–W2Ò·6VÆbçF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6µÓ°¢ÆWB6×ÆUö&÷VæF&–W2Ò·6VÆ`¢çF–ÖVÆ–æP¢æVF—Eö7W'6÷%÷6×ÆR‚g6VÆbç&ö¦V7B¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“õÓ°¢ÆWB×WB7F–öç2ÒfV3£¦æWr‚“°¢f÷"—FVÒ–â6VÆbç&ö¦V7BæVF–õö—FV×2‚’°¢ÆWB6÷W&6U÷6×ÆU÷&FRÒ6÷W&6U÷6×ÆU÷&FW0¢ææE÷F†Vâ‡Ç&FW7Â&FW2ævWB†—FVÒæÖVF–÷&Vb‚’’æ6÷–VB‚’¢çVçw&ö÷%öVÇ6R‡ÇÂ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’“°¢7Æ—EöVF–õö—FVÕö7F–öç5öE÷6×ÆW2€¢g6VÆbç&ö¦V7BÀ¢—FVÒÀ¢g6×ÆUö&÷VæF&–W2À¢6÷W&6U÷6×ÆU÷&FRÀ¢f×WB7F–öç2À¢“ó°¢Ð¢f÷"—FVÒ–â6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’°¢7Æ—EöÖ–F•ö—FVÕö7F–öç2†—FVÒÂgF–6µö&÷VæF&–W2Âf×WB7F–öç2“ó°¢Ð¢–b7F–öç2æ—5öV×G’‚’°¢ö²„æöæR¢ÒVÇ6R°¢ö²…6öÖR„Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ò’¢Ð¢Ð ¢fâ†VÅ÷6VÆV7FVEöVF–õ÷7Æ—G2‚f×WB6VÆb’°¢ÆWBÖVF–÷&Vg2Ò6VÆbç6VÆV7FVEöVF–õö†VÅöÖVF–÷&Vg2‚“°¢–bÖVF–÷&Vg2æ—5öV×G’‚’°¢6VÆbç7FGW2Ò$æòF¦6VçB6VÆV7FVBVF–ò7Æ—G2Fò†VÂ"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB6÷W&6U÷6×ÆU÷&FW2ÒÖF6‚6VÆbæVF–õ÷6÷W&6U÷6×ÆU÷&FW2†ÖVF–÷&Vg2’°¢ö²‡&FW2’Óâ&FW2À¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚$†VÂ7Æ—G2f–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB†7F–öâÂFVÆWFVEö–G2’ÒÖF6‚6VÆbç6VÆV7FVEöVF–õö†VÅö7F–öâ‚g6÷W&6U÷6×ÆU÷&FW2’°¢ö²…6öÖR†7F–öâ’’Óâ7F–öâÀ¢ö²„æöæR’Óâ°¢6VÆbç7FGW2Ò$æòF¦6VçB6VÆV7FVBVF–ò7Æ—G2Fò†VÂ"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚$†VÂ7Æ—G2f–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB&Wf–÷W6Ç•÷6VÆV7FVEö–G2Ò6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6ÆöæR‚“°¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ$VF–ò—FVÒ7Æ—G2†VÆVB"“°¢–b6VÆbç&Wf—6–öâÓÒ&Wf–÷W5÷&Wf—6–öâ°¢&WGW&ã°¢Ð¢ÆWB6VÆV7FVEö–G2Ò&Wf–÷W6Ç•÷6VÆV7FVEö–G0¢æF–ffW&Væ6R‚fFVÆWFVEö–G2¢æ6÷–VB‚¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢ÆWB6VÆV7FVBÒ6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â6VÆV7FVEö–G2æ6öçF–ç2‚f—FVÒæ–B‚’’¢æÖ‡Æ—FV×Â†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢æ6†–â‡6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æf–ÇFW%öÖ‡Æ—FV×Â°¢6VÆV7FVEö–G0¢æ6öçF–ç2‚f—FVÒæ–B‚’¢çF†Vå÷6öÖR‚†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢Ò’¢æ6öÆÆV7B‚“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2æ6ÆV"‚“°¢Ð ¢fâ6VÆV7FVEöVF–õö†VÅöÖVF–÷&Vg2‚g6VÆb’ÓâfV3Å7G&–æsâ°¢ÆWB×WBÖVF–÷&Vg2ÒfV3£¦æWr‚“°¢f÷"G&6²–â6VÆbç&ö¦V7BçG&6·2‚’°¢–bG&6²æ—5ög&÷¦Vâ‚’°¢6öçF–çVS°¢Ð¢ÆWB×WB—FV×2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢—FV×2ç6÷'E÷Vç7F&ÆUö'•ö¶W’‡Æ—FV×Â—FVÒç7F'E÷6×ÆR‚’“°¢f÷"—"–â—FV×2çv–æF÷w2ƒ"’°¢ÆWB¶ÆVgBÂ&–v‡EÒÒ—"VÇ6R°¢6öçF–çVS°¢Ó°¢–b6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚fÆVgBæ–B‚’¢bb6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚g&–v‡Bæ–B‚’¢bbÆVgBæÖVF–÷&Vb‚’ÓÒ&–v‡BæÖVF–÷&Vb‚¢bbÆVgBç7F'E÷6×ÆR‚’æ6†V6¶VEöFB†ÆVgBæÆVæwF…÷6×ÆW2‚’¢ÓÒ6öÖR‡&–v‡Bç7F'E÷6×ÆR‚’¢°¢ÖVF–÷&Vg2çW6‚†ÆVgBæÖVF–÷&Vb‚’çFõö÷væVB‚’“°¢Ð¢Ð¢Ð¢ÖVF–÷&Vg2ç6÷'E÷Vç7F&ÆR‚“°¢ÖVF–÷&Vg2æFVGW‚“°¢ÖVF–÷&Vg0¢Ð ¢fâ6VÆV7FVEöVF–õö†VÅö7F–öâ€¢g6VÆbÀ¢6÷W&6U÷6×ÆU÷&FW3¢d†6„ÖÅ7G&–ærÂS3#âÀ¢’Óâ&W7VÇCÄ÷F–öãÂ„Ft7F–öâÂ†6…6WCÄ—FVÔ–Câ“âÂ7G&–æsâ°¢ÆWB6VÆV7FVEö–G2Òg6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×3°¢ÆWB×WB7F–öç2ÒfV3£¦æWr‚“°¢ÆWB×WBFVÆWFVEö–G2Ò†6…6WC£¦æWr‚“°¢ÆWB&ö¦V7E÷6×ÆU÷&FRÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚“° ¢f÷"G&6²–â6VÆbç&ö¦V7BçG&6·2‚’°¢–bG&6²æ—5ög&÷¦Vâ‚’°¢6öçF–çVS°¢Ð¢ÆWB×WB—FV×2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢—FV×2ç6÷'E÷Vç7F&ÆUö'•ö¶W’‡Æ—FV×Â—FVÒç7F'E÷6×ÆR‚’“°¢ÆWB×WB–æFW‚Ò°¢v†–ÆR–æFW‚Â—FV×2æÆVâ‚’°¢ÆWBf—'7BÒ—FV×5¶–æFW…Ó°¢–b6VÆV7FVEö–G2æ6öçF–ç2‚ff—'7Bæ–B‚’’°¢–æFW‚³Ò°¢6öçF–çVS°¢Ð¢ÆWB×WBÆ7Eö–æFW‚Ò–æFWƒ°¢ÆWB×WBÖW&vVEöÆVæwF‚Òf—'7BæÆVæwF…÷6×ÆW2‚“°¢v†–ÆRÆWB6öÖR†æW‡B’Ò—FV×2ævWB†Æ7Eö–æFW‚²’°¢–b6VÆV7FVEö–G2æ6öçF–ç2‚fæW‡Bæ–B‚’’°¢'&V³°¢Ð¢ÆWB6öÖR‡6÷W&6U÷&FR’Ò6÷W&6U÷6×ÆU÷&FW2ævWB†f—'7BæÖVF–÷&Vb‚’’æ6÷–VB‚¢VÇ6R°¢&WGW&âW'"†f÷&ÖB€¢'6÷W&6R6×ÆR&FR—2Væf–Æ&ÆRf÷"·Ò"À¢f—'7BæÖVF–÷&Vb‚¢’“°¢Ó°¢ÆWB7W'&VçBÒ—FV×5¶Æ7Eö–æFW…Ó°¢–b7W'&VçBçG&6µö–B‚’ÒæW‡BçG&6µö–B‚¢ÇÂ7W'&VçBæÖVF–÷&Vb‚’ÒæW‡BæÖVF–÷&Vb‚¢ÇÂ7W'&VçBç7F'E÷6×ÆR‚’æ6†V6¶VEöFB†7W'&VçBæÆVæwF…÷6×ÆW2‚’¢Ò6öÖR†æW‡Bç7F'E÷6×ÆR‚’¢°¢'&V³°¢Ð¢ÆWB6÷W&6UöFVÇFÒ‡S#ƒ£¦g&öÒ†7W'&VçBæÆVæwF…÷6×ÆW2‚’¢¢S#ƒ£¦g&öÒ‡6÷W&6U÷&FR¢²S#ƒ£¦g&öÒ‡&ö¦V7E÷6×ÆU÷&FR’ò"¢òS#ƒ£¦g&öÒ‡&ö¦V7E÷6×ÆU÷&FR“°¢ÆWB6÷W&6UöFVÇFÒ“cC£§G'•ög&öÒ‡6÷W&6UöFVÇF¢æÖöW'"‡Å÷Â&VF–ò6÷W&6Röfg6WBW†6VVG2F†R7W÷'FVB&ævR"“ó°¢–b7W'&VçBç6÷W&6Uööfg6WE÷6×ÆW2‚’æ6†V6¶VEöFB‡6÷W&6UöFVÇF¢Ò6öÖR†æW‡Bç6÷W&6Uööfg6WE÷6×ÆW2‚’¢°¢'&V³°¢Ð¢ÆWB6öÖR†ÆVæwF‚’ÒÖW&vVEöÆVæwF‚æ6†V6¶VEöFB†æW‡BæÆVæwF…÷6×ÆW2‚’’VÇ6R°¢&WGW&âW'"‚&†VÆVBVF–ò—FVÒW†6VVG2F†R7W÷'FVBÆVæwF‚"çFõö÷væVB‚’“°¢Ó°¢ÖW&vVEöÆVæwF‚ÒÆVæwFƒ°¢Æ7Eö–æFW‚³Ò°¢Ð ¢–bÆ7Eö–æFW‚ÓÒ–æFW‚°¢–æFW‚³Ò°¢6öçF–çVS°¢Ð¢7F–öç2çW6‚„Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–C¢f—'7Bæ–B‚’À¢ÖVF–÷&Vc¢f—'7BæÖVF–÷&Vb‚’çFõö÷væVB‚’À¢7F'E÷6×ÆS¢f—'7Bç7F'E÷6×ÆR‚’À¢6÷W&6Uööfg6WE÷6×ÆW3¢f—'7Bç6÷W&6Uööfg6WE÷6×ÆW2‚’À¢ÆVæwF…÷6×ÆW3¢ÖW&vVEöÆVæwF‚À¢Ò“°¢f÷"—FVÒ–âf—FV×5¶–æFW‚²âãÖÆ7Eö–æFW…Ò°¢7F–öç2çW6‚„Ft7F–öã£¤FVÆWFTVF–ô—FVÒ²—FVÕö–C¢—FVÒæ–B‚’Ò“°¢FVÆWFVEö–G2æ–ç6W'B†—FVÒæ–B‚’“°¢Ð¢–æFW‚ÒÆ7Eö–æFW‚²°¢Ð¢Ð ¢–b7F–öç2æ—5öV×G’‚’°¢ö²„æöæR¢ÒVÇ6R°¢ö²…6öÖR‚€¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢ÒÀ¢FVÆWFVEö–G2À¢’’¢Ð¢Ð ¢fâ7Æ—Eö—FV×5ö–å÷&¦÷%ö&V2‚f×WB6VÆb’°¢ÆWBG&6µö&V2Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–ÇFW"‡ÇG&6·ÂG&6²ç&¦÷%öVF—Eö&V2‚’æ—5öV×G’‚’¢æÖ‡ÇG&6·Â‡G&6²æ–B‚’ÂG&6²ç&¦÷%öVF—Eö&V2‚’çFõ÷fV2‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB&V5ö'•÷G&6²ÒG&6µö&V2æ—FW"‚’æ6ÆöæVB‚’æ6öÆÆV7C££Ä†6„ÖÅòÂóãâ‚“°¢ÆWBÖVF–÷&Vg2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW%öÖ‡Æ—FV×Â°¢ÆWB&V2Ò&V5ö'•÷G&6²ævWB‚f—FVÒçG&6µö–B‚’“ó°¢&V0¢æ—FW"‚¢æç’‡Æ&VÂ°¢—FVÒç7F'E÷6×ÆR‚’Â&Vç7F'E÷6×ÆR‚¢bb&Vç7F'E÷6×ÆR‚’Â—FVÒæVæE÷6×ÆR‚¢ÇÂ—FVÒç7F'E÷6×ÆR‚’Â&VæVæE÷6×ÆR‚¢bb&VæVæE÷6×ÆR‚’Â—FVÒæVæE÷6×ÆR‚¢Ò¢çF†Vâ‡ÇÂ—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6÷W&6U÷6×ÆU÷&FW2ÒÖF6‚6VÆbæVF–õ÷6÷W&6U÷6×ÆU÷&FW2†ÖVF–÷&Vg2’°¢ö²‡&FW2’Óâ&FW2À¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB†7F–öâÂ&V2’Ð¢ÖF6‚6VÆbæÆÅö—FV×5÷&¦÷%ö&V÷7Æ—Eö7F–öâ…6öÖR‚g6÷W&6U÷6×ÆU÷&FW2’’°¢ö²…6öÖR†7F–öâ’’Óâ7F–öâÀ¢ö²„æöæR’Óâ°¢6VÆbç7FGW2Ò$æò&¦÷"VF—B&VFò7Æ—B"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%7Æ—Bf–ÆVC¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ$—FV×27Æ—BB&¦÷"VF—B&÷VæF&–W2"“°¢–b6VÆbç&Wf—6–öâÓÒ&Wf–÷W5÷&Wf—6–öâ°¢&WGW&ã°¢Ð¢ÆWB&V÷F–6·2Ò&V0¢æ—FW"‚¢æÖ‡Â‡G&6µö–BÂG&6µö&V2—Â°¢ÆWBF–6·2ÒG&6µö&V0¢æ—FW"‚¢æf–ÇFW%öÖ‡Æ&VÂ°¢6öÖR‚€¢6VÆbç&ö¦V7BçF–6µöE÷6×ÆR†&Vç7F'E÷6×ÆR‚’’æö²‚“òÀ¢6VÆbç&ö¦V7BçF–6µöE÷6×ÆR†&VæVæE÷6×ÆR‚’’æö²‚“òÀ¢’¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢‚§G&6µö–BÂF–6·2¢Ò¢æ6öÆÆV7C££Ä†6„ÖÅòÂóãâ‚“°¢ÆWB×WB6VÆV7FVBÒ6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â°¢&V2æ—FW"‚’æç’‡Â‡G&6µö–BÂG&6µö&V2—Â°¢§G&6µö–BÓÒ—FVÒçG&6µö–B‚¢bbG&6µö&V2æ—FW"‚’æç’‡Æ&VÂ°¢&Vç7F'E÷6×ÆR‚’ÃÒ—FVÒç7F'E÷6×ÆR‚¢bb—FVÒæVæE÷6×ÆR‚’ÃÒ&VæVæE÷6×ÆR‚¢Ò¢Ò¢Ò¢æÖ‡Æ—FV×Â†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢6VÆV7FVBæW‡FVæB‡6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æf–ÇFW%öÖ‡Æ—FV×Â°¢ÆWB—FVÕöVæBÒ—FVÒç7F'E÷F–6²‚’æ6†V6¶VEöFB†—FVÒæÆVæwF…÷F–6·2‚’“ó°¢&V÷F–6·0¢ævWB‚f—FVÒçG&6µö–B‚’“ð¢æ—FW"‚¢æç’‡Â‡7F'E÷F–6²ÂVæE÷F–6²—Â°¢§7F'E÷F–6²ÃÒ—FVÒç7F'E÷F–6²‚’bb—FVÕöVæBÃÒ¦VæE÷F–6°¢Ò¢çF†Vå÷6öÖR‚†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢Ò’“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢Ð ¢fâ6VÆV7EöÆÅö—FV×2‚f×WB6VÆb’°¢ÆWBG&6µö–G2Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6VÆV7FVBÒ6VÆbæ—FV×5öf÷%÷G&6·2‡G&6µö–G2“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢Ð ¢fâ–çfW'Eö—FVÕ÷6VÆV7F–öâ‚f×WB6VÆb’°¢ÆWBG&6µö–G2Ò6VÆbç&ö¦V7BçG&6·2‚’æ—FW"‚’æÖ‡ÇG&6·ÂG&6²æ–B‚’“°¢ÆWB6VÆV7FVBÒ6VÆ`¢æ—FV×5öf÷%÷G&6·2‡G&6µö–G2¢æ–çFõö—FW"‚¢æf–ÇFW"‡Â†—FVÕö–BÂò—Â6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2†—FVÕö–B’¢æ6öÆÆV7B‚“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢Ð ¢fâ&ö¦V7Eö—FVÕ÷&ævW2‚g6VÆb’ÓâfV3Â„—FVÔ–BÂG&6´–BÂScBÂScB“â°¢ÆWB×WB&ævW2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW%öÖ‡Æ—FV×Â°¢6öÖR‚€¢—FVÒæ–B‚’À¢—FVÒçG&6µö–B‚’À¢6VÆbç&ö¦V7BçF–6µöE÷6×ÆR†—FVÒç7F'E÷6×ÆR‚’’æö²‚“òÀ¢6VÆbç&ö¦V7BçF–6µöE÷6×ÆR†—FVÒæVæE÷6×ÆR‚’’æö²‚“òÀ¢’¢Ò¢æ6†–â‡6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æf–ÇFW%öÖ‡Æ—FV×Â°¢6öÖR‚€¢—FVÒæ–B‚’À¢—FVÒçG&6µö–B‚’À¢—FVÒç7F'E÷F–6²‚’À¢—FVÒç7F'E÷F–6²‚’æ6†V6¶VEöFB†—FVÒæÆVæwF…÷F–6·2‚’“òÀ¢’¢Ò’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBG&6µö÷&FW"Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æÖ‡Â†–æFW‚ÂG&6²—Â‡G&6²æ–B‚’Â–æFW‚’¢æ6öÆÆV7C££Ä†6„ÖÅòÂóãâ‚“°¢&ævW2ç6÷'Eö'•ö¶W’‡Â†—FVÕö–BÂG&6µö–BÂ7F'E÷F–6²Âò—Â°¢€¢G&6µö÷&FW"ævWB‡G&6µö–B’æ6÷–VB‚’çVçw&ö÷"‡W6—¦S£¤Ô‚’À¢§7F'E÷F–6²À¢—FVÕö–BçfÇVR‚’À¢¢Ò“°¢&ævW0¢Ð ¢V"‡7WW"’fâF¦6VçEöæöåö÷fW&Æ–æuö—FV×2‚g6VÆbÂf÷'v&C¢&ööÂ’ÓâfV3Â„—FVÔ–BÂG&6´–B“â°¢ÆWB&ævW2Ò6VÆbç&ö¦V7Eö—FVÕ÷&ævW2‚“°¢&ævW0¢æ—FW"‚¢æf–ÇFW"‡Â†—FVÕö–BÂòÂòÂò—Â6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2†—FVÕö–B’¢æf–ÇFW%öÖ‡Â†7F—fUö–BÂG&6µö–BÂ7F—fU÷7F'BÂ7F—fUöVæB—Â°¢ÆWBF¦6VçBÒ–bf÷'v&B°¢&ævW2æ—FW"‚’æf–æB‡Â…òÂ6æF–FFU÷G&6²Â7F'BÂò—Â°¢6æF–FFU÷G&6²ÓÒG&6µö–Bbb§7F'BÓÒ¦7F—fUöVæ@¢Ò¢ÒVÇ6R°¢&ævW2æ—FW"‚’ç&Wb‚’æf–æB‡Â…òÂ6æF–FFU÷G&6²ÂòÂVæB—Â°¢6æF–FFU÷G&6²ÓÒG&6µö–Bbb¦VæBÓÒ¦7F—fU÷7F'@¢Ò¢Óó°¢†F¦6VçBãÒ¦7F—fUö–B’çF†Vå÷6öÖR‚†F¦6VçBãÂ§G&6µö–B’¢Ò¢æ6öÆÆV7B‚¢Ð ¢fâ6VÆV7EöF¦6VçEöæöåö÷fW&Æ–æuö—FVÒ‚f×WB6VÆbÂf÷'v&C¢&ööÂ’°¢–b6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’°¢&WGW&ã°¢Ð¢ÆWBF¦6VçBÒ6VÆbæF¦6VçEöæöåö÷fW&Æ–æuö—FV×2†f÷'v&B“°¢ÆWB6VÆV7FVE÷G&6²Ò6VÆbçF–ÖVÆ–æRç6VÆV7FVE÷G&6³°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2†F¦6VçB“°¢6VÆbçF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6VÆV7FVE÷G&6³°¢Ð ¢V"‡7WW"’fâF¦6VçE÷G&6µö—FVÒ‚g6VÆbÂæW‡E÷G&6³¢&ööÂ’Óâ÷F–öãÂ„—FVÔ–BÂG&6´–BÂScB“â°¢ÆWB&ævW2Ò6VÆbç&ö¦V7Eö—FVÕ÷&ævW2‚“°¢ÆWB6÷W&6RÒ&ævW0¢æ—FW"‚¢æf–æB‡Â†—FVÕö–BÂòÂòÂò—Â6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2†—FVÕö–B’“ó°¢ÆWB6÷W&6U÷G&6µö–æFW‚Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢ç÷6—F–öâ‡ÇG&6·ÂG&6²æ–B‚’ÓÒ6÷W&6Rã“ó°¢ÆWBF&vWE÷G&6µö–æFW‚Ò–bæW‡E÷G&6²°¢6÷W&6U÷G&6µö–æFW‚æ6†V6¶VEöFBƒ“ð¢ÒVÇ6R°¢6÷W&6U÷G&6µö–æFW‚æ6†V6¶VE÷7V"ƒ“ð¢Ó°¢ÆWBF&vWE÷G&6µö–BÒ6VÆbç&ö¦V7BçG&6·2‚’ævWB‡F&vWE÷G&6µö–æFW‚“òæ–B‚“°¢&ævW0¢æ—FW"‚¢æf–æB‡Â…òÂG&6µö–BÂ7F'BÂVæB—Â°¢§G&6µö–BÓÒF&vWE÷G&6µö–Bbb§7F'BÂ6÷W&6Rã2bb¦VæBâ6÷W&6Rã ¢Ò¢æÖ‡Â†—FVÕö–BÂG&6µö–BÂ7F'BÂò—Â‚¦—FVÕö–BÂ§G&6µö–BÂ§7F'B’¢Ð ¢fâ6VÆV7Eö—FVÕööåöF¦6VçE÷G&6²‚f×WB6VÆbÂæW‡E÷G&6³¢&ööÂ’°¢–b6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’°¢&WGW&ã°¢Ð¢ÆWB6VÆV7FVE÷G&6²Ò6VÆbçF–ÖVÆ–æRç6VÆV7FVE÷G&6³°¢–bÆWB6öÖR‚†—FVÕö–BÂG&6µö–BÂ7F'E÷F–6²’’Ò6VÆbæF¦6VçE÷G&6µö—FVÒ†æW‡E÷G&6²’°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²†—FVÕö–BÂG&6µö–B•Ò“°¢6VÆbçF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6VÆV7FVE÷G&6³°¢6VÆbæ†æFÆU÷F–ÖVÆ–æU÷f–WuöWfVçB‡F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6WDVF—D7W'6÷"‡7F'E÷F–6²’“°¢ÒVÇ6R°¢6VÆbçF–ÖVÆ–æRæ6ÆV%÷6VÆV7FVEö—FV×2‚“°¢Ð¢Ð ¢V"‡7WW"’fâæV&W7Eö—FVÕöVFvR‚g6VÆbÂf÷'v&C¢&ööÂ’Óâ÷F–öãÇF–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öãâ°¢ÆWB7W'6÷%÷6×ÆRÒ6VÆ`¢çF–ÖVÆ–æP¢æ7W'6÷%÷÷6—F–öâ‚¢ç6×ÆU÷÷6—F–öâ‚g6VÆbç&ö¦V7B¢æö²‚“ó°¢ÆWBVF–õöVFvW2Ò6VÆbç&ö¦V7BæVF–õö—FV×2‚’æ—FW"‚’æfÆEöÖ‡Æ—FV×Â°¢¶—FVÒç7F'E÷6×ÆR‚’Â—FVÒæVæE÷6×ÆR‚•Ð¢æ–çFõö—FW"‚¢æÖ‡F–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öã£¥6×ÆR¢Ò“°¢ÆWBÖ–F•öVFvW2Ò6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æfÆEöÖ‡Æ—FV×Â°¢°¢6öÖR†—FVÒç7F'E÷F–6²‚’’À¢—FVÒç7F'E÷F–6²‚’æ6†V6¶VEöFB†—FVÒæÆVæwF…÷F–6·2‚’’À¢Ð¢æ–çFõö—FW"‚¢æfÆGFVâ‚¢æÖ‡F–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öã£¥F–6²¢Ò“°¢VF–õöVFvW0¢æ6†–â†Ö–F•öVFvW2¢æf–ÇFW%öÖ‡Ç÷6—F–öçÂ6öÖR‚‡÷6—F–öâç6×ÆU÷÷6—F–öâ‚g6VÆbç&ö¦V7B’æö²‚“òÂ÷6—F–öâ’’¢æf–ÇFW"‡Â‡6×ÆRÂò—Â°¢–bf÷'v&B°¢§6×ÆRâ7W'6÷%÷6×ÆP¢ÒVÇ6R°¢§6×ÆRÂ7W'6÷%÷6×ÆP¢Ð¢Ò¢ç&VGV6R‡Æ7W'&VçBÂæW‡GÂ°¢ÆWB6†ö÷6UöæW‡BÒ–bf÷'v&B°¢æW‡BãÂ7W'&VçBã ¢ÒVÇ6R°¢æW‡Bãâ7W'&VçBã ¢Ó°¢–b6†ö÷6UöæW‡B²æW‡BÒVÇ6R²7W'&VçBÐ¢Ò¢æÖ‡Â…òÂ÷6—F–öâ—Â÷6—F–öâ¢Ð ¢fâÖ÷fUö7W'6÷%÷Fõ÷6VÆV7FVEö—FV×5ö&÷VæF'’‚f×WB6VÆbÂÖ÷fU÷FõöVæC¢&ööÂ’°¢ÆWBVF–õ÷÷6—F–öç2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚f—FVÒæ–B‚’’¢æÖ‡Æ—FV×Â°¢F–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öã£¥6×ÆR†–bÖ÷fU÷FõöVæB°¢—FVÒæVæE÷6×ÆR‚¢ÒVÇ6R°¢—FVÒç7F'E÷6×ÆR‚¢Ò¢Ò“°¢ÆWBÖ–F•÷÷6—F–öç2Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚f—FVÒæ–B‚’’¢æf–ÇFW%öÖ‡Æ—FV×Â°¢ÆWBF–6²Ò–bÖ÷fU÷FõöVæB°¢—FVÒç7F'E÷F–6²‚’æ6†V6¶VEöFB†—FVÒæÆVæwF…÷F–6·2‚’“ð¢ÒVÇ6R°¢—FVÒç7F'E÷F–6²‚¢Ó°¢6öÖR‡F–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öã£¥F–6²‡F–6²’¢Ò“°¢ÆWB÷6—F–öâÒVF–õ÷÷6—F–öç0¢æ6†–â†Ö–F•÷÷6—F–öç2¢æf–ÇFW%öÖ‡Ç÷6—F–öçÂ6öÖR‚‡÷6—F–öâç6×ÆU÷÷6—F–öâ‚g6VÆbç&ö¦V7B’æö²‚“òÂ÷6—F–öâ’’¢ç&VGV6R‡Æ7W'&VçBÂæW‡GÂ°¢ÆWB6†ö÷6UöæW‡BÒ–bÖ÷fU÷FõöVæB°¢æW‡Bãâ7W'&VçBã ¢ÒVÇ6R°¢æW‡BãÂ7W'&VçBã ¢Ó°¢–b6†ö÷6UöæW‡B²æW‡BÒVÇ6R²7W'&VçBÐ¢Ò¢æÖ‡Â…òÂ÷6—F–öâ—Â÷6—F–öâ“°¢–bÆWB6öÖR‡÷6—F–öâ’Ò÷6—F–öâ°¢6VÆbç6WEöVF—Eö7W'6÷%÷÷6—F–öâ‡÷6—F–öâ“°¢Ð¢Ð ¢V"‡7WW"’fâ6WEöVF—Eö7W'6÷%÷÷6—F–öâ‚f×WB6VÆbÂ÷6—F–öã¢F–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öâ’°¢ÖF6‚÷6—F–öâçF–6²‚g6VÆbç&ö¦V7B’°¢ö²‡F–6²’Óâ°¢6VÆbæ†æFÆU÷F–ÖVÆ–æU÷f–WuöWfVçB‡F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6WDVF—D7W'6÷%÷6—F–öâ°¢F–6²À¢÷6—F–öâÀ¢Ò“°¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚$6÷VÆBæ÷BÖ÷fRF†RVF—B7W'6÷#¢¶W'&÷'Ò"“°¢Ð¢Ð¢Ð ¢fâ6æ÷6VÆV7FVEö—FV×2‚f×WB6VÆbÂF—&V7F–öã¢—FVÕ6æF—&V7F–öâ’°¢–b6öÖÖæG3£§&ö¦V7EöVF—Eö'W7’‡6VÆb’ÇÂ6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’°¢&WGW&ã°¢Ð¢ÆWB6öÖR†w&–E÷F–6·2’Ò6VÆ`¢çF–ÖVÆ–æP¢ç6æöw&–@¢çF–6µö–çFW'fÂ‡6VÆbç&ö¦V7Bç6WGF–æw2‚’ç‚’¢VÇ6R°¢6VÆbç7FGW2Ò%F†R7W'&VçB6æw&–B—2æ÷Bf–Æ&ÆRBF†—2"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWB×WB—FVÕö–G2Ò6VÆ`¢çF–ÖVÆ–æP¢ç6VÆV7FVEö—FV×0¢æ—FW"‚¢æ6÷–VB‚¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢—FVÕö–G2ç6÷'E÷Vç7F&ÆUö'•ö¶W’‡Æ—FVÕö–GÂ—FVÕö–BçfÇVR‚’“°¢ÆWB×WB7F–öç2ÒfV3£§v—F…ö66—G’†—FVÕö–G2æÆVâ‚’“°¢f÷"—FVÕö–B–â—FVÕö–G2°¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢ÆWBF&vWE÷6×ÆRÒÖF6‚VF–õö—FVÕ÷6æ÷F&vWB€¢g6VÆbç&ö¦V7BÀ¢—FVÒç7F'E÷6×ÆR‚’À¢w&–E÷F–6·2À¢F—&V7F–öâÀ¢6VÆbçF–ÖVÆ–æRæ7F—fU÷6æ÷7v–æuöÖ÷VçE÷W&6VçB‚’À¢’°¢ö²‡F&vWB’ÓâF&vWBÀ¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚$6÷VÆBæ÷B6æ6VÆV7FVB—FV×3¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢–bF&vWE÷6×ÆRÒ—FVÒç7F'E÷6×ÆR‚’°¢7F–öç2çW6‚„Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–BÀ¢ÖVF–÷&Vc¢—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’À¢7F'E÷6×ÆS¢F&vWE÷6×ÆRÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢—FVÒç6÷W&6Uööfg6WE÷6×ÆW2‚’À¢ÆVæwF…÷6×ÆW3¢—FVÒæÆVæwF…÷6×ÆW2‚’À¢Ò“°¢Ð¢ÒVÇ6R–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢ÆWBF&vWE÷÷6—F–öâÒÖF6‚Ö–F•ö—FVÕ÷7v–æu÷6æ÷F&vWB€¢g6VÆbç&ö¦V7BÀ¢—FVÒç7F'E÷÷6—F–öâ‚’À¢w&–E÷F–6·2À¢F—&V7F–öâÀ¢6VÆbçF–ÖVÆ–æRæ7F—fU÷6æ÷7v–æuöÖ÷VçE÷W&6VçB‚’À¢’°¢ö²‡F&vWE÷F–6²’ÓâF&vWE÷F–6²À¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚$6÷VÆBæ÷B6æ6VÆV7FVB—FV×3¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢–bF&vWE÷÷6—F–öâÒ—FVÒç7F'E÷÷6—F–öâ‚’°¢7F–öç2çW6‚„Ft7F–öã£¥6WDÖ–F”—FVÕ÷6—F–öâ°¢—FVÕö–BÀ¢÷6—F–öã¢F&vWE÷÷6—F–öâÀ¢Ò“°¢Ð¢Ð¢Ð¢–b7F–öç2æ—5öV×G’‚’°¢&WGW&ã°¢Ð¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢ÒÀ¢%6VÆV7FVB—FV×26æVBFòw&–B"À¢“°¢Ð ¢fâ6VÆV7EöÆÅö—FV×5ö–å÷G&6²‚f×WB6VÆb’°¢ÆWB6VÆV7FVBÒ6VÆbæ—FV×5öf÷%÷G&6·2‡6VÆbç6VÆV7FVE÷G&6µö–G2‚’“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢Ð ¢fâ6VÆV7EöÆÅö—FV×5ö–å÷F–ÖU÷6VÆV7F–öâ‚f×WB6VÆb’°¢ÆWB6öÖR‡6VÆV7F–öâ’Ò6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâVÇ6R°¢&WGW&ã°¢Ó°¢ÆWBG&6µö–G2Ò6VÆbç&ö¦V7BçG&6·2‚’æ—FW"‚’æÖ‡ÇG&6·ÂG&6²æ–B‚’“°¢ÆWB6VÆV7FVBÒ6VÆbæ—FV×5ö–å÷F–ÖU÷6VÆV7F–öâ‡G&6µö–G2Â6VÆV7F–öâ“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢Ð ¢fâ6VÆV7EöÆÅö—FV×5ööå÷6VÆV7FVE÷G&6·5ö–å÷F–ÖU÷6VÆV7F–öâ‚f×WB6VÆb’°¢ÆWB6öÖR‡6VÆV7F–öâ’Ò6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâVÇ6R°¢&WGW&ã°¢Ó°¢ÆWB6VÆV7FVBÒ6VÆbæ—FV×5ö–å÷F–ÖU÷6VÆV7F–öâ‡6VÆbç6VÆV7FVE÷G&6µö–G2‚’Â6VÆV7F–öâ“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢Ð ¢fâ6VÆV7FVE÷G&6µö–G2‚g6VÆb’ÓâfV3ÅG&6´–Câ°¢ÆWB×WB6VÆV7FVBÒ6VÆ`¢çF–ÖVÆ–æP¢ç6VÆV7FVE÷G&6·0¢æ—FW"‚¢æ6÷–VB‚¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢–bÆWB6öÖR†7F—fU÷G&6²’Ò6VÆbç6VÆV7FVE÷G&6µö–B‚’°¢6VÆV7FVBæ–ç6W'B†7F—fU÷G&6²“°¢Ð¢6VÆbç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–ÇFW"‡ÇG&6·Â6VÆV7FVBæ6öçF–ç2‚gG&6²æ–B‚’’¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7B‚¢Ð ¢fâ—FV×5ö–å÷F–ÖU÷6VÆV7F–öâ€¢g6VÆbÀ¢G&6µö–G3¢–×Â–çFô—FW&F÷#Ä—FVÒÒG&6´–CâÀ¢6VÆV7F–öã¢7&FS£§F–ÖVÆ–æS£¥F–ÖU6VÆV7F–öâÀ¢’ÓâfV3Â„—FVÔ–BÂG&6´–B“â°¢ÆWBG&6µö–G2ÒG&6µö–G2æ–çFõö—FW"‚’æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBG&6µö–E÷6WBÒG&6µö–G2æ—FW"‚’æ6÷–VB‚’æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢ÆWBÖF6†–æuö–G2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×ÂG&6µö–E÷6WBæ6öçF–ç2‚f—FVÒçG&6µö–B‚’’¢æf–ÇFW%öÖ‡Æ—FV×Â°¢ÆWB7F'E÷F–6²Ò6VÆbç&ö¦V7BçF–6µöE÷6×ÆR†—FVÒç7F'E÷6×ÆR‚’’æö²‚“ó°¢ÆWBVæE÷F–6²Ò6VÆbç&ö¦V7BçF–6µöE÷6×ÆR†—FVÒæVæE÷6×ÆR‚’’æö²‚“ó°¢‡7F'E÷F–6²Â6VÆV7F–öâæVæE÷F–6²bbVæE÷F–6²â6VÆV7F–öâç7F'E÷F–6²¢çF†Vå÷6öÖR†—FVÒæ–B‚’¢Ò¢æ6†–â€¢6VÆbç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×ÂG&6µö–E÷6WBæ6öçF–ç2‚f—FVÒçG&6µö–B‚’’¢æf–ÇFW%öÖ‡Æ—FV×Â°¢ÆWBVæE÷F–6²Ò—FVÒç7F'E÷F–6²‚’æ6†V6¶VEöFB†—FVÒæÆVæwF…÷F–6·2‚’“ó°¢†—FVÒç7F'E÷F–6²‚’Â6VÆV7F–öâæVæE÷F–6²bbVæE÷F–6²â6VÆV7F–öâç7F'E÷F–6²¢çF†Vå÷6öÖR†—FVÒæ–B‚’¢Ò’À¢¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢6VÆbæ—FV×5öf÷%÷G&6·2‡G&6µö–G2¢æ–çFõö—FW"‚¢æf–ÇFW"‡Â†—FVÕö–BÂò—ÂÖF6†–æuö–G2æ6öçF–ç2†—FVÕö–B’¢æ6öÆÆV7B‚¢Ð ¢fâ—FV×5öf÷%÷G&6·2€¢g6VÆbÀ¢G&6µö–G3¢–×Â–çFô—FW&F÷#Ä—FVÒÒG&6´–CâÀ¢’ÓâfV3Â„—FVÔ–BÂG&6´–B“â°¢ÆWBG&6µö–G2ÒG&6µö–G2æ–çFõö—FW"‚’æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢ÆWB×WB6VÆV7FVBÐ¢fV3£§v—F…ö66—G’‡6VÆbç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’²6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’“°¢f÷"G&6²–â6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–ÇFW"‡ÇG&6·ÂG&6µö–G2æ6öçF–ç2‚gG&6²æ–B‚’’¢°¢ÆWBG&6µö–BÒG&6²æ–B‚“°¢ÆWB×WBG&6µö—FV×2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒG&6µö–B¢æÖ‡Æ—FV×Â°¢€¢6VÆbç&ö¦V7@¢çF–6µöE÷6×ÆR†—FVÒç7F'E÷6×ÆR‚’¢çVçw&ö÷"‡ScC£¤Ô‚’À¢—FVÒæ–B‚’À¢¢Ò¢æ6†–â€¢6VÆbç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒG&6µö–B¢æÖ‡Æ—FV×Â†—FVÒç7F'E÷F–6²‚’Â—FVÒæ–B‚’’’À¢¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢G&6µö—FV×2ç6÷'E÷Vç7F&ÆUö'•ö¶W’‡Â‡F–6²Â—FVÕö–B—Â‚§F–6²Â—FVÕö–BçfÇVR‚’’“°¢6VÆV7FVBæW‡FVæB€¢G&6µö—FV×0¢æ–çFõö—FW"‚¢æÖ‡Â…òÂ—FVÕö–B—Â†—FVÕö–BÂG&6µö–B’’À¢“°¢Ð¢6VÆV7FV@¢Ð ¢fâG&–Õö—FV×5÷Fõ÷6VÆV7FVEö&V‚f×WB6VÆb’°¢–b6öÖÖæG3£§&ö¦V7EöVF—Eö'W7’‡6VÆb’°¢6VÆbç7FGW2Ð¢%7F÷Æ–&6²÷"f–æ—6‚F†R7W'&VçB&ö¦V7B÷W&F–öâ&Vf÷&RG&–ÖÖ–ær—FV×2 ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB6öÖR‡6VÆV7F–öâ’Ò6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâVÇ6R°¢&WGW&ã°¢Ó°¢–b6VÆV7F–öâç7F'E÷F–6²ãÒ6VÆV7F–öâæVæE÷F–6²ÇÂ6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’°¢&WGW&ã°¢Ð¢ÆWB‡6VÆV7F–öå÷7F'E÷6×ÆRÂ6VÆV7F–öåöVæE÷6×ÆR’ÒÖF6‚€¢6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²‡6VÆV7F–öâç7F'E÷F–6²’À¢6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²‡6VÆV7F–öâæVæE÷F–6²’À¢’°¢„ö²‡7F'B’Âö²†VæB’’Óâ‡7F'BÂVæB’À¢„W'"†W'&÷"’Âò’Â…òÂW'"†W'&÷"’’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚$6÷VÆBæ÷BG&–Ò—FV×2FòF†R6VÆV7FVB&V¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó° ¢ÆWB×WB—FVÕö–G2Ò6VÆ`¢çF–ÖVÆ–æP¢ç6VÆV7FVEö—FV×0¢æ—FW"‚¢æ6÷–VB‚¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢—FVÕö–G2ç6÷'E÷Vç7F&ÆUö'•ö¶W’‡Æ—FVÕö–GÂ—FVÕö–BçfÇVR‚’“°¢ÆWB×WB7F–öç2ÒfV3£§v—F…ö66—G’†—FVÕö–G2æÆVâ‚’“°¢f÷"—FVÕö–B–â—FVÕö–G2°¢–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢–b6VÆV7F–öå÷7F'E÷6×ÆRãÒ6VÆV7F–öåöVæE÷6×ÆR°¢6öçF–çVS°¢Ð¢ÆWB—FVÕ÷7F'BÒ—FVÒç7F'E÷6×ÆR‚“°¢ÆWB6öÖR†—FVÕöVæB’Ò—FVÕ÷7F'Bæ6†V6¶VEöFB†—FVÒæÆVæwF…÷6×ÆW2‚’’VÇ6R°¢6VÆbç7FGW2Ð¢$6÷VÆBæ÷BG&–ÒâVF–ò—FVÒv—F‚â–çfÆ–B÷6—F–öâ"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢–b—FVÕöVæBÂ6VÆV7F–öå÷7F'E÷6×ÆRÇÂ—FVÕ÷7F'Bâ6VÆV7F–öåöVæE÷6×ÆR°¢6öçF–çVS°¢Ð¢–b—FVÕöVæBÓÒ6VÆV7F–öå÷7F'E÷6×ÆRÇÂ—FVÕ÷7F'BÓÒ6VÆV7F–öåöVæE÷6×ÆR°¢7F–öç2çW6‚„Ft7F–öã£¤FVÆWFTVF–ô—FVÒ²—FVÕö–BÒ“°¢6öçF–çVS°¢Ð¢ÆWBæWu÷7F'BÒ—FVÕ÷7F'BæÖ‚‡6VÆV7F–öå÷7F'E÷6×ÆR“°¢ÆWBæWuöVæBÒ—FVÕöVæBæÖ–â‡6VÆV7F–öåöVæE÷6×ÆR“°¢–bæWu÷7F'BãÒæWuöVæB°¢7F–öç2çW6‚„Ft7F–öã£¤FVÆWFTVF–ô—FVÒ²—FVÕö–BÒ“°¢6öçF–çVS°¢Ð¢–bæWu÷7F'BÓÒ—FVÕ÷7F'BbbæWuöVæBÓÒ—FVÕöVæB°¢6öçF–çVS°¢Ð¢ÆWB6öÖR‡6÷W&6UöFVÇF’Ò“cC£§G'•ög&öÒ†æWu÷7F'BÒ—FVÕ÷7F'B’æö²‚’VÇ6R°¢6VÆbç7FGW2Ð¢$6÷VÆBæ÷BG&–ÒâVF–ò—FVÒ7B—G26÷W&6R÷6—F–öâ"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWB6öÖR‡6÷W&6Uööfg6WE÷6×ÆW2’Ð¢—FVÒç6÷W&6Uööfg6WE÷6×ÆW2‚’æ6†V6¶VEöFB‡6÷W&6UöFVÇF¢VÇ6R°¢6VÆbç7FGW2Ð¢$6÷VÆBæ÷BG&–ÒâVF–ò—FVÒ7B—G26÷W&6R÷6—F–öâ"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢7F–öç2çW6‚„Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–BÀ¢ÖVF–÷&Vc¢—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’À¢7F'E÷6×ÆS¢æWu÷7F'BÀ¢6÷W&6Uööfg6WE÷6×ÆW2À¢ÆVæwF…÷6×ÆW3¢æWuöVæBÒæWu÷7F'BÀ¢Ò“°¢ÒVÇ6R–bÆWB6öÖR†—FVÒ’Ò6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢ÆWB—FVÕ÷7F'BÒ—FVÒç7F'E÷F–6²‚“°¢ÆWB6öÖR†—FVÕöVæB’Ò—FVÕ÷7F'Bæ6†V6¶VEöFB†—FVÒæÆVæwF…÷F–6·2‚’’VÇ6R°¢6VÆbç7FGW2Ò$6÷VÆBæ÷BG&–ÒÔ”D’—FVÒv—F‚â–çfÆ–B÷6—F–öâ"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢–b—FVÕöVæBÂ6VÆV7F–öâç7F'E÷F–6²ÇÂ—FVÕ÷7F'Bâ6VÆV7F–öâæVæE÷F–6²°¢6öçF–çVS°¢Ð¢–b—FVÕöVæBÓÒ6VÆV7F–öâç7F'E÷F–6²ÇÂ—FVÕ÷7F'BÓÒ6VÆV7F–öâæVæE÷F–6²°¢7F–öç2çW6‚„Ft7F–öã£¤FVÆWFTÖ–F”—FVÒ²—FVÕö–BÒ“°¢6öçF–çVS°¢Ð¢ÆWBæWu÷7F'BÒ—FVÕ÷7F'BæÖ‚‡6VÆV7F–öâç7F'E÷F–6²“°¢ÆWBæWuöVæBÒ—FVÕöVæBæÖ–â‡6VÆV7F–öâæVæE÷F–6²“°¢–bæWu÷7F'BãÒæWuöVæB°¢7F–öç2çW6‚„Ft7F–öã£¤FVÆWFTÖ–F”—FVÒ²—FVÕö–BÒ“°¢6öçF–çVS°¢Ð¢–bæWu÷7F'BÓÒ—FVÕ÷7F'BbbæWuöVæBÓÒ—FVÕöVæB°¢6öçF–çVS°¢Ð¢ÆWBæWuöÆVæwF‚ÒæWuöVæBÒæWu÷7F'C°¢–bæWu÷7F'Bâ—FVÕ÷7F'B°¢ÆWB6öÖR‡6÷W&6Uööfg6WE÷F–6·2’Ò—FVÐ¢ç6÷W&6Uööfg6WE÷F–6·2‚¢æ6†V6¶VEöFB†æWu÷7F'BÒ—FVÕ÷7F'B¢VÇ6R°¢6VÆbç7FGW2Ð¢$6÷VÆBæ÷BG&–ÒÔ”D’—FVÒ7B—G26÷W&6R÷6—F–öâ"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢7F–öç2çW6‚„Ft7F–öã£¥G&–ÔÖ–F”—FVÕ7F'B°¢—FVÕö–BÀ¢7F'E÷F–6³¢æWu÷7F'BÀ¢ÆVæwF…÷F–6·3¢æWuöÆVæwF‚À¢6÷W&6Uööfg6WE÷F–6·2À¢Ò“°¢ÒVÇ6R°¢7F–öç2çW6‚„Ft7F–öã£¤VF—DÖ–F”—FVÒ°¢—FVÕö–BÀ¢7F'E÷F–6³¢æWu÷7F'BÀ¢ÆVæwF…÷F–6·3¢æWuöÆVæwF‚À¢Ò“°¢Ð¢Ð¢Ð¢–b7F–öç2æ—5öV×G’‚’°¢&WGW&ã°¢Ð¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2æ6ÆV"‚“°¢ÆWB7F–öâÒ–b7F–öç2æÆVâ‚’ÓÒ°¢7F–öç2ç÷‚’æW‡V7B‚'6–ævÆRG&–Ò7F–öâ—2&W6VçB"¢ÒVÇ6R°¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ð¢Ó°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ%6VÆV7FVB—FV×2G&–ÖÖVBFò6VÆV7FVB&V"“°¢Ð ¢fâ6VÆV7FVEöVF–õö—FV×5öf÷%÷F¶Uö6Æ—&ö&B‚g6VÆb’ÓâfV3ÂfFuö6÷&S£¤VF–ô—FVÓâ°¢6VÆbç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚f—FVÒæ–B‚’’¢æ6öÆÆV7B‚¢Ð ¢V"‡7WW"’fâ6åö6÷•÷6VÆV7FVEöVF–õö—FV×2‚g6VÆb’Óâ&ööÂ°¢6VÆbç6VÆV7FVEöVF–õö—FV×5öf÷%÷F¶Uö6Æ—&ö&B‚’æ—5öV×G’‚¢Ð ¢fâ6÷•÷6VÆV7FVEöVF–õö—FV×2‚f×WB6VÆb’°¢ÆWB—FV×2Ò6VÆbç6VÆV7FVEöVF–õö—FV×5öf÷%÷F¶Uö6Æ—&ö&B‚“°¢–b—FV×2æ—5öV×G’‚’°¢6VÆbç7FGW2Ò%6VÆV7BâVF–ò—FVÒFò6÷’—G27F—fRF¶R"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWBF¶W2Ò—FV×0¢æ—FW"‚¢æÖ‡Æ—FV×Â—FVÒæ7F—fU÷F¶U÷6æ6†÷B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBF¶Uö6÷VçBÒF¶W2æÆVâ‚“°¢6VÆbæVF–õ÷F¶Uö6Æ—&ö&BÒ6öÖR„VF–õF¶T6Æ—&ö&B°¢&ö¦V7EövVæW&F–öã¢6VÆbç&ö¦V7EövVæW&F–öâÀ¢F¶W2À¢Ò“°¢6VÆbç7FGW2Ò–bF¶Uö6÷VçBÓÒ°¢$7F—fRVF–òF¶R6÷–VB"çFõö÷væVB‚¢ÒVÇ6R°¢f÷&ÖB‚$6÷–VB·F¶Uö6÷VçGÒ7F—fRVF–òF¶W2"¢Ó°¢Ð ¢V"‡7WW"’fâ6å÷7FUöVF–õö—FVÕö5÷F¶R‚g6VÆb’Óâ&ööÂ°¢ÆWB6öÖR†6Æ—&ö&B’Ò6VÆbæVF–õ÷F¶Uö6Æ—&ö&Bæ5÷&Vb‚’VÇ6R°¢&WGW&âfÇ6S°¢Ó°¢–b6Æ—&ö&Bç&ö¦V7EövVæW&F–öâÒ6VÆbç&ö¦V7EövVæW&F–öâ°¢&WGW&âfÇ6S°¢Ð¢ÆWB6VÆV7FVEöVF–õö–G2Ò6öÖÖæG3£§6VÆV7FVEöVF–õö—FVÕö–G2‡6VÆb“°¢6VÆV7FVEöVF–õö–G2æ—5öV×G’‚¢bb6VÆV7FVEöVF–õö–G2æ—FW"‚’æç’‡Æ—FVÕö–GÂ°¢6VÆbç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æç’‡ÇG&6·ÂG&6²æg&÷¦VåöVF–õö—FVÕö–B‚’ÓÒ6öÖR‚¦—FVÕö–B’¢Ò¢Ð ¢fâ7FUöVF–õö—FVÕö5÷F¶R‚f×WB6VÆb’°¢–b6öÖÖæG3£§&ö¦V7EöVF—Eö'W7’‡6VÆb’°¢6VÆbç7FGW2Ð¢%7F÷Æ–&6²÷"f–æ—6‚F†R7W'&VçB&ö¦V7B÷W&F–öâ&Vf÷&R7F–ærF¶W2 ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢–b6VÆbæ6å÷7FUöVF–õö—FVÕö5÷F¶R‚’°¢&WGW&ã°¢Ð¢ÆWB6VÆV7FVEöVF–õö–G2Ò6öÖÖæG3£§6VÆV7FVEöVF–õö—FVÕö–G2‡6VÆb“°¢ÆWB6öÖR‡F¶W2’Ò6VÆ`¢æVF–õ÷F¶Uö6Æ—&ö&@¢æ5÷&Vb‚¢æÖ‡Æ6Æ—&ö&GÂ6Æ—&ö&BçF¶W2æ6ÆöæR‚’¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB×WB7F–öç2Ò6VÆV7FVEöVF–õö–G0¢æ—FW"‚¢æfÆEöÖ‡Æ—FVÕö–GÂ°¢F¶W0¢æ—FW"‚¢æ6ÆöæVB‚¢æÖ†Ö÷fRÇF¶WÂFt7F–öã£¥7FTVF–ô—FVÔ5F¶R°¢—FVÕö–C¢¦—FVÕö–BÀ¢F¶RÀ¢Ò¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB7FVE÷F¶Uö6÷VçBÒF¶W2æÆVâ‚“°¢ÆWB6VÆV7FVEö—FVÕö6÷VçBÒ6VÆV7FVEöVF–õö–G2æÆVâ‚“°¢ÆWB7F–öâÒ–b7F–öç2æÆVâ‚’ÓÒ°¢7F–öç0¢ç÷‚¢æW‡V7B‚&öæR6VÆV7FVBVF–ò—FVÒ7F–öâ—2&W6VçB"¢ÒVÇ6R°¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ð¢Ó°¢6VÆbæÇ•ö7F–öâ€¢7F–öâÀ¢–b7FVE÷F¶Uö6÷VçBÓÒbb6VÆV7FVEö—FVÕö6÷VçBÓÒ°¢%F¶R7FVB–çFòVF–ò—FVÒ ¢ÒVÇ6R–b7FVE÷F¶Uö6÷VçBÓÒ°¢%F¶R7FVB–çFò6VÆV7FVBVF–ò—FV×2 ¢ÒVÇ6R–b6VÆV7FVEö—FVÕö6÷VçBÓÒ°¢%F¶W27FVB–çFòVF–ò—FVÒ ¢ÒVÇ6R°¢%F¶W27FVB–çFò6VÆV7FVBVF–ò—FV×2 ¢ÒÀ¢“°¢Ð ¢fâ–×ÆöFU÷6VÆV7FVEöVF–õö—FV×5ö5÷F¶W2‚f×WB6VÆb’°¢–b6öÖÖæG3£§&ö¦V7EöVF—Eö'W7’‡6VÆb’°¢6VÆbç7FGW2Ð¢%7F÷Æ–&6²÷"f–æ—6‚F†R7W'&VçB&ö¦V7B÷W&F–öâ&Vf÷&R–×ÆöF–ær—FV×2 ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB6öÖR†—FVÕö–G2’Ò6öÖÖæG3£¦VF–õö—FV×5öf÷%ö–×ÆöFR‡6VÆb’VÇ6R°¢6VÆbç7FGW2Ò%6VÆV7BBÆV7BGvòVF–ò—FV×2öâF†R6ÖRG&6²"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢–b—FVÕö–G2æ—FW"‚’æç’‡Æ—FVÕö–GÂ°¢6VÆbç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æç’‡ÇG&6·ÂG&6²æg&÷¦VåöVF–õö—FVÕö–B‚’ÓÒ6öÖR‚¦—FVÕö–B’¢Ò’°¢6VÆbç7FGW2Ò$g&÷¦Vâ&VæFW"—FV×26ææ÷B&R–×ÆöFVB–çFòF¶W2"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWBF&vWEö–BÒ—FVÕö–G5³Ó°¢ÆWB6öÖR‡F&vWB’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒF&vWEö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBG&6µö–BÒF&vWBçG&6µö–B‚“°¢ÆWB÷&–v–æÅö7F—fU÷F¶RÒF&vWBæ7F—fU÷F¶Uö–æFW‚‚“°¢ÆWBFöæ÷'2Ò—FVÕö–G0¢æ—FW"‚¢ç6¶—ƒ¢æf–ÇFW%öÖ‡Æ—FVÕö–GÂ°¢6VÆbç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ¦—FVÕö–B¢æÖ‡Æ—FV×Â‚¦—FVÕö–BÂ—FVÒçF¶U÷6æ6†÷G2‚’’¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–bFöæ÷'2æÆVâ‚’²Ò—FVÕö–G2æÆVâ‚’°¢&WGW&ã°¢Ð ¢ÆWB×WB7F–öç2ÒFöæ÷'0¢æ—FW"‚¢æfÆEöÖ‡Â…òÂF¶W2—Â°¢F¶W0¢æ—FW"‚¢æ6ÆöæVB‚¢æÖ†Ö÷fRÇF¶WÂFt7F–öã£¥7FTVF–ô—FVÔ5F¶R°¢—FVÕö–C¢F&vWEö–BÀ¢F¶RÀ¢Ò¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢7F–öç2çW6‚„Ft7F–öã£¥6WDVF–ô—FVÔ7F—fUF¶R°¢—FVÕö–C¢F&vWEö–BÀ¢F¶Uö–æFWƒ¢÷&–v–æÅö7F—fU÷F¶RÀ¢Ò“°¢7F–öç2æW‡FVæB€¢Föæ÷'0¢æ—FW"‚¢æÖ‡Â†—FVÕö–BÂò—ÂFt7F–öã£¤FVÆWFTVF–ô—FVÒ²—FVÕö–C¢¦—FVÕö–BÒ’À¢“° ¢f÷"—FVÕö–B–âf—FVÕö–G2°¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2ç&VÖ÷fR†—FVÕö–B“°¢Ð¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢ÆWB&ö¦V7Eö†—7F÷'•ö7W'6÷%ö&Vf÷&RÒ6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚“°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢ÒÀ¢%6VÆV7FVBVF–ò—FV×2–×ÆöFVB–çFòF¶W2"À¢“°¢–b6VÆbç&Wf—6–öâÒ&Wf–÷W5÷&Wf—6–öâ°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²‡F&vWEö–BÂG&6µö–B•Ò“°¢ÆWBgFW"Ò6VÆbçF–ÖVÆ–æRæ—FVÕ÷6VÆV7F–öå÷7FFR‚“°¢6VÆbçF–ÖVÆ–æRæ6ÆV%÷6VÆV7FVEö—FV×2‚“°¢ÆWB&Vf÷&RÒ6VÆbçF–ÖVÆ–æRæ—FVÕ÷6VÆV7F–öå÷7FFR‚“°¢6VÆbçF–ÖVÆ–æRç&W7F÷&Uö—FVÕ÷6VÆV7F–öå÷7FFR‚fgFW"“°¢–bÖF6†W2‡6VÆbæ÷VæFõö†—7F÷'’æÆ7B‚’Â6öÖR„†—7F÷'”VçG'“£¥&ö¦V7B’’°¢6VÆbæ÷VæFõö†—7F÷'’ç÷‚“°¢6VÆbæ÷VæFõö†—7F÷'¢çW6‚„†—7F÷'”VçG'“£¥&ö¦V7Ev—F„—FVÕ6VÆV7F–öâ€¢—FVÕ6VÆV7F–öä†—7F÷'’°¢&Vf÷&S¢&÷ƒ£¦æWr†&Vf÷&R’À¢gFW#¢&÷ƒ£¦æWr†gFW"’À¢&ö¦V7Eö†—7F÷'•ö7W'6÷%ö&Vf÷&RÀ¢&ö¦V7Eö†—7F÷'•ö7W'6÷%ögFW#¢6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’À¢ÒÀ¢’“°¢Ð¢Ð¢Ð ¢fâ–×ÆöFU÷6VÆV7FVEöVF–õö—FV×5ö7&÷75÷G&6·5ö5÷F¶W2‚f×WB6VÆb’°¢–b6öÖÖæG3£§&ö¦V7EöVF—Eö'W7’‡6VÆb’°¢6VÆbç7FGW2Ð¢%7F÷Æ–&6²÷"f–æ—6‚F†R7W'&VçB&ö¦V7B÷W&F–öâ&Vf÷&R–×ÆöF–ær—FV×2 ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB6öÖR†—FVÕö–G2’Ò6öÖÖæG3£¦VF–õö—FV×5öf÷%ö7&÷75÷G&6µö–×ÆöFR‡6VÆb’VÇ6R°¢6VÆbç7FGW2Ò%6VÆV7BVF–ò—FV×2öâBÆV7BGvòG&6·2"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢–b—FVÕö–G2æ—FW"‚’æç’‡Æ—FVÕö–GÂ°¢6VÆbç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æç’‡ÇG&6·ÂG&6²æg&÷¦VåöVF–õö—FVÕö–B‚’ÓÒ6öÖR‚¦—FVÕö–B’¢Ò’°¢6VÆbç7FGW2Ò$g&÷¦Vâ&VæFW"—FV×26ææ÷B&R–×ÆöFVB–çFòF¶W2"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWBF&vWEö–BÒ—FVÕö–G5³Ó°¢ÆWB6öÖR‡F&vWB’Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒF&vWEö–B¢VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBF&vWE÷G&6µö–BÒF&vWBçG&6µö–B‚“°¢ÆWBF&vWE÷7F'E÷6×ÆRÒF&vWBç7F'E÷6×ÆR‚“°¢ÆWB÷&–v–æÅö7F—fU÷F¶RÒF&vWBæ7F—fU÷F¶Uö–æFW‚‚“°¢ÆWBFöæ÷'2Ò—FVÕö–G0¢æ—FW"‚¢ç6¶—ƒ¢æf–ÇFW%öÖ‡Æ—FVÕö–GÂ°¢6VÆbç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ¦—FVÕö–B¢æÖ‡Æ—FV×Â‚¦—FVÕö–BÂ—FVÒç7F'E÷6×ÆR‚’Â—FVÒçF¶U÷6æ6†÷G2‚’’¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–bFöæ÷'2æÆVâ‚’²Ò—FVÕö–G2æÆVâ‚’°¢&WGW&ã°¢Ð ¢ÆWBÖVF–÷&Vg2ÒFöæ÷'0¢æ—FW"‚¢æf–ÇFW"‡Â…òÂ7F'E÷6×ÆRÂò—Â§7F'E÷6×ÆRÒF&vWE÷7F'E÷6×ÆR¢æfÆEöÖ‡Â…òÂòÂF¶W2—ÂF¶W2æ—FW"‚’æÖ‡ÇF¶WÂF¶RæÖVF–÷&Vbæ6ÆöæR‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6÷W&6U÷6×ÆU÷&FW2ÒÖF6‚6VÆbæVF–õ÷6÷W&6U÷6×ÆU÷&FW2†ÖVF–÷&Vg2’°¢ö²‡&FW2’Óâ&FW2À¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Ð¢f÷&ÖB‚$6÷VÆBæ÷BÆ–vâ6÷W&6RF¶W2f÷"7&÷72×G&6²–×ÆöFS¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB&ö¦V7E÷6×ÆU÷&FRÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚“°¢ÆWB×WB7F–öç2ÒfV3£¦æWr‚“°¢f÷"…òÂFöæ÷%÷7F'E÷6×ÆRÂF¶W2’–âfFöæ÷'2°¢ÆWB&ö¦V7E÷÷6—F–öç5öF–ffW"Ò¦Föæ÷%÷7F'E÷6×ÆRÒF&vWE÷7F'E÷6×ÆS°¢f÷"×WBF¶R–âF¶W2æ—FW"‚’æ6ÆöæVB‚’°¢–b&ö¦V7E÷÷6—F–öç5öF–ffW"°¢ÆWB6öÖR‡6÷W&6U÷6×ÆU÷&FR’Ð¢6÷W&6U÷6×ÆU÷&FW2ævWB‚gF¶RæÖVF–÷&Vb’æ6÷–VB‚¢VÇ6R°¢6VÆbç7FGW2Òf÷&ÖB€¢$6÷VÆBæ÷BÆ–vâ6÷W&6RF¶R·Ó²6×ÆR&FR—2Væf–Æ&ÆR"À¢F¶RææÖP¢“°¢&WGW&ã°¢Ó°¢ÆWB6öÖR‡6÷W&6Uööfg6WE÷6×ÆW2’Ò&V&6UöVF–õ÷F¶U÷6÷W&6Uööfg6WE÷6×ÆW2€¢F¶Rç6÷W&6Uööfg6WE÷6×ÆW2À¢F&vWE÷7F'E÷6×ÆRÀ¢¦Föæ÷%÷7F'E÷6×ÆRÀ¢6÷W&6U÷6×ÆU÷&FRÀ¢&ö¦V7E÷6×ÆU÷&FRÀ¢F¶RçÆ–&6µ÷&FRÀ¢’VÇ6R°¢6VÆbç7FGW2Òf÷&ÖB€¢$6÷VÆBæ÷BÆ–vâ6÷W&6RF¶R·Ó²&W7VÇF–æröfg6WB—2÷WBöb&ævR"À¢F¶RææÖP¢“°¢&WGW&ã°¢Ó°¢F¶Rç6÷W&6Uööfg6WE÷6×ÆW2Ò6÷W&6Uööfg6WE÷6×ÆW3°¢Ð¢7F–öç2çW6‚„Ft7F–öã£¥7FTVF–ô—FVÔ5F¶R°¢—FVÕö–C¢F&vWEö–BÀ¢F¶RÀ¢Ò“°¢Ð¢Ð¢7F–öç2çW6‚„Ft7F–öã£¥6WDVF–ô—FVÔ7F—fUF¶R°¢—FVÕö–C¢F&vWEö–BÀ¢F¶Uö–æFWƒ¢÷&–v–æÅö7F—fU÷F¶RÀ¢Ò“°¢7F–öç2æW‡FVæB€¢Föæ÷'0¢æ—FW"‚¢æÖ‡Â†—FVÕö–BÂòÂò—ÂFt7F–öã£¤FVÆWFTVF–ô—FVÒ²—FVÕö–C¢¦—FVÕö–BÒ’À¢“° ¢f÷"—FVÕö–B–âf—FVÕö–G2°¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2ç&VÖ÷fR†—FVÕö–B“°¢Ð¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢ÆWB&ö¦V7Eö†—7F÷'•ö7W'6÷%ö&Vf÷&RÒ6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚“°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢ÒÀ¢%6VÆV7FVBVF–ò—FV×2–×ÆöFVB–çFòF¶W2"À¢“°¢–b6VÆbç&Wf—6–öâÒ&Wf–÷W5÷&Wf—6–öâ°¢6VÆbçF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²‡F&vWEö–BÂF&vWE÷G&6µö–B•Ò“°¢ÆWBgFW"Ò6VÆbçF–ÖVÆ–æRæ—FVÕ÷6VÆV7F–öå÷7FFR‚“°¢6VÆbçF–ÖVÆ–æRæ6ÆV%÷6VÆV7FVEö—FV×2‚“°¢ÆWB&Vf÷&RÒ6VÆbçF–ÖVÆ–æRæ—FVÕ÷6VÆV7F–öå÷7FFR‚“°¢6VÆbçF–ÖVÆ–æRç&W7F÷&Uö—FVÕ÷6VÆV7F–öå÷7FFR‚fgFW"“°¢–bÖF6†W2‡6VÆbæ÷VæFõö†—7F÷'’æÆ7B‚’Â6öÖR„†—7F÷'”VçG'“£¥&ö¦V7B’’°¢6VÆbæ÷VæFõö†—7F÷'’ç÷‚“°¢6VÆbæ÷VæFõö†—7F÷'¢çW6‚„†—7F÷'”VçG'“£¥&ö¦V7Ev—F„—FVÕ6VÆV7F–öâ€¢—FVÕ6VÆV7F–öä†—7F÷'’°¢&Vf÷&S¢&÷ƒ£¦æWr†&Vf÷&R’À¢gFW#¢&÷ƒ£¦æWr†gFW"’À¢&ö¦V7Eö†—7F÷'•ö7W'6÷%ö&Vf÷&RÀ¢&ö¦V7Eö†—7F÷'•ö7W'6÷%ögFW#¢6VÆbç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’À¢ÒÀ¢’“°¢Ð¢Ð¢Ð ¢fâW‡ÆöFU÷6VÆV7FVEöVF–õö—FVÕ÷F¶W5ö–å÷Æ6R‚f×WB6VÆb’°¢–b6öÖÖæG3£§&ö¦V7EöVF—Eö'W7’‡6VÆb’°¢6VÆbç7FGW2Ð¢%7F÷Æ–&6²÷"f–æ—6‚F†R7W'&VçB&ö¦V7B÷W&F–öâ&Vf÷&RW‡ÆöF–ærF¶W2 ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB6öÖR†—FVÕö–G2’Ò6öÖÖæG3£¦ÖVF–ö—FV×5öf÷%öW‡ÆöFU÷F¶W2‡6VÆb’VÇ6R°¢6VÆbç7FGW2Ò%6VÆV7BÖVF–—FV×2FòW‡ÆöFRF†V—"F¶W2"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWBVF–õö–G2Ò—FVÕö–G0¢æ—FW"‚¢æf–ÇFW"‡Æ—FVÕö–GÂ°¢6VÆbç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ¢¦—FVÕö–B¢æ—5÷6öÖR‚¢Ò¢æ6÷–VB‚¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBÖ–F•ö–G2Ò—FVÕö–G0¢æ—FW"‚¢æf–ÇFW"‡Æ—FVÕö–GÂ°¢6VÆbç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ¢¦—FVÕö–B¢æ—5÷6öÖR‚¢Ò¢æ6÷–VB‚¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB†5ö×VÇF•÷F¶UöVF–òÒVF–õö–G2æ—FW"‚’æç’‡Æ—FVÕö–GÂ°¢6VÆbç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ¦—FVÕö–B¢æ—5÷6öÖUöæB‡Æ—FV×Â—FVÒçF¶Uö6÷VçB‚’â¢Ò“°¢ÆWB†5ö×VÇF•÷F¶UöÖ–F’ÒÖ–F•ö–G2æ—FW"‚’æç’‡Æ—FVÕö–GÂ°¢6VÆbç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ¦—FVÕö–B¢æ—5÷6öÖUöæB‡Æ—FV×Â—FVÒçF¶Uö6÷VçB‚’â¢Ò“°¢–b†5ö×VÇF•÷F¶UöVF–òbb†5ö×VÇF•÷F¶UöÖ–F’°¢6VÆbçF–ÖVÆ–æRæ6ÆV%÷6VÆV7FVEö—FV×2‚“°¢6VÆbç7FGW2Ò$æò×VÇF’×F¶R—FV×26VÆV7FVB"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢–bVF–õö–G2æ—FW"‚’æç’‡Æ—FVÕö–GÂ°¢6VÆbç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ¦—FVÕö–B¢æ—5÷6öÖUöæB‡Æ—FV×Â°¢—FVÒçF¶Uö6÷VçB‚’â¢bb6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æç’‡ÇG&6·ÂG&6²æg&÷¦VåöVF–õö—FVÕö–B‚’ÓÒ6öÖR‚¦—FVÕö–B’¢Ò¢Ò’ÇÂÖ–F•ö–G2æ—FW"‚’æç’‡Æ—FVÕö–GÂ°¢6VÆbç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ¦—FVÕö–B¢æ—5÷6öÖUöæB‡Æ—FV×Â°¢—FVÒçF¶Uö6÷VçB‚’â¢bb6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æç’‡ÇG&6·ÂG&6²æ–B‚’ÓÒ—FVÒçG&6µö–B‚’bbG&6²æ—5ög&÷¦Vâ‚’¢Ò¢Ò’°¢6VÆbç7FGW2Ò$g&÷¦Vâ—FV×26ææ÷B&RW‡ÆöFVB"çFõö÷væVB‚“°¢&WGW&ã°¢Ð ¢f÷"—FVÕö–B–âfVF–õö–G2°¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2ç&VÖ÷fR†—FVÕö–B“°¢Ð¢ÆWB×WB7F–öç2ÒfV3£§v—F…ö66—G’ƒ"“°¢–b†5ö×VÇF•÷F¶UöVF–ò°¢7F–öç2çW6‚„Ft7F–öã£¤W‡ÆöFTVF–ô—FVÕF¶W4–åÆ6R°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢—FVÕö–G3¢VF–õö–G2À¢Ò“°¢Ð¢–b†5ö×VÇF•÷F¶UöÖ–F’°¢7F–öç2çW6‚„Ft7F–öã£¤W‡ÆöFTÖ–F”—FVÕF¶W4–åÆ6R°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢—FVÕö–G3¢Ö–F•ö–G2À¢Ò“°¢Ð¢ÆWB7F–öâÒ–b7F–öç2æÆVâ‚’ÓÒ°¢7F–öç2ç÷‚’æW‡V7B‚&öæRF¶RW‡ÆöFR7F–öâW†—7G2"¢ÒVÇ6R°¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ð¢Ó°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ%6VÆV7FVB—FVÒF¶W2W‡ÆöFVB–âÆ6R"“°¢6VÆbçF–ÖVÆ–æRæ6ÆV%÷6VÆV7FVEö—FV×2‚“°¢Ð ¢fâ6÷•öÆö÷ööe÷6VÆV7FVEö&V‚f×WB6VÆb’°¢ÆWB6öÖR‡6VÆV7F–öâ’Ò6VÆbçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâVÇ6R°¢&WGW&ã°¢Ó°¢–b6VÆV7F–öâç7F'E÷F–6²ãÒ6VÆV7F–öâæVæE÷F–6²°¢&WGW&ã°¢Ð¢ÆWB‡6VÆV7F–öå÷7F'E÷6×ÆRÂ6VÆV7F–öåöVæE÷6×ÆR’ÒÖF6‚€¢6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²‡6VÆV7F–öâç7F'E÷F–6²’À¢6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²‡6VÆV7F–öâæVæE÷F–6²’À¢’°¢„ö²‡7F'B’Âö²†VæB’’Óâ‡7F'BÂVæB’À¢„W'"†W'&÷"’Âò’Â…òÂW'"†W'&÷"’’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWB7å÷6×ÆW2Ò6VÆV7F–öåöVæE÷6×ÆRç6GW&F–æu÷7V"‡6VÆV7F–öå÷7F'E÷6×ÆR“°¢–b7å÷6×ÆW2ÓÒ°¢&WGW&ã°¢Ð ¢ÆWB×WB6öçF–æ–æuö—FV×2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚f—FVÒæ–B‚’’¢æf–ÇFW%öÖ‡Æ—FV×Â°¢ÆWBVæE÷6×ÆRÒ—FVÒç7F'E÷6×ÆR‚’æ6†V6¶VEöFB†—FVÒæÆVæwF…÷6×ÆW2‚’“ó°¢†—FVÒç7F'E÷6×ÆR‚’ÃÒ6VÆV7F–öå÷7F'E÷6×ÆP¢bbVæE÷6×ÆRãÒ6VÆV7F–öåöVæE÷6×ÆR¢çF†Vå÷6öÖR†—FVÒ¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢–b6öçF–æ–æuö—FV×2æ—5öV×G’‚’°¢&WGW&ã°¢Ð¢6öçF–æ–æuö—FV×2ç6÷'E÷Vç7F&ÆUö'•ö¶W’‡Æ—FV×Â—FVÒæ–B‚’çfÇVR‚’“°¢ÆWB6÷W&6UöÆ–Ö—G2ÒÖF6‚6VÆbæVF–õ÷6÷W&6UöÆ–Ö—G2€¢6öçF–æ–æuö—FV×0¢æ—FW"‚¢æÖ‡Æ—FV×Â—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’’À¢’°¢ö²‡&FW2’Óâ&FW2À¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢ÆWBG&6µö–æF–6W2Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æVçVÖW&FR‚¢æÖ‡Â†–æFW‚ÂG&6²—Â‡G&6²æ–B‚’Â–æFW‚’¢æ6öÆÆV7C££Ä†6„ÖÅòÂóãâ‚“°¢ÆWBf—'7E÷G&6µö–æFW‚Ò6öçF–æ–æuö—FV×0¢æ—FW"‚¢æf–ÇFW%öÖ‡Æ—FV×ÂG&6µö–æF–6W2ævWB‚f—FVÒçG&6µö–B‚’’æ6÷–VB‚’¢æÖ–â‚“°¢ÆWB6öÖR†f—'7E÷G&6µö–æFW‚’Òf—'7E÷G&6µö–æFW‚VÇ6R°¢&WGW&ã°¢Ó°¢ÆWB&ö¦V7E÷6×ÆU÷&FRÒ6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚“°¢ÆWB×WBVçG&–W2ÒfV3£§v—F…ö66—G’†6öçF–æ–æuö—FV×2æÆVâ‚’“°¢f÷"—FVÒ–â6öçF–æ–æuö—FV×2°¢ÆWB6öÖR‚‡6÷W&6U÷6×ÆU÷&FRÂg&ÖUö6÷VçB’’Ð¢6÷W&6UöÆ–Ö—G2ævWB†—FVÒæÖVF–÷&Vb‚’’æ6÷–VB‚¢VÇ6R°¢6VÆbç7FGW2Òf÷&ÖB€¢$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6R6×ÆR&FR—2Væf–Æ&ÆRf÷"·Ò"À¢—FVÒæÖVF–÷&Vb‚¢“°¢&WGW&ã°¢Ó°¢ÆWB6öÖR‡G&6µö–æFW‚’ÒG&6µö–æF–6W2ævWB‚f—FVÒçG&6µö–B‚’’æ6÷–VB‚’VÇ6R°¢6öçF–çVS°¢Ó°¢ÆWB6öÖR‡6÷W&6UöFVÇF’Ò&W66ÆU÷6×ÆW5öE÷F¶U÷&FR€¢6VÆV7F–öå÷7F'E÷6×ÆRÒ—FVÒç7F'E÷6×ÆR‚’À¢&ö¦V7E÷6×ÆU÷&FRÀ¢6÷W&6U÷6×ÆU÷&FRÀ¢—FVÒçF¶U÷Æ–&6µ÷&FR‚’À¢’VÇ6R°¢6VÆbç7FGW2Ð¢$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6Röfg6WB—2FöòÆ&vR"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWB6öÖR‡6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW2’Ò&W66ÆU÷6×ÆW5öE÷F¶U÷&FR€¢7å÷6×ÆW2À¢&ö¦V7E÷6×ÆU÷&FRÀ¢6÷W&6U÷6×ÆU÷&FRÀ¢—FVÒçF¶U÷Æ–&6µ÷&FR‚’À¢’VÇ6R°¢6VÆbç7FGW2Ð¢$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6R6V7F–öâ—2FöòÆ&vR"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢–b6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW2ÓÒ°¢6VÆbç7FGW2Ð¢$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6R6V7F–öâ—26†÷'FW"F†âöæRg&ÖR ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB&÷VæFVE÷6V7F–öâÒ—FVÐ¢ç6÷W&6U÷6V7F–öå÷7F'E÷6×ÆW2‚¢ç¦—†—FVÒç6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW2‚’“°¢ÆWB6÷W&6Uööfg6WBÒ—FVÒç6÷W&6Uööfg6WE÷6×ÆW2‚“°¢ÆWB6÷W&6U÷÷6—F–öåö–å÷f–WrÒ–b—FVÒæ—5öÆö÷÷6÷W&6R‚’°¢ÆWBf–WuöÆVæwF‚Ð¢&÷VæFVE÷6V7F–öâæÖö÷"†g&ÖUö6÷VçBÂÂ…òÂ6V7F–öåöÆVæwF‚—Â6V7F–öåöÆVæwF‚“°¢ÆWBö²†7W'&VçEööfg6WB’ÒScC£§G'•ög&öÒ‡6÷W&6Uööfg6WB’VÇ6R°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6R6V7F–öâ—2–çfÆ–B ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢–bf–WuöÆVæwF‚ÓÒÇÂ7W'&VçEööfg6WBãÒf–WuöÆVæwF‚°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6R6V7F–öâ—2–çfÆ–B ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢–bÆWB6öÖR‚‡6V7F–öå÷7F'BÂ6V7F–öåöÆVæwF‚’’Ò&÷VæFVE÷6V7F–öà¢bb6V7F–öå÷7F'@¢æ6†V6¶VEöFB‡6V7F–öåöÆVæwF‚¢æ—5öæöæUö÷"‡Ç6V7F–öåöVæGÂ6V7F–öåöVæBâg&ÖUö6÷VçB¢°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6R6V7F–öâ—2–çfÆ–B ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB6÷W&6U÷†6UöFVÇFÒ6÷W&6UöFVÇFRf–WuöÆVæwFƒ°¢ÆWBw&VE÷†6RÒ‡S#ƒ£¦g&öÒ†7W'&VçEööfg6WB’²S#ƒ£¦g&öÒ‡6÷W&6U÷†6UöFVÇF’¢RS#ƒ£¦g&öÒ‡f–WuöÆVæwF‚“°¢ÆWBö²‡w&VE÷†6R’ÒScC£§G'•ög&öÒ‡w&VE÷†6R’VÇ6R°¢6VÆbç7FGW2Ð¢$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6Röfg6WB—2FöòÆ&vR ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢w&VE÷†6P¢ÒVÇ6R°¢ÆWB6öÖR†öfg6WB’Ò“#ƒ£¦g&öÒ‡6÷W&6Uööfg6WB¢æ6†V6¶VEöFB†“#ƒ£¦g&öÒ‡6÷W&6UöFVÇF’¢ææE÷F†Vâ‡Æöfg6WGÂScC£§G'•ög&öÒ†öfg6WB’æö²‚’¢VÇ6R°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6R6V7F–öâ—2–çfÆ–B ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢öfg6W@¢Ó°¢ÆWB6öÖR‡6÷W&6U÷÷6—F–öå÷6×ÆW2’Ò&÷VæFVE÷6V7F–öà¢æÖö÷"…6öÖR‡6÷W&6U÷÷6—F–öåö–å÷f–Wr’ÂÂ‡6V7F–öå÷7F'BÂò—Â°¢6V7F–öå÷7F'Bæ6†V6¶VEöFB‡6÷W&6U÷÷6—F–öåö–å÷f–Wr¢Ò¢VÇ6R°¢6VÆbç7FGW2Ð¢$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6Röfg6WB—2FöòÆ&vR"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢–bÆWB6öÖR‚‡6V7F–öå÷7F'BÂ6V7F–öåöÆVæwF‚’’Ò&÷VæFVE÷6V7F–öâ°¢ÆWB6öÖR†6÷•öVæB’Ð¢6÷W&6U÷÷6—F–öå÷6×ÆW2æ6†V6¶VEöFB‡6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW2¢VÇ6R°¢6VÆbç7FGW2Ð¢$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6R6V7F–öâ—2FöòÆ&vR ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWB6öÖR†÷&–v–æÅöVæB’Ò6V7F–öå÷7F'Bæ6†V6¶VEöFB‡6V7F–öåöÆVæwF‚’VÇ6R°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B6÷’Æö÷öb6VÆV7FVB&V¢6÷W&6R6V7F–öâ—2–çfÆ–B ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢–b6÷W&6U÷÷6—F–öå÷6×ÆW2Â6V7F–öå÷7F'BÇÂ6÷•öVæBâ÷&–v–æÅöVæB°¢6VÆbç7FGW2Ð¢$6÷’Æö÷7&÷76–ær6÷W&6RÖÆö÷&÷VæF'’—2æ÷B7W÷'FVB"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢Ð¢VçG&–W2çW6‚„VF–ô—FVÔ6Æ—&ö&DVçG'’°¢ÖVF–÷&Vc¢—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’À¢æÖS¢–b—FVÒææÖR‚’æ—5öV×G’‚’ÇÂ—FVÒææÖR‚’æVæG5÷v—F‚‚"Æö÷"’°¢—FVÒææÖR‚’çFõö÷væVB‚¢ÒVÇ6R°¢f÷&ÖB‚'·ÒÆö÷"Â—FVÒææÖR‚’¢ÒÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢6÷W&6U÷6V7F–öå÷7F'E÷6×ÆW3¢6÷W&6U÷÷6—F–öå÷6×ÆW2À¢6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW2À¢ÆVæwF…÷6×ÆW3¢7å÷6×ÆW2À¢föÇVÖS¢—FVÒçföÇVÖR‚’À¢F¶U÷föÇVÖS¢—FVÒçF¶U÷föÇVÖR‚’À¢F¶U÷ã¢—FVÒçF¶U÷â‚’À¢F¶U÷Æ–&6µ÷&FS¢—FVÒçF¶U÷Æ–&6µ÷&FR‚’À¢×WFVC¢—FVÒæ—5ö×WFVB‚’À¢†6Uö–çfW'FVC¢—FVÒæ—5÷†6Uö–çfW'FVB‚’À¢G&6µööfg6WC¢G&6µö–æFW‚Òf—'7E÷G&6µö–æFW‚À¢Ò“°¢Ð¢–bVçG&–W2æ—5öV×G’‚’°¢&WGW&ã°¢Ð¢6VÆbæVF–õö—FVÕö6Æ—&ö&BÒ6öÖR„VF–ô—FVÔ6Æ—&ö&B°¢&ö¦V7EövVæW&F–öã¢6VÆbç&ö¦V7EövVæW&F–öâÀ¢&ö¦V7E÷6×ÆU÷&FS¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’À¢7å÷6×ÆW2À¢—FV×3¢VçG&–W2À¢Ò“°¢6VÆbç7FGW2Òf÷&ÖB€¢$6÷–VBÆö÷g&öÒ·ÒVF–ò—FV×·Ò"À¢6VÆbæVF–õö—FVÕö6Æ—&ö&@¢æ5÷&Vb‚¢æÖö÷"ƒÂÆ6Æ—&ö&GÂ6Æ—&ö&Bæ—FV×2æÆVâ‚’’À¢–b6VÆ`¢æVF–õö—FVÕö6Æ—&ö&@¢æ5÷&Vb‚¢æ—5÷6öÖUöæB‡Æ6Æ—&ö&GÂ6Æ—&ö&Bæ—FV×2æÆVâ‚’ÓÒ¢°¢" ¢ÒVÇ6R°¢'2 ¢Ð¢“°¢Ð ¢fâVæ6Æ÷6UöÖVF–ö—FV×5ö–å÷&¦÷%öVF—B‚f×WB6VÆb’°¢ÆWB×WB6VÆV7FVE÷G&6·2Ò†6…6WC£¦æWr‚“°¢ÆWB×WB7F'E÷6×ÆRÒæöæS££ÇScCã°¢ÆWB×WBVæE÷6×ÆRÒæöæS££ÇScCã°¢f÷"—FVÒ–â6VÆbç&ö¦V7BæVF–õö—FV×2‚’°¢–b6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚f—FVÒæ–B‚’’°¢6öçF–çVS°¢Ð¢6VÆV7FVE÷G&6·2æ–ç6W'B†—FVÒçG&6µö–B‚’“°¢7F'E÷6×ÆRÒ6öÖR€¢7F'E÷6×ÆRæÖö÷"†—FVÒç7F'E÷6×ÆR‚’ÂÇ7F'GÂ7F'BæÖ–â†—FVÒç7F'E÷6×ÆR‚’’’À¢“°¢VæE÷6×ÆRÐ¢6öÖR†VæE÷6×ÆRæÖö÷"†—FVÒæVæE÷6×ÆR‚’ÂÆVæGÂVæBæÖ‚†—FVÒæVæE÷6×ÆR‚’’’“°¢Ð¢f÷"—FVÒ–â6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’°¢–b6VÆbçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚f—FVÒæ–B‚’’°¢6öçF–çVS°¢Ð¢ÆWB6öÖR†VæE÷F–6²’Ò—FVÒç7F'E÷F–6²‚’æ6†V6¶VEöFB†—FVÒæÆVæwF…÷F–6·2‚’’VÇ6R°¢6VÆbç7FGW2Ò$6÷VÆBæ÷BVæ6Æ÷6R6VÆV7FVB—FV×2–â&¦÷"VF—B"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWB‡7F'BÂVæB’ÒÖF6‚€¢6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²†—FVÒç7F'E÷F–6²‚’’À¢6VÆbç&ö¦V7Bç6×ÆUöE÷F–6²†VæE÷F–6²’À¢’°¢„ö²‡7F'B’Âö²†VæB’’Óâ‡7F'BÂVæB’À¢„W'"†W'&÷"’Âò’Â…òÂW'"†W'&÷"’’Óâ°¢6VÆbç7FGW2Ð¢f÷&ÖB‚$6÷VÆBæ÷BVæ6Æ÷6R6VÆV7FVB—FV×2–â&¦÷"VF—C¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢6VÆV7FVE÷G&6·2æ–ç6W'B†—FVÒçG&6µö–B‚’“°¢7F'E÷6×ÆRÒ6öÖR‡7F'E÷6×ÆRæÖö÷"‡7F'BÂÆ7W'&VçGÂ7W'&VçBæÖ–â‡7F'B’’“°¢VæE÷6×ÆRÒ6öÖR†VæE÷6×ÆRæÖö÷"†VæBÂÆ7W'&VçGÂ7W'&VçBæÖ‚†VæB’’“°¢Ð ¢ÆWB6VÆV7FVEö&VÒ7F'E÷6×ÆP¢ç¦—†VæE÷6×ÆR¢ææE÷F†Vâ‡Â‡7F'BÂVæB—ÂFuö6÷&S£¥&¦÷$VF—D&V£¦æWr‡7F'BÂVæB’“°¢ÆWBG&6µö&V2Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·Â°¢ÆWB&V2Ò6VÆV7FVEö&V¢æf–ÇFW"‡Å÷Â6VÆV7FVE÷G&6·2æ6öçF–ç2‚gG&6²æ–B‚’’¢æ–çFõö—FW"‚¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢‡G&6²æ–B‚’Â&V2¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6†ævVBÒG&6µö&V2æ—FW"‚’æç’‡Â‡G&6µö–BÂ&V2—Â°¢6VÆbç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒ§G&6µö–B¢æ—5÷6öÖUöæB‡ÇG&6·ÂG&6²ç&¦÷%öVF—Eö&V2‚’Ò&V2¢Ò“°¢–b6†ævVB°¢6VÆbæÇ•ö7F–öâ€¢Ft7F–öã£¥6WEG&6µ&¦÷$VF—D&V2²G&6µö&V2ÒÀ¢–b6VÆV7FVEö&Væ—5÷6öÖR‚’°¢%&¦÷"VF—B&V6VÆV7FVB ¢ÒVÇ6R°¢%&¦÷"VF—B&V26ÆV&VB ¢ÒÀ¢“°¢ÒVÇ6R–b6VÆV7FVEö&Væ—5÷6öÖR‚’°¢6VÆbç7FGW2Ò%&¦÷"VF—B&V6VÆV7FVB"çFõö÷væVB‚“°¢Ð¢6VÆbçF–ÖVÆ–æRæ6ÆV%÷6VÆV7FVEö—FV×2‚“°¢Ð ¢V"‡7WW"’fâ6å÷7FUöVF–õö—FV×2‚g6VÆb’Óâ&ööÂ°¢ÆWB6öÖR†6Æ—&ö&B’Ò6VÆbæVF–õö—FVÕö6Æ—&ö&Bæ5÷&Vb‚’æf–ÇFW"‡Æ6Æ—&ö&GÂ°¢6Æ—&ö&Bç&ö¦V7EövVæW&F–öâÓÒ6VÆbç&ö¦V7EövVæW&F–öâbb6Æ—&ö&Bæ—FV×2æ—5öV×G’‚¢Ò’VÇ6R°¢&WGW&âfÇ6S°¢Ó°¢ÆWBæ6†÷%ö–æFW‚Ò6VÆ`¢ç6VÆV7FVE÷G&6µö–B‚¢ææE÷F†Vâ‡ÇG&6µö–GÂ°¢6VÆbç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢ç÷6—F–öâ‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢Ò¢çVçw&ö÷"ƒ“°¢6Æ—&ö&@¢æ—FV×0¢æ—FW"‚¢æÆÂ‡Æ—FV×Âæ6†÷%ö–æFW‚æ6†V6¶VEöFB†—FVÒçG&6µööfg6WB’æ—5÷6öÖR‚’¢Ð ¢fâ7FUöVF–õö—FV×2‚f×WB6VÆb’°¢–b6öÖÖæG3£§&ö¦V7EöVF—Eö'W7’‡6VÆb’°¢6VÆbç7FGW2Ð¢%7F÷Æ–&6²÷"f–æ—6‚F†R7W'&VçB&ö¦V7B÷W&F–öâ&Vf÷&R7F–ær—FV×2 ¢çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢–b6VÆbæ6å÷7FUöVF–õö—FV×2‚’°¢&WGW&ã°¢Ð¢ÆWB6öÖR†6Æ—&ö&B’Ò6VÆbæVF–õö—FVÕö6Æ—&ö&Bæ6ÆöæR‚’VÇ6R°¢&WGW&ã°¢Ó°¢ÆWBæ6†÷%ö–æFW‚Ò6VÆ`¢ç6VÆV7FVE÷G&6µö–B‚¢ææE÷F†Vâ‡ÇG&6µö–GÂ°¢6VÆbç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢ç÷6—F–öâ‡ÇG&6·ÂG&6²æ–B‚’ÓÒG&6µö–B¢Ò¢çVçw&ö÷"ƒ“°¢ÆWBö²‡7F'E÷6×ÆR’Ò6VÆbçF–ÖVÆ–æRæVF—Eö7W'6÷%÷6×ÆR‚g6VÆbç&ö¦V7B’VÇ6R°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B7FR—FV×2BF†RVF—B7W'6÷""çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWB6öÖR†ÆVæwF…÷6×ÆW2’Ò&W66ÆU÷6×ÆW2€¢6Æ—&ö&Bç7å÷6×ÆW2À¢6Æ—&ö&Bç&ö¦V7E÷6×ÆU÷&FRÀ¢6VÆbç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’À¢’VÇ6R°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B7FR—FV×3¢6÷–VBVF–ò7â—2FöòÆ&vR"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWB&WV—&VE÷G&6µö6÷VçBÒ6Æ—&ö&@¢æ—FV×0¢æ—FW"‚¢æf–ÇFW%öÖ‡Æ—FV×Âæ6†÷%ö–æFW‚æ6†V6¶VEöFB†—FVÒçG&6µööfg6WB’¢æÖ‚‚¢ææE÷F†Vâ‡ÇG&6µö–æFW‡ÂG&6µö–æFW‚æ6†V6¶VEöFBƒ’“°¢ÆWB6öÖR‡&WV—&VE÷G&6µö6÷VçB’Ò&WV—&VE÷G&6µö6÷VçBVÇ6R°¢&WGW&ã°¢Ó°¢ÆWB×WBFW7F–æF–öå÷G&6µö–G2Ò6VÆ`¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBG&6·5÷Fõö7&VFRÒ&WV—&VE÷G&6µö6÷VçBç6GW&F–æu÷7V"†FW7F–æF–öå÷G&6µö–G2æÆVâ‚’“°¢ÆWBf—'7EöæWu÷G&6µö–BÒ–bG&6·5÷Fõö7&VFRÓÒ°¢æöæP¢ÒVÇ6R°¢ÆWB6öÖR†f—'7Eö–B’Ò6VÆ`¢ç&ö¦V7@¢ææW‡E÷G&6µö–B‚¢æÖ‡ÇG&6µö–GÂG&6µö–BçfÇVR‚’¢VÇ6R°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B7FR—FV×3¢G&6²–FVçF–f–W'2&RW††W7FVB"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWBö²†7&VFUö6÷VçB’ÒScC£§G'•ög&öÒ‡G&6·5÷Fõö7&VFR’VÇ6R°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B7FR—FV×3¢FöòÖç’FW7F–æF–öâG&6·2"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢–bf—'7Eö–Bæ6†V6¶VEöFB†7&VFUö6÷VçB’æ—5öæöæR‚’°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B7FR—FV×3¢G&6²–FVçF–f–W'2&RW††W7FVB"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢6öÖR†f—'7Eö–B¢Ó°¢ÆWB×WB7F–öç2ÒfV3£§v—F…ö66—G’‡G&6·5÷Fõö7&VFR²6Æ—&ö&Bæ—FV×2æÆVâ‚’“°¢f÷"öfg6WB–ââçG&6·5÷Fõö7&VFR°¢ÆWB6öÖR‡&u÷G&6µö–B’Ð¢f—'7EöæWu÷G&6µö–BææE÷F†Vâ‡Æf—'7GÂf—'7Bæ6†V6¶VEöFB‡ScC£§G'•ög&öÒ†öfg6WB’æö²‚“ò’¢VÇ6R°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B7FR—FV×3¢G&6²–FVçF–f–W'2&RW††W7FVB"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢ÆWB6öÖR‡G&6µö–B’ÒG&6´–C£¦g&öÕ÷fÇVR‡&u÷G&6µö–B’VÇ6R°¢6VÆbç7FGW2Ò$6÷VÆBæ÷B7FR—FV×3¢G&6²–FVçF–f–W'2&RW††W7FVB"çFõö÷væVB‚“°¢&WGW&ã°¢Ó°¢7F–öç2çW6‚„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢FW7F–æF–öå÷G&6µö–G2æÆVâ‚’À¢æÖS¢7G&–æs£¦æWr‚’À¢Ò“°¢FW7F–æF–öå÷G&6µö–G2çW6‚‡G&6µö–B“°¢Ð¢7F–öç2æW‡FVæB†6Æ—&ö&Bæ—FV×2æ—FW"‚’æf–ÇFW%öÖ‡Æ—FV×Â°¢ÆWBG&6µö–æFW‚Òæ6†÷%ö–æFW‚æ6†V6¶VEöFB†—FVÒçG&6µööfg6WB“ó°¢ÆWBG&6µö–BÒ¦FW7F–æF–öå÷G&6µö–G2ævWB‡G&6µö–æFW‚“ó°¢6öÖR„Ft7F–öã£¤–ç6W'DÆö÷VDVF–ô—FVÕv—F„Ö—‚°¢G&6µö–BÀ¢ÖVF–÷&Vc¢—FVÒæÖVF–÷&Vbæ6ÆöæR‚’À¢æÖS¢—FVÒææÖRæ6ÆöæR‚’À¢7F'E÷6×ÆRÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢—FVÒç6÷W&6Uööfg6WE÷6×ÆW2À¢6÷W&6U÷6V7F–öå÷7F'E÷6×ÆW3¢—FVÒç6÷W&6U÷6V7F–öå÷7F'E÷6×ÆW2À¢ÆVæwF…÷6×ÆW2À¢×WFVC¢—FVÒæ×WFVBÀ¢†6Uö–çfW'FVC¢—FVÒç†6Uö–çfW'FVBÀ¢6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW3¢—FVÒç6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW2À¢föÇVÖS¢—FVÒçföÇVÖRÀ¢F¶U÷föÇVÖS¢—FVÒçF¶U÷föÇVÖRÀ¢F¶U÷ã¢—FVÒçF¶U÷âÀ¢F¶U÷Æ–&6µ÷&FS¢—FVÒçF¶U÷Æ–&6µ÷&FRÀ¢Ò¢Ò’“°¢–b7F–öç2æ—5öV×G’‚’°¢&WGW&ã°¢Ð¢ÆWB&Wf–÷W5ö–G2Ò6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â—FVÒæ–B‚’¢æ6†–â‡6VÆbç&ö¦V7BæÖ–F•ö—FV×2‚’æ—FW"‚’æÖ‡Æ—FV×Â—FVÒæ–B‚’’¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢ÆWB&Wf–÷W5÷&Wf—6–öâÒ6VÆbç&Wf—6–öã°¢ÆWB7F–öâÒ–b7F–öç2æÆVâ‚’ÓÒ°¢7F–öç0¢æ–çFõö—FW"‚¢ææW‡B‚¢æW‡V7B‚&öæR7FR7F–öâ—2&W6VçB"¢ÒVÇ6R°¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ð¢Ó°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ$VF–ò—FV×27FVB"“°¢–b6VÆbç&Wf—6–öâÒ&Wf–÷W5÷&Wf—6–öâ°¢ÆWB6VÆV7FVBÒ6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â&Wf–÷W5ö–G2æ6öçF–ç2‚f—FVÒæ–B‚’’¢æÖ‡Æ—FV×Â†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢6VÆbçF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6VÆV7FVB“°¢Ð¢Ð ¢fâFVÆWFU÷6VÆV7FVEö—FV×2‚f×WB6VÆb’°¢ÆWB×WB—FVÕö–G2Ò6VÆ`¢çF–ÖVÆ–æP¢ç6VÆV7FVEö—FV×0¢æ—FW"‚¢æ6÷–VB‚¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢—FVÕö–G2ç6÷'E÷Vç7F&ÆUö'•ö¶W’‡Æ—FVÕö–GÂ—FVÕö–BçfÇVR‚’“°¢–b—FVÕö–G2æ—5öV×G’‚’°¢6VÆbç7FGW2Ò%6VÆV7BöæR÷"Ö÷&R—FV×2FòFVÆWFR"çFõö÷væVB‚“°¢&WGW&ã°¢Ð ¢ÆWB×WB7F–öç2ÒfV3£§v—F…ö66—G’†—FVÕö–G2æÆVâ‚’“°¢f÷"—FVÕö–B–â—FVÕö–G2°¢–b6VÆ`¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æç’‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢6VÆbæVF–õö—FVÕ÷7F'EöVF—G2ç&VÖ÷fR‚f—FVÕö–B“°¢7F–öç2çW6‚„Ft7F–öã£¤FVÆWFTVF–ô—FVÒ²—FVÕö–BÒ“°¢ÒVÇ6R–b6VÆ`¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æç’‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢°¢7F–öç2çW6‚„Ft7F–öã£¤FVÆWFTÖ–F”—FVÒ²—FVÕö–BÒ“°¢Ð¢Ð¢–b7F–öç2æ—5öV×G’‚’°¢6VÆbç7FGW2Ò%6VÆV7FVB—FV×2æòÆöævW"W†—7B"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB7F–öâÒ–b7F–öç2æÆVâ‚’ÓÒ°¢7F–öç2ç÷‚’æW‡V7B‚'6–ævÆRFVÆWFR7F–öâ—2&W6VçB"¢ÒVÇ6R°¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ð¢Ó°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ%6VÆV7FVB—FV×2FVÆWFVB"“°¢Ð ¢fâGWÆ–6FU÷6VÆV7FVEö—FV×2‚f×WB6VÆb’°¢ÆWB×WB—FVÕö–G2Ò6VÆ`¢çF–ÖVÆ–æP¢ç6VÆV7FVEö—FV×0¢æ—FW"‚¢æ6÷–VB‚¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢—FVÕö–G2ç6÷'E÷Vç7F&ÆUö'•ö¶W’‡Æ—FVÕö–GÂ—FVÕö–BçfÇVR‚’“°¢–b—FVÕö–G2æ—5öV×G’‚’°¢6VÆbç7FGW2Ò%6VÆV7BöæR÷"Ö÷&R—FV×2FòGWÆ–6FR"çFõö÷væVB‚“°¢&WGW&ã°¢Ð ¢ÆWB×WB7F–öç2ÒÖF6‚GWÆ–6FUö—FVÕö7F–öç2‚g6VÆbç&ö¦V7BÂf—FVÕö–G2’°¢ö²†7F–öç2’Óâ7F–öç2À¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚$6÷VÆBæ÷BGWÆ–6FR6VÆV7FVB—FV×3¢¶W'&÷'Ò"“°¢&WGW&ã°¢Ð¢Ó°¢–b7F–öç2æ—5öV×G’‚’°¢6VÆbç7FGW2Ò%6VÆV7FVB—FV×2æòÆöævW"W†—7B"çFõö÷væVB‚“°¢&WGW&ã°¢Ð¢ÆWB7F–öâÒ–b7F–öç2æÆVâ‚’ÓÒ°¢7F–öç2ç÷‚’æW‡V7B‚'6–ævÆRGWÆ–6FR7F–öâ—2&W6VçB"¢ÒVÇ6R°¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–C¢6VÆbç&Wf—6–öâÀ¢7F–öç2À¢Ð¢Ó°¢6VÆbæÇ•ö7F–öâ†7F–öâÂ%6VÆV7FVB—FV×2GWÆ–6FVB"“°¢Ð ¢fâ6fU÷&ö¦V7Eö6öÖÖæB‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢–b6VÆbç&ö¦V7E÷F‚æ—5öæöæR‚’ÇÂ6VÆbæ7W'&VçE÷&ö¦V7Eö—5÷FV×ÆFR‚’°¢6VÆbç–6µ÷F‚…F…–6¶W%F&vWC£¥6fU&ö¦V7B¢ÒVÇ6R°¢&ö¦V7Eö–ó£§6fU÷&ö¦V7B‡6VÆbÂæöæR¢Ð¢Ð ¢fâ6fUöæWu÷&ö¦V7E÷fW'6–öåö6öÖÖæB‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢&ö¦V7Eö–ó£§6fUöæWu÷&ö¦V7E÷fW'6–öâ‡6VÆb¢Ð ¢fâ÷Vå÷&ö¦V7Eö6öÖÖæB‚f×WB6VÆb’ÓâF6³ÄÖW76vSâ°¢6VÆbæ&Vv–å÷&ö¦V7E÷G&ç6—F–öâ…VæF–æu&ö¦V7EG&ç6—F–öã£¤÷Vå&ö¦V7B¢Ð ¢fâ÷Vå÷&V6VçE÷&ö¦V7B‚f×WB6VÆbÂFƒ¢F„'Vb’ÓâF6³ÄÖW76vSâ°¢–b6VÆbç&ö¦V7E÷F‚æ5öFW&Vb‚’ÓÒ6öÖR‡F‚æ5÷F‚‚’’bb6VÆbç&ö¦V7EöÆö6²æ—5÷6öÖR‚’°¢6VÆbç7FGW2Òf÷&ÖB‚'·Ò—2Ç&VG’÷Vâ"ÂF‚æF—7Æ’‚’“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢6VÆbçVæF–æu÷&ö¦V7E÷F…÷Fõö÷VâÒ6öÖR‡F‚“°¢ÆWBF6²Ò6VÆbæ&Vv–å÷&ö¦V7E÷G&ç6—F–öâ…VæF–æu&ö¦V7EG&ç6—F–öã£¤÷Vå&ö¦V7B“°¢–b6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâæ—5öæöæR‚’°¢6VÆbçVæF–æu÷&ö¦V7E÷F…÷Fõö÷VâÒæöæS°¢Ð¢F6°¢Ð ¢fâ÷Vå÷&ö¦V7E÷FV×ÆFR‚f×WB6VÆbÂFƒ¢F„'Vb’ÓâF6³ÄÖW76vSâ°¢6VÆbçVæF–æu÷&ö¦V7E÷FV×ÆFU÷6÷W&6RÒ6öÖR‡F‚“°¢6VÆbçVæF–æu÷&ö¦V7E÷F…÷Fõö÷VâÒæöæS°¢6VÆbçVæF–æu÷&ö¦V7E÷FV×ÆFU÷v÷&¶–æuö6÷’ÒæöæS°¢ÆWBF6²Ò6VÆbæ&Vv–å÷&ö¦V7E÷G&ç6—F–öâ…VæF–æu&ö¦V7EG&ç6—F–öã£¤÷Vå&ö¦V7B“°¢–b6VÆbçVæF–æu÷&ö¦V7E÷G&ç6—F–öâæ—5öæöæR‚’°¢6VÆbçVæF–æu÷&ö¦V7E÷FV×ÆFU÷6÷W&6RÒæöæS°¢6VÆbçVæF–æu÷&ö¦V7E÷F…÷Fõö÷VâÒæöæS°¢6VÆbçVæF–æu÷&ö¦V7E÷FV×ÆFU÷v÷&¶–æuö6÷’ÒæöæS°¢Ð¢F6°¢Ð ¢fâ6fU÷&ö¦V7E÷FV×ÆFR‚f×WB6VÆbÂ6VÆV7FVE÷Fƒ¢F„'Vb’ÓâF6³ÄÖW76vSâ°¢ÆWB6öÖR‡FV×ÆFUöF—&V7F÷'’’Ò&ö¦V7E÷FV×ÆFW3£¦F—&V7F÷'’‚’VÇ6R°¢6VÆbç7FGW2Ò%&ö¦V7BFV×ÆFRföÆFW"—2Væf–Æ&ÆR"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ó°¢6VÆbç&WVW7E÷&ö¦V7E÷FV×ÆFU÷6fR‡6VÆV7FVE÷F‚ÂgFV×ÆFUöF—&V7F÷'’¢Ð ¢fâ&WVW7E÷&ö¦V7E÷FV×ÆFU÷6fR€¢f×WB6VÆbÀ¢6VÆV7FVE÷Fƒ¢F„'VbÀ¢FV×ÆFUöF—&V7F÷'“¢g7FC£§Fƒ£¥F‚À¢’ÓâF6³ÄÖW76vSâ°¢–b6VÆbæ–õö'W7’ÇÂ6VÆbçVç6fVE÷6W76–öå÷6æ6†÷Eö'W7’ÇÂ6VÆbçF…÷–6¶W%ö'W7’°¢6VÆbç7FGW2Ò%v—Bf÷"F†R7W'&VçB&ö¦V7B÷W&F–öâFòf–æ—6‚"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ÆWBF‚ÒÖF6‚&ö¦V7E÷FV×ÆFW3£§&W6öÇfU÷6fU÷F‚‚g6VÆV7FVE÷F‚ÂFV×ÆFUöF—&V7F÷'’’°¢ö²‡F‚’ÓâF‚À¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2ÒW'&÷#°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢Ó°¢ÖF6‚7FC£¦g3£§7–ÖÆ–æµöÖWFFF‚gF‚’°¢ö²…ò’–bF‚æ—5öf–ÆR‚’Óâ°¢6VÆbçVæF–æu÷&ö¦V7E÷FV×ÆFUö÷fW'w&—FRÒ6öÖR‡F‚æ6ÆöæR‚’“°¢6VÆbæ7F—fUöÖVçRÒæöæS°¢6VÆbç&ö¦V7E÷FV×ÆFW5öÖVçUö÷VâÒfÇ6S°¢6VÆbç7FGW2Òf÷&ÖB‚$f–ÆRW†—7G3¢·Òâ÷fW'w&—FSò"ÂF‚æF—7Æ’‚’“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ö²…ò’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%&ö¦V7BFV×ÆFRF‚—2æ÷Bf–ÆS¢·Ò"ÂF‚æF—7Æ’‚’“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢W'"†W'&÷"’–bW'&÷"æ¶–æB‚’Ò7FC£¦–ó£¤W'&÷$¶–æC£¤æ÷Df÷VæBÓâ°¢6VÆbç7FGW2Òf÷&ÖB‚$6÷VÆBæ÷B–ç7V7B&ö¦V7BFV×ÆFRFƒ¢¶W'&÷'Ò"“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢W'"…ò’Óâ·Ð¢Ð¢6VÆbçw&—FU÷&ö¦V7E÷FV×ÆFR‡F‚ÂfÇ6R¢Ð ¢fâ÷fW'w&—FU÷&ö¦V7E÷FV×ÆFR‚f×WB6VÆbÂFƒ¢F„'Vb’ÓâF6³ÄÖW76vSâ°¢–b6VÆbæ–õö'W7’ÇÂ6VÆbçVç6fVE÷6W76–öå÷6æ6†÷Eö'W7’ÇÂ6VÆbçF…÷–6¶W%ö'W7’°¢6VÆbç7FGW2Ò%v—Bf÷"F†R7W'&VçB&ö¦V7B÷W&F–öâFòf–æ—6‚"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢–b6VÆ`¢ç&ö¦V7E÷F€¢æ5öFW&Vb‚¢æ—5÷6öÖUöæB‡Ç&ö¦V7E÷F‡Â&ö¦V7E÷FV×ÆFW3£§6ÖU÷F‚‡&ö¦V7E÷F‚ÂgF‚’¢°¢6VÆbç7FGW2Ò$6ææ÷B÷fW'w&—FR&ö¦V7BFV×ÆFRv†–ÆR—B—2÷Vâ"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ÖF6‚7FC£¦g3£§7–ÖÆ–æµöÖWFFF‚gF‚’°¢ö²…ò’–bF‚æ—5öf–ÆR‚’Óâ6VÆbçw&—FU÷&ö¦V7E÷FV×ÆFR‡F‚ÂG'VR’À¢ö²…ò’Óâ°¢6VÆbç7FGW2Òf÷&ÖB‚%&ö¦V7BFV×ÆFRF‚—2æ÷Bf–ÆS¢·Ò"ÂF‚æF—7Æ’‚’“°¢F6³£¦æöæR‚¢Ð¢W'"†W'&÷"’Óâ°¢6VÆbç7FGW2Ò–bW'&÷"æ¶–æB‚’ÓÒ7FC£¦–ó£¤W'&÷$¶–æC£¤æ÷Df÷VæB°¢%&ö¦V7BFV×ÆFRæòÆöævW"W†—7G3²6†ö÷6RæWrFV×ÆFRæÖR"çFõö÷væVB‚¢ÒVÇ6R°¢f÷&ÖB‚$6÷VÆBæ÷B–ç7V7B&ö¦V7BFV×ÆFRFƒ¢¶W'&÷'Ò"¢Ó°¢F6³£¦æöæR‚¢Ð¢Ð¢Ð ¢fâw&—FU÷&ö¦V7E÷FV×ÆFR‚f×WB6VÆbÂFƒ¢F„'VbÂ÷fW'w&—FS¢&ööÂ’ÓâF6³ÄÖW76vSâ°¢–b6VÆbæ–õö'W7’ÇÂ6VÆbçVç6fVE÷6W76–öå÷6æ6†÷Eö'W7’ÇÂ6VÆbçF…÷–6¶W%ö'W7’°¢6VÆbç7FGW2Ò%v—Bf÷"F†R7W'&VçB&ö¦V7B÷W&F–öâFòf–æ—6‚"çFõö÷væVB‚“°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢ÆWB6æ6†÷BÒ6VÆbç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBf–Wu÷7FFRÒ6VÆbç&ö¦V7Eö'&ævVÖVçE÷f–Wu÷7FFR‚“°¢ÆWB6÷W&6UöÖVF–÷F‚Ò6VÆbæÖVF–÷7F÷&U÷F‚‚“°¢6VÆbæ–õö'W7’ÒG'VS°¢6VÆbç7FGW2Ò–b÷fW'w&—FR°¢f÷&ÖB‚$÷fW'w&—F–ær&ö¦V7BFV×ÆFR·Þ(
b"ÂF‚æF—7Æ’‚’¢ÒVÇ6R°¢f÷&ÖB‚%6f–ær&ö¦V7BFV×ÆFR·Þ(
b"ÂF‚æF—7Æ’‚’¢Ó°¢F6³£§W&f÷&Ò€¢'Våö&Æö6¶–ær‚&Fr×&ö¦V7B×FV×ÆFR×6fR"ÂÖ÷fRÇÂ°¢ÆWB6fRÒ–b÷fW'w&—FR°¢&ö¦V7Eö–ó£¦÷fW'w&—FU÷&ö¦V7E÷FV×ÆFR€¢F‚æ6ÆöæR‚’À¢6æ6†÷BÀ¢f–Wu÷7FFRÀ¢6÷W&6UöÖVF–÷F‚À¢¢ÒVÇ6R°¢&ö¦V7Eö–ó£§6fU÷&ö¦V7E÷FV×ÆFR€¢F‚æ6ÆöæR‚’À¢6æ6†÷BÀ¢f–Wu÷7FFRÀ¢6÷W&6UöÖVF–÷F‚À¢¢Ó°¢6fRæÖ‡Â‚—ÂF‚¢Ò’À¢Ç&W7VÇGÂÖF6‚&W7VÇB°¢ö²‡F‚’ÓâÖW76vS£¥&ö¦V7EFV×ÆFU6fVB‡F‚Âö²‚‚’’’À¢W'"†W'&÷"’ÓâÖW76vS£¥&ö¦V7EFV×ÆFU6fVB…F„'Vc£¦æWr‚’ÂW'"†W'&÷"’’À¢ÒÀ¢¢Ð§Ð ¦fâ'6U÷G&6µöæÖUöG&gB‡FW‡C¢g7G"’Óâ&W7VÇCÅ7G&–ærÂbw7FF–27G#â°¢ÆWBæÖRÒFW‡BçG&–Ò‚“°¢–bæÖRæ—5öV×G’‚’°¢W'"‚%G&6²æÖR×W7Bæ÷B&RV×G’"¢ÒVÇ6R°¢ö²†æÖRçFõö÷væVB‚’¢Ð§Ð ¦fâ'6UöÖ–F•ö—FVÕöæÖUöG&gB‡FW‡C¢g7G"’Óâ&W7VÇCÅ7G&–ærÂbw7FF–27G#â°¢ÆWBæÖRÒFW‡BçG&–Ò‚“°¢–bæÖRæ—5öV×G’‚’°¢W'"‚$Ô”D’—FVÒæÖR×W7Bæ÷B&RV×G’"¢ÒVÇ6R–bæÖRæ6†'2‚’æ6÷VçB‚’â#‚°¢W'"‚$Ô”D’—FVÒæÖR×W7B&R#‚6†&7FW'2÷"fWvW""¢ÒVÇ6R°¢ö²†æÖRçFõö÷væVB‚’¢Ð§Ð ¦fâ'6U÷G&6µ÷föÇVÖUöG&gB‡FW‡C¢g7G"’Óâ&W7VÇCÆc3"Âbw7FF–27G#â°¢–bÖF6†W2€¢FW‡BçG&–Ò‚’çFõöÆ÷vW&66R‚’æ5÷7G"‚’À¢"Ö–æb"Â.(‰&–æb"Â"Þ(‰â"Â.(‰.(‰â ¢’°¢&WGW&âö²†fFW#£¥4”ÄTä4UôD"“°¢Ð¢ÆWBfÇVRÒFW‡@¢çG&–Ò‚¢ç'6S££Æc3#â‚¢æÖöW'"‡Å÷Â$VçFW"fÆ–BföÇVÖR–âD""“ó°¢–bfÇVRæ—5öf–æ—FR‚’ÇÂãöc3"ç÷vb‡fÇVRò#ã’æ—5öf–æ—FR‚’°¢&WGW&âW'"‚$VçFW"f–æ—FRföÇVÖR–âD""“°¢Ð¢òò$TU"w2&V6—6RVçG'’6âW†6VVBF†RF—7Æ’fFW"&ævR†Rærâ³#D"’à¢ö²‡fÇVR§Ð ¦fâ'6U÷G&6µ÷åöG&gB‡FW‡C¢g7G"’Óâ&W7VÇCÆc3"Âbw7FF–27G#â°¢ÆWBfÇVRÒFW‡@¢çG&–Ò‚¢ç'6S££Æc3#â‚¢æÖöW'"‡Å÷Â$VçFW"âfÇVRg&öÒÓãFòã"“ó°¢–bfÇVRæ—5öf–æ—FR‚’°¢&WGW&âW'"‚$VçFW"f–æ—FRâfÇVR"“°¢Ð¢ö²‡fÇVRæ6Æ×‚ÓãÂã’§Ð ¦fâ6æ÷F–6µ÷W‡F–6³¢ScBÂw&–E÷F–6·3¢ScB’ÓâScB°¢ÆWBw&–E÷F–6·2Òw&–E÷F–6·2æÖ‚ƒ“°¢ÆWBV÷F–VçBÒF–6²òw&–E÷F–6·2²ScC£¦g&öÒ‚F–6²æ—5ö×VÇF—ÆUööb†w&–E÷F–6·2’“°¢V÷F–VçBç6GW&F–æuö×VÂ†w&–E÷F–6·2§Ð ¦fâ&W66ÆU÷6×ÆW2‡6×ÆW3¢ScBÂg&öÕ÷6×ÆU÷&FS¢S3"ÂFõ÷6×ÆU÷&FS¢S3"’Óâ÷F–öãÇScCâ°¢–bg&öÕ÷6×ÆU÷&FRÓÒÇÂFõ÷6×ÆU÷&FRÓÒ°¢&WGW&âæöæS°¢Ð¢ScC£§G'•ög&öÒ€¢‡S#ƒ£¦g&öÒ‡6×ÆW2’¢S#ƒ£¦g&öÒ‡Fõ÷6×ÆU÷&FR’²S#ƒ£¦g&öÒ†g&öÕ÷6×ÆU÷&FR’ò"¢òS#ƒ£¦g&öÒ†g&öÕ÷6×ÆU÷&FR’À¢¢æö²‚§Ð ¦fâ&W66ÆU÷6×ÆW5öE÷F¶U÷&FR€¢6×ÆW3¢ScBÀ¢&ö¦V7E÷6×ÆU÷&FS¢S3"À¢6÷W&6U÷6×ÆU÷&FS¢S3"À¢F¶U÷&FS¢ccBÀ¢’Óâ÷F–öãÇScCâ°¢–b&ö¦V7E÷6×ÆU÷&FRÓÒ ¢ÇÂ6÷W&6U÷6×ÆU÷&FRÓÒ ¢ÇÂF¶U÷&FRæ—5öf–æ—FR‚¢ÇÂF¶U÷&FRÃÒã ¢ÇÂ6×ÆW2âƒ÷ScBÃÂS2’Ò¢°¢&WGW&âæöæS°¢Ð¢ÆWB6÷W&6Uög&ÖW2Ò‡6×ÆW22ccB’¢ccC£¦g&öÒ‡6÷W&6U÷6×ÆU÷&FR’¢F¶U÷&FP¢òccC£¦g&öÒ‡&ö¦V7E÷6×ÆU÷&FR“°¢–b6÷W&6Uög&ÖW2æ—5öf–æ—FR‚’ÇÂ6÷W&6Uög&ÖW2ÂãÇÂ6÷W&6Uög&ÖW2ãÒScC£¤Ô‚2ccB°¢&WGW&âæöæS°¢Ð¢6öÖR‡6÷W&6Uög&ÖW2ç&÷VæB‚’2ScB§Ð ¦fâ7Æ—EöVF–õö—FVÕö7F–öç2€¢&ö¦V7C¢e&ö¦V7BÀ¢—FVÓ¢dVF–ô—FVÒÀ¢&÷VæF&–W3¢e·ScEÒÀ¢6÷W&6U÷6×ÆU÷&FS¢S3"À¢7F–öç3¢f×WBfV3ÄFt7F–öãâÀ¢’Óâ&W7VÇCÂ‚’Â7G&–æsâ°¢ÆWB7WE÷6×ÆW2Ò&÷VæF&–W0¢æ—FW"‚¢æf–ÇFW%öÖ‡ÇF–6·Â&ö¦V7Bç6×ÆUöE÷F–6²‚§F–6²’æö²‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢7Æ—EöVF–õö—FVÕö7F–öç5öE÷6×ÆW2‡&ö¦V7BÂ—FVÒÂf7WE÷6×ÆW2Â6÷W&6U÷6×ÆU÷&FRÂ7F–öç2§Ð ¦fâ7Æ—EöVF–õö—FVÕö7F–öç5öE÷6×ÆW2€¢&ö¦V7C¢e&ö¦V7BÀ¢—FVÓ¢dVF–ô—FVÒÀ¢&÷VæF&–W3¢e·ScEÒÀ¢6÷W&6U÷6×ÆU÷&FS¢S3"À¢7F–öç3¢f×WBfV3ÄFt7F–öãâÀ¢’Óâ&W7VÇCÂ‚’Â7G&–æsâ°¢ÆWB×WB7WE÷6×ÆW2Ò&÷VæF&–W0¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡Ç6×ÆWÂ—FVÒç7F'E÷6×ÆR‚’Â§6×ÆRbb§6×ÆRÂ—FVÒæVæE÷6×ÆR‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢7WE÷6×ÆW2ç6÷'E÷Vç7F&ÆR‚“°¢7WE÷6×ÆW2æFVGW‚“°¢–b7WE÷6×ÆW2æ—5öV×G’‚’°¢&WGW&âö²‚‚’“°¢Ð ¢ÆWB×WBö–çG2ÒfV3£§v—F…ö66—G’†7WE÷6×ÆW2æÆVâ‚’²"“°¢ö–çG2çW6‚†—FVÒç7F'E÷6×ÆR‚’“°¢ö–çG2æW‡FVæB†7WE÷6×ÆW2“°¢ö–çG2çW6‚†—FVÒæVæE÷6×ÆR‚’“°¢f÷"†–æFW‚Â6VvÖVçB’–âö–çG2çv–æF÷w2ƒ"’æVçVÖW&FR‚’°¢ÆWB7F'E÷6×ÆRÒ6VvÖVçE³Ó°¢ÆWBÆVæwF…÷6×ÆW2Ò6VvÖVçE³ÒÒ6VvÖVçE³Ó°¢ÆWB&ö¦V7E÷6×ÆU÷&FRÒ&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚“°¢ÆWB6÷W&6UöFVÇFÒ“cC£§G'•ög&öÒ€¢‡S#ƒ£¦g&öÒ‡7F'E÷6×ÆRÒ—FVÒç7F'E÷6×ÆR‚’’¢S#ƒ£¦g&öÒ‡6÷W&6U÷6×ÆU÷&FR¢²S#ƒ£¦g&öÒ‡&ö¦V7E÷6×ÆU÷&FR’ò"¢òS#ƒ£¦g&öÒ‡&ö¦V7E÷6×ÆU÷&FR’À¢¢æÖöW'"‡Å÷Â&VF–ò6÷W&6Röfg6WBW†6VVG2F†R7W÷'FVB&ævR"çFõö÷væVB‚’“ó°¢ÆWB6÷W&6Uööfg6WE÷6×ÆW2Ò—FVÐ¢ç6÷W&6Uööfg6WE÷6×ÆW2‚¢æ6†V6¶VEöFB‡6÷W&6UöFVÇF¢æöµö÷%öVÇ6R‡ÇÂ&VF–ò6÷W&6Röfg6WBW†6VVG2F†R7W÷'FVB&ævR"çFõö÷væVB‚’“ó°¢–b–æFW‚ÓÒ°¢7F–öç2çW6‚„Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–C¢—FVÒæ–B‚’À¢ÖVF–÷&Vc¢—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’À¢7F'E÷6×ÆS¢—FVÒç7F'E÷6×ÆR‚’À¢6÷W&6Uööfg6WE÷6×ÆW2À¢ÆVæwF…÷6×ÆW2À¢Ò“°¢ÒVÇ6R°¢7F–öç2çW6‚„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢—FVÒçG&6µö–B‚’À¢ÖVF–÷&Vc¢—FVÒæÖVF–÷&Vb‚’çFõö÷væVB‚’À¢7F'E÷6×ÆRÀ¢6÷W&6Uööfg6WE÷6×ÆW2À¢ÆVæwF…÷6×ÆW2À¢Ò“°¢Ð¢Ð¢ö²‚‚’§Ð ¦fâ7Æ—EöÖ–F•ö—FVÕö7F–öç2€¢—FVÓ¢dÖ–F”—FVÒÀ¢&÷VæF&–W3¢e·ScEÒÀ¢7F–öç3¢f×WBfV3ÄFt7F–öãâÀ¢’Óâ&W7VÇCÂ‚’Â7G&–æsâ°¢ÆWB—FVÕöVæBÒ—FVÐ¢ç7F'E÷F–6²‚¢æ6†V6¶VEöFB†—FVÒæÆVæwF…÷F–6·2‚’¢æöµö÷%öVÇ6R‡ÇÂ$Ô”D’—FVÒVæBW†6VVG2F†R7W÷'FVB&ævR"çFõö÷væVB‚’“ó°¢ÆWB×WB7Æ—E÷F–6·2Ò&÷VæF&–W0¢æ—FW"‚¢æ6÷–VB‚¢æf–ÇFW"‡ÇF–6·Â—FVÒç7F'E÷F–6²‚’Â§F–6²bb§F–6²Â—FVÕöVæB¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢7Æ—E÷F–6·2ç6÷'E÷Vç7F&ÆR‚“°¢7Æ—E÷F–6·2æFVGW‚“°¢–b7Æ—E÷F–6·2æ—5öV×G’‚’°¢7F–öç2çW6‚„Ft7F–öã£¥7Æ—DÖ–F”—FVÒ°¢—FVÕö–C¢—FVÒæ–B‚’À¢7Æ—E÷F–6·2À¢Ò“°¢Ð¢ö²‚‚’§Ð ¦fâ—FVÕöG&uöVF—EöwV&E÷7FGW2€¢F…÷–6¶W%ö'W7“¢&ööÂÀ¢–×÷'Eö'W7“¢&ööÂÀ¢76WEöÖævVÖVçEö'W7“¢&ööÂÀ¢Æ–&6µö'W7“¢&ööÂÀ¢Æ–&6µö7F—fS¢&ööÂÀ¢–õö'W7“¢&ööÂÀ¢’Óâ÷F–öãÂbw7FF–27G#â°¢–bF…÷–6¶W%ö'W7’°¢6öÖR‚%v—Bf÷"F†Rf–ÆRF–ÆörFòf–æ—6‚"¢ÒVÇ6R–b–×÷'Eö'W7’°¢6öÖR‚%v—Bf÷"VF–ò–×÷'BFòf–æ—6‚÷"6æ6VÂ—B"¢ÒVÇ6R–b76WEöÖævVÖVçEö'W7’°¢6öÖR‚%v—Bf÷"VF–ò76WBÖ–çFVææ6RFòf–æ—6‚÷"6æ6VÂ—B"¢ÒVÇ6R–bÆ–&6µö'W7’°¢6öÖR‚%v—Bf÷"Æ–&6²&W&F–öâFòf–æ—6‚"¢ÒVÇ6R–bÆ–&6µö7F—fR°¢6öÖR‚$6Æ÷6RVF–ò÷WGWB&Vf÷&RVF—F–ærF†R&ö¦V7B"¢ÒVÇ6R–b–õö'W7’°¢6öÖR‚%v—Bf÷"7W'&VçB&ö¦V7B÷W&F–öâFòf–æ—6‚"¢ÒVÇ6R°¢æöæP¢Ð§Ð ¢5¶6fr†ç’†fVGW&RÒ&VF–òÖFWf–6R"ÂFW7B’•Ð¦fâÆ–&6µ÷&WfVçG5÷&ö¦V7EöVF—G2‡Æ–&6µö÷Vã¢&ööÂÂG&ç7÷'E÷Æ––æs¢&ööÂ’Óâ&ööÂ°¢Æ–&6µö÷VâbbG&ç7÷'E÷Æ––æp§Ð ¦fâ&ö¦V7E÷F…ög&öÕ÷VW'’‡VW'“¢g7G"’Óâ÷F–öãÅF„'Vcâ°¢ÆWBVW'’ÒVW'’çG&–Ò‚“°¢‚VW'’æ—5öV×G’‚’’çF†Vâ‡ÇÂF„'Vc£¦g&öÒ‡VW'’’§Ð ¦fâ7&VFU÷Vç6fVE÷&ö¦V7E÷6W76–öåöF—"‚’Óâ&W7VÇCÅVç6fVE6W76–öäÖVF–Â7G&–æsâ°¢7&VFU÷Vç6fVE÷&ö¦V7E÷6W76–öåöF—%ö–â‚gVç6fVE÷6W76–öç5÷&ö÷B‚“ò§Ð ¦fâVç6fVE÷6W76–öç5÷&ö÷B‚’Óâ&W7VÇCÅF„'VbÂ7G&–æsâ°¢5¶6fr‡F&vWEö÷2Ò&æG&ö–B"•Ð¢ÆWBFFöF—&V7F÷'’Ò7&FS£¦æG&ö–E÷ÆFf÷&Ó£¦öFFöF—&V7F÷'’‚“°¢5¶6fr†æ÷B‡F&vWEö÷2Ò&æG&ö–B"’•Ð¢ÆWBFFöF—&V7F÷'’ÒF—&V7F÷&–W3£¥&ö¦V7DF—'3£¦g&öÒ‚&÷&r"Â$Dr"Â$Dr"¢æÖ‡ÆF—&V7F÷&–W7ÂF—&V7F÷&–W2æFFöÆö6ÅöF—"‚’çFõ÷F…ö'Vb‚’“° ¢FFöF—&V7F÷'¢æÖ‡ÆF—&V7F÷'—ÂF—&V7F÷'’æ¦ö–â‚'Vç6fVB×6W76–öç2"’¢æöµö÷%öVÇ6R‡ÇÂ&Æ–6F–öâFFF—&V7F÷'’—2Væf–Æ&ÆR"çFõö÷væVB‚’§Ð ¦fâ7&VFU÷Vç6fVE÷&ö¦V7E÷6W76–öåöF—%ö–â€¢6W76–öå÷&ö÷C¢g7FC£§Fƒ£¥F‚À¢’Óâ&W7VÇCÅVç6fVE6W76–öäÖVF–Â7G&–æsâ°¢7FC£¦g3£¦7&VFUöF—%öÆÂ‡6W76–öå÷&ö÷B’æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢ÆWB6W76–öåöF—"ÒFV×f–ÆS£¤'V–ÆFW#£¦æWr‚¢ç&Vf—‚‚'6W76–öâÒ"¢çFV×F—%ö–â‡6W76–öå÷&ö÷B¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢ÆWB7F÷&U÷F‚Ò6W76–öåöF—"çF‚‚’æ¦ö–â‚'6W76–öâæFr"“°¢ÆWB×WB7F÷&RÒ&ö¦V7E7F÷&S£¦÷Vâ‚g7F÷&U÷F‚’æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢7F÷&P¢ç6fU÷v—F…ö'&ævVÖVçE÷f–Wu÷7FFR‚e&ö¦V7C£¦æWr‚’Âd'&ævVÖVçEf–Wu7FFS£¦FVfVÇB‚’¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢7F÷&Ræ6Æ÷6R‚’æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢ÆWBÆö6²Ò&ö¦V7E6W76–öäÆö6³£¦7V—&R‚g7F÷&U÷F‚’æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢ö²…Vç6fVE6W76–öäÖVF–°¢F—&V7F÷'“¢6W76–öåöF—"æ¶VW‚’À¢Æö6³¢6öÖR†Æö6²’À¢Ò§Ð ¦fâæWu÷&ö¦V7E÷7FGW2‡6W76–öåöÖVF–öW'&÷#¢÷F–öãÂg7G#â’Óâ7G&–ær°¢6W76–öåöÖVF–öW'&÷"æÖö÷%öVÇ6R€¢ÇÂ$æWr&ö¦V7B7&VFVB"çFõö÷væVB‚’À¢ÆW'&÷'Âf÷&ÖB‚$æWr&ö¦V7B7&VFVBÂ'WBFV×÷&'’ÖVF–7F÷&vR—2Væf–Æ&ÆS¢¶W'&÷'Ò"’À¢§Ð ¦fâ67&öÆÅö'&ævVÖVçE÷Fò‡F&vWC¢bw7FF–27G"Âöfg6WE÷“¢c3"’ÓâF6³ÄÖW76vSâ°¢67&öÆÅ÷v–FvWE÷Fò‡F&vWBÂöfg6WE÷’§Ð ¦fâ67&öÆÅ÷v–FvWE÷Fò‡F&vWC¢bw7FF–27G"Âöfg6WE÷“¢c3"’ÓâF6³ÄÖW76vSâ°¢W6R–6VC£¦Gfæ6VC£§v–FvWC£¦÷W&F–öã£§67&öÆÆ&ÆS£§·6VÆbÂ'6öÇWFTöfg6WGÓ° ¢ÆWBF&vWBÒ–6VC£§v–FvWC£¤–C£¦æWr‡F&vWB“°¢ÆWBöfg6WBÒ'6öÇWFTöfg6WB°¢ƒ¢æöæRÀ¢“¢6öÖR†öfg6WE÷’æÖ‚ƒã’’À¢Ó°¢–6VC£¦Gfæ6VC£§v–FvWC£¦÷W&FR‡67&öÆÆ&ÆS£§67&öÆÅ÷Fò‡F&vWBÂöfg6WB’§Ð ¦fâ66ÆU÷&ö¦V7E÷6×ÆW5÷Fõ÷6÷W&6Uög&ÖW5÷F÷v&E÷¦W&ò€¢&ö¦V7E÷6×ÆW3¢“#‚À¢6÷W&6U÷&FS¢ScBÀ¢&ö¦V7E÷&FS¢ScBÀ¢’Óâ&W7VÇCÆ“#‚Â7G&–æsâ°¢–b6÷W&6U÷&FRÓÒÇÂ&ö¦V7E÷&FRÓÒ°¢&WGW&âW'"‚$VF–ò6×ÆR&FW2×W7B&R÷6—F—fR"çFõö÷væVB‚’“°¢Ð¢ÆWBçVÖW&F÷"Ò&ö¦V7E÷6×ÆW0¢æ6†V6¶VEö×VÂ†“#ƒ£¦g&öÒ‡6÷W&6U÷&FR’¢æöµö÷%öVÇ6R‡ÇÂ$VF–òG&–ÒW†6VVG2F†R7W÷'FVBF–ÖR&ævR"çFõö÷væVB‚’“ó°¢ö²†çVÖW&F÷"ò“#ƒ£¦g&öÒ‡&ö¦V7E÷&FR’§Ð ¢5¶6fr‡FW7B•Ð¦fâ66ÆU÷&ö¦V7E÷6×ÆW5÷Fõ÷6÷W&6Uög&ÖW5ö6V–Â€¢&ö¦V7E÷6×ÆW3¢“#‚À¢6÷W&6U÷&FS¢ScBÀ¢&ö¦V7E÷&FS¢ScBÀ¢’Óâ&W7VÇCÆ“#‚Â7G&–æsâ°¢ÆWBçVÖW&F÷"Ò&ö¦V7E÷6×ÆW0¢æ6†V6¶VEö×VÂ†“#ƒ£¦g&öÒ‡6÷W&6U÷&FR’¢æöµö÷%öVÇ6R‡ÇÂ$VF–òG&–ÒW†6VVG2F†R7W÷'FVBF–ÖR&ævR"çFõö÷væVB‚’“ó°¢ÆWBFVæöÖ–æF÷"Ò“#ƒ£¦g&öÒ‡&ö¦V7E÷&FR“°¢ÆWB&VÖ–æFW"ÒçVÖW&F÷"ç&VÕöWV6Æ–B†FVæöÖ–æF÷"“°¢çVÖW&F÷ ¢æF—eöWV6Æ–B†FVæöÖ–æF÷"¢æ6†V6¶VEöFB†“#ƒ£¦g&öÒ‡&VÖ–æFW"â’¢æöµö÷%öVÇ6R‡ÇÂ$VF–òG&–ÒW†6VVG2F†R7W÷'FVBF–ÖR&ævR"çFõö÷væVB‚’§Ð ¦fâ67&öÆÅö7F–öç5öÖVçU÷6VÆV7F–öâ†¢f×WBÂ6VÆV7FVC¢6öÖÖæD–B’ÓâF6³ÄÖW76vSâ°¢ÆWBVçG&–W2Ò6öÖÖæG3£¦ÖF6†–æuö7F–öç5öÖVçR†Âfæ7F–öå÷VW'’“°¢ÆWB6öÖR†öfg6WE÷’’Ð¢7F–öç5öÖVçU÷6VÆV7F–öå÷67&öÆÅööfg6WB‚fVçG&–W2Â6VÆV7FVBÂæ7F–öåöÖVçU÷67&öÆÅööfg6WB¢VÇ6R°¢&WGW&âF6³£¦æöæR‚“°¢Ó°¢æ7F–öåöÖVçU÷67&öÆÅööfg6WBÒöfg6WE÷“°¢67&öÆÅ÷v–FvWE÷Fò‚&FrÖ7F–öç2ÖÖVçR×&W7VÇG2"Âöfg6WE÷’§Ð ¦fâÖVçUö6öÖÖæEö–æFW‚€¢7W'&VçC¢÷F–öãÇW6—¦SâÀ¢6öÖÖæEö6÷VçC¢W6—¦RÀ¢æf–vF–öã¢ÖVçTæf–vF–öâÀ¢’ÓâW6—¦R°¢FV'Vuö76W'B†6öÖÖæEö6÷VçBâ“°¢ÆWBÆ7BÒ6öÖÖæEö6÷VçBÒ°¢ÖF6‚æf–vF–öâ°¢ÖVçTæf–vF–öã£¤f—'7D6öÖÖæBÓâÀ¢ÖVçTæf–vF–öã£¤Æ7D6öÖÖæBÓâÆ7BÀ¢ÖVçTæf–vF–öã£¤æW‡D6öÖÖæBÓâ7W'&VçBæÖö÷"ƒÂÆ–æFW‡Â†–æFW‚²’R6öÖÖæEö6÷VçB’À¢ÖVçTæf–vF–öã£¥&Wf–÷W46öÖÖæBÓâ°¢7W'&VçBæÖö÷"†Æ7BÂÆ–æFW‡Â†–æFW‚²Æ7B’R6öÖÖæEö6÷VçB¢Ð¢òÓâVç&V6†&ÆR‚&öæÇ’6öÖÖæBæf–vF–öâ†26öÖÖæB–æFW‚"’À¢Ð§Ð ¦fâ7F–öç5öÖVçU÷6VÆV7F–öå÷67&öÆÅööfg6WB€¢VçG&–W3¢e¶6öÖÖæG3£¤6öÖÖæDVçG'•ÒÀ¢6VÆV7FVC¢6öÖÖæD–BÀ¢7W'&VçEööfg6WC¢c3"À¢’Óâ÷F–öãÆc3#â°¢6öç7Bd”Uuõ%Eô„T”t…C¢c3"Ò#ƒBã°¢6öç7B$õuô„T”t…C¢c3"Ò#‚ã°¢6öç7B4DTtõ%•ô„T”t…C¢c3"Òbã°¢6öç7B4U$Dõ%ô„T”t…C¢c3"ÒRã°¢6öç7B$õuôt¢c3"ÒBã° ¢ÆWB×WB’Òã°¢ÆWB×WB6FVv÷'’ÒæöæS°¢ÆWB×WB6VÆV7FVEö&÷VæG2ÒæöæS°¢f÷"VçG'’–âVçG&–W2°¢–b6FVv÷'’Ò6öÖR†VçG'’æ6FVv÷'’’°¢6FVv÷'’Ò6öÖR†VçG'’æ6FVv÷'’“°¢’³Ò4DTtõ%•ô„T”t…B²$õuôt°¢Ð¢–bVçG'’ç6W&F÷%ö&Vf÷&R°¢’³Ò4U$Dõ%ô„T”t…B²$õuôt°¢Ð¢–bVçG'’æ–BÓÒ6VÆV7FVB°¢6VÆV7FVEö&÷VæG2Ò6öÖR‚‡’Â’²$õuô„T”t…B’“°¢Ð¢’³Ò$õuô„T”t…B²$õuôt°¢Ð¢ÆWB6öçFVçEö†V–v‡BÒ“°¢ÆWB‡&÷u÷F÷Â&÷uö&÷GFöÒ’Ò6VÆV7FVEö&÷VæG3ó°¢ÆWBÖ…ööfg6WBÒ†6öçFVçEö†V–v‡BÒd”Uuõ%Eô„T”t…B’æÖ‚ƒã“°¢ÆWB7W'&VçEööfg6WBÒ7W'&VçEööfg6WBæ6Æ×ƒãÂÖ…ööfg6WB“°¢ÆWBæW‡Eööfg6WBÒ–b&÷u÷F÷Â7W'&VçEööfg6WB°¢&÷u÷F÷ ¢ÒVÇ6R–b&÷uö&÷GFöÒâ7W'&VçEööfg6WB²d”Uuõ%Eô„T”t…B°¢&÷uö&÷GFöÒÒd”Uuõ%Eô„T”t…@¢ÒVÇ6R°¢&WGW&âæöæS°¢Ð¢æ6Æ×ƒãÂÖ…ööfg6WB“°¢†æW‡Eööfg6WBÒ7W'&VçEööfg6WB’çF†Vå÷6öÖR†æW‡Eööfg6WB§Ð ¢òòò'Vç2&Æö6¶–ær&ö¦V7B7F÷&vRv÷&²v’g&öÒF†R–6VBWFFRF‡&VBà¢5¶ÆÆ÷r†6Æ—“£§VçW6VEö7–æ2•Ð¦7–æ2fâ'Våö&Æö6¶–æsÅC¢6VæB²w7FF–3â€¢æÖS¢bw7FF–27G"À¢÷W&F–öã¢–×Âfäöæ6R‚’Óâ&W7VÇCÅBÂ7G&–æsâ²6VæB²w7FF–2À¢’Óâ&W7VÇCÅBÂ7G&–æsâ°¢F‡&VC£¤'V–ÆFW#£¦æWr‚¢ææÖR†æÖRçFõö÷væVB‚’¢ç7vâ†÷W&F–öâ¢æÖöW'"‡ÆW'&÷'Âf÷&ÖB‚&6÷VÆBæ÷B7F'Bv÷&¶W#¢¶W'&÷'Ò"’“ð¢æ¦ö–â‚¢æÖöW'"‡Å÷Âf÷&ÖB‚'¶æÖWÒv÷&¶W"æ–6¶VB"’“ð§Ð ¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¦fâ7F'Eö7ÅöFWf–6UöVçVÖW&F–öãÅC¢6VæB²w7FF–3â€¢ÆöF–æs¢f×WB&ööÂÀ¢W'&÷#¢f×WB÷F–öãÅ7G&–æsâÀ¢v÷&¶W%öæÖS¢bw7FF–27G"À¢VçVÖW&FS¢–×Âfäöæ6R‚’Óâ&W7VÇCÅfV3ÅCâÂ7G&–æsâ²6VæB²w7FF–2À¢ÖW76vS¢fâ…&W7VÇCÅfV3ÅCâÂ7G&–æsâ’ÓâÖW76vRÀ¢’ÓâF6³ÄÖW76vSâ°¢–b¦ÆöF–ær°¢&WGW&âF6³£¦æöæR‚“°¢Ð¢¦ÆöF–ærÒG'VS°¢¦W'&÷"ÒæöæS°¢F6³£§W&f÷&Ò‡'Våö&Æö6¶–ær‡v÷&¶W%öæÖRÂVçVÖW&FR’ÂÖW76vR§Ð ¢5¶6fr†ÆÂ€¢fVGW&RÒ&7ÂÖ&6¶VæB"À¢ç’‡F&vWEö÷2Ò'v–æF÷w2"ÂF&vWEö÷2Ò&Ö6÷2"ÂF&vWEö÷2Ò&æG&ö–B"¢’•Ð¦fâf–æ—6…ö7ÅöFWf–6UöVçVÖW&F–öãÅCâ€¢FWf–6W3¢f×WBfV3ÅCâÀ¢ÆöF–æs¢f×WB&ööÂÀ¢W'&÷#¢f×WB÷F–öãÅ7G&–æsâÀ¢fVVF&6³¢f×WB7G&–ærÀ¢F—&V7F–öã¢g7G"À¢&W7VÇC¢&W7VÇCÅfV3ÅCâÂ7G&–æsâÀ¢’°¢¦ÆöF–ærÒfÇ6S°¢ÖF6‚&W7VÇB°¢ö²†VçVÖW&FVB’Óâ°¢¦FWf–6W2ÒVçVÖW&FVC°¢¦W'&÷"ÒæöæS°¢Ð¢W'"†VçVÖW&F–öåöW'&÷"’Óâ°¢FWf–6W2æ6ÆV"‚“°¢¦W'&÷"Ò6öÖR†VçVÖW&F–öåöW'&÷"æ6ÆöæR‚’“°¢¦fVVF&6²Òf÷&ÖB€¢%7—7FVÒVF–ò¶F—&V7F–öçÒFWf–6W26÷VÆBæ÷B&RÆ—7FVC¢¶VçVÖW&F–öåöW'&÷'Ò ¢“°¢Ð¢Ð§Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¢5¶6fr‡FW7B•Ð¦fâ&W&U÷&ö¦V7E÷Æ–&6µöf–ÆR€¢Fƒ¢F„'VbÀ¢6æ6†÷C¢&ö¦V7E6æ6†÷BÀ¢F&vWE÷6×ÆS¢ScBÀ¢’Óâ&W7VÇCÅ&W&VDVF–õÆ–&6²Â7G&–æsâ°¢&W&U÷&ö¦V7E÷Æ–&6µöf–ÆU÷v—F…öÆö÷÷ö–çG2‡F‚Â6æ6†÷BÂF&vWE÷6×ÆRÂæöæR§Ð ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ð¦fâ&W&U÷&ö¦V7E÷Æ–&6µöf–ÆU÷v—F…öÆö÷÷ö–çG2€¢Fƒ¢F„'VbÀ¢6æ6†÷C¢&ö¦V7E6æ6†÷BÀ¢F&vWE÷6×ÆS¢ScBÀ¢Æö÷÷ö–çG5÷F–6·3¢÷F–öãÂ‡ScBÂScB“âÀ¢’Óâ&W7VÇCÅ&W&VDVF–õÆ–&6²Â7G&–æsâ°¢–bF‚æ—5öf–ÆR‚’°¢&WGW&âW'"†f÷&ÖB‚'&ö¦V7Bf–ÆR·ÒFöW2æ÷BW†—7B"ÂF‚æF—7Æ’‚’’“°¢Ð¢ÆWB&ö¦V7BÒ&ö¦V7C£¦g&öÕ÷6æ6†÷B‡6æ6†÷B’æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢ÆWB7F÷&RÒ&ö¦V7E7F÷&S£¦÷Vâ‡F‚’æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢ÆWBG&ç7÷'EöÆö÷÷&ævRÒÆö÷÷ö–çG5÷F–6·0¢æÖ‡Â‡7F'E÷F–6²ÂVæE÷F–6²—Â°¢ÆWB7F'E÷6×ÆRÒ&ö¦V7@¢ç6×ÆUöE÷F–6²‡7F'E÷F–6²¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢ÆWBVæE÷6×ÆRÒ&ö¦V7@¢ç6×ÆUöE÷F–6²†VæE÷F–6²¢æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“ó°¢ö³££ÅòÂ7G&–æsâ…G&ç7÷'DÆö÷&ævS£¦æWr‡7F'E÷6×ÆRÂVæE÷6×ÆR’¢Ò¢çG&ç7÷6R‚“ð¢æfÆGFVâ‚“°¢ÆWB&W&VBÒ&W&UöVF–õ÷Æ–&6µöE÷v—F…öÆö÷÷&ævR€¢g&ö¦V7BÀ¢g7F÷&RÀ¢F&vWE÷6×ÆRÀ¢G&ç7÷'EöÆö÷÷&ævRÀ¢eó3ƒBÀ¢…ó“"À¢FuöÖVF–£¥&W6×Æ–æuVÆ—G“£¥6–æ3cBÀ¢¢æÖöW'"‡ÆW'&÷#¢Æ–&6´'V–ÆDW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“°¢ÆWB6Æ÷6RÒ7F÷&Ræ6Æ÷6R‚’æÖöW'"‡ÆW'&÷'ÂW'&÷"çFõ÷7G&–ær‚’“°¢ÆWB&W&VBÒ&W&VCó°¢6Æ÷6Só°¢ö²‡&W&VB§Ð ¦fâÖVçUöæf–vF–öåöWfVçB€¢WfVçC¢f–6VC£¤WfVçBÀ¢7FGW3¢–6VC£¦WfVçC£¥7FGW2À¢v–æF÷uö–C¢–6VC£§v–æF÷s£¤–BÀ¢Ö–å÷v–æF÷uö–C¢÷F–öãÆ–6VC£§v–æF÷s£¤–CâÀ¢7F—fUöÖVçS¢÷F–öãÄÖ–äÖVçSâÀ¢’Óâ÷F–öãÄÖW76vSâ°¢–bÖ–å÷v–æF÷uö–BÒ6öÖR‡v–æF÷uö–B’°¢&WGW&âæöæS°¢Ð¢ÆWB–6VC£¤WfVçC£¤¶W–&ö&B†–6VC£¦¶W–&ö&C£¤WfVçC£¤¶W•&W76VB°¢¶W’À¢ÖöF–f–W'2À¢&WVC¢fÇ6RÀ¢âà¢Ò’ÒWfVç@¢VÇ6R°¢&WGW&âæöæS°¢Ó°¢–b7FGW2ÓÒ–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBbb¦ÖöF–f–W'2ÓÒ–6VC£¦¶W–&ö&C£¤ÖöF–f–W'3£¤ÅB°¢ÆWBÖVçRÒÖF6‚¶W’æ5÷&Vb‚’°¢–6VC£¦¶W–&ö&C£¤¶W“£¤6†&7FW"‡fÇVR’–bfÇVRæWö–væ÷&Uö66–•ö66R‚&"’Óâ°¢6öÖR„Ö–äÖVçS£¤7F–öç2¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤6†&7FW"‡fÇVR’–bfÇVRæWö–væ÷&Uö66–•ö66R‚&R"’Óâ°¢6öÖR„Ö–äÖVçS£¤VF—B¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤6†&7FW"‡fÇVR’–bfÇVRæWö–væ÷&Uö66–•ö66R‚&b"’Óâ°¢6öÖR„Ö–äÖVçS£¤f–ÆR¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤6†&7FW"‡fÇVR’–bfÇVRæWö–væ÷&Uö66–•ö66R‚&‚"’Óâ°¢6öÖR„Ö–äÖVçS£¤†VÇ¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤6†&7FW"‡fÇVR’–bfÇVRæWö–væ÷&Uö66–•ö66R‚&’"’Óâ°¢6öÖR„Ö–äÖVçS£¤–ç6W'B¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤6†&7FW"‡fÇVR’–bfÇVRæWö–væ÷&Uö66–•ö66R‚&ò"’Óâ°¢6öÖR„Ö–äÖVçS£¤÷F–öç2¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤6†&7FW"‡fÇVR’–bfÇVRæWö–væ÷&Uö66–•ö66R‚'b"’Óâ°¢6öÖR„Ö–äÖVçS£¥f–Wr¢Ð¢òÓâæöæRÀ¢Ó°¢–bÆWB6öÖR†ÖVçR’ÒÖVçR°¢&WGW&â6öÖR„ÖW76vS£¤ÖVçT¶W–&ö&B„ÖVçTæf–vF–öã£¤÷VäÖVçR†ÖVçR’’“°¢Ð¢Ð¢ÆWBæf–vF–öâÒÖF6‚¶W’æ5÷&Vb‚’°¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤W66R’–b7F—fUöÖVçRæ—5÷6öÖR‚’Óâ°¢&WGW&â6öÖR„ÖW76vS£¤W66R“°¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤†öÖR¢–b7F—fUöÖVçRæ—5÷6öÖR‚¢bb‡7FGW2ÓÒ–6VC£¦WfVçC£¥7FGW3£¤–væ÷&V@¢ÇÂ7F—fUöÖVçRÓÒ6öÖR„Ö–äÖVçS£¤7F–öç2’’Óà¢°¢ÖVçTæf–vF–öã£¤f—'7D6öÖÖæ@¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤VæB¢–b7F—fUöÖVçRæ—5÷6öÖR‚¢bb‡7FGW2ÓÒ–6VC£¦WfVçC£¥7FGW3£¤–væ÷&V@¢ÇÂ7F—fUöÖVçRÓÒ6öÖR„Ö–äÖVçS£¤7F–öç2’’Óà¢°¢ÖVçTæf–vF–öã£¤Æ7D6öÖÖæ@¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤'&÷tÆVgB¢–b7F—fUöÖVçRæ—5÷6öÖR‚¢bb‡7FGW2ÓÒ–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÇÂÖöF–f–W'2æÇB‚’’Óà¢°¢ÖVçTæf–vF–öã£¥&Wf–÷W4ÖVçP¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤'&÷u&–v‡B¢–b7F—fUöÖVçRæ—5÷6öÖR‚¢bb‡7FGW2ÓÒ–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÇÂÖöF–f–W'2æÇB‚’’Óà¢°¢ÖVçTæf–vF–öã£¤æW‡DÖVçP¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤'&÷uW¢–b7F—fUöÖVçRæ—5÷6öÖR‚¢bb‡7FGW2ÓÒ–6VC£¦WfVçC£¥7FGW3£¤–væ÷&V@¢ÇÂ7F—fUöÖVçRÓÒ6öÖR„Ö–äÖVçS£¤7F–öç2’’Óà¢°¢ÖVçTæf–vF–öã£¥&Wf–÷W46öÖÖæ@¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤'&÷tF÷vâ¢–b7F—fUöÖVçRæ—5÷6öÖR‚¢bb‡7FGW2ÓÒ–6VC£¦WfVçC£¥7FGW3£¤–væ÷&V@¢ÇÂ7F—fUöÖVçRÓÒ6öÖR„Ö–äÖVçS£¤7F–öç2’’Óà¢°¢ÖVçTæf–vF–öã£¤æW‡D6öÖÖæ@¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤VçFW"¢–b7F—fUöÖVçRæ—5÷6öÖR‚¢bb‡7FGW2ÓÒ–6VC£¦WfVçC£¥7FGW3£¤–væ÷&V@¢ÇÂ7F—fUöÖVçRÓÒ6öÖR„Ö–äÖVçS£¤7F–öç2’’Óà¢°¢&WGW&â6öÖR†–b7F—fUöÖVçRÓÒ6öÖR„Ö–äÖVçS£¤7F–öç2’°¢ÖW76vS£¥'Vä7F–öåVW'¢ÒVÇ6R°¢ÖW76vS£¤ÖVçT¶W–&ö&B„ÖVçTæf–vF–öã£¤7F—fFR¢Ò“°¢Ð¢òÓâ&WGW&âæöæRÀ¢Ó°¢6öÖR„ÖW76vS£¤ÖVçT¶W–&ö&B†æf–vF–öâ’§Ð ¦fâ'VçF–ÖUö¶W–&ö&EöWfVçB€¢WfVçC¢–6VC£¤WfVçBÀ¢7FGW3¢–6VC£¦WfVçC£¥7FGW2À¢v–æF÷uö–C¢–6VC£§v–æF÷s£¤–BÀ¢’Óâ÷F–öãÄÖW76vSâ°¢ÖF6†W2†WfVçBÂ–6VC£¤WfVçC£¤¶W–&ö&B…ò’¢çF†Vå÷6öÖR„ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB†WfVçBÂ7FGW2Âv–æF÷uö–B’§Ð ¦fâæG&ö–Eööfö7W5öWfVçB€¢WfVçC¢–6VC£¤WfVçBÀ¢÷7FGW3¢–6VC£¦WfVçC£¥7FGW2À¢÷v–æF÷uö–C¢–6VC£§v–æF÷s£¤–BÀ¢’Óâ÷F–öãÄÖW76vSâ°¢–b6fr‡F&vWEö÷2Ò&æG&ö–B"’°¢&WGW&âæöæS°¢Ð¢ÖF6‚WfVçB°¢–6VC£¤WfVçC£¥v–æF÷r†–6VC£§v–æF÷s£¤WfVçC£¤fö7W6VB’Óâ°¢6öÖR„ÖW76vS£¤æG&ö–Dfö7W46†ævVB‡G'VR’¢Ð¢–6VC£¤WfVçC£¥v–æF÷r†–6VC£§v–æF÷s£¤WfVçC£¥Væfö7W6VB’Óâ°¢6öÖR„ÖW76vS£¤æG&ö–Dfö7W46†ævVB†fÇ6R’¢Ð¢òÓâæöæRÀ¢Ð§Ð ¦fâÖ–F•öW‡&W76–öåö6öçFW‡EöÖVçUöWfVçB€¢WfVçC¢–6VC£¤WfVçBÀ¢÷7FGW3¢–6VC£¦WfVçC£¥7FGW2À¢v–æF÷uö–C¢–6VC£§v–æF÷s£¤–BÀ¢’Óâ÷F–öãÄÖW76vSâ°¢ÖF6†W2€¢WfVçBÀ¢–6VC£¤WfVçC£¤Ö÷W6R†–6VC£¦Ö÷W6S£¤WfVçC£¤'WGFöå&W76VB†–6VC£¦Ö÷W6S£¤'WGFöã£¤ÆVgB’¢¢çF†Vå÷6öÖR„ÖW76vS£¤F—6Ö—74Ö–F”W‡&W76–öä6öçFW‡DÖVçW2‡v–æF÷uö–B’§Ð ¦fâg…ö6†–å÷ÇVv–åöG&uöWfVçB€¢WfVçC¢–6VC£¤WfVçBÀ¢÷7FGW3¢–6VC£¦WfVçC£¥7FGW2À¢÷v–æF÷uö–C¢–6VC£§v–æF÷s£¤–BÀ¢’Óâ÷F–öãÄÖW76vSâ°¢ÖF6†W2€¢WfVçBÀ¢–6VC£¤WfVçC£¤Ö÷W6R†–6VC£¦Ö÷W6S£¤WfVçC£¤'WGFöå&VÆV6VB€¢–6VC£¦Ö÷W6S£¤'WGFöã£¤ÆVg@¢’¢¢çF†Vå÷6öÖR„ÖW76vS£¤f–æ—6„g„6†–åÇVv–äG&r§Ð ¦fâÇVv–å÷v–æF÷uöW66UöÖW76vR€¢WfVçC¢f–6VC£¤WfVçBÀ¢÷7FGW3¢–6VC£¦WfVçC£¥7FGW2À¢v–æF÷uö–C¢–6VC£§v–æF÷s£¤–BÀ¢g…ö6†–å÷v–æF÷uö–C¢÷F–öãÆ–6VC£§v–æF÷s£¤–CâÀ¢ÇVv–å÷–6¶W%÷v–æF÷uö–C¢÷F–öãÆ–6VC£§v–æF÷s£¤–CâÀ¢’Óâ÷F–öãÄÖW76vSâ°¢ÆWB—5öW66RÒÖF6†W2€¢WfVçBÀ¢–6VC£¤WfVçC£¤¶W–&ö&B†–6VC£¦¶W–&ö&C£¤WfVçC£¤¶W•&W76VB°¢¶W“¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤W66R’À¢ÖöF–f–W'3¢–6VC£¦¶W–&ö&C£¤ÖöF–f–W'3£¤äôäRÀ¢&WVC¢fÇ6RÀ¢âà¢Ò¢“°¢–b—5öW66R°¢&WGW&âæöæS°¢Ð¢–bÇVv–å÷–6¶W%÷v–æF÷uö–BÓÒ6öÖR‡v–æF÷uö–B’°¢6öÖR„ÖW76vS£¤6Æ÷6UÇVv–å–6¶W"¢ÒVÇ6R–bg…ö6†–å÷v–æF÷uö–BÓÒ6öÖR‡v–æF÷uö–B’°¢6öÖR„ÖW76vS£¤6Æ÷6UG&6´g„6†–â¢ÒVÇ6R°¢æöæP¢Ð§Ð ¦fâ¶W–&ö&E÷6†÷'F7WEöWfVçB€¢WfVçC¢–6VC£¤WfVçBÀ¢7FGW3¢–6VC£¦WfVçC£¥7FGW2À¢v–æF÷uö–C¢–6VC£§v–æF÷s£¤–BÀ¢Ö–å÷v–æF÷uö–C¢÷F–öãÆ–6VC£§v–æF÷s£¤–CâÀ¢6WGF–æw5÷v–æF÷uö–C¢÷F–öãÆ–6VC£§v–æF÷s£¤–CâÀ¢6†÷'F7WEö6GW&Uö–C¢÷F–öãÂg7G#âÀ¢’Óâ÷F–öãÄÖW76vSâ°¢–b‡6WGF–æw5÷v–æF÷uö–BÓÒ6öÖR‡v–æF÷uö–B¢ÇÂ‡6WGF–æw5÷v–æF÷uö–Bæ—5öæöæR‚’bbÖ–å÷v–æF÷uö–BÓÒ6öÖR‡v–æF÷uö–B’’¢bbÆWB6öÖR†7F–öåö–B’Ò6†÷'F7WEö6GW&Uö–@¢°¢ÆWB–6VC£¤WfVçC£¤¶W–&ö&B†–6VC£¦¶W–&ö&C£¤WfVçC£¤¶W•&W76VB°¢¶W’À¢ÖöF–f–W'2À¢‡—6–6Åö¶W’À¢Æö6F–öâÀ¢&WVC¢fÇ6RÀ¢âà¢Ò’ÒWfVç@¢VÇ6R°¢&WGW&âæöæS°¢Ó°¢&WGW&âÖF6‚¶W’æ5÷&Vb‚’°¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤W66R¢–bÖöF–f–W'2ÓÒ–6VC£¦¶W–&ö&C£¤ÖöF–f–W'3£¤äôäRÓà¢°¢6öÖR„ÖW76vS£¤6æ6VÅ6†÷'F7WD6GW&R¢Ð¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤&6·76R¢–bÖöF–f–W'2ÓÒ–6VC£¦¶W–&ö&C£¤ÖöF–f–W'3£¤äôäRÓà¢°¢6öÖR„ÖW76vS£¤6ÆV%6†÷'F7WD&–æF–ær†7F–öåö–BçFõö÷væVB‚’’¢Ð¢òÓâ°¢ÆWB–çWBÒ6†÷'F7WC£¥6†÷'F7WD–çWB°¢Æöv–6Åö¶W“¢¶W’À¢‡—6–6Åö¶W’À¢Æö6F–öâÀ¢ÖöF–f–W'3¢–6VC£¦¶W–&ö&C£¤ÖöF–f–W'3£¤äôäRÀ¢Ó°¢ÆWB¶W’Ò6†÷'F7WC£¥6†÷'F7WC£¦6GW&Uö–çWB‚f–çWB¢æÖ‡Æ¶W—Â¶W’æ6öæf–uöÆ&VÂ‚’¢çVçw&ö÷%öVÇ6R‡Å÷Â%Væ–FVçF–f–VB"çFõö÷væVB‚’“°¢6öÖR„ÖW76vS£¥6†÷'F7WD6GW&T¶W’°¢7F–öåö–C¢7F–öåö–BçFõö÷væVB‚’À¢¶W’À¢ÖöF–f–W'2À¢Ò¢Ð¢Ó°¢Ð¢–bÖ–å÷v–æF÷uö–BÒ6öÖR‡v–æF÷uö–B’°¢&WGW&âæöæS°¢Ð¢ÆWB—5öW66RÒÖF6†W2€¢fWfVçBÀ¢–6VC£¤WfVçC£¤¶W–&ö&B†–6VC£¦¶W–&ö&C£¤WfVçC£¤¶W•&W76VB°¢¶W“¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤W66R’À¢ÖöF–f–W'3¢–6VC£¦¶W–&ö&C£¤ÖöF–f–W'3£¤äôäRÀ¢&WVC¢fÇ6RÀ¢âà¢Ò¢“°¢–b7FGW2Ò–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBbb—5öW66R°¢&WGW&âæöæS°¢Ð¢ÆWB–6VC£¤WfVçC£¤¶W–&ö&B†–6VC£¦¶W–&ö&C£¤WfVçC£¤¶W•&W76VB°¢¶W’À¢ÖöF–f–W'2À¢‡—6–6Åö¶W’À¢Æö6F–öâÀ¢&WVC¢fÇ6RÀ¢âà¢Ò’ÒWfVç@¢VÇ6R°¢&WGW&âæöæS°¢Ó°¢–b¶W’ÓÒ–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤W66R¢bbÖöF–f–W'2ÓÒ–6VC£¦¶W–&ö&C£¤ÖöF–f–W'3£¤äôäP¢°¢6öÖR„ÖW76vS£¤W66R¢ÒVÇ6R°¢ÆWB–çWBÒ6†÷'F7WC£¥6†÷'F7WD–çWB°¢Æöv–6Åö¶W“¢¶W’À¢‡—6–6Åö¶W’À¢Æö6F–öâÀ¢ÖöF–f–W'2À¢Ó°¢–b–çWBæÆöv–6Åö¶W’ÓÒ–6VC£¦¶W–&ö&C£¤¶W“£¥Væ–FVçF–f–V@¢bb6†÷'F7WC£¥6†÷'F7WC£¦6GW&Uö–çWB‚f–çWB’æ—5öW'"‚¢°¢æöæP¢ÒVÇ6R°¢6öÖR„ÖW76vS£¥6†÷'F7WE&W76VB†–çWB’¢Ð¢Ð§Ð ¦fâÖö&–ÆUö&6µöWfVçB€¢WfVçC¢f–6VC£¤WfVçBÀ¢v–æF÷uö–C¢–6VC£§v–æF÷s£¤–BÀ¢Ö–å÷v–æF÷uö–C¢÷F–öãÆ–6VC£§v–æF÷s£¤–CâÀ¢Öö&–ÆU÷æVÅö÷Vã¢&ööÂÀ¢’Óâ÷F–öãÄÖW76vSâ°¢–bÖ–å÷v–æF÷uö–BÒ6öÖR‡v–æF÷uö–B’ÇÂÖö&–ÆU÷æVÅö÷Vâ°¢&WGW&âæöæS°¢Ð¢ÆWB–6VC£¤WfVçC£¤¶W–&ö&B†–6VC£¦¶W–&ö&C£¤WfVçC£¤¶W•&W76VB°¢¶W“ ¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB€¢–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤'&÷w6W$&6²Â–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤vô&6²À¢’À¢&WVC¢fÇ6RÀ¢âà¢Ò’ÒWfVç@¢VÇ6R°¢&WGW&âæöæS°¢Ó°¢6öÖR„ÖW76vS£¤Öö&–ÆTæf–vFT&6²§Ð ¦fâÖ–F•öVF—F÷%÷6†÷'F7WEöWfVçB€¢WfVçC¢–6VC£¤WfVçBÀ¢7FGW3¢–6VC£¦WfVçC£¥7FGW2À¢v–æF÷uö–C¢–6VC£§v–æF÷s£¤–BÀ¢Ö–F•öVF—F÷%÷v–æF÷uö–C¢÷F–öãÆ–6VC£§v–æF÷s£¤–CâÀ¢’Óâ÷F–öãÄÖW76vSâ°¢–bÖ–F•öVF—F÷%÷v–æF÷uö–BÒ6öÖR‡v–æF÷uö–B’°¢&WGW&âæöæS°¢Ð¢–bÖF6†W2€¢fWfVçBÀ¢–6VC£¤WfVçC£¤¶W–&ö&B†–6VC£¦¶W–&ö&C£¤WfVçC£¤¶W•&W76VB°¢¶W“¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤W66R’À¢&WVC¢fÇ6RÀ¢âà¢Ò¢’°¢&WGW&âæöæS°¢Ð¢¶W–&ö&E÷6†÷'F7WEöWfVçB†WfVçBÂ7FGW2Âv–æF÷uö–BÂ6öÖR‡v–æF÷uö–B’ÂæöæRÂæöæR§Ð ¢5¶6fr‡FW7B•Ð¦fâ6†÷'F7WEöÖW76vR€¢¶W“¢–6VC£¦¶W–&ö&C£¤¶W“Âg7G#âÀ¢ÖöF–f–W'3¢–6VC£¦¶W–&ö&C£¤ÖöF–f–W'2À¢&–æF–æw3¢f6öÖÖæG3£¥6†÷'F7WD&–æF–æw2À¢’Óâ÷F–öãÄÖW76vSâ°¢–bÖöF–f–W'2ÓÒ–6VC£¦¶W–&ö&C£¤ÖöF–f–W'3£¤äôäP¢bb¶W’ÓÒ–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤W66R¢°¢&WGW&â6öÖR„ÖW76vS£¤W66R“°¢Ð¢6öÖÖæG3£¦g&öÕ÷6†÷'F7WB‚f¶W’ÂÖöF–f–W'2Â&–æF–æw2ÂeµÒ’æÖ„ÖW76vS£¤W†V7WFT6öÖÖæB§Ð ¦fâ6ÖU÷F‚†ÆVgC¢g7FC£§Fƒ£¥F‚Â&–v‡C¢g7FC£§Fƒ£¥F‚’Óâ&ööÂ°¢7FC£¦g3£¦6æöæ–6Æ—¦R†ÆVgB’çVçw&ö÷%öVÇ6R‡Å÷ÂÆVgBçFõ÷F…ö'Vb‚’¢ÓÒ7FC£¦g3£¦6æöæ–6Æ—¦R‡&–v‡B’çVçw&ö÷%öVÇ6R‡Å÷Â&–v‡BçFõ÷F…ö'Vb‚’§Ð 