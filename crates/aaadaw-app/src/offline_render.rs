use crate::{
    Float32WavExport, Float32WavExportError, Pcm24WavExport, Pcm24WavExportError,
    PreparedAudioPlayback, WavChannelCount, WavExport, WavExportError, WavExportOptions,
};
use crate::{
    prepare_audio_playback_at_with_resampling_quality,
    prepare_audio_playback_with_resampling_quality,
};
use aaadaw_core::{DawAction, ItemId, Project, TrackId};
use aaadaw_engine::{
    AudioGraphError, AudioRenderGraph, ClapEffectOwner, ClapInstrumentOwner, MasterOutputCeiling,
    TrackFxProcessor, TrackInstrumentProcessor,
};
use aaadaw_media::{ResamplingQuality, StereoPcmResampler};
use aaadaw_storage::ProjectStore;
use std::cell::RefCell;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

static NEXT_NORMALIZATION_SPOOL_ID: AtomicU64 = AtomicU64::new(0);
static NEXT_STEM_STAGING_ID: AtomicU64 = AtomicU64::new(0);

/// Export decay allowance and minimum fallback when frozen CLAP plugins do not report longer tails.
pub const DEFAULT_EFFECT_TAIL_SECONDS: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrozenTrackRender {
    pub start_sample: u64,
    pub length_samples: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectWavRenderSettings {
    pub master_ceiling: MasterOutputCeiling,
    pub export_options: WavExportOptions,
    pub output_sample_rate: u32,
    pub output_channels: WavChannelCount,
    pub source: ProjectWavRenderSource,
    pub start_sample: u64,
    pub end_sample: u64,
    pub tail_milliseconds: u32,
    /// Enable sample-peak normalization on the completed output.
    pub normalize_peak: bool,
    /// Peak-normalization target in hundredths of a dBFS.
    pub normalize_peak_target_centi_dbfs: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProjectWavRenderSource {
    #[default]
    MasterMix,
    /// Render one selected track as a stem without the Master mix output stage.
    SelectedTrackStem(TrackId),
    /// Isolate one track in an in-memory project snapshot and render it through Master.
    SelectedTrackViaMaster(TrackId),
}

trait OfflineWavWriter {
    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), OfflineRenderError>;
    fn finish(self) -> Result<PathBuf, OfflineRenderError>;
}

struct SampleRateConvertingWriter<W> {
    inner: W,
    resampler: Option<StereoPcmResampler>,
}

impl<W: OfflineWavWriter> OfflineWavWriter for SampleRateConvertingWriter<W> {
    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), OfflineRenderError> {
        match &mut self.resampler {
            Some(resampler) => {
                let converted = resampler
                    .push_frames(frames)
                    .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
                self.inner.write_frames(&converted)
            }
            None => self.inner.write_frames(frames),
        }
    }

    fn finish(mut self) -> Result<PathBuf, OfflineRenderError> {
        if let Some(resampler) = self.resampler.take() {
            let tail = resampler.finish();
            self.inner.write_frames(&tail)?;
        }
        self.inner.finish()
    }
}

struct PeakNormalizationSpool {
    path: PathBuf,
    writer: Option<BufWriter<File>>,
    peak: f32,
    frame_count: u64,
}

impl PeakNormalizationSpool {
    fn create() -> Result<Self, OfflineRenderError> {
        for _ in 0..16 {
            let id = NEXT_NORMALIZATION_SPOOL_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "aaadaw-render-normalize-{}-{id}.f32",
                std::process::id()
            ));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        writer: Some(BufWriter::new(file)),
                        peak: 0.0,
                        frame_count: 0,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(OfflineRenderError::Media(format!(
                        "could not create peak-normalization spool: {error}"
                    )));
                }
            }
        }
        Err(OfflineRenderError::Media(
            "could not reserve a peak-normalization spool file".to_owned(),
        ))
    }

    fn write_frames(
        &mut self,
        frames: &[[f32; 2]],
        channels: WavChannelCount,
    ) -> Result<(), OfflineRenderError> {
        let writer = self.writer.as_mut().ok_or_else(|| {
            OfflineRenderError::Media("peak-normalization spool is already closed".to_owned())
        })?;
        let mut bytes = Vec::with_capacity(frames.len().saturating_mul(8));
        for frame in frames {
            if !frame[0].is_finite() || !frame[1].is_finite() {
                return Err(OfflineRenderError::Media(
                    "render produced a non-finite sample during peak normalization".to_owned(),
                ));
            }
            let peak_sample = match channels {
                WavChannelCount::Mono => frame[0] * 0.5 + frame[1] * 0.5,
                WavChannelCount::Stereo => frame[0].abs().max(frame[1].abs()),
            };
            self.peak = self.peak.max(peak_sample.abs());
            bytes.extend_from_slice(&frame[0].to_le_bytes());
            bytes.extend_from_slice(&frame[1].to_le_bytes());
        }
        writer.write_all(&bytes).map_err(|error| {
            OfflineRenderError::Media(format!("could not write peak-normalization spool: {error}"))
        })?;
        self.frame_count = self
            .frame_count
            .checked_add(frames.len() as u64)
            .ok_or(OfflineRenderError::TimelineRange)?;
        Ok(())
    }

    fn reader(&mut self) -> Result<BufReader<File>, OfflineRenderError> {
        if let Some(mut writer) = self.writer.take() {
            writer.flush().map_err(|error| {
                OfflineRenderError::Media(format!(
                    "could not flush peak-normalization spool: {error}"
                ))
            })?;
        }
        File::open(&self.path).map(BufReader::new).map_err(|error| {
            OfflineRenderError::Media(format!("could not read peak-normalization spool: {error}"))
        })
    }
}

impl Drop for PeakNormalizationSpool {
    fn drop(&mut self) {
        self.writer.take();
        let _ = fs::remove_file(&self.path);
    }
}

struct PeakNormalizingWriter<'a, W> {
    inner: W,
    channels: WavChannelCount,
    spool: Option<PeakNormalizationSpool>,
    target_linear: f64,
    cancelled: &'a AtomicBool,
}

impl<'a, W> PeakNormalizingWriter<'a, W> {
    fn new(
        inner: W,
        channels: WavChannelCount,
        enabled: bool,
        target_linear: f64,
        cancelled: &'a AtomicBool,
    ) -> Result<Self, OfflineRenderError> {
        Ok(Self {
            inner,
            channels,
            spool: enabled.then(PeakNormalizationSpool::create).transpose()?,
            target_linear,
            cancelled,
        })
    }
}

impl<W: OfflineWavWriter> OfflineWavWriter for PeakNormalizingWriter<'_, W> {
    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), OfflineRenderError> {
        if let Some(spool) = &mut self.spool {
            spool.write_frames(frames, self.channels)
        } else {
            self.inner.write_frames(frames)
        }
    }

    fn finish(mut self) -> Result<PathBuf, OfflineRenderError> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(OfflineRenderError::Cancelled);
        }
        let Some(mut spool) = self.spool.take() else {
            return self.inner.finish();
        };
        let gain = if spool.peak > 0.0 {
            self.target_linear / f64::from(spool.peak)
        } else {
            1.0
        };
        let mut reader = spool.reader()?;
        const FRAMES_PER_PASS: usize = 4_096;
        let mut bytes = [0_u8; FRAMES_PER_PASS * 8];
        let mut frames = [[0.0_f32; 2]; FRAMES_PER_PASS];
        let mut rendered = 0_u64;
        while rendered < spool.frame_count {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(OfflineRenderError::Cancelled);
            }
            let count = usize::try_from((spool.frame_count - rendered).min(FRAMES_PER_PASS as u64))
                .expect("normalization batch is bounded by a fixed buffer");
            reader
                .read_exact(&mut bytes[..count * 8])
                .map_err(|error| {
                    OfflineRenderError::Media(format!(
                        "could not read peak-normalization spool: {error}"
                    ))
                })?;
            for (index, frame) in frames[..count].iter_mut().enumerate() {
                let offset = index * 8;
                let left = f32::from_le_bytes(
                    bytes[offset..offset + 4]
                        .try_into()
                        .expect("four-byte sample"),
                );
                let right = f32::from_le_bytes(
                    bytes[offset + 4..offset + 8]
                        .try_into()
                        .expect("four-byte sample"),
                );
                *frame = [
                    (f64::from(left) * gain) as f32,
                    (f64::from(right) * gain) as f32,
                ];
            }
            self.inner.write_frames(&frames[..count])?;
            rendered += count as u64;
        }
        if self.cancelled.load(Ordering::Acquire) {
            return Err(OfflineRenderError::Cancelled);
        }
        self.inner.finish()
    }
}

