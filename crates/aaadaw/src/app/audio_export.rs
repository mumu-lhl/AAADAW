use super::messages::FreezeTrackResult;
use super::offline_job_queue::{EnqueueError, OfflineJobId};
use super::{App, Message, run_blocking};
use aaadaw_app::{WavChannelCount, WavSampleFormat};
use aaadaw_core::{DawAction, ItemId, MarkerId, Project, ProjectSnapshot, RegionId, TrackId};
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
    CustomTimeRange,
    TimeSelection,
    AllProjectRegions,
    SelectedProjectRegions,
    AllProjectMarkers,
    SelectedProjectMarkers,
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
    SelectedMediaItems,
    RegionRenderMatrix,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RenderFilenameWildcard {
    Project,
    Title,
    Author,
    ProjectNotes,
    ProjectNotesNamed,
    Tempo,
    TimeSignature,
    TimeSigNum,
    TimeSigDenom,
    Region,
    RegionNumber,
    RegionNumberNumbered,
    RegionName,
    Marker,
    MarkerNumber,
    MarkerNumberNumbered,
    MarkerName,
    Track,
    TrackNumber,
    TrackNameOrNumber,
    Item,
    ItemNumber,
    Start,
    End,
    Length,
    StartBeats,
    EndBeats,
    LengthBeats,
    StartTimecode,
    EndTimecode,
    StartFrames,
    EndFrames,
    LengthFrames,
    StartSeconds,
    EndSeconds,
    LengthSeconds,
    LengthHours,
    LengthMinutes,
    LengthSecondsComponent,
    LengthMilliseconds,
    Format,
    SampleRate,
    SampleRateKilohertz,
    BitDepth,
    Channels,
    Bpm,
    RegionNamedValue,
    MarkerNamedValue,
}

impl RenderFilenameWildcard {
    pub(crate) const ALL: [Self; 48] = [
        Self::Project,
        Self::Title,
        Self::Author,
        Self::ProjectNotes,
        Self::ProjectNotesNamed,
        Self::Bpm,
        Self::Tempo,
        Self::TimeSignature,
        Self::TimeSigNum,
        Self::TimeSigDenom,
        Self::Track,
        Self::TrackNumber,
        Self::TrackNameOrNumber,
        Self::Region,
        Self::RegionNumber,
        Self::RegionNumberNumbered,
        Self::RegionName,
        Self::Marker,
        Self::MarkerNumber,
        Self::MarkerNumberNumbered,
        Self::MarkerName,
        Self::Item,
        Self::ItemNumber,
        Self::Start,
        Self::End,
        Self::Length,
        Self::StartBeats,
        Self::EndBeats,
        Self::LengthBeats,
        Self::StartTimecode,
        Self::EndTimecode,
        Self::StartFrames,
        Self::EndFrames,
        Self::LengthFrames,
        Self::StartSeconds,
        Self::EndSeconds,
        Self::LengthSeconds,
        Self::LengthHours,
        Self::LengthMinutes,
        Self::LengthSecondsComponent,
        Self::LengthMilliseconds,
        Self::Format,
        Self::SampleRate,
        Self::SampleRateKilohertz,
        Self::BitDepth,
        Self::Channels,
        Self::RegionNamedValue,
        Self::MarkerNamedValue,
    ];

    pub(crate) const fn token(self) -> &'static str {
        match self {
            Self::Project => "$project",
            Self::Title => "$title",
            Self::Author => "$author",
            Self::ProjectNotes => "$projectnotes",
            Self::ProjectNotesNamed => "$projectnotes(name)",
            Self::Tempo => "$tempo",
            Self::TimeSignature => "$timesignature",
            Self::TimeSigNum => "$timesignum",
            Self::TimeSigDenom => "$timesigdenom",
            Self::Region => "$region",
            Self::RegionNumber => "$regionnumber",
            Self::RegionNumberNumbered => "$regionnumber[N]",
            Self::RegionName => "$regionname",
            Self::Marker => "$marker",
            Self::MarkerNumber => "$markernumber",
            Self::MarkerNumberNumbered => "$markernumber[N]",
            Self::MarkerName => "$markername",
            Self::Track => "$track",
            Self::TrackNumber => "$tracknumber",
            Self::TrackNameOrNumber => "$tracknamenumber",
            Self::Item => "$item",
            Self::ItemNumber => "$itemnumber",
            Self::Start => "$start",
            Self::End => "$end",
            Self::Length => "$length",
            Self::StartBeats => "$startbeats",
            Self::EndBeats => "$endbeats",
            Self::LengthBeats => "$lengthbeats",
            Self::StartTimecode => "$starttc",
            Self::EndTimecode => "$endtc",
            Self::StartFrames => "$startframes",
            Self::EndFrames => "$endframes",
            Self::LengthFrames => "$lengthframes",
            Self::StartSeconds => "$startseconds",
            Self::EndSeconds => "$endseconds",
            Self::LengthSeconds => "$lengthseconds",
            Self::LengthHours => "$lenhh",
            Self::LengthMinutes => "$lenmm",
            Self::LengthSecondsComponent => "$lenss",
            Self::LengthMilliseconds => "$lentt",
            Self::Format => "$format",
            Self::SampleRate => "$samplerate",
            Self::SampleRateKilohertz => "$sampleratek",
            Self::BitDepth => "$bitdepth",
            Self::Channels => "$channels",
            Self::Bpm => "$bpm",
            Self::RegionNamedValue => "$region(name)",
            Self::MarkerNamedValue => "$marker(name)",
        }
    }
}

impl RenderSource {
    pub(crate) const ALL: [Self; 6] = [
        Self::MasterMix,
        Self::SelectedTracksStems,
        Self::MasterMixAndSelectedTracks,
        Self::SelectedTracksViaMaster,
        Self::SelectedMediaItems,
        Self::RegionRenderMatrix,
    ];
}

impl std::fmt::Display for RenderSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::MasterMix => "Master mix",
            Self::SelectedTracksStems => "Selected tracks (stems)",
            Self::SelectedTracksViaMaster => "Selected tracks via master",
            Self::MasterMixAndSelectedTracks => "Master mix and stems",
            Self::SelectedMediaItems => "Selected media items",
            Self::RegionRenderMatrix => "Region Render Matrix",
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
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Render tail must be a non-negative whole number of milliseconds");
    }
    value.parse::<u32>().map_err(|_| "Render tail is too large")
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

fn selected_media_item_render_jobs(
    base_path: &Path,
    project: &Project,
    selected_items: &HashSet<ItemId>,
    settings: aaadaw_app::ProjectWavRenderSettings,
) -> Result<Vec<(aaadaw_app::ProjectWavRenderSettings, PathBuf)>, String> {
    let selected_midi_count = project
        .midi_items()
        .iter()
        .filter(|item| selected_items.contains(&item.id()))
        .count();
    if selected_midi_count > 0 {
        return Err(
            "Selected media item rendering currently supports audio items; deselect MIDI items"
                .to_owned(),
        );
    }
    let items = project
        .audio_items()
        .iter()
        .filter(|item| selected_items.contains(&item.id()))
        .collect::<Vec<_>>();
    if items.is_empty() {
        return Err("Select at least one audio media item in Arrange to render it".to_owned());
    }
    let parent = base_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let base_name = base_path
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "Choose a WAV filename to use as the selected-item prefix".to_owned())?;
    let extension = base_path
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .unwrap_or("wav");
    let mut outputs = Vec::with_capacity(items.len());
    let mut used_names = HashSet::with_capacity(items.len());
    for item in items {
        let ordinal = project
            .audio_items()
            .iter()
            .position(|candidate| candidate.id() == item.id())
            .unwrap_or(0)
            .saturating_add(1);
        let mut name = format!("{base_name}-item_{ordinal:02}");
        let mut disambiguator = 2;
        while !used_names.insert(name.to_lowercase()) {
            name = format!("{base_name}-item_{ordinal:02}-{disambiguator}");
            disambiguator += 1;
        }
        outputs.push((
            aaadaw_app::ProjectWavRenderSettings {
                source: aaadaw_app::ProjectWavRenderSource::SelectedAudioItem(item.id()),
                start_sample: item.start_sample(),
                end_sample: item.end_sample(),
                tail_milliseconds: 0,
                ..settings
            },
            parent.join(format!("{name}.{extension}")),
        ));
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
    selected_region_ids: Option<&HashSet<RegionId>>,
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
    let regions = project
        .regions()
        .iter()
        .enumerate()
        .filter(|(_, region)| {
            selected_region_ids.is_none_or(|selected| selected.contains(&region.id()))
        })
        .collect::<Vec<_>>();
    if regions.is_empty() {
        return Err(
            "Add or select at least one project region before rendering regions".to_owned(),
        );
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
        RenderSource::SelectedMediaItems => {
            unreachable!("selected media items are not a track stem mode")
        }
        RenderSource::RegionRenderMatrix => SelectedTrackStemMode::ViaMaster,
    };
    let mut used_region_names = HashSet::with_capacity(regions.len());
    let mut jobs = Vec::new();
    for (index, region) in regions {
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
            RenderSource::SelectedMediaItems => {
                unreachable!("selected media items are not a region source mode")
            }
            RenderSource::RegionRenderMatrix => {
                let assigned_tracks = project
                    .tracks()
                    .iter()
                    .filter(|track| region.render_track_ids().contains(&track.id()))
                    .map(|track| track.id())
                    .collect::<Vec<_>>();
                selected_track_stem_destinations(
                    &region_base,
                    project,
                    &assigned_tracks,
                    region.renders_master_mix(),
                    SelectedTrackStemMode::ViaMaster,
                )?
            }
        };
        for (render_source, destination) in destinations {
            let mut region_settings = settings;
            region_settings.source = render_source;
            region_settings.start_sample = start_sample;
            region_settings.end_sample = end_sample;
            jobs.push((region_settings, destination));
        }
    }
    if source == RenderSource::RegionRenderMatrix && jobs.is_empty() {
        return Err("Assign a Master mix or track to a project region before rendering".to_owned());
    }
    Ok(jobs)
}

fn project_marker_render_jobs(
    base_path: &Path,
    project: &Project,
    source: RenderSource,
    selected_tracks: &[TrackId],
    stems_pre_fader: bool,
    settings: aaadaw_app::ProjectWavRenderSettings,
    selected_marker_ids: Option<&HashSet<MarkerId>>,
) -> Result<Vec<(aaadaw_app::ProjectWavRenderSettings, PathBuf)>, String> {
    let parent = base_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let base_name = base_path
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "Choose a WAV filename to use as the marker prefix".to_owned())?;
    let extension = base_path
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .unwrap_or("wav");
    let project_end_sample = aaadaw_app::project_content_end_sample(project)
        .map_err(|error| format!("Could not determine project render end: {error}"))?;
    let markers = project
        .markers()
        .iter()
        .enumerate()
        .filter(|(_, marker)| {
            selected_marker_ids.is_none_or(|selected| selected.contains(&marker.id()))
        })
        .collect::<Vec<_>>();
    if markers.is_empty() {
        return Err(
            "Add or select at least one project marker before rendering markers".to_owned(),
        );
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
        RenderSource::SelectedMediaItems => {
            unreachable!("selected media items are not a track stem mode")
        }
        RenderSource::RegionRenderMatrix => SelectedTrackStemMode::ViaMaster,
    };
    let mut used_marker_names = HashSet::with_capacity(markers.len());
    let mut jobs = Vec::new();
    for (index, marker) in markers {
        let start_sample = project
            .sample_at_tick(marker.position_ticks())
            .map_err(|error| format!("Could not convert marker position: {error}"))?;
        let end_sample = match project.markers().get(index + 1) {
            Some(next_marker) => project
                .sample_at_tick(next_marker.position_ticks())
                .map_err(|error| format!("Could not convert next marker position: {error}"))?,
            None => project_end_sample,
        };
        // REAPER skips a marker whose next marker or project end leaves it
        // with no audio frames to render (for example, a marker at project end).
        if end_sample <= start_sample {
            continue;
        }
        aaadaw_app::render_range_length_samples_with_tail_milliseconds(
            project.settings().sample_rate(),
            start_sample,
            end_sample,
            settings.tail_milliseconds,
        )
        .map_err(|error| {
            format!(
                "Project marker {} has an invalid render range: {error}",
                marker.number()
            )
        })?;

        let mut marker_name = safe_stem_component(marker.name());
        if marker_name.is_empty() {
            marker_name = format!("{:02}", marker.number());
        }
        let initial_name = marker_name.clone();
        let mut suffix = index + 1;
        while !used_marker_names.insert(marker_name.to_lowercase()) {
            marker_name = format!("{initial_name}-{suffix}");
            suffix += 1;
        }
        let marker_base = parent.join(format!("{base_name}-{marker_name}.{extension}"));
        let destinations = match source {
            RenderSource::MasterMix => {
                vec![(aaadaw_app::ProjectWavRenderSource::MasterMix, marker_base)]
            }
            RenderSource::SelectedTracksStems | RenderSource::SelectedTracksViaMaster => {
                selected_track_stem_destinations(
                    &marker_base,
                    project,
                    selected_tracks,
                    false,
                    stem_mode,
                )?
            }
            RenderSource::MasterMixAndSelectedTracks => selected_track_stem_destinations(
                &marker_base,
                project,
                selected_tracks,
                true,
                stem_mode,
            )?,
            RenderSource::SelectedMediaItems => {
                unreachable!("selected media items are not a marker source mode")
            }
            RenderSource::RegionRenderMatrix => {
                unreachable!("region render matrix requires project region bounds")
            }
        };
        for (render_source, destination) in destinations {
            let mut marker_settings = settings;
            marker_settings.source = render_source;
            marker_settings.start_sample = start_sample;
            marker_settings.end_sample = end_sample;
            jobs.push((marker_settings, destination));
        }
    }
    if jobs.is_empty() {
        return Err("Selected project markers have no non-empty render ranges".to_owned());
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

fn apply_render_filename_pattern(
    outputs: Vec<(aaadaw_app::ProjectWavRenderSettings, PathBuf)>,
    pattern: &str,
    project: &Project,
    project_path: &Path,
) -> Result<Vec<(aaadaw_app::ProjectWavRenderSettings, PathBuf)>, String> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Ok(outputs);
    }

    let mut used_names = HashSet::with_capacity(outputs.len());
    outputs
        .into_iter()
        .map(|(settings, path)| {
            let fallback = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.is_empty())
                .unwrap_or("render");
            let expanded = expand_render_filename_pattern(pattern, fallback, project, project_path, settings);
            let name = safe_stem_component(&expanded);
            if name.is_empty() {
                return Err("Filename pattern must produce a non-empty file name".to_owned());
            }
            if !used_names.insert(name.to_lowercase()) {
                return Err(format!(
                    "Filename pattern creates duplicate output names ({name}); include a wildcard such as $region, $track, or $item"
                ));
            }
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .filter(|extension| !extension.is_empty())
                .unwrap_or("wav");
            Ok((settings, parent.join(format!("{name}.{extension}"))))
        })
        .collect()
}

