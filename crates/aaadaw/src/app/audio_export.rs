use super::messages::FreezeTrackResult;
use super::offline_job_queue::{EnqueueError, OfflineJobId};
use super::{App, Message, run_blocking};
use aaadaw_app::WavChannelCount;
use aaadaw_core::{DawAction, Project, ProjectSnapshot, TrackId};
use aaadaw_storage::ProjectStore;
use iced::Task;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

const JOB_HISTORY_CAPACITY: usize = 4;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RenderBounds {
    #[default]
    EntireProject,
    TimeSelection,
    AllProjectRegions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RenderSampleRate {
    Project,
    Fixed(u32),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RenderSource {
    #[default]
    MasterMix,
    SelectedTracksStems,
    SelectedTracksViaMaster,
    MasterMixAndSelectedTracks,
}

impl RenderSource {
    pub(crate) const ALL: [Self; 4] = [
        Self::MasterMix,
        Self::SelectedTracksStems,
        Self::MasterMixAndSelectedTracks,
        Self::SelectedTracksViaMaster,
    ];
}

impl std::fmt::Display for RenderSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::MasterMix => "Master mix",
            Self::SelectedTracksStems => "Selected tracks (stems)",
            Self::SelectedTracksViaMaster => "Selected tracks via master",
            Self::MasterMixAndSelectedTracks => "Master mix and stems",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectedTrackStemMode {
    PostFader,
    PreFader,
    ViaMaster,
}

pub(super) fn parse_render_tail_milliseconds(value: &str) -> Result<u32, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Render tail must be a non-negative number with up to 3 decimal places");
    }
    let mut parts = value.split('.');
    let whole_text = parts.next().unwrap_or_default();
    let fractional_text = parts.next();
    if parts.next().is_some() {
        return Err("Render tail must be a non-negative number with up to 3 decimal places");
    }
    let whole_seconds = if whole_text.is_empty() {
        0
    } else {
        whole_text
            .parse::<u32>()
            .map_err(|_| "Render tail must be a non-negative number with up to 3 decimal places")?
    };
    let fractional = fractional_text.unwrap_or_default();
    if (whole_text.is_empty() && fractional.is_empty())
        || fractional.len() > 3
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("Render tail must be a non-negative number with up to 3 decimal places");
    }
    let fractional_milliseconds = if fractional.is_empty() {
        0
    } else {
        fractional
            .parse::<u32>()
            .map_err(|_| "Render tail must be a non-negative number with up to 3 decimal places")?
            * 10_u32.pow((3 - fractional.len()) as u32)
    };
    whole_seconds
        .checked_mul(1_000)
        .and_then(|whole_milliseconds| whole_milliseconds.checked_add(fractional_milliseconds))
        .ok_or("Render tail is too large")
}

pub(super) fn parse_render_peak_target_centi_dbfs(value: &str) -> Result<i32, &'static str> {
    let value = value.trim();
    let (negative, magnitude) = match value.strip_prefix('-') {
        Some(magnitude) => (true, magnitude),
        None => (false, value),
    };
    let mut parts = magnitude.split('.');
    let whole_text = parts.next().unwrap_or_default();
    let fractional_text = parts.next();
    if parts.next().is_some() {
        return Err("Peak target must be between -60 and 0 dBFS with up to 2 decimals");
    }
    let fractional = fractional_text.unwrap_or_default();
    if (whole_text.is_empty() && fractional.is_empty())
        || fractional.len() > 2
        || !whole_text.bytes().all(|byte| byte.is_ascii_digit())
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("Peak target must be between -60 and 0 dBFS with up to 2 decimals");
    }
    let whole = if whole_text.is_empty() {
        0
    } else {
        whole_text
            .parse::<i32>()
            .map_err(|_| "Peak target is too large")?
    };
    let fractional_centi = if fractional.is_empty() {
        0
    } else {
        fractional
            .parse::<i32>()
            .map_err(|_| "Peak target must be between -60 and 0 dBFS with up to 2 decimals")?
            * 10_i32.pow((2 - fractional.len()) as u32)
    };
    let magnitude_centi = whole
        .checked_mul(100)
        .and_then(|whole| whole.checked_add(fractional_centi))
        .ok_or("Peak target is too large")?;
    let target = if negative {
        -magnitude_centi
    } else {
        magnitude_centi
    };
    if !(-6_000..=0).contains(&target) {
        return Err("Peak target must be between -60 and 0 dBFS with up to 2 decimals");
    }
    Ok(target)
}

fn selected_track_stem_destinations(
    base_path: &Path,
    project: &Project,
    selected_tracks: &[TrackId],
    include_master_mix: bool,
    stem_mode: SelectedTrackStemMode,
) -> Result<Vec<(aaadaw_app::ProjectWavRenderSource, PathBuf)>, String> {
    let parent = base_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let base_name = base_path
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "Choose a WAV filename to use as the stem prefix".to_owned())?;
    let extension = base_path
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .unwrap_or("wav");
    let mut used_names =
        HashSet::with_capacity(selected_tracks.len() + usize::from(include_master_mix));
    let mut outputs = Vec::with_capacity(selected_tracks.len() + usize::from(include_master_mix));
    if include_master_mix {
        used_names.insert(format!("{base_name}-master").to_lowercase());
        outputs.push((
            aaadaw_app::ProjectWavRenderSource::MasterMix,
            parent.join(format!("{base_name}-Master.{extension}")),
        ));
    }

    for (index, track_id) in selected_tracks.iter().enumerate() {
        let track = project
            .tracks()
            .iter()
            .find(|track| track.id() == *track_id)
            .ok_or_else(|| {
                format!(
                    "Selected render track {} no longer exists",
                    track_id.value()
                )
            })?;
        let mut track_name = safe_stem_component(track.name());
        if track_name.is_empty() {
            track_name = format!("Track-{}", index + 1);
        }
        let mut candidate = format!("{base_name}-{track_name}");
        if !used_names.insert(candidate.to_lowercase()) {
            candidate.push_str(&format!("-{}", index + 1));
            let mut disambiguator = 2;
            while !used_names.insert(candidate.to_lowercase()) {
                candidate = format!("{base_name}-{track_name}-{}-{disambiguator}", index + 1);
                disambiguator += 1;
            }
        }
        let source = match stem_mode {
            SelectedTrackStemMode::PostFader => {
                aaadaw_app::ProjectWavRenderSource::SelectedTrackStem(*track_id)
            }
            SelectedTrackStemMode::PreFader => {
                aaadaw_app::ProjectWavRenderSource::SelectedTrackStemPreFader(*track_id)
            }
            SelectedTrackStemMode::ViaMaster => {
                aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(*track_id)
            }
        };
        outputs.push((source, parent.join(format!("{candidate}.{extension}"))));
    }
    Ok(outputs)
}

fn project_region_render_jobs(
    base_path: &Path,
    project: &Project,
    source: RenderSource,
    selected_tracks: &[TrackId],
    stems_pre_fader: bool,
    settings: aaadaw_app::ProjectWavRenderSettings,
) -> Result<Vec<(aaadaw_app::ProjectWavRenderSettings, PathBuf)>, String> {
    let parent = base_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let base_name = base_path
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "Choose a WAV filename to use as the region prefix".to_owned())?;
    let extension = base_path
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .unwrap_or("wav");
    if project.regions().is_empty() {
        return Err("Add at least one project region before rendering all regions".to_owned());
    }

    let stem_mode = match source {
        RenderSource::SelectedTracksViaMaster => SelectedTrackStemMode::ViaMaster,
        RenderSource::SelectedTracksStems | RenderSource::MasterMixAndSelectedTracks
            if stems_pre_fader =>
        {
            SelectedTrackStemMode::PreFader
        }
        RenderSource::SelectedTracksStems | RenderSource::MasterMixAndSelectedTracks => {
            SelectedTrackStemMode::PostFader
        }
        RenderSource::MasterMix => SelectedTrackStemMode::PostFader,
    };
    let mut used_region_names = HashSet::with_capacity(project.regions().len());
    let mut jobs = Vec::new();
    for (index, region) in project.regions().iter().enumerate() {
        let start_sample = project
            .sample_at_tick(region.start_ticks())
            .map_err(|error| format!("Could not convert region start: {error}"))?;
        let end_sample = project
            .sample_at_tick(region.end_ticks())
            .map_err(|error| format!("Could not convert region end: {error}"))?;
        if end_sample <= start_sample {
            return Err(format!(
                "Project region {} must span at least one sample to render",
                region.number()
            ));
        }
        let mut region_name = safe_stem_component(region.name());
        if region_name.is_empty() {
            region_name = format!("Region-{}", region.number());
        }
        let initial_name = region_name.clone();
        let mut suffix = index + 1;
        while !used_region_names.insert(region_name.to_lowercase()) {
            region_name = format!("{initial_name}-{}", suffix);
            suffix += 1;
        }
        let region_base = parent.join(format!("{base_name}-{region_name}.{extension}"));
        let destinations = match source {
            RenderSource::MasterMix => {
                vec![(aaadaw_app::ProjectWavRenderSource::MasterMix, region_base)]
            }
            RenderSource::SelectedTracksStems | RenderSource::SelectedTracksViaMaster => {
                selected_track_stem_destinations(
                    &region_base,
                    project,
                    selected_tracks,
                    false,
                    stem_mode,
                )?
            }
            RenderSource::MasterMixAndSelectedTracks => selected_track_stem_destinations(
                &region_base,
                project,
                selected_tracks,
                true,
                stem_mode,
            )?,
        };
        for (render_source, destination) in destinations {
            let mut region_settings = settings;
            region_settings.source = render_source;
            region_settings.start_sample = start_sample;
            region_settings.end_sample = end_sample;
            jobs.push((region_settings, destination));
        }
    }
    Ok(jobs)
}