impl OfflineWavWriter for Pcm24WavExport {
    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), OfflineRenderError> {
        Pcm24WavExport::write_frames(self, frames).map_err(OfflineRenderError::Wav)
    }

    fn finish(self) -> Result<PathBuf, OfflineRenderError> {
        Pcm24WavExport::finish(self).map_err(OfflineRenderError::Wav)
    }
}

impl OfflineWavWriter for WavExport {
    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), OfflineRenderError> {
        WavExport::write_frames(self, frames).map_err(OfflineRenderError::WavExport)
    }

    fn finish(self) -> Result<PathBuf, OfflineRenderError> {
        WavExport::finish(self).map_err(OfflineRenderError::WavExport)
    }
}

impl OfflineWavWriter for Float32WavExport {
    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), OfflineRenderError> {
        Float32WavExport::write_frames(self, frames).map_err(OfflineRenderError::Float32Wav)
    }

    fn finish(self) -> Result<PathBuf, OfflineRenderError> {
        Float32WavExport::finish(self).map_err(OfflineRenderError::Float32Wav)
    }
}

fn freeze_tail_frames(
    sample_rate: u32,
    plugin_tail_frames: &[u32],
) -> Result<u64, OfflineRenderError> {
    let minimum_tail = u64::from(sample_rate)
        .checked_mul(u64::from(DEFAULT_EFFECT_TAIL_SECONDS))
        .ok_or(OfflineRenderError::TimelineRange)?;
    let reported_tail = plugin_tail_frames.iter().try_fold(0_u64, |total, frames| {
        total
            .checked_add(u64::from(*frames))
            .ok_or(OfflineRenderError::TimelineRange)
    })?;
    Ok(minimum_tail.max(reported_tail))
}

/// Failure while rendering a prepared graph to an offline PCM WAV file.
#[derive(Debug)]
pub enum OfflineRenderError {
    Wav(Pcm24WavExportError),
    Float32Wav(Float32WavExportError),
    WavExport(WavExportError),
    Graph(AudioGraphError),
    Cancelled,
    InputUnderrun { samples: usize },
    Media(String),
    SourceEndedEarly { item_id: ItemId },
    TimelineRange,
}

struct RenderRequest<'a> {
    total_frames: u64,
    start_sample: u64,
    cancelled: &'a AtomicBool,
}

impl std::fmt::Display for OfflineRenderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wav(error) => error.fmt(formatter),
            Self::Float32Wav(error) => error.fmt(formatter),
            Self::WavExport(error) => error.fmt(formatter),
            Self::Graph(error) => write!(formatter, "offline render failed: {error}"),
            Self::Cancelled => formatter.write_str("offline render was cancelled"),
            Self::InputUnderrun { samples } => write!(
                formatter,
                "offline render stopped because {samples} source samples were unavailable"
            ),
            Self::Media(error) => write!(formatter, "media decoding failed during export: {error}"),
            Self::SourceEndedEarly { item_id } => write!(
                formatter,
                "audio source for item {} ended before its timeline range was rendered",
                item_id.value()
            ),
            Self::TimelineRange => {
                formatter.write_str("project render length exceeds the sample timeline range")
            }
        }
    }
}

impl std::error::Error for OfflineRenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Wav(error) => Some(error),
            Self::Float32Wav(error) => Some(error),
            Self::WavExport(error) => Some(error),
            Self::Graph(error) => Some(error),
            Self::Media(_)
            | Self::Cancelled
            | Self::InputUnderrun { .. }
            | Self::SourceEndedEarly { .. }
            | Self::TimelineRange => None,
        }
    }
}

/// Computes the render length through all placed items plus the selected fixed effect tail.
pub fn project_render_length_samples(
    project: &Project,
    tail_seconds: u32,
) -> Result<u64, OfflineRenderError> {
    let content_end = project_content_end_sample(project)?;
    render_range_length_samples(
        project.settings().sample_rate(),
        0,
        content_end,
        tail_seconds,
    )
}

/// Returns the end sample of the furthest placed audio or MIDI item.
pub fn project_content_end_sample(project: &Project) -> Result<u64, OfflineRenderError> {
    let mut content_end = 0_u64;
    for item in project.audio_items() {
        let end = item
            .start_sample()
            .checked_add(item.length_samples())
            .ok_or(OfflineRenderError::TimelineRange)?;
        content_end = content_end.max(end);
    }
    for item in project.midi_items() {
        let end_tick = item
            .start_tick()
            .checked_add(item.length_ticks())
            .ok_or(OfflineRenderError::TimelineRange)?;
        let end_sample = project
            .sample_at_tick(end_tick)
            .map_err(|_| OfflineRenderError::TimelineRange)?;
        content_end = content_end.max(end_sample);
    }
    Ok(content_end)
}

/// Computes frames for a start-inclusive, end-exclusive range followed by a fixed tail.
pub fn render_range_length_samples(
    sample_rate: u32,
    start_sample: u64,
    end_sample: u64,
    tail_seconds: u32,
) -> Result<u64, OfflineRenderError> {
    let range_frames = end_sample
        .checked_sub(start_sample)
        .ok_or(OfflineRenderError::TimelineRange)?;
    let tail_frames = u64::from(sample_rate)
        .checked_mul(u64::from(tail_seconds))
        .ok_or(OfflineRenderError::TimelineRange)?;
    range_frames
        .checked_add(tail_frames)
        .filter(|frames| *frames > 0)
        .ok_or(OfflineRenderError::TimelineRange)
}

/// Computes frames for a project-sample range followed by a millisecond-precision tail.
pub fn render_range_length_samples_with_tail_milliseconds(
    sample_rate: u32,
    start_sample: u64,
    end_sample: u64,
    tail_milliseconds: u32,
) -> Result<u64, OfflineRenderError> {
    let scaled_tail_frames = u64::from(sample_rate)
        .checked_mul(u64::from(tail_milliseconds))
        .ok_or(OfflineRenderError::TimelineRange)?;
    let tail_frames = scaled_tail_frames
        .checked_add(500)
        .ok_or(OfflineRenderError::TimelineRange)?
        / 1_000;
    let range_frames = end_sample
        .checked_sub(start_sample)
        .ok_or(OfflineRenderError::TimelineRange)?;
    range_frames
        .checked_add(tail_frames)
        .filter(|frames| *frames > 0)
        .ok_or(OfflineRenderError::TimelineRange)
}

/// Renders an already prepared graph to stereo PCM24 WAV using fixed-size buffers.
///
/// The graph must contain all configured instruments/effects and its media feeders must remain
/// alive while this function runs. Source underruns fail the export, so an incomplete render is
/// never published as a successful file. Progress is reported on the calling thread; invoke this
/// function on a background worker when called from the desktop UI.
pub fn render_graph_to_pcm24_wav(
    graph: &mut AudioRenderGraph,
    destination: impl AsRef<Path>,
    sample_rate: u32,
    total_frames: u64,
    cancelled: &AtomicBool,
    report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    render_graph_to_wav(
        graph,
        destination,
        sample_rate,
        total_frames,
        WavExportOptions::default(),
        cancelled,
        report_progress,
    )
}

pub fn render_graph_to_wav(
    graph: &mut AudioRenderGraph,
    destination: impl AsRef<Path>,
    sample_rate: u32,
    total_frames: u64,
    options: WavExportOptions,
    cancelled: &AtomicBool,
    mut report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    let export = WavExport::create(destination.as_ref(), sample_rate, options)
        .map_err(OfflineRenderError::WavExport)?;
    render_graph_with_waiter(
        graph,
        RenderRequest {
            total_frames,
            start_sample: 0,
            cancelled,
        },
        export,
        |_, frames| Ok(frames),
        || Ok(()),
        &mut report_progress,
    )
}

/// Renders a prepared project's AudioItems while waiting off the audio thread for decoder queues.
pub fn render_prepared_audio_to_pcm24_wav(
    prepared: PreparedAudioPlayback,
    project: &Project,
    destination: impl AsRef<Path>,
    total_frames: u64,
    cancelled: &AtomicBool,
    report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    render_prepared_audio_to_wav(
        prepared,
        project,
        destination,
        total_frames,
        WavExportOptions::default(),
        cancelled,
        report_progress,
    )
}

pub fn render_prepared_audio_to_wav(
    prepared: PreparedAudioPlayback,
    project: &Project,
    destination: impl AsRef<Path>,
    total_frames: u64,
    options: WavExportOptions,
    cancelled: &AtomicBool,
    mut report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    let export = WavExport::create(
        destination.as_ref(),
        project.settings().sample_rate(),
        options,
    )
    .map_err(OfflineRenderError::WavExport)?;
    let (_graph, result) = render_prepared_inner(
        prepared,
        project,
        export,
        total_frames,
        0,
        cancelled,
        &mut report_progress,
    );
    result
}