fn apply_stem_filename_pattern(
    outputs: Vec<(aaadaw_app::ProjectWavRenderSource, PathBuf)>,
    settings: aaadaw_app::ProjectWavRenderSettings,
    pattern: &str,
    project: &Project,
    project_path: &Path,
) -> Result<Vec<(aaadaw_app::ProjectWavRenderSource, PathBuf)>, String> {
    apply_render_filename_pattern(
        outputs
            .into_iter()
            .map(|(source, path)| {
                let mut settings = settings;
                settings.source = source;
                (settings, path)
            })
            .collect(),
        pattern,
        project,
        project_path,
    )
    .map(|outputs| {
        outputs
            .into_iter()
            .map(|(settings, path)| (settings.source, path))
            .collect()
    })
}

fn format_render_wildcard_seconds(seconds: f64) -> String {
    if !seconds.is_finite() {
        return String::new();
    }
    format!("{seconds:.3}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn format_render_wildcard_clock(project: &Project, sample: u64) -> String {
    project
        .project_time_at_sample(sample)
        .map(crate::project_time::format_project_time)
        .unwrap_or_default()
        .replace(':', "-")
}

fn format_render_wildcard_timecode(project: &Project, sample: u64) -> String {
    project
        .project_time_at_sample(sample)
        .and_then(|seconds| {
            crate::project_time::format_project_timecode(project.settings().frame_rate(), seconds)
        })
        .unwrap_or_default()
        .replace(':', ".")
}

fn format_render_wildcard_frames(seconds: f64, frame_rate: aaadaw_core::FrameRate) -> String {
    if !seconds.is_finite() {
        return String::new();
    }
    let frames = ((seconds.abs() + 1e-8) * frame_rate.frames_per_second()).floor();
    let frames = if frames >= u64::MAX as f64 {
        u64::MAX
    } else {
        frames as u64
    };
    if seconds < 0.0 {
        format!("-{frames}")
    } else {
        frames.to_string()
    }
}

fn format_render_wildcard_beat_position(project: &Project, tick: u64) -> String {
    let Ok(position) = project.musical_position_at_tick(tick) else {
        return String::new();
    };
    let meter = project.time_signature_at_tick(tick);
    let ticks_per_beat =
        (u64::from(project.settings().ppq()) * 4 / u64::from(meter.denominator())).max(1);
    let hundredths = (position.tick_in_beat() * 100 + ticks_per_beat / 2) / ticks_per_beat;
    if hundredths >= 100 {
        let next_tick = tick.saturating_add(ticks_per_beat.saturating_sub(position.tick_in_beat()));
        if let Ok(next) = project.musical_position_at_tick(next_tick) {
            return format!("{}.{}.{:02}", next.measure(), next.beat(), 0);
        }
    }
    format!("{}.{}.{hundredths:02}", position.measure(), position.beat())
}

fn format_render_wildcard_beat_length(project: &Project, start_tick: u64, end_tick: u64) -> String {
    let meter = project.time_signature_at_tick(start_tick);
    let ticks_per_beat =
        (u64::from(project.settings().ppq()) * 4 / u64::from(meter.denominator())).max(1);
    let ticks = end_tick.saturating_sub(start_tick);
    let mut beats = ticks / ticks_per_beat;
    let mut hundredths = ((ticks % ticks_per_beat) * 100 + ticks_per_beat / 2) / ticks_per_beat;
    if hundredths >= 100 {
        beats = beats.saturating_add(1);
        hundredths = 0;
    }
    format!("0.{beats}.{hundredths:02}")
}

fn format_render_wildcard_length_components(seconds: f64) -> [String; 4] {
    if !seconds.is_finite() || seconds < 0.0 {
        return std::array::from_fn(|_| String::new());
    }
    let total_milliseconds = ((seconds + 1e-8) * 1_000.0).floor();
    let total_milliseconds = if total_milliseconds >= u64::MAX as f64 {
        u64::MAX
    } else {
        total_milliseconds as u64
    };
    let total_seconds = total_milliseconds / 1_000;
    [
        (total_seconds / 3_600).to_string(),
        (total_seconds / 60 % 60).to_string(),
        (total_seconds % 60).to_string(),
        format!("{:03}", total_milliseconds % 1_000),
    ]
}

fn format_render_wildcard_sample_rate_kilohertz(sample_rate: u32) -> String {
    let kilohertz = sample_rate / 1_000;
    let remainder = sample_rate % 1_000;
    if remainder == 0 {
        kilohertz.to_string()
    } else if remainder.is_multiple_of(100) {
        format!("{kilohertz}.{}", remainder / 100)
    } else if remainder.is_multiple_of(10) {
        format!("{kilohertz}.{:02}", remainder / 10)
    } else {
        format!("{kilohertz}.{remainder:03}")
    }
}

fn format_render_wildcard_bit_depth(sample_format: WavSampleFormat) -> String {
    match sample_format {
        WavSampleFormat::Pcm8 | WavSampleFormat::MuLaw8 => "8",
        WavSampleFormat::Pcm16 => "16",
        WavSampleFormat::Pcm24 => "24",
        WavSampleFormat::Pcm32 | WavSampleFormat::Float32 => "32",
        WavSampleFormat::Float64 => "64",
        WavSampleFormat::ImaAdpcm4 => "4",
        WavSampleFormat::Cadpcm2 => "2",
    }
    .to_owned()
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RenderFilenameWildcardModifier {
    ReplaceCharacters {
        characters: Vec<char>,
        replacement: char,
    },
    RemoveCharacter(char),
    Truncate(usize),
    Numbered {
        start: u64,
        width: usize,
    },
}

fn parse_render_filename_wildcard_modifiers(
    characters: &[char],
    start: usize,
    token: &str,
) -> (usize, Vec<RenderFilenameWildcardModifier>) {
    let mut index = start;
    let mut modifiers = Vec::new();
    while index < characters.len() {
        match characters[index] {
            '<' => {
                let Some(relative_end) = characters[index + 1..]
                    .iter()
                    .position(|character| *character == '>')
                else {
                    break;
                };
                let end = index + 1 + relative_end;
                if end == index + 1 {
                    break;
                }
                let modifier = &characters[index + 1..end];
                if modifier.len() == 1 {
                    modifiers.push(RenderFilenameWildcardModifier::RemoveCharacter(modifier[0]));
                } else if let Some(replacement) = modifier.last().copied() {
                    modifiers.push(RenderFilenameWildcardModifier::ReplaceCharacters {
                        characters: modifier[..modifier.len() - 1].to_vec(),
                        replacement,
                    });
                }
                index = end + 1;
            }
            '[' => {
                let Some(relative_end) = characters[index + 1..]
                    .iter()
                    .position(|character| *character == ']')
                else {
                    break;
                };
                let end = index + 1 + relative_end;
                let suffix = &characters[index + 1..end];
                if let Some(digits) = suffix.strip_prefix(&['<']) {
                    let digits = digits.iter().collect::<String>();
                    let Ok(limit) = digits.parse::<usize>() else {
                        break;
                    };
                    modifiers.push(RenderFilenameWildcardModifier::Truncate(limit));
                } else if matches!(token, "regionnumber" | "markernumber")
                    && suffix.iter().all(char::is_ascii_digit)
                {
                    let width = suffix.len();
                    let digits = suffix.iter().collect::<String>();
                    let Ok(start) = digits.parse::<u64>() else {
                        break;
                    };
                    modifiers.push(RenderFilenameWildcardModifier::Numbered { start, width });
                } else {
                    // Other [N] forms use render-order contexts which are not
                    // modeled by the current single-source filename resolver.
                    break;
                }
                index = end + 1;
            }
            _ => break,
        }
    }
    (index, modifiers)
}

fn apply_render_filename_wildcard_case(value: &str, token: &str) -> String {
    let mut token_characters = token.chars();
    let Some(first) = token_characters.next() else {
        return value.to_owned();
    };
    let Some(second) = token_characters.next() else {
        return value.to_owned();
    };

    if first.is_ascii_uppercase() && second.is_ascii_uppercase() {
        value.to_uppercase()
    } else if first.is_ascii_lowercase() && second.is_ascii_uppercase() {
        value.to_lowercase()
    } else if first.is_ascii_uppercase() {
        let mut value_characters = value.chars();
        let Some(first_value_character) = value_characters.next() else {
            return String::new();
        };
        first_value_character
            .to_uppercase()
            .chain(value_characters)
            .collect()
    } else {
        value.to_owned()
    }
}

fn apply_render_filename_wildcard_modifiers(
    value: &str,
    token: &str,
    modifiers: &[RenderFilenameWildcardModifier],
) -> String {
    let mut value = apply_render_filename_wildcard_case(value, token);
    for modifier in modifiers {
        value = match modifier {
            RenderFilenameWildcardModifier::ReplaceCharacters {
                characters,
                replacement,
            } => value
                .chars()
                .map(|character| {
                    if characters.contains(&character) {
                        *replacement
                    } else {
                        character
                    }
                })
                .collect(),
            RenderFilenameWildcardModifier::RemoveCharacter(character) => {
                value.chars().filter(|value| *value != *character).collect()
            }
            RenderFilenameWildcardModifier::Truncate(limit) => value.chars().take(*limit).collect(),
            RenderFilenameWildcardModifier::Numbered { start, width } => value
                .parse::<u64>()
                .map(|number| start.saturating_add(number.saturating_sub(1)))
                .map(|number| format!("{number:0width$}"))
                .unwrap_or(value),
        };
    }
    value
}

fn is_render_filename_wildcard_separator(character: char) -> bool {
    matches!(character, ' ' | '-' | '_' | '.')
}

fn render_filename_named_value(text: &str, name: &str, separator: Option<char>) -> String {
    text.split(|character| character == '\n' || character == '\r' || separator == Some(character))
        .find_map(|entry| {
            let (key, value) = entry.split_once('=')?;
            (key == name).then(|| value.to_owned())
        })
        .unwrap_or_default()
}

fn expand_render_filename_pattern(
    pattern: &str,
    fallback: &str,
    project: &Project,
    project_path: &Path,
    settings: aaadaw_app::ProjectWavRenderSettings,
) -> String {
    let project_name = project_path
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback);
    let track_id = match settings.source {
        aaadaw_app::ProjectWavRenderSource::SelectedTrackStem(track_id)
        | aaadaw_app::ProjectWavRenderSource::SelectedTrackStemPreFader(track_id)
        | aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(track_id) => Some(track_id),
        aaadaw_app::ProjectWavRenderSource::SelectedAudioItem(item_id) => project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            .map(|item| item.track_id()),
        aaadaw_app::ProjectWavRenderSource::MasterMix => None,
    };
    let track_name = track_id
        .and_then(|track_id| project.tracks().iter().find(|track| track.id() == track_id))
        .map(|track| track.name())
        .unwrap_or_else(|| {
            if settings.source == aaadaw_app::ProjectWavRenderSource::MasterMix {
                "Master"
            } else {
                ""
            }
        });
    let track_number = track_id
        .and_then(|track_id| {
            project
                .tracks()
                .iter()
                .position(|track| track.id() == track_id)
        })
        .map_or(0, |index| index + 1);
    let track_number_text = format!("{track_number:02}");
    let track_name_or_number = track_id
        .and_then(|track_id| project.tracks().iter().find(|track| track.id() == track_id))
        .map(|track| {
            if track.name().is_empty() {
                format!("Track {track_number_text}")
            } else {
                track.name().to_owned()
            }
        })
        .unwrap_or_else(|| {
            if settings.source == aaadaw_app::ProjectWavRenderSource::MasterMix {
                "Master".to_owned()
            } else {
                String::new()
            }
        });
    let item_index = match settings.source {
        aaadaw_app::ProjectWavRenderSource::SelectedAudioItem(item_id) => project
            .audio_items()
            .iter()
            .position(|item| item.id() == item_id),
        _ => None,
    };
    let item_name = item_index
        .map(|index| format!("item_{:02}", index + 1))
        .unwrap_or_default();
    let item_number_text = item_index
        .map(|index| format!("{:02}", index + 1))
        .unwrap_or_default();
    let region = project.regions().iter().find(|region| {
        project.sample_at_tick(region.start_ticks()).ok() == Some(settings.start_sample)
            && project.sample_at_tick(region.end_ticks()).ok() == Some(settings.end_sample)
    });
    let marker = project
        .markers()
        .iter()
        .enumerate()
        .find(|(index, marker)| {
            let Ok(start) = project.sample_at_tick(marker.position_ticks()) else {
                return false;
            };
            let end = project
                .markers()
                .get(index + 1)
                .and_then(|next| project.sample_at_tick(next.position_ticks()).ok())
                .or_else(|| aaadaw_app::project_content_end_sample(project).ok());
            start == settings.start_sample && end == Some(settings.end_sample)
        });
    let bpm = project
        .tick_at_sample(settings.start_sample)
        .ok()
        .map(|tick| project.tempo_at_tick(tick))
        .unwrap_or_else(|| project.settings().initial_tempo_bpm());
    let bpm_text = if bpm.fract().abs() < f64::EPSILON {
        format!("{bpm:.0}")
    } else {
        bpm.to_string()
    };
    let tick = project.tick_at_sample(settings.start_sample).unwrap_or(0);
    let time_signature = project.time_signature_at_tick(tick);
    let time_signature_text = format!(
        "{}-{}",
        time_signature.numerator(),
        time_signature.denominator()
    );
    let sample_rate = f64::from(project.settings().sample_rate());
    let start_seconds = settings.start_sample as f64 / sample_rate;
    let end_seconds = settings.end_sample as f64 / sample_rate;
    let length_seconds = end_seconds - start_seconds;
    let start_tick = project.tick_at_sample(settings.start_sample).unwrap_or(0);
    let end_tick = project
        .tick_at_sample(settings.end_sample)
        .unwrap_or(start_tick);
    let length_components = format_render_wildcard_length_components(length_seconds);
    let frame_rate = project.settings().frame_rate();
    let region_number_text =
        region.map_or_else(String::new, |region| format!("{:02}", region.number()));
    let marker_number_text =
        marker.map_or_else(String::new, |(_, marker)| format!("{:02}", marker.number()));
    let region_name_or_number = region.map_or_else(String::new, |region| {
        if region.name().is_empty() {
            format!("{:02}", region.number())
        } else {
            region.name().to_owned()
        }
    });
    let marker_name_or_number = marker.map_or_else(String::new, |(_, marker)| {
        if marker.name().is_empty() {
            format!("{:02}", marker.number())
        } else {
            marker.name().to_owned()
        }
    });
    let region_name = region.map_or_else(String::new, |region| region.name().to_owned());
    let marker_name = marker.map_or_else(String::new, |(_, marker)| marker.name().to_owned());

    let mut result = String::with_capacity(pattern.len());
    let characters = pattern.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < characters.len() {
        if characters[index] != '$' {
            result.push(characters[index]);
            index += 1;
            continue;
        }
        let token_start = index + 1;
        let mut token_end = token_start;
        while token_end < characters.len() && characters[token_end].is_ascii_alphabetic() {
            token_end += 1;
        }
        if token_end == token_start {
            result.push('$');
            index += 1;
            continue;
        }
        let token_original = characters[token_start..token_end]
            .iter()
            .collect::<String>();
        let token = token_original.to_ascii_lowercase();
        let known_token = token == "tracknameornumber"
            || RenderFilenameWildcard::ALL.iter().any(|wildcard| {
                wildcard.token().trim_start_matches('$').split('(').next() == Some(token.as_str())
            });
        let (expression_end, named_value_key, named_value_separator) =
            if matches!(token.as_str(), "projectnotes" | "region" | "marker")
                && characters.get(token_end) == Some(&'(')
            {
                let expression_start = token_end + 1;
                characters[expression_start..]
                    .iter()
                    .position(|character| *character == ')')
                    .map(|relative_end| {
                        let expression_end = expression_start + relative_end + 1;
                        let name = characters[expression_start..expression_end - 1]
                            .iter()
                            .collect::<String>();
                        let separator = if matches!(token.as_str(), "region" | "marker")
                            && characters.get(expression_end) == Some(&'[')
                            && characters.get(expression_end + 2) == Some(&']')
                        {
                            Some(characters[expression_end + 1])
                        } else {
                            None
                        };
                        let expression_end = if separator.is_some() {
                            expression_end + 3
                        } else {
                            expression_end
                        };
                        (expression_end, Some(name), separator)
                    })
                    .unwrap_or((token_end, None, None))
            } else {
                (token_end, None, None)
            };
        let (wildcard_end, modifiers) = if known_token {
            parse_render_filename_wildcard_modifiers(&characters, expression_end, &token)
        } else {
            (expression_end, Vec::new())
        };
        let expansion_start = result.len();
        match token.as_str() {
            "project" => result.push_str(project_name),
            "title" => result.push_str(&project.project_notes().title),
            "author" => result.push_str(&project.project_notes().author),
            "projectnotes" => result.push_str(
                &named_value_key
                    .as_deref()
                    .map(|name| {
                        render_filename_named_value(&project.project_notes().text, name, None)
                    })
                    .unwrap_or_else(|| project.project_notes().text.clone()),
            ),
            "region" if named_value_key.is_some() => result.push_str(
                &named_value_key
                    .as_deref()
                    .map(|name| {
                        region.map_or_else(String::new, |region| {
                            render_filename_named_value(region.name(), name, named_value_separator)
                        })
                    })
                    .unwrap_or_default(),
            ),
            "marker" if named_value_key.is_some() => result.push_str(
                &named_value_key
                    .as_deref()
                    .map(|name| {
                        marker.map_or_else(String::new, |(_, marker)| {
                            render_filename_named_value(marker.name(), name, named_value_separator)
                        })
                    })
                    .unwrap_or_default(),
            ),
            "start" => result.push_str(&format_render_wildcard_clock(
                project,
                settings.start_sample,
            )),
            "end" => result.push_str(&format_render_wildcard_clock(project, settings.end_sample)),
            "length" => result.push_str(
                &crate::project_time::format_project_time(length_seconds).replace(':', "-"),
            ),
            "startbeats" => {
                result.push_str(&format_render_wildcard_beat_position(project, start_tick));
            }
            "endbeats" => {
                result.push_str(&format_render_wildcard_beat_position(project, end_tick));
            }
            "lengthbeats" => result.push_str(&format_render_wildcard_beat_length(
                project, start_tick, end_tick,
            )),
            "starttc" => {
                result.push_str(&format_render_wildcard_timecode(
                    project,
                    settings.start_sample,
                ));
            }
            "endtc" => {
                result.push_str(&format_render_wildcard_timecode(
                    project,
                    settings.end_sample,
                ));
            }
            "startframes" => {
                result.push_str(&format_render_wildcard_frames(start_seconds, frame_rate))
            }
            "endframes" => {
                result.push_str(&format_render_wildcard_frames(end_seconds, frame_rate));
            }
            "lengthframes" => {
                result.push_str(&format_render_wildcard_frames(length_seconds, frame_rate));
            }
            "startseconds" => result.push_str(&format_render_wildcard_seconds(start_seconds)),
            "endseconds" => result.push_str(&format_render_wildcard_seconds(end_seconds)),
            "lengthseconds" => {
                result.push_str(&format_render_wildcard_seconds(length_seconds));
            }
            "lenhh" => result.push_str(&length_components[0]),
            "lenmm" => result.push_str(&length_components[1]),
            "lenss" => result.push_str(&length_components[2]),
            "lentt" => result.push_str(&length_components[3]),
            "format" => result.push_str("wav"),
            "samplerate" => result.push_str(&settings.output_sample_rate.to_string()),
            "sampleratek" => result.push_str(&format_render_wildcard_sample_rate_kilohertz(
                settings.output_sample_rate,
            )),
            "bitdepth" => result.push_str(&format_render_wildcard_bit_depth(
                settings.export_options.sample_format,
            )),
            "channels" => result.push_str(&settings.output_channels.channels().to_string()),
            "tempo" => result.push_str(&bpm_text),
            "timesignature" => result.push_str(&time_signature_text),
            "timesignum" => result.push_str(&time_signature.numerator().to_string()),
            "timesigdenom" => result.push_str(&time_signature.denominator().to_string()),
            "track" => result.push_str(track_name),
            "tracknumber" => result.push_str(&track_number_text),
            "tracknamenumber" | "tracknameornumber" => result.push_str(&track_name_or_number),
            "region" => result.push_str(&region_name_or_number),
            "regionnumber" => result.push_str(&region_number_text),
            "regionname" => result.push_str(&region_name),
            "marker" => result.push_str(&marker_name_or_number),
            "markernumber" => result.push_str(&marker_number_text),
            "markername" => result.push_str(&marker_name),
            "item" => result.push_str(&item_name),
            "itemnumber" => result.push_str(&item_number_text),
            "bpm" => result.push_str(&bpm_text),
            _ => {
                result.push('$');
                result.extend(&characters[token_start..token_end]);
            }
        }
        if known_token {
            let expansion = result.split_off(expansion_start);
            let expansion =
                apply_render_filename_wildcard_modifiers(&expansion, &token_original, &modifiers);
            if expansion.is_empty() {
                if result
                    .chars()
                    .next_back()
                    .is_some_and(is_render_filename_wildcard_separator)
                {
                    result.pop();
                } else if characters
                    .get(wildcard_end)
                    .copied()
                    .is_some_and(is_render_filename_wildcard_separator)
                {
                    index = wildcard_end + 1;
                    continue;
                }
            } else {
                result.push_str(&expansion);
            }
            index = wildcard_end;
        } else {
            index = token_end;
        }
    }
    result
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
    pub(crate) const ALL: [Self; 7] = [
        Self::CustomTimeRange,
        Self::EntireProject,
        Self::TimeSelection,
        Self::AllProjectRegions,
        Self::SelectedProjectRegions,
        Self::AllProjectMarkers,
        Self::SelectedProjectMarkers,
    ];
    pub(crate) const REGION_MATRIX: [Self; 2] =
        [Self::AllProjectRegions, Self::SelectedProjectRegions];
}

impl std::fmt::Display for RenderBounds {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::CustomTimeRange => "Custom time range",
            Self::EntireProject => "Entire project",
            Self::TimeSelection => "Time selection",
            Self::AllProjectRegions => "All project regions",
            Self::SelectedProjectRegions => "Selected regions",
            Self::AllProjectMarkers => "All project markers",
            Self::SelectedProjectMarkers => "Selected markers",
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct RenderDialogState {
    pub(super) bounds: RenderBounds,
    pub(super) custom_start_text: String,
    pub(super) custom_end_text: String,
    custom_range_initialized: bool,
    pub(super) source: RenderSource,
    pub(super) sample_rate: RenderSampleRate,
    pub(super) channels: WavChannelCount,
    pub(super) normalize_peak: bool,
    pub(super) normalize_peak_target_text: String,
    pub(super) tail_enabled: bool,
    pub(super) tail_duration_text: String,
    pub(super) stems_pre_fader: bool,
    pub(super) filename_pattern: String,
    pub(super) wildcard_menu_open: bool,
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
            custom_start_text: "0:00.000".to_owned(),
            custom_end_text: "0:00.000".to_owned(),
            custom_range_initialized: false,
            source: RenderSource::MasterMix,
            sample_rate: RenderSampleRate::Project,
            channels: WavChannelCount::Stereo,
            normalize_peak: false,
            normalize_peak_target_text: "0".to_owned(),
            tail_enabled: false,
            tail_duration_text: "1000".to_owned(),
            stems_pre_fader: false,
            filename_pattern: String::new(),
            wildcard_menu_open: false,
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
    ProjectWavBatch {
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
    pub(super) fn initialize_custom_render_range(&mut self) {
        if self.render_dialog.custom_range_initialized {
            return;
        }
        let start_seconds = self.project.project_time_at_sample(0).unwrap_or(0.0);
        let end_seconds = aaadaw_app::project_content_end_sample(&self.project)
            .ok()
            .and_then(|sample| self.project.project_time_at_sample(sample).ok())
            .unwrap_or(start_seconds);
        self.render_dialog.custom_start_text =
            crate::project_time::format_project_time(start_seconds);
        self.render_dialog.custom_end_text = crate::project_time::format_project_time(end_seconds);
        self.render_dialog.custom_range_initialized = true;
    }

    pub(super) fn custom_render_length_text(&self) -> Option<String> {
        let (start_sample, end_sample) = self.selected_render_bounds().ok()?;
        let duration_seconds =
            (end_sample - start_sample) as f64 / f64::from(self.project.settings().sample_rate());
        Some(crate::project_time::format_project_time(duration_seconds))
    }

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
        let filename_pattern = self.render_dialog.filename_pattern.clone();
        let naming_project_path = self
            .project_path
            .clone()
            .unwrap_or_else(|| project_path.clone());
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
            (RenderSource::SelectedMediaItems, _) => self
                .effective_selected_audio_item_ids()
                .first()
                .copied()
                .map(aaadaw_app::ProjectWavRenderSource::SelectedAudioItem)
                .unwrap_or(aaadaw_app::ProjectWavRenderSource::MasterMix),
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
        if self.render_dialog.source == RenderSource::SelectedMediaItems {
            let selected_items = self.effective_selected_media_item_ids();
            let outputs = match selected_media_item_render_jobs(
                &destination,
                &self.project,
                &selected_items,
                settings,
            ) {
                Ok(outputs) => outputs,
                Err(error) => {
                    self.status = error;
                    return Task::none();
                }
            };
            let outputs = if outputs.len() == 1 {
                vec![(outputs[0].0, destination.clone())]
            } else {
                outputs
            };
            let outputs = match apply_render_filename_pattern(
                outputs,
                &filename_pattern,
                &self.project,
                &naming_project_path,
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
                    "Choose a new render filename; selected-item output already exists: {}",
                    existing.display()
                );
                return Task::none();
            }
            if outputs.iter().any(|(_, path)| path == &project_path) {
                self.status =
                    "Choose selected-item output separate from the project file".to_owned();
                return Task::none();
            }
            if outputs.len() == 1 {
                let (settings, _) = outputs[0];
                return self.enqueue_offline_job(OfflineRenderJob {
                    kind: OfflineRenderKind::ProjectWav {
                        destination,
                        settings,
                    },
                    snapshot: Arc::new(self.project.snapshot()),
                    project_path,
                    project_generation: self.project_generation,
                    label: format!("Render {file_name}"),
                });
            }
            if cfg!(target_os = "android") {
                self.status =
                    "Rendering multiple selected media items requires a desktop build".to_owned();
                return Task::none();
            }
            let job = OfflineRenderJob {
                kind: OfflineRenderKind::ProjectWavBatch { outputs },
                snapshot: Arc::new(self.project.snapshot()),
                project_path,
                project_generation: self.project_generation,
                label: format!("Render {} selected media items", selected_items.len()),
            };
            return self.enqueue_offline_job(job);
        }
        if matches!(
            self.render_dialog.bounds,
            RenderBounds::AllProjectRegions
                | RenderBounds::SelectedProjectRegions
                | RenderBounds::AllProjectMarkers
                | RenderBounds::SelectedProjectMarkers
        ) {
            if cfg!(target_os = "android") {
                self.status = "Rendering project regions or markers to multiple files requires a desktop build".to_owned();
                return Task::none();
            }
            let selected_regions = (self.render_dialog.bounds
                == RenderBounds::SelectedProjectRegions)
                .then(|| self.effective_selected_region_ids())
                .flatten();
            let selected_markers = (self.render_dialog.bounds
                == RenderBounds::SelectedProjectMarkers)
                .then(|| self.effective_selected_marker_ids())
                .flatten();
            let output_result = match self.render_dialog.bounds {
                RenderBounds::AllProjectRegions | RenderBounds::SelectedProjectRegions => {
                    project_region_render_jobs(
                        &destination,
                        &self.project,
                        self.render_dialog.source,
                        &selected_tracks,
                        self.render_dialog.stems_pre_fader,
                        settings,
                        selected_regions.as_ref(),
                    )
                }
                RenderBounds::AllProjectMarkers | RenderBounds::SelectedProjectMarkers => {
                    project_marker_render_jobs(
                        &destination,
                        &self.project,
                        self.render_dialog.source,
                        &selected_tracks,
                        self.render_dialog.stems_pre_fader,
                        settings,
                        selected_markers.as_ref(),
                    )
                }
                _ => unreachable!("only region and marker bounds create render batches"),
            };
            let outputs = match output_result {
                Ok(outputs) => outputs,
                Err(error) => {
                    self.status = error;
                    return Task::none();
                }
            };
            let outputs = match apply_render_filename_pattern(
                outputs,
                &filename_pattern,
                &self.project,
                &naming_project_path,
            ) {
                Ok(outputs) => outputs,
                Err(error) => {
                    self.status = error;
                    return Task::none();
                }
            };
            let outputs_per_range = match self.render_dialog.source {
                RenderSource::MasterMix => 1,
                RenderSource::SelectedMediaItems => 1,
                RenderSource::RegionRenderMatrix => 1,
                RenderSource::SelectedTracksStems | RenderSource::SelectedTracksViaMaster => {
                    selected_tracks.len()
                }
                RenderSource::MasterMixAndSelectedTracks => selected_tracks.len().saturating_add(1),
            }
            .max(1);
            let range_count = if self.render_dialog.source == RenderSource::RegionRenderMatrix {
                self.project
                    .regions()
                    .iter()
                    .filter(|region| {
                        selected_regions
                            .as_ref()
                            .is_none_or(|selected| selected.contains(&region.id()))
                    })
                    .filter(|region| {
                        region.renders_master_mix() || !region.render_track_ids().is_empty()
                    })
                    .count()
            } else {
                outputs.len() / outputs_per_range
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
                    "Choose a new render prefix; batch output already exists: {}",
                    existing.display()
                );
                return Task::none();
            }
            if outputs.iter().any(|(_, path)| path == &project_path) {
                self.status = "Choose batch output names separate from the project file".to_owned();
                return Task::none();
            }
            let label = match self.render_dialog.bounds {
                RenderBounds::AllProjectRegions | RenderBounds::SelectedProjectRegions => {
                    format!("Render {range_count} project regions")
                }
                RenderBounds::AllProjectMarkers | RenderBounds::SelectedProjectMarkers => {
                    format!("Render {range_count} project markers")
                }
                _ => unreachable!("only region and marker bounds create render batches"),
            };
            let job = OfflineRenderJob {
                kind: OfflineRenderKind::ProjectWavBatch { outputs },
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
                    RenderSource::SelectedMediaItems => {
                        unreachable!("selected media item jobs are enqueued above")
                    }
                    RenderSource::RegionRenderMatrix => {
                        unreachable!("region render matrix is submitted as a region batch")
                    }
                },
            ) {
                Ok(outputs) => outputs,
                Err(error) => {
                    self.status = error;
                    return Task::none();
                }
            };
            let outputs = match apply_stem_filename_pattern(
                outputs,
                settings,
                &filename_pattern,
                &self.project,
                &naming_project_path,
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
            let mut outputs = match apply_render_filename_pattern(
                vec![(settings, destination.clone())],
                &filename_pattern,
                &self.project,
                &naming_project_path,
            ) {
                Ok(outputs) => outputs,
                Err(error) => {
                    self.status = error;
                    return Task::none();
                }
            };
            let (settings, destination) = outputs
                .pop()
                .expect("single output pattern expansion preserves its job");
            if destination == project_path {
                self.status = "Choose a WAV destination separate from the project file".to_owned();
                return Task::none();
            }
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
        if self.render_dialog.source == RenderSource::RegionRenderMatrix
            && !matches!(
                self.render_dialog.bounds,
                RenderBounds::AllProjectRegions | RenderBounds::SelectedProjectRegions
            )
        {
            return false;
        }
        if self.render_dialog.source != RenderSource::SelectedMediaItems
            && cfg!(target_os = "android")
            && matches!(
                self.render_dialog.bounds,
                RenderBounds::AllProjectRegions
                    | RenderBounds::SelectedProjectRegions
                    | RenderBounds::AllProjectMarkers
                    | RenderBounds::SelectedProjectMarkers
            )
        {
            return false;
        }
        self.selected_render_settings().is_ok()
    }

    pub(super) fn has_valid_render_source(&self) -> bool {
        match self.render_dialog.source {
            RenderSource::MasterMix => true,
            RenderSource::SelectedMediaItems => self.selected_media_item_source_error().is_none(),
            RenderSource::RegionRenderMatrix => {
                if !matches!(
                    self.render_dialog.bounds,
                    RenderBounds::AllProjectRegions | RenderBounds::SelectedProjectRegions
                ) {
                    return false;
                }
                let selected = (self.render_dialog.bounds == RenderBounds::SelectedProjectRegions)
                    .then(|| self.effective_selected_region_ids())
                    .flatten();
                self.project.regions().iter().any(|region| {
                    selected
                        .as_ref()
                        .is_none_or(|selected| selected.contains(&region.id()))
                        && (region.renders_master_mix() || !region.render_track_ids().is_empty())
                })
            }
            RenderSource::SelectedTracksStems
            | RenderSource::SelectedTracksViaMaster
            | RenderSource::MasterMixAndSelectedTracks => {
                !self.selected_tracks_for_render().is_empty()
            }
        }
    }

    pub(super) fn render_requires_multiple_files(&self) -> bool {
        if self.render_dialog.source == RenderSource::SelectedMediaItems {
            return self.effective_selected_media_item_ids().len() > 1;
        }
        if matches!(
            self.render_dialog.bounds,
            RenderBounds::AllProjectRegions
                | RenderBounds::SelectedProjectRegions
                | RenderBounds::AllProjectMarkers
                | RenderBounds::SelectedProjectMarkers
        ) {
            return true;
        }
        let outputs_per_range = match self.render_dialog.source {
            RenderSource::MasterMix => 1,
            RenderSource::SelectedMediaItems => 1,
            RenderSource::RegionRenderMatrix => 1,
            RenderSource::SelectedTracksStems | RenderSource::SelectedTracksViaMaster => {
                self.selected_render_track_count()
            }
            RenderSource::MasterMixAndSelectedTracks => {
                self.selected_render_track_count().saturating_add(1)
            }
        };
        outputs_per_range > 1
    }

    pub(super) fn selected_render_track_count(&self) -> usize {
        self.selected_tracks_for_render().len()
    }

    pub(super) fn render_source_description(&self) -> &'static str {
        match self.render_dialog.source {
            RenderSource::MasterMix => "Renders the project through the Master mix.",
            RenderSource::SelectedMediaItems => {
                "Renders each selected audio item from its trimmed media source as a separate WAV."
            }
            RenderSource::SelectedTracksStems => {
                "Renders each selected track as a separate stem without the Master mix gain."
            }
            RenderSource::SelectedTracksViaMaster => {
                "Renders each selected track through the Master as its own WAV stem."
            }
            RenderSource::MasterMixAndSelectedTracks => {
                "Renders one Master mix WAV plus one stem for each selected track."
            }
            RenderSource::RegionRenderMatrix => {
                "Renders the Master mix and tracks assigned to each project region."
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

    pub(super) fn render_bounds_summary(&self) -> Option<String> {
        if self.render_dialog.source == RenderSource::SelectedMediaItems {
            return None;
        }
        let count = match self.render_dialog.bounds {
            RenderBounds::AllProjectRegions => self.project.regions().len(),
            RenderBounds::SelectedProjectRegions => self
                .effective_selected_region_ids()
                .map_or(self.project.regions().len(), |selected| selected.len()),
            RenderBounds::AllProjectMarkers | RenderBounds::SelectedProjectMarkers => {
                let selected = if self.render_dialog.bounds == RenderBounds::SelectedProjectMarkers
                {
                    self.effective_selected_marker_ids()
                } else {
                    None
                };
                let Ok(project_end_sample) = aaadaw_app::project_content_end_sample(&self.project)
                else {
                    return Some("No renderable markers".to_owned());
                };
                self.project
                    .markers()
                    .iter()
                    .enumerate()
                    .filter(|(_, marker)| {
                        selected
                            .as_ref()
                            .is_none_or(|selected| selected.contains(&marker.id()))
                    })
                    .filter(|(index, marker)| {
                        let Ok(start_sample) = self.project.sample_at_tick(marker.position_ticks())
                        else {
                            return false;
                        };
                        let end_sample = match self.project.markers().get(index + 1) {
                            Some(next_marker) => self
                                .project
                                .sample_at_tick(next_marker.position_ticks())
                                .unwrap_or(start_sample),
                            None => project_end_sample,
                        };
                        end_sample > start_sample
                    })
                    .count()
            }
            _ => return None,
        };
        let noun = if matches!(
            self.render_dialog.bounds,
            RenderBounds::AllProjectRegions | RenderBounds::SelectedProjectRegions
        ) {
            "region"
        } else {
            "marker"
        };
        Some(format!(
            "{count} {noun}{}",
            if count == 1 { "" } else { "s" }
        ))
    }

    pub(super) fn selected_media_items_summary(&self) -> String {
        let count = self.effective_selected_media_item_ids().len();
        format!("{count} media item{}", if count == 1 { "" } else { "s" })
    }

    pub(super) fn selected_media_item_source_error(&self) -> Option<&'static str> {
        if self
            .project
            .midi_items()
            .iter()
            .any(|item| self.timeline.selected_items.contains(&item.id()))
        {
            return Some(
                "Selected media item rendering currently supports audio items; deselect MIDI items.",
            );
        }
        if self.effective_selected_audio_item_ids().is_empty() {
            return Some("Select one or more audio media items in Arrange to render them.");
        }
        None
    }

    fn effective_selected_media_item_ids(&self) -> HashSet<ItemId> {
        self.project
            .audio_items()
            .iter()
            .map(|item| item.id())
            .chain(self.project.midi_items().iter().map(|item| item.id()))
            .filter(|item_id| self.timeline.selected_items.contains(item_id))
            .collect()
    }

    fn effective_selected_audio_item_ids(&self) -> Vec<ItemId> {
        self.project
            .audio_items()
            .iter()
            .filter(|item| self.timeline.selected_items.contains(&item.id()))
            .map(|item| item.id())
            .collect()
    }

    fn selected_media_item_union_range(&self) -> Result<(u64, u64), String> {
        if let Some(error) = self.selected_media_item_source_error() {
            return Err(error.to_owned());
        }
        let mut selected = self
            .project
            .audio_items()
            .iter()
            .filter(|item| self.timeline.selected_items.contains(&item.id()));
        let first = selected
            .next()
            .expect("selection validation requires one audio media item");
        let (start, end) = selected.fold(
            (first.start_sample(), first.end_sample()),
            |(start, end), item| (start.min(item.start_sample()), end.max(item.end_sample())),
        );
        (end > start)
            .then_some((start, end))
            .ok_or_else(|| "Selected audio items must span at least one project sample".to_owned())
    }

    fn selected_render_settings(&self) -> Result<ResolvedRenderSettings, String> {
        if self.render_dialog.source == RenderSource::RegionRenderMatrix
            && !matches!(
                self.render_dialog.bounds,
                RenderBounds::AllProjectRegions | RenderBounds::SelectedProjectRegions
            )
        {
            return Err("Region Render Matrix source requires project region bounds".to_owned());
        }
        let tail_milliseconds = if self.render_dialog.source == RenderSource::SelectedMediaItems {
            0
        } else if self.render_dialog.tail_enabled {
            parse_render_tail_milliseconds(&self.render_dialog.tail_duration_text)
                .map_err(str::to_owned)?
        } else {
            0
        };
        let (start_sample, end_sample) =
            if self.render_dialog.source == RenderSource::SelectedMediaItems {
                self.selected_media_item_union_range()?
            } else {
                self.selected_render_bounds()?
            };
        aaadaw_app::render_range_length_samples_with_tail_milliseconds(
            self.project.settings().sample_rate(),
            start_sample,
            end_sample,
            tail_milliseconds,
        )
        .map_err(|error| format!("Render range is empty or too large: {error}"))?;
        let selected_tracks = match self.render_dialog.source {
            RenderSource::MasterMix
            | RenderSource::SelectedMediaItems
            | RenderSource::RegionRenderMatrix => Vec::new(),
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
        let output_sample_rate = self.render_output_sample_rate();
        if self.render_dialog.source != RenderSource::SelectedMediaItems
            && matches!(
                self.render_dialog.bounds,
                RenderBounds::AllProjectMarkers | RenderBounds::SelectedProjectMarkers
            )
        {
            let selected_markers = (self.render_dialog.bounds
                == RenderBounds::SelectedProjectMarkers)
                .then(|| self.effective_selected_marker_ids())
                .flatten();
            let validation_settings = aaadaw_app::ProjectWavRenderSettings {
                master_ceiling: self.audio_settings.master_output_ceiling,
                export_options: self.wav_export_options,
                output_sample_rate,
                output_channels: self.render_dialog.channels,
                source: aaadaw_app::ProjectWavRenderSource::MasterMix,
                normalize_peak_target_centi_dbfs,
                start_sample,
                end_sample,
                tail_milliseconds,
                normalize_peak: self.render_dialog.normalize_peak,
            };
            project_marker_render_jobs(
                Path::new("marker-range-validation.wav"),
                &self.project,
                RenderSource::MasterMix,
                &[],
                false,
                validation_settings,
                selected_markers.as_ref(),
            )?;
        }
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

    fn effective_selected_region_ids(&self) -> Option<HashSet<RegionId>> {
        let selected = self
            .project
            .regions()
            .iter()
            .filter_map(|region| {
                self.selected_render_regions
                    .contains(&region.id())
                    .then_some(region.id())
            })
            .collect::<HashSet<_>>();
        (!selected.is_empty()).then_some(selected)
    }

    fn effective_selected_marker_ids(&self) -> Option<HashSet<MarkerId>> {
        let selected = self
            .project
            .markers()
            .iter()
            .filter_map(|marker| {
                self.selected_render_markers
                    .contains(&marker.id())
                    .then_some(marker.id())
            })
            .collect::<HashSet<_>>();
        (!selected.is_empty()).then_some(selected)
    }

    fn selected_render_bounds(&self) -> Result<(u64, u64), String> {
        match self.render_dialog.bounds {
            RenderBounds::EntireProject => aaadaw_app::project_content_end_sample(&self.project)
                .map(|end_sample| (0, end_sample))
                .map_err(|error| format!("Could not determine render bounds: {error}")),
            RenderBounds::CustomTimeRange => {
                let start_seconds =
                    crate::project_time::parse_project_time(&self.render_dialog.custom_start_text)
                        .map_err(|error| format!("Invalid custom start time: {error}"))?;
                let end_seconds =
                    crate::project_time::parse_project_time(&self.render_dialog.custom_end_text)
                        .map_err(|error| format!("Invalid custom end time: {error}"))?;
                let start_sample = self
                    .project
                    .sample_at_project_time(start_seconds)
                    .map_err(|error| format!("Could not convert custom start: {error}"))?;
                let end_sample = self
                    .project
                    .sample_at_project_time(end_seconds)
                    .map_err(|error| format!("Could not convert custom end: {error}"))?;
                if end_sample <= start_sample {
                    return Err(
                        "Custom time range end must be after its start by at least one project sample"
                            .to_owned(),
                    );
                }
                Ok((start_sample, end_sample))
            }
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
            RenderBounds::AllProjectRegions | RenderBounds::SelectedProjectRegions => {
                let selected_regions = (self.render_dialog.bounds
                    == RenderBounds::SelectedProjectRegions)
                    .then(|| self.effective_selected_region_ids())
                    .flatten();
                let regions = self
                    .project
                    .regions()
                    .iter()
                    .filter(|region| {
                        selected_regions
                            .as_ref()
                            .is_none_or(|selected| selected.contains(&region.id()))
                    })
                    .collect::<Vec<_>>();
                if regions.is_empty() {
                    return Err(
                        "Add at least one project region before rendering regions".to_owned()
                    );
                }
                let start_sample = regions
                    .iter()
                    .map(|region| {
                        self.project
                            .sample_at_tick(region.start_ticks())
                            .map_err(|error| format!("Could not convert region start: {error}"))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .min()
                    .expect("non-empty regions have a first start");
                let end_sample = regions
                    .iter()
                    .map(|region| {
                        self.project
                            .sample_at_tick(region.end_ticks())
                            .map_err(|error| format!("Could not convert region end: {error}"))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .max()
                    .expect("non-empty regions have a last end");
                if end_sample <= start_sample {
                    return Err("Project regions must span at least one project sample".to_owned());
                }
                Ok((start_sample, end_sample))
            }
            RenderBounds::AllProjectMarkers | RenderBounds::SelectedProjectMarkers => {
                let selected_markers = (self.render_dialog.bounds
                    == RenderBounds::SelectedProjectMarkers)
                    .then(|| self.effective_selected_marker_ids())
                    .flatten();
                if self.project.markers().is_empty() {
                    return Err(
                        "Add at least one project marker before rendering markers".to_owned()
                    );
                }
                let end_sample = aaadaw_app::project_content_end_sample(&self.project)
                    .map_err(|error| format!("Could not determine project render end: {error}"))?;
                let mut ranges = Vec::new();
                for (index, marker) in self.project.markers().iter().enumerate() {
                    if selected_markers
                        .as_ref()
                        .is_some_and(|selected| !selected.contains(&marker.id()))
                    {
                        continue;
                    }
                    let start_sample = self
                        .project
                        .sample_at_tick(marker.position_ticks())
                        .map_err(|error| format!("Could not convert marker position: {error}"))?;
                    let marker_end_sample = match self.project.markers().get(index + 1) {
                        Some(next_marker) => self
                            .project
                            .sample_at_tick(next_marker.position_ticks())
                            .map_err(|error| {
                                format!("Could not convert next marker position: {error}")
                            })?,
                        None => end_sample,
                    };
                    if marker_end_sample > start_sample {
                        ranges.push((start_sample, marker_end_sample));
                    }
                }
                let Some(start_sample) = ranges.iter().map(|range| range.0).min() else {
                    return Err("Project markers have no non-empty render ranges".to_owned());
                };
                let end_sample = ranges
                    .iter()
                    .map(|range| range.1)
                    .max()
                    .expect("non-empty marker ranges have a last end");
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
                OfflineRenderKind::ProjectWavBatch { outputs } => outputs
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
                OfflineRenderKind::ProjectWavBatch { outputs } => {
                    let outputs = outputs.clone();
                    let result_path = outputs
                        .first()
                        .map(|(_, path)| path.clone())
                        .unwrap_or_default();
                    Task::perform(
                        run_blocking("aaadaw-offline-batch", move || {
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
    use aaadaw_core::{DawAction, ProjectNotes};

    #[test]
    fn render_filename_patterns_expand_native_wildcard_contexts_safely() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Drums/Kit".to_owned(),
            })
            .expect("filename fixture track should be created");
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: "fixture.wav".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 96_000,
            })
            .expect("filename fixture item should be inserted");
        project
            .apply(DawAction::InsertProjectRegion {
                start_ticks: 0,
                end_ticks: 3_840,
                name: "Intro".to_owned(),
            })
            .expect("filename fixture region should be inserted");
        project
            .apply(DawAction::InsertProjectMarker {
                position_ticks: 0,
                name: "Start".to_owned(),
            })
            .expect("filename fixture marker should be inserted");
        project
            .apply(DawAction::SetProjectNotes {
                notes: ProjectNotes {
                    title: "Session title".to_owned(),
                    author: "Render author".to_owned(),
                    text: "Mix notes".to_owned(),
                    ..ProjectNotes::default()
                },
            })
            .expect("filename fixture project notes should be set");
        let settings = |source| aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: 48_000,
            output_channels: WavChannelCount::Stereo,
            source,
            start_sample: 0,
            end_sample: 96_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let project_path = Path::new("/projects/session.aaadaw");
        let output = Path::new("/renders/base.wav").to_owned();
        let track_output = apply_render_filename_pattern(
            vec![(
                settings(aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(
                    track_id,
                )),
                output.clone(),
            )],
            "$project-$title-$author-$projectnotes-$region-$regionnumber-$marker-$markernumber-$track-$tracknumber-$tracknamenumber-$tracknameornumber-$bpm-$tempo-$timesignature-$timesignum-$timesigdenom",
            &project,
            project_path,
        )
        .expect("project, track, region, marker and tempo tokens should resolve");
        assert_eq!(
            track_output[0].1,
            Path::new(
                "/renders/session-Session title-Render author-Mix notes-Intro-01-Start-01-Drums_Kit-01-Drums_Kit-Drums_Kit-120-120-4-4-4-4.wav"
            )
        );

        let item_id = project.audio_items()[0].id();
        let item_output = apply_render_filename_pattern(
            vec![(
                settings(aaadaw_app::ProjectWavRenderSource::SelectedAudioItem(
                    item_id,
                )),
                output,
            )],
            "$region-$item",
            &project,
            project_path,
        )
        .expect("region and item tokens should resolve");
        assert_eq!(item_output[0].1, Path::new("/renders/Intro-item_01.wav"));

        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: String::new(),
            })
            .expect("an unnamed filename fixture track should be created");
        let unnamed_track_id = project.tracks()[1].id();
        let unnamed_track_output = apply_render_filename_pattern(
            vec![(
                settings(aaadaw_app::ProjectWavRenderSource::SelectedTrackStem(
                    unnamed_track_id,
                )),
                Path::new("/renders/base.wav").to_owned(),
            )],
            "$tracknamenumber-$tracknameornumber",
            &project,
            project_path,
        )
        .expect("unnamed tracks should resolve to their numbered display name");
        assert_eq!(
            unnamed_track_output[0].1,
            Path::new("/renders/Track 02-Track 02.wav")
        );
    }

    #[test]
    fn render_wildcard_picker_lists_and_inserts_supported_tokens() {
        assert_eq!(
            RenderFilenameWildcard::ALL.map(RenderFilenameWildcard::token),
            [
                "$project",
                "$title",
                "$author",
                "$projectnotes",
                "$projectnotes(name)",
                "$bpm",
                "$tempo",
                "$timesignature",
                "$timesignum",
                "$timesigdenom",
                "$track",
                "$tracknumber",
                "$tracknamenumber",
                "$region",
                "$regionnumber",
                "$regionnumber[N]",
                "$regionname",
                "$marker",
                "$markernumber",
                "$markernumber[N]",
                "$markername",
                "$item",
                "$itemnumber",
                "$start",
                "$end",
                "$length",
                "$startbeats",
                "$endbeats",
                "$lengthbeats",
                "$starttc",
                "$endtc",
                "$startframes",
                "$endframes",
                "$lengthframes",
                "$startseconds",
                "$endseconds",
                "$lengthseconds",
                "$lenhh",
                "$lenmm",
                "$lenss",
                "$lentt",
                "$format",
                "$samplerate",
                "$sampleratek",
                "$bitdepth",
                "$channels",
                "$region(name)",
                "$marker(name)",
            ]
        );

        let mut app = App::default();
        let _ = app.update(Message::ToggleRenderWildcardMenu);
        assert!(app.render_dialog.wildcard_menu_open);

        let _ = app.update(Message::SetRenderFilenamePattern("$project-".to_owned()));
        let _ = app.update(Message::InsertRenderFilenameWildcard(
            RenderFilenameWildcard::Track,
        ));
        assert_eq!(app.render_dialog.filename_pattern, "$project-$track");
        assert!(!app.render_dialog.wildcard_menu_open);

        let _ = app.update(Message::InsertRenderFilenameWildcard(
            RenderFilenameWildcard::Project,
        ));
        assert_eq!(
            app.render_dialog.filename_pattern,
            "$project-$track$project"
        );
        let _ = app.update(Message::InsertRenderFilenameWildcard(
            RenderFilenameWildcard::ProjectNotesNamed,
        ));
        assert_eq!(
            app.render_dialog.filename_pattern,
            "$project-$track$project$projectnotes(name)"
        );
    }

    #[test]
    fn render_filename_region_and_marker_numbers_match_native_start_number_rules() {
        let mut project = Project::default();
        for (start_ticks, end_ticks, name) in [(0, 1_920, "First"), (1_920, 3_840, "Second")] {
            project
                .apply(DawAction::InsertProjectRegion {
                    start_ticks,
                    end_ticks,
                    name: name.to_owned(),
                })
                .expect("numbered region should be inserted");
        }
        for (position_ticks, name) in [(0, "First"), (1_920, "Second"), (3_840, "End")] {
            project
                .apply(DawAction::InsertProjectMarker {
                    position_ticks,
                    name: name.to_owned(),
                })
                .expect("numbered marker should be inserted");
        }
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: 48_000,
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: project
                .sample_at_tick(1_920)
                .expect("start tick should convert"),
            end_sample: project
                .sample_at_tick(3_840)
                .expect("end tick should convert"),
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let output = apply_render_filename_pattern(
            vec![(settings, Path::new("/renders/base.wav").to_owned())],
            "$regionnumber[001]-$markernumber[005]",
            &project,
            Path::new("/projects/numbered.aaadaw"),
        )
        .expect("region and marker start-number modifiers should expand");
        assert_eq!(output[0].1, Path::new("/renders/002-006.wav"));

        let output = apply_render_filename_pattern(
            vec![(settings, Path::new("/renders/base.wav").to_owned())],
            "$regionnumber[01]-$markernumber[01]",
            &project,
            Path::new("/projects/numbered.aaadaw"),
        )
        .expect("two-digit number modifiers should retain their requested width");
        assert_eq!(output[0].1, Path::new("/renders/02-02.wav"));
    }

    #[test]
    fn render_filename_regionname_and_markername_match_native_name_only_values() {
        let mut project = Project::default();
        for (start_ticks, end_ticks, name) in [(0, 1_920, "Verse"), (1_920, 3_840, "Chorus")] {
            project
                .apply(DawAction::InsertProjectRegion {
                    start_ticks,
                    end_ticks,
                    name: name.to_owned(),
                })
                .expect("named region should be inserted");
        }
        for (position_ticks, name) in [(0, "Start"), (1_920, "Next"), (3_840, "End")] {
            project
                .apply(DawAction::InsertProjectMarker {
                    position_ticks,
                    name: name.to_owned(),
                })
                .expect("named marker should be inserted");
        }

        for (start_ticks, end_ticks, expected_name) in [
            (0, 1_920, "Verse-Start.wav"),
            (1_920, 3_840, "Chorus-Next.wav"),
        ] {
            let settings = aaadaw_app::ProjectWavRenderSettings {
                master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
                export_options: aaadaw_app::WavExportOptions::default(),
                output_sample_rate: 48_000,
                output_channels: WavChannelCount::Stereo,
                source: aaadaw_app::ProjectWavRenderSource::MasterMix,
                start_sample: project
                    .sample_at_tick(start_ticks)
                    .expect("start tick should convert"),
                end_sample: project
                    .sample_at_tick(end_ticks)
                    .expect("end tick should convert"),
                tail_milliseconds: 0,
                normalize_peak: false,
                normalize_peak_target_centi_dbfs: 0,
            };
            let output = apply_render_filename_pattern(
                vec![(settings, Path::new("/renders/base.wav").to_owned())],
                "$regionname-$markername",
                &project,
                Path::new("/projects/named.aaadaw"),
            )
            .expect("name-only Region and Marker values should expand");
            assert_eq!(output[0].1, Path::new("/renders").join(expected_name));
        }

        let mut unnamed_project = Project::default();
        unnamed_project
            .apply(DawAction::InsertProjectRegion {
                start_ticks: 0,
                end_ticks: 1_920,
                name: String::new(),
            })
            .expect("unnamed region should be inserted");
        for position_ticks in [0, 1_920] {
            unnamed_project
                .apply(DawAction::InsertProjectMarker {
                    position_ticks,
                    name: String::new(),
                })
                .expect("unnamed marker should be inserted");
        }
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: 48_000,
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 48_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        for (pattern, expected_name) in [
            ("$regionname-$regionnumber", "01.wav"),
            ("$markername-$markernumber", "01.wav"),
        ] {
            let output = apply_render_filename_pattern(
                vec![(settings, Path::new("/renders/base.wav").to_owned())],
                pattern,
                &unnamed_project,
                Path::new("/projects/unnamed.aaadaw"),
            )
            .expect("an empty name-only value should not add a number fallback");
            assert_eq!(output[0].1, Path::new("/renders").join(expected_name));
        }
    }

    #[test]
    fn render_filename_region_and_marker_named_values_match_native_key_value_rules() {
        let mut project = Project::default();
        for (start_ticks, end_ticks, name) in
            [(0, 1_920, "role=Verse"), (1_920, 3_840, "role=Chorus")]
        {
            project
                .apply(DawAction::InsertProjectRegion {
                    start_ticks,
                    end_ticks,
                    name: name.to_owned(),
                })
                .expect("named-value region should be inserted");
        }
        for (position_ticks, name) in [(0, "role=Start"), (1_920, "role=Next"), (3_840, "role=End")]
        {
            project
                .apply(DawAction::InsertProjectMarker {
                    position_ticks,
                    name: name.to_owned(),
                })
                .expect("named-value marker should be inserted");
        }

        for (start_ticks, end_ticks, expected_name) in [
            (0, 1_920, "Verse-Start.wav"),
            (1_920, 3_840, "Chorus-Next.wav"),
        ] {
            let settings = aaadaw_app::ProjectWavRenderSettings {
                master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
                export_options: aaadaw_app::WavExportOptions::default(),
                output_sample_rate: 48_000,
                output_channels: WavChannelCount::Stereo,
                source: aaadaw_app::ProjectWavRenderSource::MasterMix,
                start_sample: project
                    .sample_at_tick(start_ticks)
                    .expect("start tick should convert"),
                end_sample: project
                    .sample_at_tick(end_ticks)
                    .expect("end tick should convert"),
                tail_milliseconds: 0,
                normalize_peak: false,
                normalize_peak_target_centi_dbfs: 0,
            };
            let output = apply_render_filename_pattern(
                vec![(settings, Path::new("/renders/base.wav").to_owned())],
                "$region(role)-$marker(role)",
                &project,
                Path::new("/projects/named-values.aaadaw"),
            )
            .expect("Region and Marker named values should expand");
            assert_eq!(output[0].1, Path::new("/renders").join(expected_name));

            let missing_key_output = apply_render_filename_pattern(
                vec![(settings, Path::new("/renders/base.wav").to_owned())],
                "$region(missing)-$marker(role)",
                &project,
                Path::new("/projects/named-values.aaadaw"),
            )
            .expect("missing named values should expand to empty");
            let marker_name = if start_ticks == 0 {
                "Start.wav"
            } else {
                "Next.wav"
            };
            assert_eq!(
                missing_key_output[0].1,
                Path::new("/renders").join(marker_name)
            );
        }
    }

    #[test]
    fn render_filename_region_and_marker_named_values_accept_native_separators() {
        let mut project = Project::default();
        project
            .apply(DawAction::InsertProjectRegion {
                start_ticks: 0,
                end_ticks: 1_920,
                name: "role=Verse|alternate=Intro".to_owned(),
            })
            .expect("separator fixture region should be inserted");
        project
            .apply(DawAction::InsertProjectMarker {
                position_ticks: 0,
                name: "role=Start|alternate=Downbeat".to_owned(),
            })
            .expect("separator fixture marker should be inserted");
        project
            .apply(DawAction::InsertProjectMarker {
                position_ticks: 1_920,
                name: "role=End|alternate=Outro".to_owned(),
            })
            .expect("separator fixture end marker should be inserted");
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: 48_000,
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: project
                .sample_at_tick(1_920)
                .expect("region end tick should convert"),
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };

        for (pattern, expected_name) in [
            (
                "$region(role)-$marker(role)",
                "Verse_alternate=Intro-Start_alternate=Downbeat.wav",
            ),
            ("$region(role)[|]-$marker(role)[|]", "Verse-Start.wav"),
            (
                "$region(alternate)[|]-$marker(alternate)[|]",
                "Intro-Downbeat.wav",
            ),
        ] {
            let output = apply_render_filename_pattern(
                vec![(settings, Path::new("/renders/base.wav").to_owned())],
                pattern,
                &project,
                Path::new("/projects/named-values.aaadaw"),
            )
            .expect("native named-value separators should expand");
            assert_eq!(output[0].1, Path::new("/renders").join(expected_name));
        }
    }

    #[test]
    fn render_filename_unnamed_regions_and_markers_fall_back_to_native_numbers() {
        let mut project = Project::default();
        for (start_ticks, end_ticks) in [(0, 1_920), (1_920, 3_840)] {
            project
                .apply(DawAction::InsertProjectRegion {
                    start_ticks,
                    end_ticks,
                    name: String::new(),
                })
                .expect("unnamed region should be inserted");
        }
        for position_ticks in [0, 1_920, 3_840] {
            project
                .apply(DawAction::InsertProjectMarker {
                    position_ticks,
                    name: String::new(),
                })
                .expect("unnamed marker should be inserted");
        }

        for (start_ticks, end_ticks, expected_name) in
            [(0, 1_920, "01-01.wav"), (1_920, 3_840, "02-02.wav")]
        {
            let settings = aaadaw_app::ProjectWavRenderSettings {
                master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
                export_options: aaadaw_app::WavExportOptions::default(),
                output_sample_rate: 48_000,
                output_channels: WavChannelCount::Stereo,
                source: aaadaw_app::ProjectWavRenderSource::MasterMix,
                start_sample: project
                    .sample_at_tick(start_ticks)
                    .expect("start tick should convert"),
                end_sample: project
                    .sample_at_tick(end_ticks)
                    .expect("end tick should convert"),
                tail_milliseconds: 0,
                normalize_peak: false,
                normalize_peak_target_centi_dbfs: 0,
            };
            let output = apply_render_filename_pattern(
                vec![(settings, Path::new("/renders/base.wav").to_owned())],
                "$region-$marker",
                &project,
                Path::new("/projects/unnamed.aaadaw"),
            )
            .expect("unnamed Region and Marker values should use their native numbers");
            assert_eq!(output[0].1, Path::new("/renders").join(expected_name));
        }
    }

    #[test]
    fn render_filename_named_project_notes_match_native_name_value_rules() {
        let mut project = Project::default();
        project
            .apply(DawAction::SetProjectNotes {
                notes: ProjectNotes {
                    text: "first=john\nlast=doe".to_owned(),
                    ..ProjectNotes::default()
                },
            })
            .expect("named project notes fixture should be set");
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: 48_000,
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 48_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let output = apply_render_filename_pattern(
            vec![(settings, Path::new("/renders/base.wav").to_owned())],
            "$projectnotes(first)-$projectnotes(last)-$projectnotes(missing)",
            &project,
            Path::new("/projects/named-notes.aaadaw"),
        )
        .expect("named project note values should expand");
        assert_eq!(output[0].1, Path::new("/renders/john-doe.wav"));

        project
            .apply(DawAction::SetProjectNotes {
                notes: ProjectNotes {
                    text: "first=john;last=doe".to_owned(),
                    ..ProjectNotes::default()
                },
            })
            .expect("delimiter-separated project notes fixture should be set");
        let output = apply_render_filename_pattern(
            vec![(settings, Path::new("/renders/base.wav").to_owned())],
            "$projectnotes(last)[;]",
            &project,
            Path::new("/projects/named-notes.aaadaw"),
        )
        .expect("unsupported project-note separator suffix should remain literal");
        assert_eq!(output[0].1, Path::new("/renders/[;].wav"));
    }

    #[test]
    fn render_filename_position_and_length_wildcards_match_native_formats() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Position fixture".to_owned(),
            })
            .expect("position fixture track should be created");
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: "position-fixture.wav".to_owned(),
                start_sample: 102_000,
                source_offset_samples: 0,
                length_samples: 59_232,
            })
            .expect("fractional position fixture item should be inserted");
        let item_id = project.audio_items()[0].id();
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: 48_000,
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::SelectedAudioItem(item_id),
            start_sample: 102_000,
            end_sample: 161_232,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let output = apply_render_filename_pattern(
            vec![(settings, Path::new("/renders/base.wav").to_owned())],
            "$start-$end-$length-$startbeats-$endbeats-$lengthbeats-$starttc-$endtc-$startframes-$endframes-$lengthframes-$startseconds-$endseconds-$lengthseconds-$lenhh-$lenmm-$lenss-$lentt",
            &project,
            Path::new("/projects/position.aaadaw"),
        )
        .expect("position and length wildcard values should expand");
        assert_eq!(
            output[0].1,
            Path::new(
                "/renders/0-02.125-0-03.359-0-01.234-2.1.25-2.3.72-0.2.47-00.00.02.03-00.00.03.10-63-100-37-2.125-3.359-1.234-0-0-1-234.wav"
            )
        );
    }

    #[test]
    fn render_filename_output_wildcards_match_native_format_and_channel_values() {
        let project = Project::default();
        let mut settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: 44_100,
            output_channels: WavChannelCount::Mono,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 48_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        settings.export_options.sample_format = WavSampleFormat::Pcm16;

        let output = apply_render_filename_pattern(
            vec![(settings, Path::new("/renders/base.wav").to_owned())],
            "$format-$samplerate-$sampleratek-$bitdepth-$channels",
            &project,
            Path::new("/projects/format.aaadaw"),
        )
        .expect("output-format wildcards should expand");
        assert_eq!(output[0].1, Path::new("/renders/wav-44100-44.1-16-1.wav"));

        settings.output_sample_rate = 22_050;
        let output = apply_render_filename_pattern(
            vec![(settings, Path::new("/renders/base.wav").to_owned())],
            "$samplerate-$sampleratek-$channels",
            &project,
            Path::new("/projects/format.aaadaw"),
        )
        .expect("fractional kilohertz sample rates should retain precision");
        assert_eq!(output[0].1, Path::new("/renders/22050-22.05-1.wav"));

        settings.output_sample_rate = 48_000;
        settings.output_channels = WavChannelCount::Stereo;
        for (sample_format, bit_depth) in [
            (WavSampleFormat::Pcm8, "8"),
            (WavSampleFormat::Pcm16, "16"),
            (WavSampleFormat::Pcm24, "24"),
            (WavSampleFormat::Pcm32, "32"),
            (WavSampleFormat::Float32, "32"),
            (WavSampleFormat::Float64, "64"),
            (WavSampleFormat::ImaAdpcm4, "4"),
            (WavSampleFormat::Cadpcm2, "2"),
            (WavSampleFormat::MuLaw8, "8"),
        ] {
            settings.export_options.sample_format = sample_format;
            let output = apply_render_filename_pattern(
                vec![(settings, Path::new("/renders/base.wav").to_owned())],
                "$bitdepth-$sampleratek-$channels",
                &project,
                Path::new("/projects/format.aaadaw"),
            )
            .expect("supported WAV bit depth should expand");
            assert_eq!(
                output[0].1.file_name().and_then(|name| name.to_str()),
                Some(format!("{bit_depth}-48-2.wav").as_str())
            );
        }
    }

    #[test]
    fn render_filename_wildcard_case_and_character_rules_match_native_help() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Drums Kit".to_owned(),
            })
            .expect("modifier fixture track should be created");
        let track_id = project.tracks()[0].id();
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: 48_000,
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::SelectedTrackStem(track_id),
            start_sample: 0,
            end_sample: 48_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };

        let output = apply_render_filename_pattern(
            vec![(settings, Path::new("/renders/base.wav").to_owned())],
            "$Track|$TRACK|$tRACK|$track< >|$track< _>|$track[<5]",
            &project,
            Path::new("/projects/modifiers.aaadaw"),
        )
        .expect("native wildcard text modifiers should expand");
        assert_eq!(
            output[0].1,
            Path::new("/renders/Drums Kit_DRUMS KIT_drums kit_DrumsKit_Drums_Kit_Drums.wav")
        );
    }

    #[test]
    fn render_filename_item_number_matches_the_selected_items_project_order() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Item number fixture".to_owned(),
            })
            .expect("item number fixture track should be created");
        let track_id = project.tracks()[0].id();
        for start_sample in [0, 48_000, 96_000] {
            project
                .apply(DawAction::InsertAudioItem {
                    track_id,
                    media_ref: "fixture.wav".to_owned(),
                    start_sample,
                    source_offset_samples: 0,
                    length_samples: 48_000,
                })
                .expect("item number fixture should be inserted");
        }
        let item_id = project.audio_items()[1].id();
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: 48_000,
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::SelectedAudioItem(item_id),
            start_sample: 48_000,
            end_sample: 96_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let output = apply_render_filename_pattern(
            vec![(settings, Path::new("/renders/base.wav").to_owned())],
            "$item-$itemnumber",
            &project,
            Path::new("/projects/item-number.aaadaw"),
        )
        .expect("the selected item's identity and number should expand");
        assert_eq!(output[0].1, Path::new("/renders/item_02-02.wav"));
    }

    #[test]
    fn render_filename_empty_wildcards_omit_the_nearest_native_separator() {
        let project = Project::default();
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: 48_000,
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 48_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let output = apply_render_filename_pattern(
            vec![(settings, Path::new("/renders/base.wav").to_owned())],
            "$region-$track.$marker_$format",
            &project,
            Path::new("/projects/empty.aaadaw"),
        )
        .expect("empty native wildcard values should omit adjacent separators");
        assert_eq!(output[0].1, Path::new("/renders/Master_wav.wav"));

        let output = apply_render_filename_pattern(
            vec![(settings, Path::new("/renders/base.wav").to_owned())],
            "$marker_$format",
            &project,
            Path::new("/projects/empty.aaadaw"),
        )
        .expect("a leading empty wildcard should omit its following separator");
        assert_eq!(output[0].1, Path::new("/renders/wav.wav"));
    }

    #[test]
    fn render_filename_patterns_preserve_unknown_tokens_and_reject_duplicate_targets() {
        let project = Project::default();
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: 48_000,
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 48_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let project_path = Path::new("session.aaadaw");
        let first = PathBuf::from("renders/first.wav");
        let second = PathBuf::from("renders/second.wav");
        let unchanged = apply_render_filename_pattern(
            vec![(settings, first.clone())],
            "  ",
            &project,
            project_path,
        )
        .expect("blank pattern should preserve generated destinations");
        assert_eq!(unchanged[0].1, first);

        let unknown = apply_render_filename_pattern(
            vec![(settings, second)],
            "$project-$unknown",
            &project,
            project_path,
        )
        .expect("unknown wildcard text should remain visible");
        assert_eq!(unknown[0].1, Path::new("renders/session-$unknown.wav"));

        let duplicate = apply_render_filename_pattern(
            vec![
                (settings, PathBuf::from("renders/first.wav")),
                (settings, PathBuf::from("renders/second.wav")),
            ],
            "$project",
            &project,
            project_path,
        )
        .expect_err("duplicate wildcard output paths must be rejected");
        assert!(duplicate.contains("duplicate output names"));
    }

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
                RenderSource::SelectedMediaItems,
                RenderSource::RegionRenderMatrix,
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
        assert_eq!(
            RenderSource::SelectedMediaItems.to_string(),
            "Selected media items"
        );
        assert_eq!(
            RenderSource::RegionRenderMatrix.to_string(),
            "Region Render Matrix"
        );
        let mut app = App::default();
        let _task = app.update(Message::SetRenderSource(RenderSource::RegionRenderMatrix));
        assert_eq!(app.render_dialog.bounds, RenderBounds::AllProjectRegions);
        assert!(!app.has_valid_render_source());
        assert!(!app.can_submit_render());
    }

    #[test]
    fn selected_media_items_queue_independent_direct_source_outputs() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Media source".to_owned(),
            })
            .expect("source track should be created");
        let track_id = project.tracks()[0].id();
        for (start_sample, length_samples) in [(24_000, 48_000), (96_000, 24_000)] {
            project
                .apply(DawAction::InsertAudioItem {
                    track_id,
                    media_ref: format!("linked-{start_sample}"),
                    start_sample,
                    source_offset_samples: 0,
                    length_samples,
                })
                .expect("audio item should be inserted");
        }
        let item_ids = project
            .audio_items()
            .iter()
            .map(|item| item.id())
            .collect::<Vec<_>>();
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: project.settings().sample_rate(),
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 120_000,
            tail_milliseconds: 250,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let prefix =
            std::env::temp_dir().join(format!("aaadaw-selected-items-{}.wav", std::process::id()));
        let jobs = selected_media_item_render_jobs(
            &prefix,
            &project,
            &item_ids.iter().copied().collect::<HashSet<_>>(),
            settings,
        )
        .expect("each selected audio item should produce one job");
        assert_eq!(jobs.len(), 2);
        assert_eq!(
            jobs[0].0.source,
            aaadaw_app::ProjectWavRenderSource::SelectedAudioItem(item_ids[0])
        );
        assert_eq!(
            (jobs[0].0.start_sample, jobs[0].0.end_sample),
            (24_000, 72_000)
        );
        assert_eq!(jobs[0].0.tail_milliseconds, 0);
        assert_eq!(
            jobs[1].0.source,
            aaadaw_app::ProjectWavRenderSource::SelectedAudioItem(item_ids[1])
        );
        assert_eq!(
            (jobs[1].0.start_sample, jobs[1].0.end_sample),
            (96_000, 120_000)
        );
        assert_eq!(
            jobs[1]
                .1
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap(),
            format!("aaadaw-selected-items-{}-item_02.wav", std::process::id())
        );

        let mut app = App {
            project,
            project_path: Some(std::env::temp_dir().join(format!(
                "aaadaw-selected-items-project-{}.aaadaw",
                std::process::id()
            ))),
            render_dialog: RenderDialogState {
                source: RenderSource::SelectedMediaItems,
                bounds: RenderBounds::AllProjectMarkers,
                tail_enabled: true,
                tail_duration_text: "invalid-but-disabled-for-items".to_owned(),
                ..RenderDialogState::default()
            },
            offline_render_busy: true,
            ..App::default()
        };
        app.timeline.selected_items.extend(item_ids.iter().copied());
        assert_eq!(app.selected_media_items_summary(), "2 media items");
        assert!(app.has_valid_render_source());
        assert!(app.can_submit_render());
        assert!(app.render_requires_multiple_files());
        let _task = app.start_offline_render(prefix.clone());
        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("selected item set should queue one atomic batch");
        assert_eq!(queued.job.label, "Render 2 selected media items");
        let OfflineRenderKind::ProjectWavBatch { outputs } = queued.job.kind else {
            panic!("multiple selected media items should use a grouped job")
        };
        assert_eq!(outputs.len(), 2);
        assert_eq!(
            outputs[0].0.source,
            aaadaw_app::ProjectWavRenderSource::SelectedAudioItem(item_ids[0])
        );
        assert_eq!(
            outputs[1].0.source,
            aaadaw_app::ProjectWavRenderSource::SelectedAudioItem(item_ids[1])
        );

        app.timeline.selected_items.remove(&item_ids[1]);
        let single_destination = std::env::temp_dir().join(format!(
            "aaadaw-selected-item-single-{}.wav",
            std::process::id()
        ));
        let _task = app.start_offline_render(single_destination.clone());
        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("one selected item should queue a single-file render");
        let OfflineRenderKind::ProjectWav {
            destination,
            settings,
        } = queued.job.kind
        else {
            panic!("one selected item should use the user-selected destination")
        };
        assert_eq!(destination, single_destination);
        assert_eq!(
            settings.source,
            aaadaw_app::ProjectWavRenderSource::SelectedAudioItem(item_ids[0])
        );
        assert_eq!(
            (settings.start_sample, settings.end_sample),
            (24_000, 72_000)
        );
        assert_eq!(settings.tail_milliseconds, 0);
    }

    #[test]
    fn selected_media_item_source_rejects_empty_and_midi_only_selection() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "MIDI source".to_owned(),
            })
            .expect("source track should be created");
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 960,
            })
            .expect("MIDI item should be inserted");
        let midi_item_id = project.midi_items()[0].id();
        let mut app = App {
            project,
            render_dialog: RenderDialogState {
                source: RenderSource::SelectedMediaItems,
                ..RenderDialogState::default()
            },
            ..App::default()
        };
        assert!(!app.has_valid_render_source());
        assert!(
            app.selected_media_item_source_error()
                .unwrap()
                .contains("audio media items")
        );
        app.timeline.selected_items.insert(midi_item_id);
        assert!(!app.has_valid_render_source());
        assert!(!app.can_submit_render());
        assert!(
            app.selected_media_item_source_error()
                .unwrap()
                .contains("deselect MIDI")
        );
    }

    #[test]
    fn custom_render_range_maps_project_clock_values_to_sample_boundaries_and_freezes_them() {
        let mut project = Project::default();
        project
            .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: -5.0 })
            .expect("project clock offset should be set");
        let mut app = App {
            project,
            project_path: Some(std::env::temp_dir().join(format!(
                "aaadaw-custom-range-queued-{}.aaadaw",
                std::process::id()
            ))),
            render_dialog: RenderDialogState {
                bounds: RenderBounds::CustomTimeRange,
                custom_start_text: "-0:04.877".to_owned(),
                custom_end_text: "-0:02.544".to_owned(),
                custom_range_initialized: true,
                ..RenderDialogState::default()
            },
            offline_render_busy: true,
            ..App::default()
        };

        assert_eq!(app.selected_render_bounds(), Ok((5_904, 117_888)));
        assert_eq!(app.custom_render_length_text().as_deref(), Some("0:02.333"));
        assert!(app.can_submit_render());

        let output =
            std::env::temp_dir().join(format!("aaadaw-custom-range-{}.wav", std::process::id()));
        let _task = app.start_offline_render(output);
        app.render_dialog.custom_start_text = "0:00.000".to_owned();
        app.render_dialog.custom_end_text = "0:01.000".to_owned();
        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("custom time range should queue as a project render");
        let OfflineRenderKind::ProjectWav { settings, .. } = queued.job.kind else {
            panic!("a custom time range should queue one bounded WAV render")
        };
        assert_eq!(settings.start_sample, 5_904);
        assert_eq!(settings.end_sample, 117_888);
    }

    #[test]
    fn render_tail_is_opt_in_and_freezes_its_enabled_duration() {
        let mut app = App {
            project_path: Some(std::env::temp_dir().join(format!(
                "aaadaw-render-tail-queued-{}.aaadaw",
                std::process::id()
            ))),
            render_dialog: RenderDialogState {
                bounds: RenderBounds::CustomTimeRange,
                custom_start_text: "0:00.000".to_owned(),
                custom_end_text: "0:02.000".to_owned(),
                custom_range_initialized: true,
                tail_duration_text: "invalid while disabled".to_owned(),
                ..RenderDialogState::default()
            },
            offline_render_busy: true,
            ..App::default()
        };

        assert!(!app.render_dialog.tail_enabled);
        assert_eq!(app.selected_render_settings().unwrap().tail_milliseconds, 0);
        assert!(app.can_submit_render(), "disabled tail input is ignored");

        app.render_dialog.tail_enabled = true;
        assert!(!app.can_submit_render(), "enabled tail input must validate");
        app.render_dialog.tail_duration_text = "1250".to_owned();
        assert!(app.can_submit_render());

        let output =
            std::env::temp_dir().join(format!("aaadaw-render-tail-{}.wav", std::process::id()));
        let _task = app.start_offline_render(output);
        app.render_dialog.tail_enabled = false;
        app.render_dialog.tail_duration_text = "0".to_owned();

        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("the enabled tail render should queue");
        let OfflineRenderKind::ProjectWav { settings, .. } = queued.job.kind else {
            panic!("custom time range should queue one WAV render")
        };
        assert_eq!(settings.tail_milliseconds, 1_250);
    }

    #[test]
    fn custom_render_range_initializes_from_project_clock_and_content_end_once() {
        let mut project = Project::default();
        project
            .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: -5.0 })
            .expect("project clock offset should be set");
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Range".to_owned(),
            })
            .expect("range track should be created");
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 2_400,
            })
            .expect("project content should have a 1.25-second end");
        let mut app = App {
            project,
            ..App::default()
        };

        app.initialize_custom_render_range();
        assert_eq!(app.render_dialog.custom_start_text, "-0:05.000");
        assert_eq!(app.render_dialog.custom_end_text, "-0:03.750");

        app.render_dialog.custom_start_text = "-0:04.500".to_owned();
        app.project = Project::default();
        app.initialize_custom_render_range();
        assert_eq!(app.render_dialog.custom_start_text, "-0:04.500");
        assert_eq!(app.render_dialog.custom_end_text, "-0:03.750");
    }

    #[test]
    fn custom_render_range_rejects_invalid_out_of_project_and_empty_ranges() {
        let mut app = App {
            render_dialog: RenderDialogState {
                bounds: RenderBounds::CustomTimeRange,
                custom_range_initialized: true,
                ..RenderDialogState::default()
            },
            ..App::default()
        };

        app.render_dialog.custom_start_text = "not a time".to_owned();
        assert!(
            app.selected_render_bounds()
                .expect_err("malformed start should be rejected")
                .contains("Invalid custom start time")
        );

        app.render_dialog.custom_start_text = "0:02.000".to_owned();
        app.render_dialog.custom_end_text = "0:01.000".to_owned();
        assert!(
            app.selected_render_bounds()
                .expect_err("reversed range should be rejected")
                .contains("end must be after")
        );

        app.render_dialog.custom_start_text = "0:01.000".to_owned();
        app.render_dialog.custom_end_text = "0:01.000".to_owned();
        assert!(
            app.selected_render_bounds()
                .expect_err("zero-length range should be rejected")
                .contains("end must be after")
        );

        app.render_dialog.custom_start_text = "-0:00.001".to_owned();
        app.render_dialog.custom_end_text = "0:01.000".to_owned();
        assert!(
            app.selected_render_bounds()
                .expect_err("time before project sample zero should be rejected")
                .contains("Could not convert custom start")
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

        app.render_dialog.tail_enabled = true;
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
            None,
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
        let OfflineRenderKind::ProjectWavBatch { outputs } = queued.job.kind else {
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
    fn region_render_matrix_uses_each_regions_master_and_track_assignments() {
        let mut project = Project::default();
        for (index, name) in [(0, "Lead"), (1, "Guitar")] {
            project
                .apply(DawAction::CreateTrack {
                    index,
                    name: name.to_owned(),
                })
                .expect("render track should be created");
        }
        for (start_ticks, end_ticks) in [(0, 1_920), (1_920, 3_840)] {
            project
                .apply(DawAction::InsertProjectRegion {
                    start_ticks,
                    end_ticks,
                    name: "Verse".to_owned(),
                })
                .expect("region should be inserted");
        }
        let regions = project.regions().to_vec();
        let lead = project.tracks()[0].id();
        let guitar = project.tracks()[1].id();
        project
            .apply(DawAction::SetProjectRegionRenderMatrix {
                region_id: regions[0].id(),
                render_master_mix: true,
                render_track_ids: vec![lead],
            })
            .expect("first region matrix assignments should be stored");
        project
            .apply(DawAction::SetProjectRegionRenderMatrix {
                region_id: regions[1].id(),
                render_master_mix: false,
                render_track_ids: vec![guitar],
            })
            .expect("second region matrix assignments should be stored");
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: project.settings().sample_rate(),
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 120_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let outputs = project_region_render_jobs(
            Path::new("matrix-prefix.wav"),
            &project,
            RenderSource::RegionRenderMatrix,
            &[],
            false,
            settings,
            None,
        )
        .expect("assigned matrix targets should produce a grouped render");

        assert_eq!(outputs.len(), 3);
        assert_eq!(
            outputs[0],
            (
                aaadaw_app::ProjectWavRenderSettings {
                    start_sample: 0,
                    end_sample: 48_000,
                    ..settings
                },
                PathBuf::from("./matrix-prefix-Verse-Master.wav")
            )
        );
        assert_eq!(
            outputs[1],
            (
                aaadaw_app::ProjectWavRenderSettings {
                    source: aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(lead),
                    start_sample: 0,
                    end_sample: 48_000,
                    ..settings
                },
                PathBuf::from("./matrix-prefix-Verse-Lead.wav")
            )
        );
        assert_eq!(
            outputs[2],
            (
                aaadaw_app::ProjectWavRenderSettings {
                    source: aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(guitar),
                    start_sample: 48_000,
                    end_sample: 96_000,
                    ..settings
                },
                PathBuf::from("./matrix-prefix-Verse-2-Guitar.wav")
            )
        );
    }

    #[test]
    fn region_render_matrix_wavs_match_native_target_count_ranges_and_samples() {
        fn riff_chunk<'a>(bytes: &'a [u8], chunk_id: &[u8; 4]) -> &'a [u8] {
            assert_eq!(&bytes[0..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WAVE");
            let mut offset = 12;
            while offset + 8 <= bytes.len() {
                let id = &bytes[offset..offset + 4];
                let length =
                    u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
                let start = offset + 8;
                let end = start + length;
                assert!(end <= bytes.len(), "RIFF chunk must fit within the file");
                if id == chunk_id {
                    return &bytes[start..end];
                }
                offset = end + (length & 1);
            }
            panic!("RIFF chunk {:?} should exist", chunk_id);
        }

        let directory = tempfile::tempdir().expect("render fixture directory should be created");
        let project_path = directory.path().join("region-matrix.aaadaw");
        let mut store = ProjectStore::open(&project_path).expect("project store should be created");
        let mut media_refs = Vec::new();
        for (name, value) in [("Drums", 0.125_f32), ("Bass", 0.25), ("Guitar", 0.375)] {
            let source_path = directory.path().join(format!("{name}.wav"));
            let mut source = aaadaw_app::WavExport::create(
                &source_path,
                48_000,
                aaadaw_app::WavExportOptions {
                    sample_format: aaadaw_app::WavSampleFormat::Pcm24,
                    dither: false,
                },
            )
            .expect("matrix source WAV should be created");
            source
                .write_frames(&vec![[value, value]; 4 * 48_000])
                .expect("matrix source frames should be written");
            source
                .finish()
                .expect("matrix source WAV should be published");
            media_refs.push(
                store
                    .import_audio_file(source_path)
                    .expect("matrix source should be embedded in the project"),
            );
        }
        store
            .close()
            .expect("project store should close before rendering");

        let mut project = Project::default();
        for (index, name) in [("Drums", 0), ("Bass", 1), ("Guitar", 2)] {
            project
                .apply(DawAction::CreateTrack {
                    index: name,
                    name: index.to_owned(),
                })
                .expect("matrix track should be created");
        }
        let track_ids = project
            .tracks()
            .iter()
            .map(|track| track.id())
            .collect::<Vec<_>>();
        for (track_id, media_ref) in track_ids.iter().copied().zip(media_refs) {
            project
                .apply(DawAction::InsertAudioItem {
                    track_id,
                    media_ref,
                    start_sample: 0,
                    source_offset_samples: 0,
                    length_samples: 4 * 48_000,
                })
                .expect("matrix audio item should be inserted");
        }
        for (start_ticks, end_ticks, name) in [(0, 3_840, "Intro"), (3_840, 7_680, "Verse")] {
            project
                .apply(DawAction::InsertProjectRegion {
                    start_ticks,
                    end_ticks,
                    name: name.to_owned(),
                })
                .expect("matrix region should be inserted");
        }
        let regions = project.regions().to_vec();
        project
            .apply(DawAction::SetProjectRegionRenderMatrix {
                region_id: regions[0].id(),
                render_master_mix: true,
                render_track_ids: vec![track_ids[0], track_ids[2]],
            })
            .expect("Intro matrix assignments should be stored");
        project
            .apply(DawAction::SetProjectRegionRenderMatrix {
                region_id: regions[1].id(),
                render_master_mix: false,
                render_track_ids: vec![track_ids[1]],
            })
            .expect("Verse matrix assignments should be stored");

        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions {
                sample_format: aaadaw_app::WavSampleFormat::Pcm24,
                dither: false,
            },
            output_sample_rate: 48_000,
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 4 * 48_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let prefix = directory.path().join("matrix-prefix.wav");
        let outputs = project_region_render_jobs(
            &prefix,
            &project,
            RenderSource::RegionRenderMatrix,
            &[],
            false,
            settings,
            None,
        )
        .expect("assigned matrix targets should produce render jobs");
        let outputs = apply_render_filename_pattern(
            outputs,
            "$region-$track",
            &project,
            Path::new("matrix-prefix.aaadaw"),
        )
        .expect("REAPER-style region/track wildcards should name matrix outputs");
        assert_eq!(
            outputs
                .iter()
                .map(|(_, path)| path.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            [
                "Intro-Master.wav",
                "Intro-Drums.wav",
                "Intro-Guitar.wav",
                "Verse-Bass.wav",
            ]
        );
        assert_eq!(
            outputs
                .iter()
                .map(|(settings, _)| (settings.start_sample, settings.end_sample))
                .collect::<Vec<_>>(),
            [(0, 96_000), (0, 96_000), (0, 96_000), (96_000, 192_000)]
        );

        aaadaw_app::render_project_wav_jobs_to_wavs(
            &project_path,
            &project,
            &outputs,
            &std::sync::atomic::AtomicBool::new(false),
            |_, _| {},
        )
        .expect("all matrix render jobs should publish as one WAV batch");

        let native_fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/verification/reaper-parity/fixtures/region-render-matrix-render");
        for (name, native_name, left, right) in [
            ("Intro-Master.wav", "Intro-Master.wav", 0.75_f64, 0.75_f64),
            ("Intro-Drums.wav", "Intro-Drums.wav", 0.125, 0.125),
            ("Intro-Guitar.wav", "Intro-Guitar.wav", 0.375, 0.375),
            ("Verse-Bass.wav", "Verse-Bass.wav", 0.25, 0.25),
        ] {
            let bytes = std::fs::read(directory.path().join(name))
                .expect("matrix output should be readable");
            let native = std::fs::read(native_fixtures.join(native_name))
                .expect("native REAPER matrix output fixture should be available");
            let format = riff_chunk(&bytes, b"fmt ");
            let native_format = riff_chunk(&native, b"fmt ");
            assert_eq!(
                u32::from_le_bytes(format[4..8].try_into().unwrap()),
                48_000,
                "{name} sample rate"
            );
            assert_eq!(u16::from_le_bytes(format[2..4].try_into().unwrap()), 2);
            assert_eq!(u16::from_le_bytes(format[14..16].try_into().unwrap()), 24);
            assert_eq!(format, native_format, "{name} format should match REAPER");
            let samples = riff_chunk(&bytes, b"data");
            let native_samples = riff_chunk(&native, b"data");
            assert_eq!(samples.len(), 96_000 * 6, "{name} should be 96,000 frames");
            assert_eq!(
                samples, native_samples,
                "{name} PCM payload should match the REAPER matrix render"
            );
            let sample = |encoded: &[u8]| {
                let padded = [
                    encoded[0],
                    encoded[1],
                    encoded[2],
                    if encoded[2] & 0x80 == 0 { 0 } else { 0xff },
                ];
                i32::from_le_bytes(padded)
            };
            let expected_left = (left * 8_388_608.0).round() as i32;
            let expected_right = (right * 8_388_608.0).round() as i32;
            assert_eq!(
                sample(&samples[0..3]),
                expected_left,
                "{name} native left amplitude"
            );
            assert_eq!(
                sample(&samples[3..6]),
                expected_right,
                "{name} native right amplitude"
            );
        }
    }

    #[test]
    fn render_all_markers_freezes_next_marker_ranges_and_output_names() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Lead".to_owned(),
            })
            .expect("marker render track should be created");
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 4_800,
            })
            .expect("project content should extend past the last marker");
        for (position_ticks, name) in [(960, "Verse/Part"), (2_880, "Verse/Part"), (3_840, "")] {
            project
                .apply(DawAction::InsertProjectMarker {
                    position_ticks,
                    name: name.to_owned(),
                })
                .expect("project marker should be inserted");
        }
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: project.settings().sample_rate(),
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 120_000,
            tail_milliseconds: 250,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let prefix = std::env::temp_dir().join("aaadaw-marker-prefix.wav");
        let outputs = project_marker_render_jobs(
            &prefix,
            &project,
            RenderSource::MasterMix,
            &[],
            false,
            settings,
            None,
        )
        .expect("every marker should create a bounded render job");
        assert_eq!(outputs.len(), 3);
        assert_eq!(
            outputs,
            [
                (
                    aaadaw_app::ProjectWavRenderSettings {
                        source: aaadaw_app::ProjectWavRenderSource::MasterMix,
                        start_sample: 24_000,
                        end_sample: 72_000,
                        ..settings
                    },
                    std::env::temp_dir().join("aaadaw-marker-prefix-Verse_Part.wav")
                ),
                (
                    aaadaw_app::ProjectWavRenderSettings {
                        start_sample: 72_000,
                        end_sample: 96_000,
                        ..settings
                    },
                    std::env::temp_dir().join("aaadaw-marker-prefix-Verse_Part-2.wav")
                ),
                (
                    aaadaw_app::ProjectWavRenderSettings {
                        start_sample: 96_000,
                        end_sample: 120_000,
                        ..settings
                    },
                    std::env::temp_dir().join("aaadaw-marker-prefix-03.wav")
                ),
            ]
        );
        assert!(
            outputs
                .iter()
                .all(|(job_settings, _)| job_settings.tail_milliseconds == 250)
        );

        let mut app = App {
            project,
            project_path: Some(std::env::temp_dir().join(format!(
                "aaadaw-marker-queued-{}.aaadaw",
                std::process::id()
            ))),
            render_dialog: RenderDialogState {
                bounds: RenderBounds::AllProjectMarkers,
                ..RenderDialogState::default()
            },
            offline_render_busy: true,
            ..App::default()
        };
        assert_eq!(app.selected_render_bounds(), Ok((24_000, 120_000)));
        assert!(app.render_requires_multiple_files());
        assert!(app.can_submit_render());
        let output_prefix = std::env::temp_dir().join(format!(
            "aaadaw-queued-marker-prefix-{}.wav",
            std::process::id()
        ));
        let _task = app.start_offline_render(output_prefix);
        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("all project markers should queue as one grouped job");
        let OfflineRenderKind::ProjectWavBatch { outputs } = queued.job.kind else {
            panic!("All project markers should use the bounded multi-file worker")
        };
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].0.start_sample, 24_000);
        assert_eq!(outputs[0].0.end_sample, 72_000);
        assert_eq!(outputs[1].0.start_sample, 72_000);
        assert_eq!(outputs[1].0.end_sample, 96_000);
        assert_eq!(outputs[2].0.start_sample, 96_000);
        assert_eq!(outputs[2].0.end_sample, 120_000);

        app.project = Project::default();
        assert!(
            app.selected_render_bounds()
                .expect_err("rendering markers requires at least one marker")
                .contains("Add at least one project marker")
        );
        assert!(!app.can_submit_render());
    }

    #[test]
    fn render_selected_regions_and_markers_use_their_selected_ranges() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Selection fixture".to_owned(),
            })
            .expect("render track should be created");
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 4_800,
            })
            .expect("project media should extend to five seconds");
        for (start_ticks, end_ticks, name) in [
            (960, 1_920, "First region"),
            (2_880, 4_800, "Second region"),
        ] {
            project
                .apply(DawAction::InsertProjectRegion {
                    start_ticks,
                    end_ticks,
                    name: name.to_owned(),
                })
                .expect("region should be inserted");
        }
        for (position_ticks, name) in [
            (960, "First marker"),
            (2_880, "Second marker"),
            (3_840, "Last marker"),
        ] {
            project
                .apply(DawAction::InsertProjectMarker {
                    position_ticks,
                    name: name.to_owned(),
                })
                .expect("marker should be inserted");
        }
        project
            .apply(DawAction::InsertProjectMarker {
                position_ticks: 4_800,
                name: "At project end".to_owned(),
            })
            .expect("project-end marker should be inserted");
        let selected_region = project.regions()[1].id();
        let selected_marker = project.markers()[1].id();
        let final_marker = project.markers()[2].id();
        let project_end_marker = project.markers()[3].id();
        let settings = aaadaw_app::ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: aaadaw_app::WavExportOptions::default(),
            output_sample_rate: project.settings().sample_rate(),
            output_channels: WavChannelCount::Stereo,
            source: aaadaw_app::ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 120_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };
        let region_ids = HashSet::from([selected_region]);
        let region_outputs = project_region_render_jobs(
            Path::new("selected-region-prefix.wav"),
            &project,
            RenderSource::MasterMix,
            &[],
            false,
            settings,
            Some(&region_ids),
        )
        .expect("selected region should produce a render job");
        assert_eq!(region_outputs.len(), 1);
        assert_eq!(region_outputs[0].0.start_sample, 72_000);
        assert_eq!(region_outputs[0].0.end_sample, 120_000);
        assert_eq!(
            region_outputs[0]
                .1
                .file_name()
                .and_then(|name| name.to_str()),
            Some("selected-region-prefix-Second region.wav")
        );

        let marker_ids = HashSet::from([selected_marker, final_marker]);
        let marker_outputs = project_marker_render_jobs(
            Path::new("selected-marker-prefix.wav"),
            &project,
            RenderSource::MasterMix,
            &[],
            false,
            settings,
            Some(&marker_ids),
        )
        .expect("selected markers should produce bounded render jobs");
        assert_eq!(marker_outputs.len(), 2);
        assert_eq!(marker_outputs[0].0.start_sample, 72_000);
        assert_eq!(marker_outputs[0].0.end_sample, 96_000);
        assert_eq!(marker_outputs[1].0.start_sample, 96_000);
        assert_eq!(marker_outputs[1].0.end_sample, 120_000);

        let mut app = App {
            project,
            project_path: Some(std::env::temp_dir().join(format!(
                "aaadaw-selected-bounds-{}.aaadaw",
                std::process::id()
            ))),
            render_dialog: RenderDialogState {
                bounds: RenderBounds::SelectedProjectMarkers,
                ..RenderDialogState::default()
            },
            offline_render_busy: true,
            ..App::default()
        };
        assert!(app.selected_render_markers.is_empty());
        assert!(app.selected_render_regions.is_empty());
        assert_eq!(app.selected_render_bounds(), Ok((24_000, 120_000)));
        assert_eq!(app.render_bounds_summary().as_deref(), Some("3 markers"));
        app.render_dialog.bounds = RenderBounds::SelectedProjectRegions;
        assert_eq!(app.selected_render_bounds(), Ok((24_000, 120_000)));
        assert_eq!(app.render_bounds_summary().as_deref(), Some("2 regions"));
        app.render_dialog.bounds = RenderBounds::SelectedProjectMarkers;
        app.selected_render_markers
            .extend([selected_marker, final_marker, project_end_marker]);
        assert_eq!(app.selected_render_bounds(), Ok((72_000, 120_000)));
        assert_eq!(app.render_bounds_summary().as_deref(), Some("2 markers"));
        assert!(app.render_requires_multiple_files());
        assert!(app.can_submit_render());

        let output_prefix = std::env::temp_dir().join(format!(
            "aaadaw-selected-marker-prefix-{}.wav",
            std::process::id()
        ));
        let _task = app.start_offline_render(output_prefix);
        let queued = app
            .offline_job_queue
            .pop_front()
            .expect("selected marker jobs should be queued as a batch");
        assert_eq!(queued.job.label, "Render 2 project markers");
        let OfflineRenderKind::ProjectWavBatch { outputs } = queued.job.kind else {
            panic!("selected markers should use the bounded multi-file worker")
        };
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].0.start_sample, 72_000);
        assert_eq!(outputs[0].0.end_sample, 96_000);
        assert_eq!(outputs[1].0.start_sample, 96_000);
        assert_eq!(outputs[1].0.end_sample, 120_000);

        app.render_dialog.bounds = RenderBounds::SelectedProjectRegions;
        app.selected_render_regions.insert(selected_region);
        assert_eq!(app.selected_render_bounds(), Ok((72_000, 120_000)));
        assert_eq!(app.render_bounds_summary().as_deref(), Some("1 region"));
        assert!(app.can_submit_render());
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
    fn render_tail_parses_integer_milliseconds_exactly() {
        for (input, expected) in [
            ("2000", 2_000),
            ("1200", 1_200),
            ("125", 125),
            ("500", 500),
            (" 3125 ", 3_125),
        ] {
            assert_eq!(parse_render_tail_milliseconds(input), Ok(expected));
        }
        for invalid in ["", ".", "-1", "+1", "1.0001", "1..2", "1,5", "4294967296"] {
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
        app.render_dialog.tail_enabled = true;
        app.render_dialog.tail_duration_text = "3125".to_owned();
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
            OfflineRenderKind::ProjectWavBatch { .. } => {
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
        app.project
            .apply(DawAction::InsertMidiItem {
                track_id: selected,
                start_tick: 0,
                length_ticks: 960,
            })
            .expect("selected render track should contain non-empty media");
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
            OfflineRenderKind::ProjectWavBatch { .. } => {
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
            OfflineRenderKind::ProjectWavBatch { .. } => {
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
        let _ = app.update(Message::SetRenderFilenamePattern(
            "$project-$track".to_owned(),
        ));

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
                            std::env::temp_dir().join("aaadaw-render-stems-snapshot-Drums.wav")
                        ),
                        (
                            aaadaw_app::ProjectWavRenderSource::SelectedTrackViaMaster(second),
                            std::env::temp_dir().join("aaadaw-render-stems-snapshot-Bass.wav")
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
            OfflineRenderKind::ProjectWavBatch { .. } => {
                panic!("entire-project bounds should create a track stem job")
            }
            OfflineRenderKind::FreezeTrack { .. } => panic!("expected a stem render job"),
        }

        app.timeline.selected_track = Some(first);
        app.timeline.selected_tracks.extend([first, second]);
        app.render_dialog.filename_pattern = "$project".to_owned();
        let _task = app.start_offline_render(std::env::temp_dir().join("duplicate-stems.wav"));
        assert!(app.offline_job_queue.is_empty());
        assert!(app.status.contains("duplicate output names"));
        app.render_dialog.filename_pattern.clear();

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
            OfflineRenderKind::ProjectWavBatch { .. } => {
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
            OfflineRenderKind::ProjectWavBatch { .. } => {
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
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 960,
            })
            .unwrap();
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