fn safe_stem_component(name: &str) -> String {
    name.trim()
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>()
        .trim_end_matches([' ', '.'])
        .to_owned()
}

impl RenderSampleRate {
    pub(crate) const ALL: [Self; 12] = [
        Self::Project,
        Self::Fixed(8_000),
        Self::Fixed(11_025),
        Self::Fixed(16_000),
        Self::Fixed(22_050),
        Self::Fixed(32_000),
        Self::Fixed(44_100),
        Self::Fixed(48_000),
        Self::Fixed(88_200),
        Self::Fixed(96_000),
        Self::Fixed(176_400),
        Self::Fixed(192_000),
    ];
}

impl std::fmt::Display for RenderSampleRate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Project => formatter.write_str("Project rate"),
            Self::Fixed(sample_rate) => write!(formatter, "{sample_rate} Hz"),
        }
    }
}

impl RenderBounds {
    pub(crate) const ALL: [Self; 3] = [
        Self::EntireProject,
        Self::TimeSelection,
        Self::AllProjectRegions,
    ];
}

impl std::fmt::Display for RenderBounds {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::EntireProject => "Entire project",
            Self::TimeSelection => "Time selection",
            Self::AllProjectRegions => "All project regions",
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct RenderDialogState {
    pub(super) bounds: RenderBounds,
    pub(super) source: RenderSource,
    pub(super) sample_rate: RenderSampleRate,
    pub(super) channels: WavChannelCount,
    pub(super) normalize_peak: bool,
    pub(super) normalize_peak_target_text: String,
    pub(super) tail_duration_text: String,
    pub(super) stems_pre_fader: bool,
}

#[derive(Debug)]
struct ResolvedRenderSettings {
    start_sample: u64,
    end_sample: u64,
    tail_milliseconds: u32,
    output_sample_rate: u32,
    selected_tracks: Vec<TrackId>,
    normalize_peak_target_centi_dbfs: i32,
}

impl Default for RenderDialogState {
    fn default() -> Self {
        Self {
            bounds: RenderBounds::EntireProject,
            source: RenderSource::MasterMix,
            sample_rate: RenderSampleRate::Project,
            channels: WavChannelCount::Stereo,
            normalize_peak: false,
            normalize_peak_target_text: "0".to_owned(),
            tail_duration_text: aaadaw_app::DEFAULT_EFFECT_TAIL_SECONDS.to_string(),
            stems_pre_fader: false,
        }
    }
}

#[derive(Clone)]
pub(super) struct OfflineRenderJob {
    kind: OfflineRenderKind,
    snapshot: Arc<ProjectSnapshot>,
    project_path: PathBuf,
    project_generation: u64,
    label: String,
}

#[derive(Clone)]
enum OfflineRenderKind {
    ProjectWav {
        destination: PathBuf,
        settings: aaadaw_app::ProjectWavRenderSettings,
    },
    ProjectWavStems {
        outputs: Vec<(aaadaw_app::ProjectWavRenderSource, PathBuf)>,
        settings: aaadaw_app::ProjectWavRenderSettings,
    },
    ProjectWavRegions {
        outputs: Vec<(aaadaw_app::ProjectWavRenderSettings, PathBuf)>,
    },
    FreezeTrack {
        track_id: TrackId,
    },
}

impl OfflineRenderJob {
    pub(super) fn label(&self) -> &str {
        &self.label
    }
}

impl App {
    pub(super) fn offline_job_submission_allowed(&self) -> bool {
        #[cfg(feature = "audio-device")]
        let recording_active =
            self.recording.is_some() || self.recording_starting || self.recording_stopping;
        #[cfg(not(feature = "audio-device"))]
        let recording_active = false;
        let active_freeze_owns_io = self
            .active_offline_job
            .as_ref()
            .is_some_and(|queued| matches!(queued.job.kind, OfflineRenderKind::FreezeTrack { .. }));
        !recording_active
            && !self.path_picker_busy
            && !self.import_busy
            && !self.audio_asset_management_busy
            && (!self.io_busy || active_freeze_owns_io)
    }