fn render_prepared_inner(
    prepared: PreparedAudioPlayback,
    project: &Project,
    export: impl OfflineWavWriter,
    total_frames: u64,
    start_sample: u64,
    cancelled: &AtomicBool,
    report_progress: &mut impl FnMut(u64, u64),
) -> (AudioRenderGraph, Result<(), OfflineRenderError>) {
    let (mut graph, feeders) = prepared.into_parts();
    let feeders = RefCell::new(feeders.into_iter().map(Some).collect::<Vec<_>>());
    let item_ids: Vec<_> = project.audio_items().iter().map(|item| item.id()).collect();
    let result = render_graph_with_waiter(
        &mut graph,
        RenderRequest {
            total_frames,
            start_sample,
            cancelled,
        },
        export,
        |graph, requested_frames| loop {
            let available_frames =
                graph.audio_item_frames_available_for_next_block(requested_frames);
            if available_frames > 0 {
                break Ok(available_frames);
            }
            let Some(item_id) = graph.audio_item_missing_for_next_block(1) else {
                // A feeder can publish the first frame between the prefix query above and this
                // one-frame readiness check. Re-query the prefix instead of assuming the entire
                // requested block is ready; bounded queues may only have one frame at a time.
                thread::yield_now();
                continue;
            };
            if cancelled.load(Ordering::Acquire) {
                break Err(OfflineRenderError::Cancelled);
            }
            let Some(feeder_index) = item_ids.iter().position(|candidate| *candidate == item_id)
            else {
                break Err(OfflineRenderError::SourceEndedEarly { item_id });
            };
            let feeder_finished = {
                let mut feeders = feeders.borrow_mut();
                let Some(feeder) = feeders.get_mut(feeder_index).and_then(Option::as_mut) else {
                    break Err(OfflineRenderError::SourceEndedEarly { item_id });
                };
                if feeder.is_finished() {
                    feeders[feeder_index].take()
                } else {
                    None
                }
            };
            if let Some(feeder) = feeder_finished {
                if let Err(error) = feeder.join() {
                    break Err(OfflineRenderError::Media(error.to_string()));
                }
                break Err(OfflineRenderError::SourceEndedEarly { item_id });
            }
            thread::sleep(Duration::from_millis(1));
        },
        || {
            for feeder in feeders.borrow_mut().iter_mut().filter_map(Option::take) {
                feeder
                    .join()
                    .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
            }
            Ok(())
        },
        report_progress,
    );
    (graph, result)
}

/// Builds and exports a saved project snapshot, including its configured CLAP instruments and FX.
///
/// Invoke on a worker thread. Plugin libraries are trusted native code and are loaded in-process,
/// matching normal playback behavior. The project database is used for media lookup; project state
/// and live playback are not modified.
pub fn render_project_file_to_pcm24_wav(
    project_path: impl AsRef<Path>,
    project: &Project,
    destination: impl AsRef<Path>,
    master_ceiling: MasterOutputCeiling,
    cancelled: &AtomicBool,
    report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    render_project_file_to_wav(
        project_path,
        project,
        destination,
        master_ceiling,
        WavExportOptions::default(),
        cancelled,
        report_progress,
    )
}

pub fn render_project_file_to_wav(
    project_path: impl AsRef<Path>,
    project: &Project,
    destination: impl AsRef<Path>,
    master_ceiling: MasterOutputCeiling,
    options: WavExportOptions,
    cancelled: &AtomicBool,
    report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(OfflineRenderError::Cancelled);
    }
    let end_sample = project_content_end_sample(project)?;
    render_project_file_to_wav_with_settings(
        project_path,
        project,
        destination,
        ProjectWavRenderSettings {
            master_ceiling,
            export_options: options,
            output_sample_rate: project.settings().sample_rate(),
            output_channels: WavChannelCount::Stereo,
            source: ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample,
            tail_milliseconds: DEFAULT_EFFECT_TAIL_SECONDS * 1_000,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        },
        cancelled,
        report_progress,
    )
}

/// Renders an inclusive/exclusive project-sample range followed by a fixed effect tail.
///
/// The output begins at `start_sample` and contains `end_sample - start_sample` timeline frames,
/// followed by `tail_milliseconds` of tail. The saved project and live playback state are not modified.
pub fn render_project_file_to_wav_with_settings(
    project_path: impl AsRef<Path>,
    project: &Project,
    destination: impl AsRef<Path>,
    settings: ProjectWavRenderSettings,
    cancelled: &AtomicBool,
    mut report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    let ProjectWavRenderSettings {
        master_ceiling,
        export_options,
        output_sample_rate,
        output_channels,
        source,
        start_sample,
        end_sample,
        tail_milliseconds,
        normalize_peak,
        normalize_peak_target_centi_dbfs,
    } = settings;
    if cancelled.load(Ordering::Acquire) {
        return Err(OfflineRenderError::Cancelled);
    }
    let total_frames = render_range_length_samples_with_tail_milliseconds(
        project.settings().sample_rate(),
        start_sample,
        end_sample,
        tail_milliseconds,
    )?;
    let (isolated_project, bypass_master_output) = match source {
        ProjectWavRenderSource::MasterMix => (None, false),
        ProjectWavRenderSource::SelectedTrackStem(track_id) => {
            (Some(selected_track_stem_project(project, track_id)?), true)
        }
        ProjectWavRenderSource::SelectedTrackViaMaster(track_id) => (
            Some(selected_track_via_master_project(project, track_id)?),
            false,
        ),
    };
    let render_project = isolated_project.as_ref().unwrap_or(project);
    let export = WavExport::create_with_channels(
        destination.as_ref(),
        output_sample_rate,
        export_options,
        output_channels,
    )
    .map_err(OfflineRenderError::WavExport)?;
    let project_sample_rate = project.settings().sample_rate();
    let resampler = (output_sample_rate != project_sample_rate)
        .then(|| {
            StereoPcmResampler::new(
                project_sample_rate,
                output_sample_rate,
                ResamplingQuality::Sinc192,
            )
        })
        .transpose()
        .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
    let normalization_target = 10_f64.powf(f64::from(normalize_peak_target_centi_dbfs) / 2_000.0);
    let export = PeakNormalizingWriter::new(
        export,
        output_channels,
        normalize_peak,
        normalization_target,
        cancelled,
    )?;
    let export = SampleRateConvertingWriter {
        inner: export,
        resampler,
    };
    let store = ProjectStore::open(project_path.as_ref())
        .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
    let prepared = prepare_audio_playback_at_with_resampling_quality(
        render_project,
        &store,
        start_sample,
        16_384,
        2_048,
        ResamplingQuality::Sinc192,
    )
    .map_err(|error| OfflineRenderError::Media(error.to_string()));
    let close = store
        .close()
        .map_err(|error| OfflineRenderError::Media(error.to_string()));
    let mut prepared = prepared?;
    close?;
    if bypass_master_output {
        prepared
            .graph()
            .master_output_safety_controller()
            .set_guard_enabled(false);
    } else {
        prepared.set_master_output_ceiling_dbfs(master_ceiling);
    }

    let mut instrument_owners = Vec::new();
    let mut instrument_processors = Vec::new();
    let mut effect_owners = Vec::new();
    let mut effect_processors = Vec::new();
    let sample_rate = render_project.settings().sample_rate();
    let max_block_frames = prepared.graph().max_block_frames();
    for track in render_project.tracks() {
        if track.is_frozen() {
            continue;
        }
        if cancelled.load(Ordering::Acquire) {
            cleanup_uninstalled_processors(
                instrument_owners,
                instrument_processors,
                effect_owners,
                effect_processors,
            );
            return Err(OfflineRenderError::Cancelled);
        }
        if let Some(instrument) = track.instrument() {
            let max_events = prepared.graph().midi_event_capacity_for_track(track.id());
            // SAFETY: the user assigned this saved CLAP instrument through the normal plugin UI.
            let loaded = unsafe {
                ClapInstrumentOwner::load_with_state(
                    Path::new(instrument.bundle_path()),
                    instrument.plugin_id(),
                    instrument.state(),
                    sample_rate,
                    max_block_frames,
                    max_events,
                )
            };
            let (owner, processor) = match loaded {
                Ok(loaded) => loaded,
                Err(error) => {
                    cleanup_uninstalled_processors(
                        instrument_owners,
                        instrument_processors,
                        effect_owners,
                        effect_processors,
                    );
                    return Err(OfflineRenderError::Media(error.to_string()));
                }
            };
            let instance_id = owner.instance_id();
            instrument_owners.push((instance_id, owner));
            instrument_processors.push(TrackInstrumentProcessor::new(track.id(), processor));
        }
        for (chain_index, effect) in track.fx_chain().iter().enumerate() {
            if !effect.is_enabled() {
                continue;
            }
            let parameter_values: Vec<_> = effect.parameter_values().collect();
            // SAFETY: this saved effect was assigned through the normal plugin UI.
            let loaded = unsafe {
                ClapEffectOwner::load_with_state(
                    Path::new(effect.bundle_path()),
                    effect.plugin_id(),
                    effect.state(),
                    &parameter_values,
                    sample_rate,
                    max_block_frames,
                )
            };
            let (owner, processor) = match loaded {
                Ok(loaded) => loaded,
                Err(error) => {
                    cleanup_uninstalled_processors(
                        instrument_owners,
                        instrument_processors,
                        effect_owners,
                        effect_processors,
                    );
                    return Err(OfflineRenderError::Media(error.to_string()));
                }
            };
            let instance_id = owner.instance_id();
            effect_owners.push((instance_id, owner));
            effect_processors.push(TrackFxProcessor::new(
                track.id(),
                chain_index,
                effect.plugin_id(),
                processor,
            ));
        }
    }
    if cancelled.load(Ordering::Acquire) {
        cleanup_uninstalled_processors(
            instrument_owners,
            instrument_processors,
            effect_owners,
            effect_processors,
        );
        return Err(OfflineRenderError::Cancelled);
    }
    if let Err(error) = prepared
        .graph_mut()
        .install_instrument_processors(render_project, &mut instrument_processors)
    {
        cleanup_uninstalled_processors(
            instrument_owners,
            instrument_processors,
            effect_owners,
            effect_processors,
        );
        return Err(OfflineRenderError::Media(error.to_string()));
    }
    if let Err(error) = prepared
        .graph_mut()
        .install_fx_processors(render_project, &mut effect_processors)
    {
        let _stop_failures = prepared.graph_mut().stop_processors_after_offline_render();
        let stopped_instruments = prepared.graph_mut().take_stopped_instruments();
        deactivate_instruments(stopped_instruments, instrument_owners);
        cleanup_uninstalled_processors(Vec::new(), Vec::new(), effect_owners, effect_processors);
        return Err(OfflineRenderError::Media(error.to_string()));
    }

    let (mut graph, result) = render_prepared_inner(
        prepared,
        render_project,
        export,
        total_frames,
        start_sample,
        cancelled,
        &mut report_progress,
    );
    let _stop_failures = graph.stop_processors_after_offline_render();
    deactivate_instruments(graph.take_stopped_instruments(), instrument_owners);
    deactivate_effects(graph.take_stopped_fx_processors(), effect_owners);
    result
}

/// Renders each selected track through Master and publishes the complete WAV stem set.
///
/// Stems are rendered into a private staging directory on the destination filesystem. Existing
/// output files are rejected, and the staged set is published with no-clobber hard links only
/// after every track has rendered successfully. Cancellation or a render/publication error
/// removes the staged files and any outputs already linked by this call.
pub fn render_selected_tracks_via_master_to_wavs(
    project_path: impl AsRef<Path>,
    project: &Project,
    destinations: &[(TrackId, PathBuf)],
    settings: ProjectWavRenderSettings,
    cancelled: &AtomicBool,
    report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    let outputs = destinations
        .iter()
        .map(|(track_id, destination)| {
            (
                ProjectWavRenderSource::SelectedTrackViaMaster(*track_id),
                destination.clone(),
            )
        })
        .collect::<Vec<_>>();
    render_project_wav_sources_to_wavs(
        project_path,
        project,
        &outputs,
        settings,
        cancelled,
        report_progress,
    )
}

/// Renders the requested Master/selected-track outputs and publishes the complete WAV set.
///
/// Outputs are rendered into a private staging directory on the destination filesystem. Existing
/// output files are rejected, and staged files are published with no-clobber hard links only after
/// every source has rendered successfully. Cancellation or a render/publication error removes the
/// staged files and outputs already linked by this call.
pub fn render_project_wav_sources_to_wavs(
    project_path: impl AsRef<Path>,
    project: &Project,
    outputs: &[(ProjectWavRenderSource, PathBuf)],
    settings: ProjectWavRenderSettings,
    cancelled: &AtomicBool,
    mut report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    if outputs.is_empty() {
        return Err(OfflineRenderError::Media(
            "at least one render output is required".to_owned(),
        ));
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(OfflineRenderError::Cancelled);
    }

    let destination_directory = outputs[0]
        .1
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !destination_directory.is_dir() {
        return Err(OfflineRenderError::Media(format!(
            "stem output directory does not exist: {}",
            destination_directory.display()
        )));
    }

    let mut unique_destinations = HashSet::with_capacity(outputs.len());
    for (source, destination) in outputs {
        if let ProjectWavRenderSource::SelectedTrackStem(track_id)
        | ProjectWavRenderSource::SelectedTrackViaMaster(track_id) = source
            && !project.tracks().iter().any(|track| track.id() == *track_id)
        {
            return Err(OfflineRenderError::Media(format!(
                "selected render track {} no longer exists",
                track_id.value()
            )));
        }
        let parent = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        if parent != destination_directory {
            return Err(OfflineRenderError::Media(
                "all stem outputs must use the same directory".to_owned(),
            ));
        }
        if !unique_destinations.insert(destination.clone()) {
            return Err(OfflineRenderError::Media(format!(
                "stem outputs resolve to the same file: {}",
                destination.display()
            )));
        }
        match fs::symlink_metadata(destination) {
            Ok(_) => {
                return Err(OfflineRenderError::Media(format!(
                    "stem output already exists: {}",
                    destination.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(OfflineRenderError::Media(format!(
                    "could not inspect stem output {}: {error}",
                    destination.display()
                )));
            }
        }
    }

    let total_frames = render_range_length_samples_with_tail_milliseconds(
        project.settings().sample_rate(),
        settings.start_sample,
        settings.end_sample,
        settings.tail_milliseconds,
    )?;
    let total_work = total_frames
        .checked_mul(outputs.len() as u64)
        .ok_or(OfflineRenderError::TimelineRange)?;
    let staging_directory = StemStagingDirectory::create(destination_directory)?;
    let mut staged = Vec::with_capacity(outputs.len());

    for (index, (source, destination)) in outputs.iter().enumerate() {
        if cancelled.load(Ordering::Acquire) {
            return Err(OfflineRenderError::Cancelled);
        }
        let staging_path = staging_directory.0.join(format!("stem-{index:04}.wav"));
        let mut track_settings = settings;
        track_settings.source = *source;
        let work_offset = total_frames
            .checked_mul(index as u64)
            .ok_or(OfflineRenderError::TimelineRange)?;
        render_project_file_to_wav_with_settings(
            project_path.as_ref(),
            project,
            &staging_path,
            track_settings,
            cancelled,
            |done, _| {
                report_progress(
                    work_offset.saturating_add(done.min(total_frames)),
                    total_work,
                )
            },
        )?;
        staged.push((staging_path, destination));
    }

    if cancelled.load(Ordering::Acquire) {
        return Err(OfflineRenderError::Cancelled);
    }

    let mut published = Vec::with_capacity(staged.len());
    for (staging_path, destination) in staged {
        if let Err(error) = fs::hard_link(&staging_path, destination) {
            let rollback_error = rollback_stem_publication(&published);
            let detail = rollback_error.map_or_else(String::new, |error| {
                format!("; could not roll back a partial stem set: {error}")
            });
            return Err(OfflineRenderError::Media(format!(
                "could not publish stem {}: {error}{detail}",
                destination.display()
            )));
        }
        published.push(destination.clone());
    }
    report_progress(total_work, total_work);
    Ok(())
}

struct StemStagingDirectory(PathBuf);

impl StemStagingDirectory {
    fn create(parent: &Path) -> Result<Self, OfflineRenderError> {
        for _ in 0..16 {
            let id = NEXT_STEM_STAGING_ID.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".aaadaw-render-stems-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Err(error) =
                            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                        {
                            let _ = fs::remove_dir(&path);
                            return Err(OfflineRenderError::Media(format!(
                                "could not secure stem staging directory: {error}"
                            )));
                        }
                    }
                    return Ok(Self(path));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(OfflineRenderError::Media(format!(
                        "could not create stem staging directory: {error}"
                    )));
                }
            }
        }
        Err(OfflineRenderError::Media(
            "could not reserve a stem staging directory".to_owned(),
        ))
    }
}