    pub(super) fn start_freeze_track(&mut self, track_id: TrackId) -> Task<Message> {
        if !self.offline_job_submission_allowed() {
            self.status = "Finish the current project operation before freezing a track".to_owned();
            return Task::none();
        }
        #[cfg(feature = "audio-device")]
        if self.recording.is_some() || self.recording_starting || self.recording_stopping {
            self.status = "Stop recording before freezing a track".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.media_store_path() else {
            self.status = "Temporary project media storage is unavailable".to_owned();
            return Task::none();
        };
        let track_name = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            .map_or_else(
                || format!("track {}", track_id.value()),
                |track| track.name().to_owned(),
            );
        let job = OfflineRenderJob {
            kind: OfflineRenderKind::FreezeTrack { track_id },
            snapshot: Arc::new(self.project.snapshot()),
            project_path,
            project_generation: self.project_generation,
            label: format!("Freeze {track_name}"),
        };
        self.enqueue_offline_job(job)
    }

    pub(super) fn start_offline_render(&mut self, destination: PathBuf) -> Task<Message> {
        #[cfg(feature = "audio-device")]
        if self.recording.is_some() || self.recording_starting || self.recording_stopping {
            self.status = "Stop recording before rendering audio".to_owned();
            return Task::none();
        }
        if !self.offline_job_submission_allowed() {
            self.status = "Wait for the current project operation to finish".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.media_store_path() else {
            self.status = "Temporary project media storage is unavailable".to_owned();
            return Task::none();
        };
        if destination == project_path {
            self.status = "Choose a WAV destination separate from the project file".to_owned();
            return Task::none();
        }
        let ResolvedRenderSettings {
            start_sample,
            end_sample,
            tail_milliseconds,
            output_sample_rate,
            selected_tracks,
            normalize_peak_target_centi_dbfs,
        } = match self.selected_render_settings() {
            Ok(settings) => settings,
            Err(error) => {
                self.status = error;
                return Task::none();
            }
        };

        let file_name = destination.file_name().map_or_else(
            || destination.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let source = match (self.render_dialog.source, selected_tracks.as_slice()) {
            (RenderSource::SelectedTracksStems, [track_id]) => {
                if self.render_dialog.stems_pre_fader {
                    aaadaw_app::ProjectWavRenderSource::SelectedTrackStemPreFader(*track_id)
                } else {
                    aaadaw_app::ProjectWavRenderSource::SelectedTrackStem(*track_id)
                }
            }
            (RenderSource::SelectedTracksViaMaster, [track_id]) => {
                aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(*track_id)
            }
            _ => aaadaw_app::ProjectWavRenderSource::MasterMix,
        };
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: self.audio_settings.master_output_ceiling,
            export_options: self.wav_export_options,
            output_sample_rate,
            output_channels: self.render_dialog.channels,
            source,
            normalize_peak_target_centi_dbfs,
            start_sample,
            end_sample,
            tail_milliseconds,
            normalize_peak: self.render_dialog.normalize_peak,
        };
        if self.render_dialog.bounds == RenderBounds::AllProjectRegions {
            if cfg!(target_os = "android") {
                self.status =
                    "Rendering project regions to multiple files requires a desktop build"
                        .to_owned();
                return Task::none();
            }
            let outputs = match project_region_render_jobs(
                &destination,
                &self.project,
                self.render_dialog.source,
                &selected_tracks,
                self.render_dialog.stems_pre_fader,
                settings,
            ) {
                Ok(outputs) => outputs,
                Err(error) => {
                    self.status = error;
                    return Task::none();
                }
            };
            if let Some((_, existing)) =
                outputs
                    .iter()
                    .find(|(_, path)| match std::fs::symlink_metadata(path) {
                        Ok(_) => true,
                        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
                    })
            {
                self.status = format!(
                    "Choose a new render prefix; region output already exists: {}",
                    existing.display()
                );
                return Task::none();
            }
            if outputs.iter().any(|(_, path)| path == &project_path) {
                self.status =
                    "Choose region output names separate from the project file".to_owned();
                return Task::none();
            }
            let label = format!("Render {} project regions", self.project.regions().len());
            let job = OfflineRenderJob {
                kind: OfflineRenderKind::ProjectWavRegions { outputs },
                snapshot: Arc::new(self.project.snapshot()),
                project_path,
                project_generation: self.project_generation,
                label,
            };
            return self.enqueue_offline_job(job);
        }
        let requires_multiple_files = self.render_requires_multiple_files();
        let (kind, label) = if requires_multiple_files {
            if cfg!(target_os = "android") {
                self.status = "The Android document picker supports one render file at a time; use a desktop build for stems".to_owned();
                return Task::none();
            }
            match std::fs::symlink_metadata(&destination) {
                Ok(_) => {
                    self.status =
                        "Choose a new filename for the stem prefix; that file already exists"
                            .to_owned();
                    return Task::none();
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    self.status = format!("Could not inspect stem prefix: {error}");
                    return Task::none();
                }
            }
            let outputs = match selected_track_stem_destinations(
                &destination,
                &self.project,
                &selected_tracks,
                self.render_dialog.source == RenderSource::MasterMixAndSelectedTracks,
                match self.render_dialog.source {
                    RenderSource::SelectedTracksViaMaster => SelectedTrackStemMode::ViaMaster,
                    RenderSource::SelectedTracksStems
                    | RenderSource::MasterMixAndSelectedTracks
                        if self.render_dialog.stems_pre_fader =>
                    {
                        SelectedTrackStemMode::PreFader
                    }
                    RenderSource::SelectedTracksStems
                    | RenderSource::MasterMixAndSelectedTracks => SelectedTrackStemMode::PostFader,
                    RenderSource::MasterMix => {
                        unreachable!("Master mix cannot request a multi-track output set")
                    }
                },
            ) {
                Ok(outputs) => outputs,
                Err(error) => {
                    self.status = error;
                    return Task::none();
                }
            };
            if outputs.iter().any(|(_, path)| path == &project_path) {
                self.status = "Choose stem output names separate from the project file".to_owned();
                return Task::none();
            }
            (
                OfflineRenderKind::ProjectWavStems { outputs, settings },
                if self.render_dialog.source == RenderSource::MasterMixAndSelectedTracks {
                    format!(
                        "Render Master mix and {} selected stems",
                        selected_tracks.len()
                    )
                } else {
                    format!("Render {} selected tracks as stems", selected_tracks.len())
                },
            )
        } else {
            (
                OfflineRenderKind::ProjectWav {
                    destination,
                    settings,
                },
                format!("Render {file_name}"),
            )
        };
        let job = OfflineRenderJob {
            kind,
            snapshot: Arc::new(self.project.snapshot()),
            project_path,
            project_generation: self.project_generation,
            label,
        };
        self.enqueue_offline_job(job)
    }

    fn enqueue_offline_job(&mut self, job: OfflineRenderJob) -> Task<Message> {
        match self.offline_job_queue.enqueue(job) {
            Ok(_) => {
                if self.offline_render_busy {
                    self.status = format!(
                        "Queued offline job ({} waiting)",
                        self.offline_job_queue.len()
                    );
                    Task::none()
                } else {
                    self.start_next_offline_job()
                }
            }
            Err(EnqueueError::Full) => {
                self.status = format!(
                    "Offline render queue is full (maximum {} waiting jobs)",
                    super::offline_job_queue::MAX_PENDING_OFFLINE_JOBS
                );
                Task::none()
            }
            Err(EnqueueError::IdExhausted) => {
                self.status = "No more offline render queue IDs are available".to_owned();
                Task::none()
            }
        }
    }

    pub(super) fn can_submit_render(&self) -> bool {
        if cfg!(target_os = "android") && self.render_requires_multiple_files() {
            return false;
        }
        if cfg!(target_os = "android")
            && self.render_dialog.bounds == RenderBounds::AllProjectRegions
        {
            return false;
        }
        self.selected_render_settings().is_ok()
    }

    pub(super) fn has_valid_render_source(&self) -> bool {
        match self.render_dialog.source {
            RenderSource::MasterMix => true,
            RenderSource::SelectedTracksStems
            | RenderSource::SelectedTracksViaMaster
            | RenderSource::MasterMixAndSelectedTracks => {
                !self.selected_tracks_for_render().is_empty()
            }
        }
    }

    pub(super) fn render_requires_multiple_files(&self) -> bool {
        if self.render_dialog.bounds == RenderBounds::AllProjectRegions {
            return true;
        }
        let outputs_per_range = match self.render_dialog.source {
            RenderSource::MasterMix => 1,
            RenderSource::SelectedTracksStems | RenderSource::SelectedTracksViaMaster => {
                self.selected_render_track_count()
            }
            RenderSource::MasterMixAndSelectedTracks => {
                self.selected_render_track_count().saturating_add(1)
            }
        };
        let region_count = if self.render_dialog.bounds == RenderBounds::AllProjectRegions {
            self.project.regions().len()
        } else {
            1
        };
        region_count.saturating_mul(outputs_per_range) > 1
    }

    pub(super) fn selected_render_track_count(&self) -> usize {
        self.selected_tracks_for_render().len()
    }

    pub(super) fn render_source_description(&self) -> &'static str {
        match self.render_dialog.source {
            RenderSource::MasterMix => "Renders the project through the Master mix.",
            RenderSource::SelectedTracksStems => {
                "Renders each selected track as a separate stem without the Master mix gain."
            }
            RenderSource::SelectedTracksViaMaster => {
                "Renders each selected track through the Master as its own WAV stem."
            }
            RenderSource::MasterMixAndSelectedTracks => {
                "Renders one Master mix WAV plus one stem for each selected track."
            }
        }
    }

    pub(super) fn can_render_stems_pre_fader(&self) -> bool {
        matches!(
            self.render_dialog.source,
            RenderSource::SelectedTracksStems | RenderSource::MasterMixAndSelectedTracks
        )
    }

    pub(super) fn render_output_sample_rate(&self) -> u32 {
        match self.render_dialog.sample_rate {
            RenderSampleRate::Project => self.project.settings().sample_rate(),
            RenderSampleRate::Fixed(sample_rate) => sample_rate,
        }
    }

    fn selected_render_settings(&self) -> Result<ResolvedRenderSettings, String> {
        let tail_milliseconds =
            parse_render_tail_milliseconds(&self.render_dialog.tail_duration_text)
                .map_err(str::to_owned)?;
        let (start_sample, end_sample) = self.selected_render_bounds()?;
        aaadaw_app::render_range_length_samples_with_tail_milliseconds(
            self.project.settings().sample_rate(),
            start_sample,
            end_sample,
            tail_milliseconds,
        )
        .map_err(|error| format!("Render range is empty or too large: {error}"))?;
        let selected_tracks = match self.render_dialog.source {
            RenderSource::MasterMix => Vec::new(),
            RenderSource::SelectedTracksStems
            | RenderSource::SelectedTracksViaMaster
            | RenderSource::MasterMixAndSelectedTracks => {
                let selected_tracks = self.selected_tracks_for_render();
                if selected_tracks.is_empty() {
                    return Err("Select at least one track in Arrange to render stems".to_owned());
                }
                selected_tracks
            }
        };
        let normalize_peak_target_centi_dbfs = if self.render_dialog.normalize_peak {
            parse_render_peak_target_centi_dbfs(&self.render_dialog.normalize_peak_target_text)
                .map_err(str::to_owned)?
        } else {
            0
        };
        Ok(ResolvedRenderSettings {
            start_sample,
            end_sample,
            tail_milliseconds,
            output_sample_rate: self.render_output_sample_rate(),
            selected_tracks,
            normalize_peak_target_centi_dbfs,
        })
    }

    fn selected_tracks_for_render(&self) -> Vec<TrackId> {
        if self.timeline.selected_tracks.is_empty() {
            return self
                .timeline
                .selected_track
                .filter(|selected| {
                    self.project
                        .tracks()
                        .iter()
                        .any(|track| track.id() == *selected)
                })
                .into_iter()
                .collect();
        }
        self.project
            .tracks()
            .iter()
            .filter(|track| self.timeline.selected_tracks.contains(&track.id()))
            .map(|track| track.id())
            .collect()
    }

    fn selected_render_bounds(&self) -> Result<(u64, u64), String> {
        match self.render_dialog.bounds {
            RenderBounds::EntireProject => aaadaw_app::project_content_end_sample(&self.project)
                .map(|end_sample| (0, end_sample))
                .map_err(|error| format!("Could not determine render bounds: {error}")),
            RenderBounds::TimeSelection => {
                let Some(selection) = self.timeline.time_selection else {
                    return Err("Create a time selection in Arrange before rendering it".to_owned());
                };
                let start_sample = self
                    .project
                    .sample_at_tick(selection.start_tick)
                    .map_err(|error| format!("Could not convert render start: {error}"))?;
                let end_sample = self
                    .project
                    .sample_at_tick(selection.end_tick)
                    .map_err(|error| format!("Could not convert render end: {error}"))?;
                if end_sample <= start_sample {
                    return Err("Time selection must span at least one project sample".to_owned());
                }
                Ok((start_sample, end_sample))
            }
            RenderBounds::AllProjectRegions => {
                let Some(first) = self.project.regions().first() else {
                    return Err(
                        "Add at least one project region before rendering all regions".to_owned(),
                    );
                };
                let last = self
                    .project
                    .regions()
                    .last()
                    .expect("a first region guarantees a last region");
                let start_sample = self
                    .project
                    .sample_at_tick(first.start_ticks())
                    .map_err(|error| format!("Could not convert first region start: {error}"))?;
                let end_sample = self
                    .project
                    .sample_at_tick(last.end_ticks())
                    .map_err(|error| format!("Could not convert last region end: {error}"))?;
                if end_sample <= start_sample {
                    return Err("Project regions must span at least one project sample".to_owned());
                }
                Ok((start_sample, end_sample))
            }
        }
    }

    fn start_next_offline_job(&mut self) -> Task<Message> {
        if !self.offline_job_queue.is_empty() && !self.offline_job_submission_allowed() {
            self.status = "Offline jobs are waiting for the current project operation".to_owned();
            return Task::none();
        }
        while let Some(queued) = self.offline_job_queue.pop_front() {
            let writes_project_file = match &queued.job.kind {
                OfflineRenderKind::ProjectWav { destination, .. } => {
                    destination == &queued.job.project_path
                }
                OfflineRenderKind::ProjectWavStems { outputs, .. } => outputs
                    .iter()
                    .any(|(_, destination)| destination == &queued.job.project_path),
                OfflineRenderKind::ProjectWavRegions { outputs } => outputs
                    .iter()
                    .any(|(_, destination)| destination == &queued.job.project_path),
                OfflineRenderKind::FreezeTrack { .. } => false,
            };
            if writes_project_file {
                self.record_offline_job_result(format!(
                    "{} failed: destination is the project file",
                    queued.job.label
                ));
                continue;
            }

            let job = &queued.job;
            let snapshot = Arc::clone(&job.snapshot);
            let project_path = job.project_path.clone();
            let worker_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let cancel = Arc::clone(&worker_cancel);
            let progress = Arc::new(Mutex::new((0, 1)));
            let worker_progress = Arc::clone(&progress);
            self.offline_render_busy = true;
            self.offline_render_is_freeze =
                matches!(job.kind, OfflineRenderKind::FreezeTrack { .. });
            self.io_busy = self.offline_render_is_freeze;
            self.offline_render_cancel = Some(cancel);
            self.offline_render_progress = Some(progress);
            self.status = format!("{}…", job.label);
            let task = match &job.kind {
                OfflineRenderKind::ProjectWav {
                    destination,
                    settings,
                } => {
                    let destination = destination.clone();
                    let result_path = destination.clone();
                    let settings = *settings;
                    Task::perform(
                        run_blocking("aaadaw-offline-render", move || {
                            let project = Project::from_snapshot((*snapshot).clone())
                                .map_err(|error| error.to_string())?;
                            aaadaw_app::render_project_file_to_wav_with_settings(
                                project_path,
                                &project,
                                &destination,
                                settings,
                                &worker_cancel,
                                |done, total| {
                                    *worker_progress
                                        .lock()
                                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                        (done, total);
                                },
                            )
                            .map_err(|error| error.to_string())?;
                            #[cfg(target_os = "android")]
                            crate::android_platform::sync_saf_document(&destination)?;
                            Ok(result_path)
                        }),
                        Message::OfflineRenderFinished,
                    )
                }
                OfflineRenderKind::ProjectWavStems { outputs, settings } => {
                    let outputs = outputs.clone();
                    let settings = *settings;
                    let result_path = outputs
                        .first()
                        .map(|(_, path)| path.clone())
                        .unwrap_or_default();
                    Task::perform(
                        run_blocking("aaadaw-offline-stems", move || {
                            let project = Project::from_snapshot((*snapshot).clone())
                                .map_err(|error| error.to_string())?;
                            aaadaw_app::render_project_wav_sources_to_wavs(
                                project_path,
                                &project,
                                &outputs,
                                settings,
                                &worker_cancel,
                                |done, total| {
                                    *worker_progress
                                        .lock()
                                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                        (done, total);
                                },
                            )
                            .map_err(|error| error.to_string())?;
                            Ok(result_path)
                        }),
                        Message::OfflineRenderFinished,
                    )
                }
                OfflineRenderKind::ProjectWavRegions { outputs } => {
                    let outputs = outputs.clone();
                    let result_path = outputs
                        .first()
                        .map(|(_, path)| path.clone())
                        .unwrap_or_default();
                    Task::perform(
                        run_blocking("aaadaw-offline-regions", move || {
                            let project = Project::from_snapshot((*snapshot).clone())
                                .map_err(|error| error.to_string())?;
                            aaadaw_app::render_project_wav_jobs_to_wavs(
                                project_path,
                                &project,
                                &outputs,
                                &worker_cancel,
                                |done, total| {
                                    *worker_progress
                                        .lock()
                                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                        (done, total);
                                },
                            )
                            .map_err(|error| error.to_string())?;
                            Ok(result_path)
                        }),
                        Message::OfflineRenderFinished,
                    )
                }
                OfflineRenderKind::FreezeTrack { track_id } => {
                    let track_id = *track_id;
                    Task::perform(
                        run_blocking("aaadaw-track-freeze", move || {
                            let project = Project::from_snapshot((*snapshot).clone())
                                .map_err(|error| error.to_string())?;
                            let tempdir = tempfile::tempdir().map_err(|error| error.to_string())?;
                            let wav_path = tempdir.path().join("freeze.wav");
                            let render = aaadaw_app::render_freeze_track_to_float32_wav(
                                &project_path,
                                &project,
                                track_id,
                                &wav_path,
                                &worker_cancel,
                                |done, total| {
                                    *worker_progress
                                        .lock()
                                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                        (done, total);
                                },
                            )
                            .map_err(|error| error.to_string())?;
                            if worker_cancel.load(Ordering::Acquire) {
                                return Err("freeze was cancelled".to_owned());
                            }
                            let mut store = ProjectStore::open(&project_path)
                                .map_err(|error| error.to_string())?;
                            let media_ref = store
                                .import_audio_file(&wav_path)
                                .map_err(|error| error.to_string())?;
                            if worker_cancel.load(Ordering::Acquire) {
                                store
                                    .remove_unreferenced_audio_asset(&media_ref)
                                    .map_err(|error| error.to_string())?;
                                store.close().map_err(|error| error.to_string())?;
                                return Err("freeze was cancelled".to_owned());
                            }
                            store.close().map_err(|error| error.to_string())?;
                            Ok(FreezeTrackResult {
                                track_id,
                                media_ref,
                                start_sample: render.start_sample,
                                length_samples: render.length_samples,
                            })
                        }),
                        Message::FreezeTrackFinished,
                    )
                }
            };
            self.active_offline_job = Some(queued);
            return task;
        }
        self.active_offline_job = None;
        self.offline_render_busy = false;
        self.offline_render_is_freeze = false;
        self.io_busy = false;
        self.offline_render_cancel = None;
        self.offline_render_progress = None;
        Task::none()
    }

    pub(super) fn resume_offline_job_queue(&mut self) -> Task<Message> {
        if self.offline_render_busy
            || self.offline_job_queue.is_empty()
            || !self.offline_job_submission_allowed()
        {
            return Task::none();
        }
        self.start_next_offline_job()
    }

    pub(super) fn remove_queued_offline_job(&mut self, id: u64) {
        if let Some(job) = self.offline_job_queue.remove(OfflineJobId::from_value(id)) {
            self.record_offline_job_result(format!("Removed queued {}", job.label));
            self.status = format!("Removed queued {}", job.label);
        }
    }

    pub(super) fn cancel_offline_render(&mut self) {
        if let Some(cancel) = &self.offline_render_cancel {
            cancel.store(true, Ordering::Release);
            self.status = "Cancelling active offline job…".to_owned();
        }
    }

    pub(super) fn finish_freeze_track(
        &mut self,
        result: Result<FreezeTrackResult, String>,
    ) -> Task<Message> {
        let Some(active) = self.active_offline_job.take() else {
            return Task::none();
        };
        let was_cancelled = self
            .offline_render_cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Acquire));
        self.clear_active_offline_job();
        let OfflineRenderKind::FreezeTrack { track_id } = &active.job.kind else {
            self.record_offline_job_result(format!(
                "{} failed: unexpected completion",
                active.job.label
            ));
            self.status = "Offline job returned an unexpected completion type".to_owned();
            return self.start_next_offline_job();
        };
        if was_cancelled {
            let cleanup = result
                .as_ref()
                .map(|frozen| remove_freeze_media(&active.job.project_path, &frozen.media_ref))
                .unwrap_or(Ok(()));
            let message = match cleanup {
                Ok(()) => format!("{} cancelled", active.job.label),
                Err(error) => format!(
                    "{} cancelled; unused media cleanup failed: {error}",
                    active.job.label
                ),
            };
            self.record_offline_job_result(message.clone());
            self.status = message;
            return self.start_next_offline_job();
        }
        let FreezeTrackResult {
            track_id: result_track_id,
            media_ref,
            start_sample,
            length_samples,
        } = match result {
            Ok(frozen) => frozen,
            Err(error) => {
                let message = if error.contains("cancelled") {
                    format!("{} cancelled", active.job.label)
                } else {
                    format!("{} failed: {error}", active.job.label)
                };
                self.record_offline_job_result(message.clone());
                self.status = message;
                return self.start_next_offline_job();
            }
        };
        if result_track_id != *track_id
            || self.project_generation != active.job.project_generation
            || self.media_store_path().as_ref() != Some(&active.job.project_path)
            || !freeze_source_matches(&self.project.snapshot(), &active.job.snapshot, *track_id)
        {
            let cleanup = remove_freeze_media(&active.job.project_path, &media_ref);
            let message = match cleanup {
                Ok(()) => format!(
                    "{} skipped: its source or project changed since submission",
                    active.job.label
                ),
                Err(error) => format!(
                    "{} skipped because its source changed; unused media cleanup failed: {error}",
                    active.job.label
                ),
            };
            self.record_offline_job_result(message.clone());
            self.status = message;
            return self.start_next_offline_job();
        }