impl Drop for StemStagingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn rollback_stem_publication(paths: &[PathBuf]) -> Option<std::io::Error> {
    let mut first_error = None;
    for path in paths.iter().rev() {
        if let Err(error) = fs::remove_file(path)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error
}

fn selected_track_via_master_project(
    project: &Project,
    selected_track_id: TrackId,
) -> Result<Project, OfflineRenderError> {
    selected_track_render_project(project, selected_track_id, false)
}

fn selected_track_stem_project(
    project: &Project,
    selected_track_id: TrackId,
) -> Result<Project, OfflineRenderError> {
    selected_track_render_project(project, selected_track_id, true)
}

fn selected_track_render_project(
    project: &Project,
    selected_track_id: TrackId,
    bypass_master_mix: bool,
) -> Result<Project, OfflineRenderError> {
    let mut render_project = Project::from_snapshot(project.snapshot())
        .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
    if !render_project
        .tracks()
        .iter()
        .any(|track| track.id() == selected_track_id)
    {
        return Err(OfflineRenderError::Media(
            "selected render track no longer exists".to_owned(),
        ));
    }
    if bypass_master_mix {
        render_project
            .apply(DawAction::SetMasterMix {
                mix: aaadaw_core::MasterMix::default(),
            })
            .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
    }
    let solo_updates: Vec<_> = render_project
        .tracks()
        .iter()
        .filter_map(|track| {
            let solo = track.id() == selected_track_id;
            (track.is_solo() != solo).then_some((track.id(), solo))
        })
        .collect();
    for (track_id, solo) in solo_updates {
        render_project
            .apply(DawAction::SetTrackSolo { track_id, solo })
            .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
    }
    Ok(render_project)
}

/// Renders one instrument track to float WAV from its first MIDI item through its last item and
/// the plugin tail. Float output preserves headroom because track and Master gain remain live
/// after freezing. Track gain, pan, routing, mute, solo, and volume automation are neutralized in
/// this temporary source render. The saved project and playback graph are never modified.
pub fn render_freeze_track_to_float32_wav(
    project_path: impl AsRef<Path>,
    project: &Project,
    track_id: aaadaw_core::TrackId,
    destination: impl AsRef<Path>,
    cancelled: &AtomicBool,
    mut report_progress: impl FnMut(u64, u64),
) -> Result<FrozenTrackRender, OfflineRenderError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(OfflineRenderError::Cancelled);
    }
    let track = project
        .tracks()
        .iter()
        .find(|track| track.id() == track_id)
        .ok_or_else(|| OfflineRenderError::Media("freeze track no longer exists".into()))?;
    if track.is_frozen() || track.is_bus() || track.is_folder() || track.instrument().is_none() {
        return Err(OfflineRenderError::Media(
            "only an unfrozen instrument track can be frozen".into(),
        ));
    }
    let midi_items: Vec<_> = project
        .midi_items()
        .iter()
        .filter(|item| item.track_id() == track_id)
        .collect();
    if !midi_items.iter().any(|item| !item.notes().is_empty()) {
        return Err(OfflineRenderError::Media(
            "track has no MIDI notes to render".into(),
        ));
    }
    let mut start_sample = u64::MAX;
    let mut content_end = 0_u64;
    for item in &midi_items {
        let end_tick = item
            .start_tick()
            .checked_add(item.length_ticks())
            .ok_or(OfflineRenderError::TimelineRange)?;
        start_sample = start_sample.min(
            project
                .sample_at_tick(item.start_tick())
                .map_err(|_| OfflineRenderError::TimelineRange)?,
        );
        content_end = content_end.max(
            project
                .sample_at_tick(end_tick)
                .map_err(|_| OfflineRenderError::TimelineRange)?,
        );
    }
    let mut snapshot = project.snapshot();
    snapshot
        .tracks
        .retain(|candidate| candidate.id == track_id.value());
    let source_track = snapshot
        .tracks
        .first_mut()
        .ok_or_else(|| OfflineRenderError::Media("freeze track snapshot is missing".into()))?;
    source_track.output_track_id = None;
    source_track.parent_track_id = None;
    source_track.main_send_enabled = true;
    source_track.sends.clear();
    source_track.volume_db = 0.0;
    source_track.pan = 0.0;
    source_track.muted = false;
    source_track.phase_inverted = false;
    snapshot.master_mix = aaadaw_core::MasterMix::default();
    source_track.solo = false;
    source_track.record_armed = false;
    source_track.volume_automation.clear();
    source_track.frozen_audio_item_id = None;
    snapshot.audio_items.clear();
    snapshot
        .midi_items
        .retain(|item| item.track_id == track_id.value());
    let source_project = Project::from_snapshot(snapshot)
        .map_err(|error| OfflineRenderError::Media(error.to_string()))?;

    let store = ProjectStore::open(project_path.as_ref())
        .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
    let prepared = prepare_audio_playback_with_resampling_quality(
        &source_project,
        &store,
        16_384,
        2_048,
        ResamplingQuality::Sinc192,
    )
    .map_err(|error| OfflineRenderError::Media(error.to_string()));
    let close = store
        .close()
        .map_err(|error| OfflineRenderError::Media(error.to_string()));
    let mut prepared = prepared?;
    close?;
    prepared
        .graph()
        .master_output_safety_controller()
        .set_guard_enabled(false);

    let mut instrument_owners = Vec::new();
    let mut instrument_processors = Vec::new();
    let mut effect_owners = Vec::new();
    let mut effect_processors = Vec::new();
    let mut plugin_tail_frames = Vec::new();
    let sample_rate = source_project.settings().sample_rate();
    let max_block_frames = prepared.graph().max_block_frames();
    let source_track = &source_project.tracks()[0];
    let instrument = source_track
        .instrument()
        .expect("validated freeze source has an instrument");
    let max_events = prepared.graph().midi_event_capacity_for_track(track_id);
    // SAFETY: this saved instrument was assigned through the normal plugin UI.
    let loaded = unsafe {
        ClapInstrumentOwner::load_with_state(
            Path::new(instrument.bundle_path()),
            instrument.plugin_id(),
            instrument.state(),
            sample_rate,
            max_block_frames,
            max_events,
        )
    }
    .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
    let (owner, mut processor) = loaded;
    match processor.tail_length_samples() {
        Some(frames) => plugin_tail_frames.push(frames),
        None => {
            instrument_owners.push((owner.instance_id(), owner));
            instrument_processors.push(TrackInstrumentProcessor::new(track_id, processor));
            cleanup_uninstalled_processors(
                instrument_owners,
                instrument_processors,
                effect_owners,
                effect_processors,
            );
            return Err(OfflineRenderError::Media(
                "cannot freeze a plugin that reports an infinite tail".into(),
            ));
        }
    }
    instrument_owners.push((owner.instance_id(), owner));
    instrument_processors.push(TrackInstrumentProcessor::new(track_id, processor));
    for (chain_index, effect) in source_track.fx_chain().iter().enumerate() {
        if !effect.is_enabled() {
            continue;
        }
        let parameter_values: Vec<_> = effect.parameter_values().collect();
        // SAFETY: this saved effect was assigned through the normal plugin UI.
        let loaded = unsafe {
            ClapEffectOwner::load_with_state(
                Path::new(effect.bundle_path()),
                effect.plugin_id(),
                effect.state(),
                &parameter_values,
                sample_rate,
                max_block_frames,
            )
        };
        let (owner, mut processor) = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                cleanup_uninstalled_processors(
                    instrument_owners,
                    instrument_processors,
                    effect_owners,
                    effect_processors,
                );
                return Err(OfflineRenderError::Media(error.to_string()));
            }
        };
        match processor.tail_length_samples() {
            Some(frames) => plugin_tail_frames.push(frames),
            None => {
                effect_owners.push((owner.instance_id(), owner));
                effect_processors.push(TrackFxProcessor::new(
                    track_id,
                    chain_index,
                    effect.plugin_id(),
                    processor,
                ));
                cleanup_uninstalled_processors(
                    instrument_owners,
                    instrument_processors,
                    effect_owners,
                    effect_processors,
                );
                return Err(OfflineRenderError::Media(
                    "cannot freeze a plugin that reports an infinite tail".into(),
                ));
            }
        }
        effect_owners.push((owner.instance_id(), owner));
        effect_processors.push(TrackFxProcessor::new(
            track_id,
            chain_index,
            effect.plugin_id(),
            processor,
        ));
    }
    let tail_frames = match freeze_tail_frames(sample_rate, &plugin_tail_frames) {
        Ok(frames) => frames,
        Err(error) => {
            cleanup_uninstalled_processors(
                instrument_owners,
                instrument_processors,
                effect_owners,
                effect_processors,
            );
            return Err(error);
        }
    };
    let total_frames = match content_end
        .checked_add(tail_frames)
        .and_then(|end_sample| end_sample.checked_sub(start_sample))
        .filter(|frames| *frames > 0)
    {
        Some(frames) => frames,
        None => {
            cleanup_uninstalled_processors(
                instrument_owners,
                instrument_processors,
                effect_owners,
                effect_processors,
            );
            return Err(OfflineRenderError::TimelineRange);
        }
    };
    if cancelled.load(Ordering::Acquire) {
        cleanup_uninstalled_processors(
            instrument_owners,
            instrument_processors,
            effect_owners,
            effect_processors,
        );
        return Err(OfflineRenderError::Cancelled);
    }
    // Open the destination before installing processors in the graph. If the destination is
    // unavailable, the processors are still owned by the local vectors and can be deactivated
    // through the normal uninstalled-processor cleanup path.
    let export = Float32WavExport::create(destination.as_ref(), sample_rate)
        .map_err(OfflineRenderError::Float32Wav);
    let export = match export {
        Ok(export) => export,
        Err(error) => {
            cleanup_uninstalled_processors(
                instrument_owners,
                instrument_processors,
                effect_owners,
                effect_processors,
            );
            return Err(error);
        }
    };
    if let Err(error) = prepared
        .graph_mut()
        .install_instrument_processors(&source_project, &mut instrument_processors)
    {
        cleanup_uninstalled_processors(
            instrument_owners,
            instrument_processors,
            effect_owners,
            effect_processors,
        );
        return Err(OfflineRenderError::Media(error.to_string()));
    }
    if let Err(error) = prepared
        .graph_mut()
        .install_fx_processors(&source_project, &mut effect_processors)
    {
        let _ = prepared.graph_mut().stop_processors_after_offline_render();
        deactivate_instruments(
            prepared.graph_mut().take_stopped_instruments(),
            instrument_owners,
        );
        cleanup_uninstalled_processors(Vec::new(), Vec::new(), effect_owners, effect_processors);
        return Err(OfflineRenderError::Media(error.to_string()));
    }
    let result = render_graph_with_waiter(
        prepared.graph_mut(),
        RenderRequest {
            total_frames,
            start_sample,
            cancelled,
        },
        export,
        |_, frames| Ok(frames),
        || Ok(()),
        &mut report_progress,
    );
    let graph = prepared.graph_mut();
    let _ = graph.stop_processors_after_offline_render();
    deactivate_instruments(graph.take_stopped_instruments(), instrument_owners);
    deactivate_effects(graph.take_stopped_fx_processors(), effect_owners);
    result?;
    Ok(FrozenTrackRender {
        start_sample,
        length_samples: total_frames,
    })
}