        let previous_revision = self.revision;
        self.apply_action(
            DawAction::FreezeTrack {
                track_id: *track_id,
                media_ref: media_ref.clone(),
                start_sample,
                length_samples,
            },
            "Track frozen; source remains available for unfreeze",
        );
        if self.revision == previous_revision {
            let cleanup = remove_freeze_media(&active.job.project_path, &media_ref);
            if let Err(error) = cleanup {
                self.status = format!(
                    "{}; unused freeze media cleanup failed: {error}",
                    self.status
                );
            }
            self.record_offline_job_result(format!(
                "{} failed: project rejected the freeze",
                active.job.label
            ));
        } else {
            self.record_offline_job_result(format!("{} complete", active.job.label));
        }
        self.start_next_offline_job()
    }

    pub(super) fn finish_offline_render(
        &mut self,
        result: Result<PathBuf, String>,
    ) -> Task<Message> {
        let Some(active) = self.active_offline_job.take() else {
            return Task::none();
        };
        self.clear_active_offline_job();
        let message = match result {
            Ok(path) => format!("WAV render complete: {}", path.display()),
            Err(error) if error.contains("cancelled") => {
                "WAV render cancelled; no output file was published".to_owned()
            }
            Err(error) => format!("WAV render failed: {error}"),
        };
        self.record_offline_job_result(format!("{}: {message}", active.job.label));
        self.status = message;
        self.start_next_offline_job()
    }

    pub(super) fn update_offline_render_progress(&mut self) {
        if !self.offline_render_busy {
            return;
        }
        let Some(progress) = &self.offline_render_progress else {
            return;
        };
        let (done, total) = *progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(percent) = done.saturating_mul(100).checked_div(total) {
            let operation = if self.offline_render_is_freeze {
                "Freezing track"
            } else {
                "Rendering offline"
            };
            let label = self
                .active_offline_job
                .as_ref()
                .map_or(operation, |job| job.job.label());
            self.status = format!("{label}… {percent}% ({done}/{total} frames)");
        }
    }

    fn clear_active_offline_job(&mut self) {
        self.offline_render_busy = false;
        self.offline_render_is_freeze = false;
        self.io_busy = false;
        self.offline_render_cancel = None;
        self.offline_render_progress = None;
    }

    fn record_offline_job_result(&mut self, message: String) {
        self.offline_job_history.push_front(message);
        self.offline_job_history.truncate(JOB_HISTORY_CAPACITY);
    }
}

fn remove_freeze_media(project_path: &std::path::Path, media_ref: &str) -> Result<(), String> {
    let mut store = ProjectStore::open(project_path).map_err(|error| error.to_string())?;
    let cleanup = store
        .remove_unreferenced_audio_asset(media_ref)
        .map_err(|error| error.to_string());
    let close = store.close().map_err(|error| error.to_string());
    cleanup.and(close)
}

fn freeze_source_matches(
    current: &ProjectSnapshot,
    captured: &ProjectSnapshot,
    track_id: TrackId,
) -> bool {
    let track_value = track_id.value();
    current.settings == captured.settings
        && current.tempo_points == captured.tempo_points
        && current.meter_points == captured.meter_points
        && current.tracks.iter().find(|track| track.id == track_value)
            == captured.tracks.iter().find(|track| track.id == track_value)
        && current
            .midi_items
            .iter()
            .filter(|item| item.track_id == track_value)
            .eq(captured
                .midi_items
                .iter()
                .filter(|item| item.track_id == track_value))
}

#[cfg(test)]
mod tests {
    use super::super::offline_job_queue::OfflineJobQueue;
    use super::super::offline_job_queue::QueuedOfflineJob;
    use super::*;
    use aaadaw_core::DawAction;

    fn wav_job(label: &str, destination: &str) -> OfflineRenderJob {
        OfflineRenderJob {
            kind: OfflineRenderKind::ProjectWav {
                destination: PathBuf::from(destination),
                settings: aaadaw_app::ProjectWavRenderSettings {
                    master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
                    export_options: aaadaw_app::WavExportOptions::default(),
                    output_sample_rate: Project::default().settings().sample_rate(),
                    output_channels: WavChannelCount::Stereo,
                    source: aaadaw_app::ProjectWavRenderSource::MasterMix,
                    start_sample: 0,
                    end_sample: 0,
                    tail_milliseconds: aaadaw_app::DEFAULT_EFFECT_TAIL_SECONDS * 1_000,
                    normalize_peak: false,
                    normalize_peak_target_centi_dbfs: 0,
                },
            },
            snapshot: Arc::new(Project::default().snapshot()),
            project_path: PathBuf::from("session.aaadaw"),
            project_generation: 0,
            label: label.to_owned(),
        }
    }

    #[test]
    fn cancelling_a_render_requests_worker_cancellation() {
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut app = App {
            offline_render_busy: true,
            offline_render_cancel: Some(Arc::clone(&cancel)),
            ..App::default()
        };
        app.cancel_offline_render();
        assert!(cancel.load(Ordering::Acquire));
        assert_eq!(app.status, "Cancelling active offline job…");
    }

    #[test]
    fn background_progress_updates_the_visible_job_status() {
        let progress = Arc::new(Mutex::new((25, 100)));
        let mut app = App {
            offline_render_busy: true,
            offline_render_progress: Some(progress),
            ..App::default()
        };
        app.update_offline_render_progress();
        assert_eq!(app.status, "Rendering offline… 25% (25/100 frames)");
    }

    #[test]
    fn queue_state_is_empty_by_default() {
        let queue: OfflineJobQueue<&str> = OfflineJobQueue::default();
        assert!(queue.is_empty());
    }

    #[test]
    fn render_source_menu_keeps_master_and_stem_sources_grouped() {
        assert_eq!(
            RenderSource::ALL,
            [
                RenderSource::MasterMix,
                RenderSource::SelectedTracksStems,
                RenderSource::MasterMixAndSelectedTracks,
                RenderSource::SelectedTracksViaMaster,
            ]
        );
        assert_eq!(
            RenderSource::SelectedTracksStems.to_string(),
            "Selected tracks (stems)"
        );
        assert_eq!(
            RenderSource::MasterMixAndSelectedTracks.to_string(),
            "Master mix and stems"
        );
        assert_eq!(
            RenderSource::SelectedTracksViaMaster.to_string(),
            "Selected tracks via master"
        );
    }

    #[test]
    fn pre_fader_stem_control_is_available_only_for_stem_sources() {
        let mut app = App::default();
        assert!(!app.can_render_stems_pre_fader());
        let _task = app.update(Message::SetRenderStemsPreFader(true));
        assert!(!app.render_dialog.stems_pre_fader);

        app.render_dialog.source = RenderSource::SelectedTracksStems;
        assert!(app.can_render_stems_pre_fader());
        let _task = app.update(Message::SetRenderStemsPreFader(true));
        assert!(app.render_dialog.stems_pre_fader);

        app.render_dialog.source = RenderSource::SelectedTracksViaMaster;
        assert!(!app.can_render_stems_pre_fader());
        let _task = app.update(Message::SetRenderStemsPreFader(false));
        assert!(app.render_dialog.stems_pre_fader);

        app.render_dialog.source = RenderSource::MasterMixAndSelectedTracks;
        assert!(app.can_render_stems_pre_fader());
    }

    #[test]
    fn render_bounds_resolve_the_current_time_selection_to_project_samples() {
        let mut app = App::default();
        app.render_dialog.bounds = RenderBounds::TimeSelection;
        assert!(!app.can_submit_render());
        assert!(
            app.selected_render_bounds()
                .expect_err("a selection is required")
                .contains("Create a time selection")
        );

        app.timeline.time_selection = Some(crate::timeline::TimeSelection {
            start_tick: 960,
            end_tick: 2_880,
        });
        assert_eq!(
            app.selected_render_bounds()
                .expect("selection should convert on the project sample clock"),
            (24_000, 72_000)
        );
        assert!(app.can_submit_render());

        app.render_dialog.sample_rate = RenderSampleRate::Fixed(44_100);
        assert_eq!(
            app.render_output_sample_rate(),
            44_100,
            "fixed render rate should be resolved independently of the project rate"
        );

        app.render_dialog.tail_duration_text = "two".to_owned();
        assert!(!app.can_submit_render());

        app.render_dialog.bounds = RenderBounds::EntireProject;
        app.render_dialog.tail_duration_text = "0".to_owned();
        assert!(!app.can_submit_render());
    }