fn cleanup_uninstalled_processors(
    mut instrument_owners: Vec<(u64, ClapInstrumentOwner)>,
    instruments: Vec<TrackInstrumentProcessor>,
    mut effect_owners: Vec<(u64, ClapEffectOwner)>,
    effects: Vec<TrackFxProcessor>,
) {
    for processor in instruments {
        let instance_id = processor.instance_id();
        let (track_id, processor) = processor.into_parts();
        if let Some(index) = instrument_owners
            .iter()
            .position(|(candidate, _)| *candidate == instance_id)
        {
            let (_, owner) = instrument_owners.swap_remove(index);
            owner.deactivate(processor.stop());
        } else {
            let _ = track_id;
        }
    }
    for processor in effects {
        let (instance_id, _, _, _, processor) = processor.into_parts();
        if let Some(index) = effect_owners
            .iter()
            .position(|(candidate, _)| *candidate == instance_id)
        {
            let (_, owner) = effect_owners.swap_remove(index);
            owner.deactivate(processor.stop());
        }
    }
    for (_, mut owner) in instrument_owners {
        let _ = owner.try_deactivate_unused();
    }
    for (_, mut owner) in effect_owners {
        let _ = owner.try_deactivate_unused();
    }
}

fn deactivate_instruments(
    processors: Vec<aaadaw_engine::StoppedTrackInstrument>,
    mut owners: Vec<(u64, ClapInstrumentOwner)>,
) {
    for processor in processors {
        let instance_id = processor.instance_id();
        let (_, processor) = processor.into_parts();
        if let Some(index) = owners
            .iter()
            .position(|(candidate, _)| *candidate == instance_id)
        {
            let (_, owner) = owners.swap_remove(index);
            owner.deactivate(processor);
        }
    }
    for (_, mut owner) in owners {
        let _ = owner.try_deactivate_unused();
    }
}

fn deactivate_effects(
    processors: Vec<aaadaw_engine::StoppedTrackFxProcessor>,
    mut owners: Vec<(u64, ClapEffectOwner)>,
) {
    for processor in processors {
        let instance_id = processor.instance_id();
        let (_, _, _, processor) = processor.into_parts();
        if let Some(index) = owners
            .iter()
            .position(|(candidate, _)| *candidate == instance_id)
        {
            let (_, owner) = owners.swap_remove(index);
            owner.deactivate(processor);
        }
    }
    for (_, mut owner) in owners {
        let _ = owner.try_deactivate_unused();
    }
}