    #[test]
    fn render_all_regions_freezes_each_region_range_and_output_name() {
        let mut project = Project::default();
        project
            .apply(DawAction::InsertProjectRegion {
                start_ticks: 960,
                end_ticks: 2_880,
                name: "Verse/Part".to_owned(),
            })
            .expect("first region should be inserted");
        project
            .apply(DawAction::InsertProjectRegion {
                start_ticks: 3_840,
                end_ticks: 4_800,
                name: "Verse/Part".to_owned(),
            })
            .expect("second region should be inserted");
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Lead".to_owned(),
            })
            .expect("render source track should be created");
        let track_id = project.tracks()[0].id();
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: project.settings().sample_rate(),
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 120_000,
            tail_milliseconds: 250,
            normalize_peak: true,
            normalize_peak_target_centi_dbfs: -602,
        };
        let base = std::env::temp_dir().join("aaadaw-region-prefix.wav");

        let outputs = project_region_render_jobs(
            &base,
            &project,
            RenderSource::SelectedTracksViaMaster,
            &[track_id],
            false,
            settings,
        )
        .expect("each project region should make a stem render job");

        assert_eq!(outputs.len(), 2);
        assert_eq!(
            outputs[0],
            (
                aaadaw_app::ProjectWavRenderSettings {
                    source: aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(track_id),
                    start_sample: 24_000,
                    end_sample: 72_000,
                    ..settings
                },
                std::env::temp_dir().join("aaadaw-region-prefix-Verse_Part-Lead.wav")
            )
        );
        assert_eq!(
            outputs[1],
            (
                aaadaw_app::ProjectWavRenderSettings {
                    source: aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(track_id),
                    start_sample: 96_000,
                    end_sample: 120_000,
                    ..settings
                },
                std::env::temp_dir().join("aaadaw-region-prefix-Verse_Part-2-Lead.wav")
            )
        );
        assert!(outputs.iter().all(|(settings, _)| {
            settings.tail_milliseconds == 250
                && settings.normalize_peak
                && settings.normalize_peak_target_centi_dbfs == -602
        }));

        let mut app = App {
            project,
            project_path: Some(std::env::temp_dir().join(format!(
                "aaadaw-region-queued-{}.aaadaw",
                std::process::id()
            ))),
            render_dialog: RenderDialogState {
                bounds: RenderBounds::AllProjectRegions,
                ..RenderDialogState::default()
            },
            offline_render_busy: true,
            ..App::default()
        };
        assert_eq!(app.selected_render_bounds(), Ok((24_000, 120_000)));
        assert!(app.render_requires_multiple_files());
        assert!(app.can_submit_render());
        app.render_dialog.source = RenderSource::SelectedTracksViaMaster;
        app.timeline.selected_track = Some(track_id);
        app.timeline.selected_tracks.insert(track_id);
        let output_prefix = std::env::temp_dir().join(format!(
            "aaadaw-queued-region-prefix-{}.wav",
            std::process::id()
        ));
        let _task = app.start_offline_render(output_prefix);
        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("all project regions should queue as one grouped job");
        let OfflineRenderKind::ProjectWavRegions { outputs } = queued.job.kind else {
            panic!("All project regions should use the bounded multi-file worker")
        };
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].0.start_sample, 24_000);
        assert_eq!(outputs[0].0.end_sample, 72_000);
        assert_eq!(outputs[1].0.start_sample, 96_000);
        assert_eq!(outputs[1].0.end_sample, 120_000);
        assert!(outputs.iter().all(|(settings, _)| {
            settings.source == aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(track_id)
        }));

        app.project = Project::default();
        assert!(
            app.selected_render_bounds()
                .expect_err("rendering regions requires at least one region")
                .contains("Add at least one project region")
        );
        assert!(!app.can_submit_render());
    }

    #[test]
    fn selected_track_render_accepts_one_or_more_valid_track_selections_in_track_order() {
        let mut app = App::default();
        app.render_dialog.source = RenderSource::SelectedTracksViaMaster;
        app.render_dialog.bounds = RenderBounds::TimeSelection;
        app.timeline.time_selection = Some(crate::timeline::TimeSelection {
            start_tick: 0,
            end_tick: 960,
        });
        assert!(!app.has_valid_render_source());
        assert!(
            app.selected_render_settings()
                .expect_err("a selected track is required")
                .contains("Select at least one track")
        );

        app.project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "First".to_owned(),
            })
            .expect("first track should be created");
        app.project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "Second".to_owned(),
            })
            .expect("second track should be created");
        let first = app.project.tracks()[0].id();
        let second = app.project.tracks()[1].id();
        app.timeline.selected_track = Some(first);
        app.timeline.selected_tracks.insert(first);
        assert!(app.has_valid_render_source());
        assert_eq!(
            app.selected_render_settings()
                .expect("one selected track should be renderable")
                .selected_tracks,
            vec![first]
        );

        app.timeline.selected_tracks.insert(second);
        assert!(app.has_valid_render_source());
        assert_eq!(app.selected_render_track_count(), 2);
        assert_eq!(
            app.selected_render_settings()
                .expect("multiple selected tracks should render as stems")
                .selected_tracks,
            vec![first, second],
            "stem order follows Arrange track order"
        );
    }

    #[test]
    fn selected_track_stem_names_use_track_names_and_disambiguate_duplicates() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Lead Vox".to_owned(),
            })
            .expect("first track should be created");
        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "Lead Vox".to_owned(),
            })
            .expect("second track should be created");
        let ids = project
            .tracks()
            .iter()
            .map(|track| track.id())
            .collect::<Vec<_>>();
        let base = std::env::temp_dir().join("aaadaw-stem-prefix.wav");

        let outputs = selected_track_stem_destinations(
            &base,
            &project,
            &ids,
            false,
            SelectedTrackStemMode::ViaMaster,
        )
        .expect("selected tracks should receive distinct output names");
        assert_eq!(
            outputs[0].1,
            std::env::temp_dir().join("aaadaw-stem-prefix-Lead Vox.wav")
        );
        assert_eq!(
            outputs[1].1,
            std::env::temp_dir().join("aaadaw-stem-prefix-Lead Vox-2.wav")
        );
        assert!(matches!(
            outputs[0].0,
            aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(_)
        ));
        let plain_stems = selected_track_stem_destinations(
            &base,
            &project,
            &ids,
            false,
            SelectedTrackStemMode::PostFader,
        )
        .expect("plain selected stems should keep the selected track source");
        assert!(matches!(
            plain_stems[0].0,
            aaadaw_app::ProjectWavRenderSource::SelectedTrackStem(_)
        ));
        let pre_fader_stems = selected_track_stem_destinations(
            &base,
            &project,
            &ids,
            false,
            SelectedTrackStemMode::PreFader,
        )
        .expect("pre-fader stems should keep the selected track source");
        assert!(matches!(
            pre_fader_stems[0].0,
            aaadaw_app::ProjectWavRenderSource::SelectedTrackStemPreFader(_)
        ));
        let combined = selected_track_stem_destinations(
            &base,
            &project,
            &ids,
            true,
            SelectedTrackStemMode::PostFader,
        )
        .expect("Master mix plus stems should receive distinct output names");
        assert_eq!(
            combined[0],
            (
                aaadaw_app::ProjectWavRenderSource::MasterMix,
                std::env::temp_dir().join("aaadaw-stem-prefix-Master.wav")
            )
        );
        assert_eq!(combined.len(), 3);
        assert_eq!(
            combined[1].1,
            std::env::temp_dir().join("aaadaw-stem-prefix-Lead Vox.wav")
        );
    }

    #[test]
    fn render_tail_parses_decimal_seconds_to_exact_milliseconds() {
        for (input, expected) in [
            ("2", 2_000),
            ("1.2", 1_200),
            ("0.125", 125),
            (".5", 500),
            ("1.", 1_000),
            (" 3.125 ", 3_125),
        ] {
            assert_eq!(parse_render_tail_milliseconds(input), Ok(expected));
        }
        for invalid in ["", ".", "-1", "1.0001", "1..2", "1,5", "4294967.296"] {
            assert!(
                parse_render_tail_milliseconds(invalid).is_err(),
                "{invalid:?} must be rejected"
            );
        }
    }

    #[test]
    fn render_peak_target_parses_exact_dbfs_to_hundredths() {
        for (input, expected) in [
            ("0", 0),
            ("-0.5", -50),
            ("-6", -600),
            ("-6.02", -602),
            ("-60", -6_000),
            ("-60.00", -6_000),
        ] {
            assert_eq!(parse_render_peak_target_centi_dbfs(input), Ok(expected));
        }
        for invalid in ["", ".", "-", "1", "-60.01", "-1.001", "--1", "NaN"] {
            assert!(
                parse_render_peak_target_centi_dbfs(invalid).is_err(),
                "{invalid:?} must be rejected"
            );
        }
    }

    #[test]
    fn queued_render_keeps_its_original_range_and_tail_after_arrange_changes() {
        let project_path = std::env::temp_dir().join("aaadaw-render-snapshot.aaadaw");
        let mut app = App {
            project_path: Some(project_path.clone()),
            offline_render_busy: true,
            ..App::default()
        };
        app.project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Selected render source".to_owned(),
            })
            .expect("render source track should be created");
        let selected_track_id = app.project.tracks()[0].id();
        app.timeline.selected_track = Some(selected_track_id);
        app.timeline.selected_tracks.insert(selected_track_id);
        app.render_dialog.bounds = RenderBounds::TimeSelection;
        app.render_dialog.source = RenderSource::SelectedTracksViaMaster;
        app.render_dialog.sample_rate = RenderSampleRate::Fixed(44_100);
        app.render_dialog.channels = WavChannelCount::Mono;
        app.render_dialog.normalize_peak = true;
        app.render_dialog.normalize_peak_target_text = " -6.02 ".to_owned();
        app.render_dialog.tail_duration_text = "3.125".to_owned();
        app.timeline.time_selection = Some(crate::timeline::TimeSelection {
            start_tick: 960,
            end_tick: 2_880,
        });

        let destination = std::env::temp_dir().join("aaadaw-render-snapshot.wav");
        let _task = app.start_offline_render(destination);
        assert_eq!(app.offline_job_queue.len(), 1);
        app.timeline.time_selection = Some(crate::timeline::TimeSelection {
            start_tick: 0,
            end_tick: 960,
        });
        app.render_dialog.sample_rate = RenderSampleRate::Project;
        app.render_dialog.source = RenderSource::MasterMix;
        app.render_dialog.channels = WavChannelCount::Stereo;
        app.render_dialog.normalize_peak = false;
        app.render_dialog.normalize_peak_target_text = "-12.0".to_owned();
        app.render_dialog.tail_duration_text = "0".to_owned();

        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("queued job should remain available while another render runs");
        match queued.job.kind {
            OfflineRenderKind::ProjectWav { settings, .. } => {
                assert_eq!(settings.start_sample, 24_000);
                assert_eq!(settings.end_sample, 72_000);
                assert_eq!(settings.tail_milliseconds, 3_125);
                assert_eq!(settings.output_sample_rate, 44_100);
                assert_eq!(settings.output_channels, WavChannelCount::Mono);
                assert_eq!(settings.normalize_peak_target_centi_dbfs, -602);
                assert_eq!(
                    settings.source,
                    aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(selected_track_id)
                );
                assert!(settings.normalize_peak);
            }
            OfflineRenderKind::ProjectWavStems { .. } => {
                panic!("one selected track should keep the single-file render path")
            }
            OfflineRenderKind::ProjectWavRegions { .. } => {
                panic!("entire-project bounds should keep the single-file render path")
            }
            OfflineRenderKind::FreezeTrack { .. } => panic!("expected WAV render"),
        }
    }

    #[test]
    fn queued_single_selected_stem_keeps_the_plain_track_output_source() {
        let project_path = std::env::temp_dir().join("aaadaw-render-plain-stem.aaadaw");
        let mut app = App {
            project_path: Some(project_path),
            offline_render_busy: true,
            ..App::default()
        };
        app.project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Selected source".to_owned(),
            })
            .expect("render source track should be created");
        let selected = app.project.tracks()[0].id();
        app.timeline.selected_track = Some(selected);
        app.timeline.selected_tracks.insert(selected);
        app.render_dialog.source = RenderSource::SelectedTracksStems;

        let _task = app.start_offline_render(std::env::temp_dir().join("plain-stem.wav"));
        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("single selected stem should be queued");
        match queued.job.kind {
            OfflineRenderKind::ProjectWav { settings, .. } => assert_eq!(
                settings.source,
                aaadaw_app::ProjectWavRenderSource::SelectedTrackStem(selected)
            ),
            OfflineRenderKind::ProjectWavStems { .. } => {
                panic!("one selected stem should keep the single-file path")
            }
            OfflineRenderKind::ProjectWavRegions { .. } => {
                panic!("entire-project bounds should keep the single-file path")
            }
            OfflineRenderKind::FreezeTrack { .. } => panic!("expected a WAV render"),
        }

        app.render_dialog.stems_pre_fader = true;
        let _task = app.start_offline_render(std::env::temp_dir().join("plain-stem-prefader.wav"));
        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("pre-fader stem should be queued");
        match queued.job.kind {
            OfflineRenderKind::ProjectWav { settings, .. } => assert_eq!(
                settings.source,
                aaadaw_app::ProjectWavRenderSource::SelectedTrackStemPreFader(selected)
            ),
            OfflineRenderKind::ProjectWavStems { .. } => {
                panic!("one selected stem should keep the single-file path")
            }
            OfflineRenderKind::ProjectWavRegions { .. } => {
                panic!("entire-project bounds should keep the single-file path")
            }
            OfflineRenderKind::FreezeTrack { .. } => panic!("expected a WAV render"),
        }
    }

    #[test]
    fn queued_selected_track_stems_freeze_track_order_and_output_names() {
        let project_path = std::env::temp_dir().join("aaadaw-render-stems-snapshot.aaadaw");
        let mut app = App {
            project_path: Some(project_path),
            offline_render_busy: true,
            ..App::default()
        };
        for (index, name) in [(0, "Drums"), (1, "Bass"), (2, "Guide")] {
            app.project
                .apply(DawAction::CreateTrack {
                    index,
                    name: name.to_owned(),
                })
                .expect("render tracks should be created");
        }
        let first = app.project.tracks()[0].id();
        let second = app.project.tracks()[1].id();
        app.timeline.selected_track = Some(first);
        app.timeline.selected_tracks.extend([first, second]);
        app.render_dialog.source = RenderSource::SelectedTracksViaMaster;
        app.render_dialog.bounds = RenderBounds::TimeSelection;
        app.timeline.time_selection = Some(crate::timeline::TimeSelection {
            start_tick: 0,
            end_tick: 960,
        });

        let _task = app.start_offline_render(std::env::temp_dir().join("selected-stems.wav"));
        assert_eq!(app.offline_job_queue.len(), 1);
        app.timeline.selected_tracks.clear();
        app.timeline.selected_track = None;
        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("queued stems should be retained while another render runs");
        match queued.job.kind {
            OfflineRenderKind::ProjectWavStems { outputs, settings } => {
                assert_eq!(
                    outputs,
                    [
                        (
                            aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(first),
                            std::env::temp_dir().join("selected-stems-Drums.wav")
                        ),
                        (
                            aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(second),
                            std::env::temp_dir().join("selected-stems-Bass.wav")
                        ),
                    ]
                );
                assert_eq!(settings.start_sample, 0);
                assert_eq!(settings.end_sample, 24_000);
                assert_eq!(
                    settings.source,
                    aaadaw_app::ProjectWavRenderSource::MasterMix
                );
            }
            OfflineRenderKind::ProjectWav { .. } => {
                panic!("multiple selected tracks should create a stem job")
            }
            OfflineRenderKind::ProjectWavRegions { .. } => {
                panic!("entire-project bounds should create a track stem job")
            }
            OfflineRenderKind::FreezeTrack { .. } => panic!("expected a stem render job"),
        }

        app.timeline.selected_track = Some(first);
        app.timeline.selected_tracks.extend([first, second]);
        let prefix = std::env::temp_dir().join(format!(
            "aaadaw-existing-stem-prefix-{}.wav",
            std::process::id()
        ));
        std::fs::write(&prefix, b"preserve selected prefix")
            .expect("existing stem prefix should be written");
        let _task = app.start_offline_render(prefix.clone());
        assert!(app.offline_job_queue.is_empty());
        assert!(app.status.contains("stem prefix"));
        assert_eq!(std::fs::read(&prefix).unwrap(), b"preserve selected prefix");
        std::fs::remove_file(prefix).expect("existing stem prefix should be removed");

        app.render_dialog.source = RenderSource::MasterMixAndSelectedTracks;
        let _task = app.start_offline_render(std::env::temp_dir().join("combined-stems.wav"));
        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("Master mix plus stems should enter the render queue");
        match queued.job.kind {
            OfflineRenderKind::ProjectWavStems { outputs, settings } => {
                assert_eq!(
                    outputs,
                    [
                        (
                            aaadaw_app::ProjectWavRenderSource::MasterMix,
                            std::env::temp_dir().join("combined-stems-Master.wav")
                        ),
                        (
                            aaadaw_app::ProjectWavRenderSource::SelectedTrackStem(first),
                            std::env::temp_dir().join("combined-stems-Drums.wav")
                        ),
                        (
                            aaadaw_app::ProjectWavRenderSource::SelectedTrackStem(second),
                            std::env::temp_dir().join("combined-stems-Bass.wav")
                        ),
                    ]
                );
                assert_eq!(
                    settings.source,
                    aaadaw_app::ProjectWavRenderSource::MasterMix
                );
            }
            OfflineRenderKind::ProjectWav { .. } => {
                panic!("Master mix plus stems should make a grouped output job")
            }
            OfflineRenderKind::ProjectWavRegions { .. } => {
                panic!("entire-project bounds should create a track stem job")
            }
            OfflineRenderKind::FreezeTrack { .. } => panic!("expected grouped render job"),
        }

        app.render_dialog.stems_pre_fader = true;
        let _task =
            app.start_offline_render(std::env::temp_dir().join("combined-stems-prefader.wav"));
        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("pre-fader Master mix and stems should enter the render queue");
        match queued.job.kind {
            OfflineRenderKind::ProjectWavStems { outputs, .. } => {
                assert_eq!(
                    outputs,
                    [
                        (
                            aaadaw_app::ProjectWavRenderSource::MasterMix,
                            std::env::temp_dir().join("combined-stems-prefader-Master.wav")
                        ),
                        (
                            aaadaw_app::ProjectWavRenderSource::SelectedTrackStemPreFader(first),
                            std::env::temp_dir().join("combined-stems-prefader-Drums.wav")
                        ),
                        (
                            aaadaw_app::ProjectWavRenderSource::SelectedTrackStemPreFader(second),
                            std::env::temp_dir().join("combined-stems-prefader-Bass.wav")
                        ),
                    ]
                );
            }
            OfflineRenderKind::ProjectWav { .. } => {
                panic!("pre-fader Master mix plus stems should make a grouped output job")
            }
            OfflineRenderKind::ProjectWavRegions { .. } => {
                panic!("entire-project bounds should create a track stem job")
            }
            OfflineRenderKind::FreezeTrack { .. } => panic!("expected grouped render job"),
        }
    }

    #[test]
    fn offline_jobs_panel_can_be_opened_and_dismissed_with_escape() {
        let mut app = App::default();
        let _open_task = app.update(Message::ToggleOfflineJobsPanel);
        assert!(app.offline_jobs_panel_open);

        let _close_task = app.update(Message::Escape);
        assert!(!app.offline_jobs_panel_open);
    }

    #[test]
    fn failed_render_hands_off_to_the_next_queued_job_in_fifo_order() {
        let mut app = App {
            offline_render_busy: true,
            active_offline_job: Some(QueuedOfflineJob {
                id: OfflineJobId::from_value(1),
                job: wav_job("Render active.wav", "active.wav"),
            }),
            ..App::default()
        };
        app.offline_job_queue
            .enqueue(wav_job("Render first.wav", "first.wav"))
            .unwrap();
        app.offline_job_queue
            .enqueue(wav_job("Render second.wav", "second.wav"))
            .unwrap();

        let _first_task = app.finish_offline_render(Err("expected failure".to_owned()));
        assert_eq!(
            app.active_offline_job.as_ref().map(|job| job.job.label()),
            Some("Render first.wav")
        );
        assert!(app.offline_render_busy);
        assert_eq!(app.offline_job_queue.len(), 1);

        let _second_task = app.finish_offline_render(Err("expected failure".to_owned()));
        assert_eq!(
            app.active_offline_job.as_ref().map(|job| job.job.label()),
            Some("Render second.wav")
        );
        assert!(app.offline_job_history[0].contains("Render first.wav"));
    }

    #[test]
    fn full_pending_queue_keeps_existing_jobs_and_explains_its_limit() {
        let mut app = App {
            offline_render_busy: true,
            active_offline_job: Some(QueuedOfflineJob {
                id: OfflineJobId::from_value(1),
                job: wav_job("Render active.wav", "active.wav"),
            }),
            ..App::default()
        };
        for index in 0..super::super::offline_job_queue::MAX_PENDING_OFFLINE_JOBS {
            app.offline_job_queue
                .enqueue(wav_job(&format!("Render {index}.wav"), "pending.wav"))
                .unwrap();
        }

        let _task = app.enqueue_offline_job(wav_job("Render rejected.wav", "rejected.wav"));

        assert_eq!(
            app.offline_job_queue.len(),
            super::super::offline_job_queue::MAX_PENDING_OFFLINE_JOBS
        );
        assert!(app.status.contains("maximum 4 waiting jobs"));
        assert_eq!(
            app.active_offline_job.as_ref().map(|job| job.job.label()),
            Some("Render active.wav")
        );
    }

    #[test]
    fn queued_jobs_wait_for_project_io_and_resume_when_it_finishes() {
        let mut app = App {
            io_busy: true,
            ..App::default()
        };
        app.offline_job_queue
            .enqueue(wav_job("Render queued.wav", "queued.wav"))
            .unwrap();

        let _blocked_task = app.resume_offline_job_queue();
        assert!(!app.offline_render_busy);
        assert_eq!(app.offline_job_queue.len(), 1);

        app.io_busy = false;
        let _started_task = app.resume_offline_job_queue();
        assert!(app.offline_render_busy);
        assert_eq!(
            app.active_offline_job.as_ref().map(|job| job.job.label()),
            Some("Render queued.wav")
        );
        assert!(app.offline_job_queue.is_empty());
    }

    #[test]
    fn freeze_and_wav_requests_queue_behind_an_active_render() {
        let project_path = PathBuf::from("session.aaadaw");
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Instrument".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        let mut app = App {
            project,
            project_path: Some(project_path.clone()),
            offline_render_busy: true,
            active_offline_job: Some(QueuedOfflineJob {
                id: OfflineJobId::from_value(1),
                job: wav_job("Render active.wav", "active.wav"),
            }),
            ..App::default()
        };

        let _freeze_task = app.start_freeze_track(track_id);
        let _render_task = app.start_offline_render(PathBuf::from("second.wav"));

        let labels: Vec<_> = app
            .offline_job_queue
            .iter()
            .map(|(_, job)| job.label())
            .collect();
        assert_eq!(labels, ["Freeze Instrument", "Render second.wav"]);
        assert_eq!(
            app.active_offline_job.as_ref().map(|job| job.job.label()),
            Some("Render active.wav")
        );
        assert!(app.offline_render_busy);
    }

    #[test]
    fn freeze_source_validation_ignores_unrelated_track_edits() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Source".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "Other".to_owned(),
            })
            .unwrap();
        let source_id = project.tracks()[0].id();
        let other_id = project.tracks()[1].id();
        let captured = project.snapshot();

        project
            .apply(DawAction::SetTrackMute {
                track_id: other_id,
                muted: true,
            })
            .unwrap();

        assert!(freeze_source_matches(
            &project.snapshot(),
            &captured,
            source_id
        ));
    }

    #[test]
    fn freeze_source_validation_rejects_edits_to_the_frozen_track() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Source".to_owned(),
            })
            .unwrap();
        let source_id = project.tracks()[0].id();
        let captured = project.snapshot();

        project
            .apply(DawAction::SetTrackVolume {
                track_id: source_id,
                volume_db: -3.0,
            })
            .unwrap();

        assert!(!freeze_source_matches(
            &project.snapshot(),
            &captured,
            source_id
        ));
    }

    #[test]
    fn stale_freeze_completion_keeps_the_track_live_and_removes_rendered_media() {
        let directory = tempfile::tempdir().unwrap();
        let project_path = directory.path().join("session.aaadaw");
        let rendered_file = directory.path().join("freeze.wav");
        std::fs::write(&rendered_file, b"rendered audio bytes").unwrap();
        let mut store = ProjectStore::open(&project_path).unwrap();
        let media_ref = store.import_audio_file(&rendered_file).unwrap();
        store.close().unwrap();

        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Source".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        let captured = Arc::new(project.snapshot());
        project
            .apply(DawAction::SetTrackVolume {
                track_id,
                volume_db: -3.0,
            })
            .unwrap();
        let live_project_snapshot = project.snapshot();
        let job = OfflineRenderJob {
            kind: OfflineRenderKind::FreezeTrack { track_id },
            snapshot: captured,
            project_path: project_path.clone(),
            project_generation: 0,
            label: "Freeze Source".to_owned(),
        };
        let mut app = App {
            project,
            offline_render_busy: true,
            offline_render_is_freeze: true,
            io_busy: true,
            active_offline_job: Some(QueuedOfflineJob {
                id: OfflineJobId::from_value(1),
                job,
            }),
            ..App::default()
        };

        let _next_task = app.finish_freeze_track(Ok(FreezeTrackResult {
            track_id,
            media_ref: media_ref.clone(),
            start_sample: 0,
            length_samples: 128,
        }));

        assert_eq!(app.project.snapshot(), live_project_snapshot);
        assert!(
            app.status
                .contains("skipped: its source or project changed")
        );
        let store = ProjectStore::open(&project_path).unwrap();
        assert!(store.resolve_audio_asset(&media_ref).is_err());
    }

    #[test]
    fn cancelled_successful_freeze_does_not_apply_or_leave_rendered_media() {
        let directory = tempfile::tempdir().unwrap();
        let project_path = directory.path().join("session.aaadaw");
        let rendered_file = directory.path().join("freeze.wav");
        std::fs::write(&rendered_file, b"rendered audio bytes").unwrap();
        let mut store = ProjectStore::open(&project_path).unwrap();
        let media_ref = store.import_audio_file(&rendered_file).unwrap();
        store.close().unwrap();

        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Source".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        let snapshot = Arc::new(project.snapshot());
        let job = OfflineRenderJob {
            kind: OfflineRenderKind::FreezeTrack { track_id },
            snapshot: Arc::clone(&snapshot),
            project_path: project_path.clone(),
            project_generation: 0,
            label: "Freeze Source".to_owned(),
        };
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let mut app = App {
            project,
            offline_render_busy: true,
            offline_render_is_freeze: true,
            io_busy: true,
            active_offline_job: Some(QueuedOfflineJob {
                id: OfflineJobId::from_value(1),
                job,
            }),
            offline_render_cancel: Some(cancel),
            ..App::default()
        };

        let _next_task = app.finish_freeze_track(Ok(FreezeTrackResult {
            track_id,
            media_ref: media_ref.clone(),
            start_sample: 0,
            length_samples: 128,
        }));

        assert_eq!(app.project.snapshot(), snapshot.as_ref().clone());
        assert!(app.status.contains("cancelled"));
        let store = ProjectStore::open(&project_path).unwrap();
        assert!(store.resolve_audio_asset(&media_ref).is_err());
    }
}