fn render_graph_with_waiter<W: OfflineWavWriter>(
    graph: &mut AudioRenderGraph,
    request: RenderRequest<'_>,
    mut export: W,
    mut wait_for_input: impl FnMut(&mut AudioRenderGraph, usize) -> Result<usize, OfflineRenderError>,
    before_finish: impl FnOnce() -> Result<(), OfflineRenderError>,
    report_progress: &mut impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    let RenderRequest {
        total_frames,
        start_sample,
        cancelled,
    } = request;
    let block_capacity = graph.max_block_frames();
    debug_assert!(
        block_capacity > 0,
        "render graphs reject zero block capacity"
    );
    let mut output = vec![[0.0_f32; 2]; block_capacity];
    graph.transport_mut().seek_sample(start_sample);
    graph.transport_mut().start();
    let mut rendered = 0_u64;
    report_progress(rendered, total_frames);

    while rendered < total_frames {
        if cancelled.load(Ordering::Acquire) {
            return Err(OfflineRenderError::Cancelled);
        }
        let requested_frames =
            usize::try_from((total_frames - rendered).min(block_capacity as u64))
                .expect("block frame count is bounded by usize capacity");
        let frames = wait_for_input(graph, requested_frames)?;
        if frames == 0 || frames > requested_frames {
            return Err(OfflineRenderError::InputUnderrun {
                samples: requested_frames,
            });
        }
        let stats = graph
            .render_into(&mut output[..frames])
            .map_err(OfflineRenderError::Graph)?;
        if stats.underrun_samples != 0 {
            return Err(OfflineRenderError::InputUnderrun {
                samples: stats.underrun_samples,
            });
        }
        export.write_frames(&output[..frames])?;
        rendered += frames as u64;
        report_progress(rendered, total_frames);
    }

    before_finish()?;
    if cancelled.load(Ordering::Acquire) {
        return Err(OfflineRenderError::Cancelled);
    }
    export.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_core::{DawAction, Project};
    use aaadaw_engine::{AudioRenderGraph, pcm_stream};
    use std::fs;
    use std::io::Read;
    use std::sync::atomic::AtomicU64;

    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

    fn destination() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "aaadaw-offline-render-{}-{}.wav",
            std::process::id(),
            NEXT_PATH.fetch_add(1, Ordering::Relaxed)
        ))
    }

    struct CancelAfterFirstWrite<'a> {
        inner: WavExport,
        cancelled: &'a AtomicBool,
        wrote_once: bool,
    }

    impl OfflineWavWriter for CancelAfterFirstWrite<'_> {
        fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), OfflineRenderError> {
            self.inner
                .write_frames(frames)
                .map_err(OfflineRenderError::WavExport)?;
            if !self.wrote_once {
                self.cancelled.store(true, Ordering::Release);
                self.wrote_once = true;
            }
            Ok(())
        }

        fn finish(self) -> Result<PathBuf, OfflineRenderError> {
            self.inner.finish().map_err(OfflineRenderError::WavExport)
        }
    }

    fn project_with_one_track() -> Project {
        let mut project = Project::with_settings(
            aaadaw_core::ProjectSettings::default()
                .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
        );
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Render".to_owned(),
            })
            .expect("track creation should succeed");
        project
    }

    #[test]
    fn project_render_length_includes_midi_end_and_the_fixed_effect_tail() {
        let mut project = project_with_one_track();
        project
            .apply(DawAction::InsertMidiItem {
                track_id: project.tracks()[0].id(),
                start_tick: 960,
                length_ticks: 960,
            })
            .expect("MIDI item should be inserted");
        assert_eq!(
            project_render_length_samples(&project, DEFAULT_EFFECT_TAIL_SECONDS)
                .expect("render length should be representable"),
            48_000 + 2 * 48_000
        );
    }

    #[test]
    fn selected_track_source_clears_other_solos_only_in_its_render_snapshot() {
        let mut project = project_with_one_track();
        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "Other".to_owned(),
            })
            .expect("second track should be created");
        let selected = project.tracks()[0].id();
        let other = project.tracks()[1].id();
        project
            .apply(DawAction::SetTrackSolo {
                track_id: other,
                solo: true,
            })
            .expect("the original solo should be set");

        let render_project = selected_track_via_master_project(&project, selected)
            .expect("render snapshot should isolate the requested track");
        assert!(render_project.tracks()[0].is_solo());
        assert!(!render_project.tracks()[1].is_solo());
        assert!(!project.tracks()[0].is_solo());
        assert!(project.tracks()[1].is_solo());
    }

    #[test]
    fn selected_track_stems_stage_as_a_set_and_publish_only_after_all_tracks_finish() {
        let source_path = destination();
        let mut source_export =
            WavExport::create(&source_path, 48_000, WavExportOptions::default())
                .expect("control source should be created");
        source_export
            .write_frames(&vec![[0.125, 0.375]; 48_000])
            .expect("control source should be written");
        source_export
            .finish()
            .expect("control source should be published");

        let mut project = project_with_one_track();
        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "Second".to_owned(),
            })
            .expect("second track should be created");
        let base = destination();
        let unique = base
            .file_stem()
            .expect("test destination should have a basename")
            .to_string_lossy();
        let project_path = base.with_extension("aaadaw");
        let mut store =
            ProjectStore::open(&project_path).expect("test project store should be created");
        let media_ref = store
            .import_audio_file(&source_path)
            .expect("control audio should be imported");
        store.close().expect("test project store should close");
        for index in 0..project.tracks().len() {
            let track_id = project.tracks()[index].id();
            project
                .apply(DawAction::InsertAudioItem {
                    track_id,
                    media_ref: media_ref.clone(),
                    start_sample: 0,
                    source_offset_samples: 0,
                    length_samples: 48_000,
                })
                .expect("control audio should be placed on both tracks");
        }
        let first = base.with_file_name(format!("{unique}-first.wav"));
        let second = base.with_file_name(format!("{unique}-second.wav"));
        let outputs = vec![
            (project.tracks()[0].id(), first.clone()),
            (project.tracks()[1].id(), second.clone()),
        ];
        let settings = ProjectWavRenderSettings {
            master_ceiling: MasterOutputCeiling::default(),
            export_options: WavExportOptions::default(),
            output_sample_rate: project.settings().sample_rate(),
            output_channels: WavChannelCount::Stereo,
            source: ProjectWavRenderSource::MasterMix,
            start_sample: 0,
            end_sample: 48_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        };

        fs::write(&first, b"preserve existing output")
            .expect("collision sentinel should be written");
        let cancelled = AtomicBool::new(false);
        assert!(
            render_selected_tracks_via_master_to_wavs(
                &project_path,
                &project,
                &outputs,
                settings,
                &cancelled,
                |_, _| {},
            )
            .is_err()
        );
        assert_eq!(fs::read(&first).unwrap(), b"preserve existing output");
        assert!(!second.exists(), "a collision must not publish other stems");
        fs::remove_file(&first).expect("collision sentinel should be removed");

        let cancelled = AtomicBool::new(false);
        let cancelled_after_first_stem = &cancelled;
        assert!(matches!(
            render_selected_tracks_via_master_to_wavs(
                &project_path,
                &project,
                &outputs,
                settings,
                &cancelled,
                |done, _| {
                    if done == 48_000 {
                        cancelled_after_first_stem.store(true, Ordering::Release);
                    }
                },
            ),
            Err(OfflineRenderError::Cancelled)
        ));
        assert!(!first.exists() && !second.exists());
        cancelled.store(false, Ordering::Release);

        let mut created_late_collision = false;
        assert!(
            render_selected_tracks_via_master_to_wavs(
                &project_path,
                &project,
                &outputs,
                settings,
                &cancelled,
                |done, total| {
                    if done == total && !created_late_collision {
                        fs::write(&second, b"preserve concurrent output")
                            .expect("late collision should be written");
                        created_late_collision = true;
                    }
                },
            )
            .is_err()
        );
        assert!(created_late_collision);
        assert!(
            !first.exists(),
            "a later publish failure rolls back earlier stems"
        );
        assert_eq!(fs::read(&second).unwrap(), b"preserve concurrent output");
        fs::remove_file(&second).expect("late collision sentinel should be removed");

        let mut progress = Vec::new();
        render_selected_tracks_via_master_to_wavs(
            &project_path,
            &project,
            &outputs,
            settings,
            &cancelled,
            |done, total| progress.push((done, total)),
        )
        .expect("all selected tracks should publish as stems");
        assert_eq!(progress.last(), Some(&(96_000, 96_000)));
        for path in [&first, &second] {
            let bytes = fs::read(path).expect("published stem should be readable");
            assert_eq!(bytes.len(), 44 + 48_000 * 2 * 3);
            assert_eq!(
                u32::from_le_bytes(bytes[40..44].try_into().unwrap()),
                48_000 * 6
            );
            let sample = |offset: usize| {
                let third = bytes[offset + 2];
                let signed = i32::from_le_bytes([
                    bytes[offset],
                    bytes[offset + 1],
                    third,
                    if third & 0x80 == 0 { 0 } else { 0xff },
                ]);
                signed as f64 / 8_388_608.0
            };
            assert!((sample(44) - 0.125).abs() < 1.0e-6);
            assert!((sample(47) - 0.375).abs() < 1.0e-6);
        }
        fs::remove_file(&first).expect("first stem should be removed");
        fs::remove_file(&second).expect("second stem should be removed");

        let mix = base.with_file_name(format!("{unique}-mix.wav"));
        let mixed_first = base.with_file_name(format!("{unique}-mix-first.wav"));
        let mixed_second = base.with_file_name(format!("{unique}-mix-second.wav"));
        let via_master = base.with_file_name(format!("{unique}-mix-via-master.wav"));
        project
            .apply(DawAction::SetMasterMix {
                mix: aaadaw_core::MasterMix::new(-6.0206, 0.0)
                    .expect("half-gain Master mix should be valid"),
            })
            .expect("Master mix should be lowered for the source comparison");
        let mix_and_stems = vec![
            (ProjectWavRenderSource::MasterMix, mix.clone()),
            (
                ProjectWavRenderSource::SelectedTrackStem(project.tracks()[0].id()),
                mixed_first.clone(),
            ),
            (
                ProjectWavRenderSource::SelectedTrackStem(project.tracks()[1].id()),
                mixed_second.clone(),
            ),
            (
                ProjectWavRenderSource::SelectedTrackViaMaster(project.tracks()[0].id()),
                via_master.clone(),
            ),
        ];
        render_project_wav_sources_to_wavs(
            &project_path,
            &project,
            &mix_and_stems,
            settings,
            &cancelled,
            |_, _| {},
        )
        .expect("Master mix and selected stems should publish together");
        for path in [&mix, &mixed_first, &mixed_second, &via_master] {
            let bytes = fs::read(path).expect("Master/stem output should be readable");
            assert_eq!(bytes.len(), 44 + 48_000 * 2 * 3);
        }
        let mix_bytes = fs::read(&mix).expect("Master mix should be readable");
        let read_pcm24 = |bytes: &[u8], offset: usize| {
            let third = bytes[offset + 2];
            let signed = i32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                third,
                if third & 0x80 == 0 { 0 } else { 0xff },
            ]);
            signed as f64 / 8_388_608.0
        };
        assert!((read_pcm24(&mix_bytes, 44) - 0.125).abs() < 1.0e-6);
        assert!((read_pcm24(&mix_bytes, 47) - 0.375).abs() < 1.0e-6);
        for path in [&mixed_first, &mixed_second] {
            let bytes = fs::read(path).expect("plain track stem should be readable");
            assert!((read_pcm24(&bytes, 44) - 0.125).abs() < 1.0e-6);
            assert!((read_pcm24(&bytes, 47) - 0.375).abs() < 1.0e-6);
        }
        let via_master_bytes = fs::read(&via_master).expect("via-Master track should be readable");
        assert!((read_pcm24(&via_master_bytes, 44) - 0.0625).abs() < 1.0e-6);
        assert!((read_pcm24(&via_master_bytes, 47) - 0.1875).abs() < 1.0e-6);
        fs::remove_file(&mix).expect("Master mix should be removed");
        fs::remove_file(&mixed_first).expect("first combined stem should be removed");
        fs::remove_file(&mixed_second).expect("second combined stem should be removed");
        fs::remove_file(&via_master).expect("via-Master comparison should be removed");
        fs::remove_file(source_path).expect("control source should be removed");
        for suffix in ["", "-wal", "-shm"] {
            let mut path = project_path.as_os_str().to_owned();
            path.push(suffix);
            let _ = fs::remove_file(PathBuf::from(path));
        }
    }

    #[test]
    fn freeze_tail_uses_the_longer_of_default_allowance_and_serial_plugin_tails() {
        assert_eq!(
            freeze_tail_frames(48_000, &[24_000, 120_000]).unwrap(),
            144_000
        );
        assert_eq!(freeze_tail_frames(48_000, &[24_000]).unwrap(), 96_000);
    }

    #[test]
    fn render_graph_to_wav_writes_exact_requested_frames_and_reports_progress() {
        let project = project_with_one_track();
        let (mut producer, consumer) = pcm_stream(8).expect("queue should be created");
        assert_eq!(producer.push_samples(&[0.5; 4]), 4);
        let mut graph = AudioRenderGraph::new(&project, vec![consumer], 4)
            .expect("graph should match the track count");
        let path = destination();
        let cancelled = AtomicBool::new(false);
        let mut progress = Vec::new();
        render_graph_to_pcm24_wav(
            &mut graph,
            &path,
            project.settings().sample_rate(),
            4,
            &cancelled,
            |done, total| progress.push((done, total)),
        )
        .expect("render should complete");
        assert_eq!(progress, [(0, 4), (4, 4)]);
        let mut bytes = Vec::new();
        fs::File::open(&path)
            .expect("export should exist")
            .read_to_end(&mut bytes)
            .expect("export should be readable");
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 24);
        let expected = (0.5_f64 * std::f64::consts::FRAC_1_SQRT_2 * 8_388_608.0).round() as i32;
        let sample = &bytes[44..47];
        assert_eq!(sample, &expected.to_le_bytes()[..3]);
        fs::remove_file(path).expect("test export should be removed");
    }

    #[test]
    fn render_graph_exports_the_stereo_mix_of_multiple_tracks() {
        let mut project = project_with_one_track();
        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "Second".to_owned(),
            })
            .expect("second track should be created");
        let (mut first_producer, first_consumer) = pcm_stream(8).expect("queue should be created");
        let (mut second_producer, second_consumer) =
            pcm_stream(8).expect("queue should be created");
        assert_eq!(first_producer.push_samples(&[0.1; 4]), 4);
        assert_eq!(second_producer.push_samples(&[0.1; 4]), 4);
        let mut graph = AudioRenderGraph::new(&project, vec![first_consumer, second_consumer], 4)
            .expect("graph should have one source per track");
        let path = destination();
        let cancelled = AtomicBool::new(false);
        render_graph_to_pcm24_wav(
            &mut graph,
            &path,
            project.settings().sample_rate(),
            4,
            &cancelled,
            |_, _| {},
        )
        .expect("both tracks should render");

        let bytes = fs::read(&path).expect("export should exist");
        let sample = i32::from_le_bytes([
            bytes[44],
            bytes[45],
            bytes[46],
            if bytes[46] & 0x80 == 0 { 0 } else { 0xff },
        ]);
        let expected = (0.1_f64 * std::f64::consts::SQRT_2 * 8_388_608.0).round() as i32;
        assert!((sample - expected).abs() <= 1);
        assert_eq!(&bytes[47..50], &bytes[44..47]);
        fs::remove_file(path).expect("test export should be removed");
    }

    #[test]
    fn underrun_and_cancellation_leave_no_published_export() {
        let project = project_with_one_track();
        let (_, consumer) = pcm_stream(8).expect("queue should be created");
        let mut graph = AudioRenderGraph::new(&project, vec![consumer], 4)
            .expect("graph should match the track count");
        let path = destination();
        let cancelled = AtomicBool::new(false);
        assert!(matches!(
            render_graph_to_pcm24_wav(
                &mut graph,
                &path,
                project.settings().sample_rate(),
                4,
                &cancelled,
                |_, _| {},
            ),
            Err(OfflineRenderError::InputUnderrun { .. })
        ));
        assert!(!path.exists());

        cancelled.store(true, Ordering::Release);
        assert!(matches!(
            render_graph_to_pcm24_wav(
                &mut graph,
                &path,
                project.settings().sample_rate(),
                4,
                &cancelled,
                |_, _| {},
            ),
            Err(OfflineRenderError::Cancelled)
        ));
        assert!(!path.exists());
    }

    #[test]
    fn range_render_length_adds_tail_at_the_output_sample_rate_and_checks_bounds() {
        assert_eq!(
            render_range_length_samples(48_000, 24_000, 72_000, 2)
                .expect("valid range should have a render length"),
            144_000
        );
        assert_eq!(
            render_range_length_samples_with_tail_milliseconds(48_000, 24_000, 72_000, 1_250)
                .expect("fractional tail should have a render length"),
            108_000
        );
        assert_eq!(
            render_range_length_samples_with_tail_milliseconds(44_100, 0, 0, 1)
                .expect("one millisecond should round to the nearest sample"),
            44
        );
        assert!(matches!(
            render_range_length_samples(48_000, 72_000, 24_000, 2),
            Err(OfflineRenderError::TimelineRange)
        ));
        assert!(matches!(
            render_range_length_samples(48_000, 24_000, 24_000, 0),
            Err(OfflineRenderError::TimelineRange)
        ));
    }

    #[test]
    fn cancelled_range_render_keeps_an_existing_destination_unchanged() {
        let path = destination();
        fs::write(&path, b"keep existing render").expect("sentinel destination should exist");
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            render_project_file_to_wav_with_settings(
                "unused-project.aaadaw",
                &Project::default(),
                &path,
                ProjectWavRenderSettings {
                    master_ceiling: MasterOutputCeiling::default(),
                    export_options: WavExportOptions::default(),
                    output_sample_rate: Project::default().settings().sample_rate(),
                    output_channels: WavChannelCount::Stereo,
                    source: ProjectWavRenderSource::MasterMix,
                    start_sample: 0,
                    end_sample: 48_000,
                    tail_milliseconds: 2_000,
                    normalize_peak: false,
                    normalize_peak_target_centi_dbfs: 0,
                },
                &cancelled,
                |_, _| {},
            ),
            Err(OfflineRenderError::Cancelled)
        ));
        assert_eq!(
            fs::read(&path).expect("existing file should survive"),
            b"keep existing render"
        );
        fs::remove_file(path).expect("sentinel destination should be removed");
    }

    #[test]
    fn cancelling_peak_normalization_drops_spool_and_does_not_publish_output() {
        let path = destination();
        let cancelled = AtomicBool::new(false);
        let wav = WavExport::create(&path, 48_000, WavExportOptions::default())
            .expect("temporary WAV should be created");
        let inner = CancelAfterFirstWrite {
            inner: wav,
            cancelled: &cancelled,
            wrote_once: false,
        };
        let mut export =
            PeakNormalizingWriter::new(inner, WavChannelCount::Stereo, true, 1.0, &cancelled)
                .expect("normalization spool should be created");
        let spool_path = export
            .spool
            .as_ref()
            .expect("normalization should spool")
            .path
            .clone();
        export
            .write_frames(&vec![[0.25, -0.5]; 8_192])
            .expect("frames should be spooled");

        assert!(matches!(
            export.finish(),
            Err(OfflineRenderError::Cancelled)
        ));
        assert!(!path.exists(), "cancelled output must not be published");
        assert!(!spool_path.exists(), "cancelled spool must be removed");
    }

    #[test]
    fn peak_normalization_scales_to_a_custom_linear_target() {
        let path = destination();
        let cancelled = AtomicBool::new(false);
        let wav = WavExport::create(&path, 48_000, WavExportOptions::default())
            .expect("temporary WAV should be created");
        let target_linear = 10_f64.powf(-602.0 / 2_000.0);
        let mut export = PeakNormalizingWriter::new(
            wav,
            WavChannelCount::Stereo,
            true,
            target_linear,
            &cancelled,
        )
        .expect("normalization spool should be created");
        export
            .write_frames(&[[0.125, 0.375]])
            .expect("reference frame should be spooled");
        export.finish().expect("normalized WAV should be published");

        let bytes = fs::read(&path).expect("normalized WAV should be readable");
        let sample = |offset: usize| {
            let third = bytes[offset + 2];
            let signed = i32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                third,
                if third & 0x80 == 0 { 0 } else { 0xff },
            ]);
            signed as f64 / 8_388_608.0
        };
        assert!((sample(44) - target_linear / 3.0).abs() < 1.0e-6);
        assert!((sample(47) - target_linear).abs() < 1.0e-6);
        fs::remove_file(path).expect("normalized WAV should be removed");
    }
}
