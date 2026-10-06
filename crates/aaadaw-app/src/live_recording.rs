//! Non-realtime recording-file ownership and finalization.
//!
//! The audio callback only writes fixed-size stereo frames to the bounded engine queue. This
//! worker drains that queue, writes bounded PCM WAV segments, and publishes them on stop.

use crate::CaptureTimelineAnchor;
use aaadaw_engine::{AudioCaptureConsumer, AudioCaptureControl};
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const WAV_HEADER_SIZE: u64 = 44;
const MAX_RIFF_DATA_BYTES: u64 = u32::MAX as u64 - WAV_HEADER_SIZE;
const MAX_SEGMENT_DATA_BYTES: u64 = MAX_RIFF_DATA_BYTES - MAX_RIFF_DATA_BYTES % 6;
const MAX_CAPTURE_GAP_SECONDS: u64 = 10;
const DRAIN_FRAMES: usize = 4096;
static NEXT_RECORDING_ID: AtomicU64 = AtomicU64::new(1);

const RECOVERY_MANIFEST_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingRecoveryManifest {
    pub version: u32,
    pub project_path: PathBuf,
    pub sample_rate: u32,
    /// Stable numeric `TrackId::value()` values; the core ID type intentionally stays opaque.
    pub track_ids: Vec<u64>,
    pub start_sample: Option<u64>,
    /// True until the capture-start playhead sample is durably refined by the writer.
    #[serde(default = "default_true")]
    pub start_sample_is_estimate: bool,
    pub stem: String,
    pub segments: Vec<RecordingSegment>,
    pub finalized: bool,
    #[serde(default)]
    pub discarded_frames: u64,
    #[serde(default)]
    pub discarded_tail_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingSegment {
    pub path: PathBuf,
    pub frame_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingRecoveryCandidate {
    pub manifest_path: PathBuf,
    pub manifest: RecordingRecoveryManifest,
    pub segment_paths: Vec<PathBuf>,
    pub recorded_frames: u64,
    pub discarded_frames: u64,
    pub discarded_tail_bytes: u64,
}

enum Command {
    RefineStartSample(u64),
    SetCaptureTimelineAnchor(CaptureTimelineAnchor, SyncSender<()>),
    Finish,
}

fn default_true() -> bool {
    true
}

/// Errors while recording or finalizing a take.
#[derive(Debug)]
pub enum AudioRecordingError {
    InvalidSampleRate,
    ProjectDirectoryUnavailable(PathBuf),
    Io(io::Error),
    CaptureFailed { overflow_frames: u64 },
    CaptureTimingInvalid,
    EmptyTake,
    TakeTooLarge,
    WorkerPanicked,
}

impl fmt::Display for AudioRecordingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => f.write_str("recording sample rate must be positive"),
            Self::ProjectDirectoryUnavailable(path) => write!(
                f,
                "project directory is not available for recording: {}",
                path.display()
            ),
            Self::Io(error) => write!(f, "recording file I/O failed: {error}"),
            Self::CaptureFailed { overflow_frames } => write!(
                f,
                "audio capture failed; {overflow_frames} frames were dropped"
            ),
            Self::CaptureTimingInvalid => {
                f.write_str("audio capture timing was invalid; the take was not aligned")
            }
            Self::EmptyTake => f.write_str("no audio frames were recorded"),
            Self::TakeTooLarge => f.write_str("take exceeds the WAV RIFF size limit"),
            Self::WorkerPanicked => f.write_str("recording file worker stopped unexpectedly"),
        }
    }
}

impl StdError for AudioRecordingError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for AudioRecordingError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Background segmented WAV writer for a single stereo take.
///
/// The writer owns the queue consumer. Call `finish` only after stopping and joining the input
/// backend, so no callback can publish frames after the worker drains the queue.
pub struct AudioRecordingWorker {
    command: Sender<Command>,
    thread: Option<JoinHandle<Result<Vec<PathBuf>, AudioRecordingError>>>,
    control: AudioCaptureControl,
    recovery_manifest: Option<(PathBuf, Arc<Mutex<RecordingRecoveryManifest>>)>,
}

impl AudioRecordingWorker {
    /// Starts a dedicated writer and creates temporary segment files next to the project.
    pub fn start(
        project_path: impl AsRef<Path>,
        sample_rate: u32,
        consumer: AudioCaptureConsumer,
        control: AudioCaptureControl,
    ) -> Result<Self, AudioRecordingError> {
        Self::start_with_segment_limit(
            project_path,
            sample_rate,
            consumer,
            control,
            MAX_SEGMENT_DATA_BYTES,
        )
    }

    /// Starts recording with a durable sidecar that identifies the owning project and tracks.
    pub fn start_recoverable(
        project_path: impl AsRef<Path>,
        sample_rate: u32,
        track_ids: Vec<u64>,
        consumer: AudioCaptureConsumer,
        control: AudioCaptureControl,
    ) -> Result<Self, AudioRecordingError> {
        Self::start_internal(
            project_path.as_ref(),
            sample_rate,
            consumer,
            control,
            MAX_SEGMENT_DATA_BYTES,
            Some(track_ids),
        )
    }

    fn start_with_segment_limit(
        project_path: impl AsRef<Path>,
        sample_rate: u32,
        consumer: AudioCaptureConsumer,
        control: AudioCaptureControl,
        segment_data_limit: u64,
    ) -> Result<Self, AudioRecordingError> {
        Self::start_internal(
            project_path.as_ref(),
            sample_rate,
            consumer,
            control,
            segment_data_limit,
            None,
        )
    }

    fn start_internal(
        project_path: &Path,
        sample_rate: u32,
        consumer: AudioCaptureConsumer,
        control: AudioCaptureControl,
        segment_data_limit: u64,
        recovery_tracks: Option<Vec<u64>>,
    ) -> Result<Self, AudioRecordingError> {
        if sample_rate == 0 {
            return Err(AudioRecordingError::InvalidSampleRate);
        }
        if segment_data_limit < 6 || !segment_data_limit.is_multiple_of(6) {
            return Err(AudioRecordingError::TakeTooLarge);
        }
        let directory = project_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_owned();
        if !directory.is_dir() {
            return Err(AudioRecordingError::ProjectDirectoryUnavailable(
                directory.to_owned(),
            ));
        }
        let stem = loop {
            let id = NEXT_RECORDING_ID.fetch_add(1, Ordering::Relaxed);
            let stem = project_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("project");
            let base = format!(".{stem}-take-{}-{id}", std::process::id());
            let first_part = directory.join(format!("{base}-000000.wav.part"));
            let first_final = directory.join(format!("{base}-000000.wav"));
            let manifest_path = directory.join(format!("{base}.recovery.json"));
            if !first_part.exists() && !first_final.exists() && !manifest_path.exists() {
                break base;
            }
        };
        let recovery_manifest = recovery_tracks.map(|track_ids| {
            let manifest_path = directory.join(format!("{stem}.recovery.json"));
            let manifest = RecordingRecoveryManifest {
                version: RECOVERY_MANIFEST_VERSION,
                project_path: project_path.to_path_buf(),
                sample_rate,
                track_ids,
                start_sample: None,
                start_sample_is_estimate: true,
                stem: stem.clone(),
                segments: Vec::new(),
                finalized: false,
                discarded_frames: 0,
                discarded_tail_bytes: 0,
            };
            (manifest_path, Arc::new(Mutex::new(manifest)))
        });
        if let Some((path, manifest)) = &recovery_manifest {
            write_recovery_manifest(path, &manifest.lock().unwrap_or_else(|e| e.into_inner()))?;
        }
        let (command, commands) = mpsc::channel();
        let worker_control = control.clone();
        let writer_manifest = recovery_manifest.clone();
        let thread = thread::Builder::new()
            .name("aaadaw-recording-writer".to_owned())
            .spawn(move || {
                write_take(RecordingWriter {
                    directory,
                    stem,
                    sample_rate,
                    segment_data_limit,
                    consumer,
                    control: worker_control,
                    commands,
                    recovery_manifest: writer_manifest,
                })
            })
            .map_err(|error| {
                if let Some((path, _)) = &recovery_manifest {
                    let _ = fs::remove_file(path);
                }
                AudioRecordingError::Io(error)
            })?;
        Ok(Self {
            command,
            thread: Some(thread),
            control,
            recovery_manifest,
        })
    }

    /// Persists a provisional transport position before capture is activated.
    pub fn set_start_sample(&self, sample: u64) -> Result<(), AudioRecordingError> {
        let Some((path, manifest)) = &self.recovery_manifest else {
            return Ok(());
        };
        let mut manifest = manifest.lock().unwrap_or_else(|error| error.into_inner());
        manifest.start_sample = Some(sample);
        manifest.start_sample_is_estimate = true;
        write_recovery_manifest(path, &manifest)
    }

    /// Queues the capture-start position for durable update without blocking capture activation.
    pub fn refine_start_sample(&self, sample: u64) -> Result<(), AudioRecordingError> {
        if self.recovery_manifest.is_none() {
            return Ok(());
        }
        self.command
            .send(Command::RefineStartSample(sample))
            .map_err(|_| AudioRecordingError::WorkerPanicked)
    }

    /// Installs the frame-to-project anchor on the writer before enabling a timestamped backend.
    pub fn set_capture_timeline_anchor(
        &self,
        anchor: CaptureTimelineAnchor,
    ) -> Result<(), AudioRecordingError> {
        if self.recovery_manifest.is_none() {
            return Ok(());
        }
        let (reply, response) = mpsc::sync_channel(0);
        self.command
            .send(Command::SetCaptureTimelineAnchor(anchor, reply))
            .map_err(|_| AudioRecordingError::WorkerPanicked)?;
        response
            .recv()
            .map_err(|_| AudioRecordingError::WorkerPanicked)
    }

    /// Returns the durable recovery sidecar path, if this writer is recoverable.
    pub fn recovery_manifest_path(&self) -> Option<&Path> {
        self.recovery_manifest
            .as_ref()
            .map(|(path, _)| path.as_path())
    }

    /// Requests finalization and waits for all queued frames to be written.
    pub fn finish(mut self) -> Result<Vec<PathBuf>, AudioRecordingError> {
        let _ = self.command.send(Command::Finish);
        self.thread
            .take()
            .ok_or(AudioRecordingError::WorkerPanicked)?
            .join()
            .map_err(|_| AudioRecordingError::WorkerPanicked)?
    }

    /// Invalidates the take and requests the writer to remove its temporary file.
    pub fn cancel(mut self) {
        self.control.fail();
        let _ = self.command.send(Command::Finish);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Some((manifest_path, _)) = &self.recovery_manifest {
            let _ = discard_recording_recovery(manifest_path);
        }
    }
}

impl Drop for AudioRecordingWorker {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.control.fail();
            let _ = self.command.send(Command::Finish);
            let _ = thread.join();
        }
    }
}

struct RecordingWriter {
    directory: PathBuf,
    stem: String,
    sample_rate: u32,
    segment_data_limit: u64,
    consumer: AudioCaptureConsumer,
    control: AudioCaptureControl,
    commands: Receiver<Command>,
    recovery_manifest: Option<(PathBuf, Arc<Mutex<RecordingRecoveryManifest>>)>,
}

struct RecordingSegments {
    directory: PathBuf,
    stem: String,
    sample_rate: u32,
    segment_data_limit: u64,
    recovery_manifest: Option<(PathBuf, Arc<Mutex<RecordingRecoveryManifest>>)>,
    current_segment: Option<OpenRecordingSegment>,
    completed_segments: Vec<PathBuf>,
    bytes: Vec<u8>,
}

impl RecordingSegments {
    fn new(
        directory: PathBuf,
        stem: String,
        sample_rate: u32,
        segment_data_limit: u64,
        recovery_manifest: Option<(PathBuf, Arc<Mutex<RecordingRecoveryManifest>>)>,
    ) -> Self {
        Self {
            directory,
            stem,
            sample_rate,
            segment_data_limit,
            recovery_manifest,
            current_segment: None,
            completed_segments: Vec::new(),
            bytes: Vec::with_capacity(DRAIN_FRAMES * 6),
        }
    }

    fn append_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), AudioRecordingError> {
        let mut frame_offset = 0;
        while frame_offset < frames.len() {
            if self.current_segment.is_none() {
                self.current_segment = Some(OpenRecordingSegment::create(
                    &self.directory,
                    &self.stem,
                    self.completed_segments.len(),
                    self.recovery_manifest.is_some(),
                )?);
            }
            let segment = self
                .current_segment
                .as_mut()
                .expect("segment was created before writing");
            let remaining_frames = ((self.segment_data_limit - segment.data_bytes) / 6) as usize;
            let write_frames = remaining_frames.min(frames.len() - frame_offset);
            self.bytes.clear();
            for frame in &frames[frame_offset..frame_offset + write_frames] {
                for sample in frame {
                    let sample = if sample.is_finite() {
                        sample.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    };
                    let pcm = if sample <= -1.0 {
                        -8_388_608_i32
                    } else {
                        (sample * 8_388_607.0).round() as i32
                    };
                    self.bytes.extend_from_slice(&pcm.to_le_bytes()[..3]);
                }
            }
            segment
                .file
                .as_mut()
                .expect("open segment retains its file")
                .write_all(&self.bytes)?;
            segment.data_bytes += (write_frames as u64) * 6;
            frame_offset += write_frames;
            if segment.data_bytes == self.segment_data_limit {
                let segment = self
                    .current_segment
                    .take()
                    .expect("full recording segment exists");
                let frame_count = segment.data_bytes / 6;
                let path = segment.finish(self.sample_rate)?;
                self.completed_segments.push(path.clone());
                record_finalized_segment(&self.recovery_manifest, path, frame_count)?;
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<Vec<PathBuf>, AudioRecordingError> {
        if let Some(segment) = self.current_segment.take() {
            let frame_count = segment.data_bytes / 6;
            let path = segment.finish(self.sample_rate)?;
            self.completed_segments.push(path.clone());
            record_finalized_segment(&self.recovery_manifest, path, frame_count)?;
        }
        if let Some((path, manifest)) = &self.recovery_manifest {
            let mut manifest = manifest.lock().unwrap_or_else(|error| error.into_inner());
            manifest.finalized = true;
            write_recovery_manifest(path, &manifest)?;
        }
        Ok(self.completed_segments.clone())
    }

    fn cleanup_after_error(&mut self) {
        self.current_segment.take();
        if self.recovery_manifest.is_none() {
            for path in self.completed_segments.drain(..) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn write_take(writer: RecordingWriter) -> Result<Vec<PathBuf>, AudioRecordingError> {
    let RecordingWriter {
        directory,
        stem,
        sample_rate,
        segment_data_limit,
        mut consumer,
        control,
        commands,
        recovery_manifest,
    } = writer;
    let mut segments = RecordingSegments::new(
        directory,
        stem,
        sample_rate,
        segment_data_limit,
        recovery_manifest,
    );
    let result = (|| {
        let mut frames = [[0.0_f32; 2]; DRAIN_FRAMES];
        let silence = [[0.0_f32; 2]; DRAIN_FRAMES];
        let mut finishing = false;
        let mut expected_frame: Option<u64> = None;
        let mut capture_timeline_anchor = None;
        loop {
            if !finishing {
                match commands.recv_timeout(Duration::from_millis(5)) {
                    Ok(Command::Finish) | Err(RecvTimeoutError::Disconnected) => finishing = true,
                    Ok(Command::RefineStartSample(sample)) => {
                        refine_recovery_start_sample(&segments.recovery_manifest, sample)?;
                    }
                    Ok(Command::SetCaptureTimelineAnchor(anchor, reply)) => {
                        capture_timeline_anchor = Some(anchor);
                        let _ = reply.send(());
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
            let Some(block) = consumer.pop_timed_frames(&mut frames) else {
                if finishing {
                    break;
                }
                continue;
            };
            if block.frame_count == 0 {
                return Err(AudioRecordingError::CaptureTimingInvalid);
            }
            let block_end = block
                .first_frame
                .checked_add(block.frame_count as u64)
                .ok_or(AudioRecordingError::CaptureTimingInvalid)?;
            if let Some(anchor) = capture_timeline_anchor.take() {
                let start_sample = anchor
                    .project_sample_at(block.first_frame)
                    .ok_or(AudioRecordingError::CaptureTimingInvalid)?;
                refine_recovery_start_sample(&segments.recovery_manifest, start_sample)?;
            }
            let max_gap_frames = u64::from(sample_rate)
                .checked_mul(MAX_CAPTURE_GAP_SECONDS)
                .ok_or(AudioRecordingError::CaptureTimingInvalid)?;
            if let Some(expected) = expected_frame {
                if block.first_frame < expected {
                    return Err(AudioRecordingError::CaptureTimingInvalid);
                }
                let mut gap = block.first_frame - expected;
                if gap > max_gap_frames {
                    return Err(AudioRecordingError::CaptureTimingInvalid);
                }
                while gap > 0 {
                    let silence_frames = gap.min(DRAIN_FRAMES as u64) as usize;
                    segments.append_frames(&silence[..silence_frames])?;
                    gap -= silence_frames as u64;
                }
            }
            segments.append_frames(&frames[..block.frame_count])?;
            expected_frame = Some(block_end);
        }
        if control.has_failed() {
            if control.has_timing_error() {
                return Err(AudioRecordingError::CaptureTimingInvalid);
            }
            return Err(AudioRecordingError::CaptureFailed {
                overflow_frames: control.overflow_frames(),
            });
        }
        if segments.current_segment.is_none() && segments.completed_segments.is_empty() {
            return Err(AudioRecordingError::EmptyTake);
        }
        segments.finish()
    })();
    if result.is_err() {
        control.fail();
        segments.cleanup_after_error();
    }
    result
}

fn refine_recovery_start_sample(
    recovery_manifest: &Option<(PathBuf, Arc<Mutex<RecordingRecoveryManifest>>)>,
    sample: u64,
) -> Result<(), AudioRecordingError> {
    let Some((path, manifest)) = recovery_manifest else {
        return Ok(());
    };
    let mut manifest = manifest.lock().unwrap_or_else(|error| error.into_inner());
    manifest.start_sample = Some(sample);
    manifest.start_sample_is_estimate = false;
    write_recovery_manifest(path, &manifest)
}

fn record_finalized_segment(
    recovery_manifest: &Option<(PathBuf, Arc<Mutex<RecordingRecoveryManifest>>)>,
    path: PathBuf,
    frame_count: u64,
) -> Result<(), AudioRecordingError> {
    if let Some((manifest_path, manifest)) = recovery_manifest {
        let mut manifest = manifest.lock().unwrap_or_else(|error| error.into_inner());
        manifest
            .segments
            .push(RecordingSegment { path, frame_count });
        write_recovery_manifest(manifest_path, &manifest)?;
    }
    Ok(())
}

fn write_recovery_manifest(
    path: &Path,
    manifest: &RecordingRecoveryManifest,
) -> Result<(), AudioRecordingError> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let temporary = path.with_extension("recovery.json.tmp");
    let contents = serde_json::to_vec(manifest)
        .map_err(|error| AudioRecordingError::Io(io::Error::other(error)))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)?;
    file.write_all(&contents)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

/// Finds durable recording manifests associated with the requested project path.
pub fn scan_recording_recoveries(
    project_path: impl AsRef<Path>,
) -> io::Result<Vec<RecordingRecoveryCandidate>> {
    let project_path = project_path.as_ref();
    let directory = project_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let project_identity = canonical_or_original(project_path);
    let mut candidates = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json")
            || !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".recovery.json"))
        {
            continue;
        }
        let contents = match fs::read(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let Ok(manifest) = serde_json::from_slice::<RecordingRecoveryManifest>(&contents) else {
            continue;
        };
        if manifest.version != RECOVERY_MANIFEST_VERSION
            || canonical_or_original(&manifest.project_path) != project_identity
            || path.file_name().and_then(|name| name.to_str())
                != Some(format!("{}.recovery.json", manifest.stem).as_str())
        {
            continue;
        }
        let mut segment_paths = Vec::new();
        let mut recorded_frames = 0_u64;
        let mut newly_discarded_frames = 0_u64;
        let mut newly_discarded_tail_bytes = 0_u64;
        for segment in recording_files(directory, &manifest.stem)? {
            if segment.extension().and_then(|extension| extension.to_str()) == Some("part") {
                let wav_path = segment.with_extension("");
                if wav_path.is_file() {
                    continue;
                }
                let bytes = fs::metadata(&segment)?
                    .len()
                    .saturating_sub(WAV_HEADER_SIZE);
                let complete_pcm_bytes = bytes - bytes % 6;
                let frame_count = complete_pcm_bytes / 6;
                if frame_count > 0 {
                    recorded_frames = recorded_frames.saturating_add(frame_count);
                    segment_paths.push(segment);
                    newly_discarded_tail_bytes =
                        newly_discarded_tail_bytes.saturating_add(bytes % 6);
                }
                continue;
            }
            if let Ok(decoder) = aaadaw_media::AudioStreamDecoder::open(&segment) {
                let metadata = decoder.metadata();
                if metadata.sample_rate == Some(manifest.sample_rate)
                    && metadata.channel_count == Some(2)
                    && let Some(frame_count) = metadata.frame_count.filter(|frames| *frames > 0)
                {
                    recorded_frames = recorded_frames.saturating_add(frame_count);
                    segment_paths.push(segment);
                }
            }
        }
        segment_paths.sort();
        for expected in &manifest.segments {
            if !segment_paths.contains(&expected.path) {
                newly_discarded_frames =
                    newly_discarded_frames.saturating_add(expected.frame_count);
            }
        }
        let discarded_frames = manifest
            .discarded_frames
            .saturating_add(newly_discarded_frames);
        let discarded_tail_bytes = manifest
            .discarded_tail_bytes
            .saturating_add(newly_discarded_tail_bytes);
        candidates.push(RecordingRecoveryCandidate {
            manifest_path: path,
            manifest,
            segment_paths,
            recorded_frames,
            discarded_frames,
            discarded_tail_bytes,
        });
    }
    candidates.sort_by(|left, right| left.manifest_path.cmp(&right.manifest_path));
    Ok(candidates)
}

/// Repairs a partial PCM WAV, validates candidate segments by decoding them, and updates manifest.
pub fn recover_recording_candidate(
    mut candidate: RecordingRecoveryCandidate,
) -> io::Result<RecordingRecoveryCandidate> {
    let scanned_frames = candidate.recorded_frames;
    let mut valid_paths = Vec::new();
    let mut recorded_frames = 0_u64;
    let mut segments = Vec::new();
    for path in &candidate.segment_paths {
        let wav_path = if path.extension().and_then(|extension| extension.to_str()) == Some("part")
        {
            finalize_partial_segment(path, candidate.manifest.sample_rate)?
        } else {
            path.clone()
        };
        let Ok(mut decoder) = aaadaw_media::AudioStreamDecoder::open(&wav_path) else {
            continue;
        };
        let metadata = decoder.metadata();
        if metadata.sample_rate != Some(candidate.manifest.sample_rate)
            || metadata.channel_count != Some(2)
        {
            continue;
        }
        let mut decoded_frames = 0_u64;
        let mut decode_failed = false;
        loop {
            match decoder.next_chunk() {
                Ok(Some(chunk)) => {
                    decoded_frames = decoded_frames.saturating_add(chunk.frame_count() as u64)
                }
                Ok(None) => break,
                Err(_) => {
                    decode_failed = true;
                    break;
                }
            }
        }
        if decode_failed || decoded_frames == 0 {
            continue;
        }
        recorded_frames = recorded_frames.saturating_add(decoded_frames);
        valid_paths.push(wav_path.clone());
        segments.push(RecordingSegment {
            path: wav_path,
            frame_count: decoded_frames,
        });
    }
    candidate.discarded_frames = candidate
        .discarded_frames
        .saturating_add(scanned_frames.saturating_sub(recorded_frames));
    candidate.manifest.segments = segments;
    candidate.manifest.finalized = true;
    candidate.manifest.discarded_frames = candidate.discarded_frames;
    candidate.manifest.discarded_tail_bytes = candidate.discarded_tail_bytes;
    write_recovery_manifest(&candidate.manifest_path, &candidate.manifest)
        .map_err(|error| io::Error::other(error.to_string()))?;
    candidate.segment_paths = valid_paths;
    candidate.recorded_frames = recorded_frames;
    Ok(candidate)
}

/// Removes a recovery session and its numbered WAV/partial files after explicit discard or save.
pub fn discard_recording_recovery(manifest_path: impl AsRef<Path>) -> io::Result<()> {
    let manifest_path = manifest_path.as_ref();
    let contents = match fs::read(manifest_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return remove_if_exists(&manifest_path.with_extension("recovery.json.tmp"));
        }
        Err(error) => return Err(error),
    };
    let manifest: RecordingRecoveryManifest = serde_json::from_slice(&contents)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let directory = manifest_path.parent().unwrap_or(Path::new("."));
    for path in recording_files(directory, &manifest.stem)? {
        remove_if_exists(&path)?;
    }
    remove_if_exists(&manifest_path.with_extension("recovery.json.tmp"))?;
    remove_if_exists(manifest_path)?;
    Ok(())
}

fn recording_files(directory: &Path, stem: &str) -> io::Result<Vec<PathBuf>> {
    let prefix = format!("{stem}-");
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with(&prefix) && (name.ends_with(".wav") || name.ends_with(".wav.part")) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn finalize_partial_segment(path: &Path, sample_rate: u32) -> io::Result<PathBuf> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let file_len = file.metadata()?.len();
    let pcm_bytes = file_len.saturating_sub(WAV_HEADER_SIZE);
    let complete_pcm_bytes = pcm_bytes - pcm_bytes % 6;
    let frame_count = complete_pcm_bytes / 6;
    if frame_count == 0 || complete_pcm_bytes > MAX_RIFF_DATA_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "partial take has no complete WAV frames",
        ));
    }
    let data_len = u32::try_from(complete_pcm_bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "partial take exceeds WAV size limit",
        )
    })?;
    let riff_len = 36_u32.checked_add(data_len).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "partial take exceeds WAV size limit",
        )
    })?;
    file.set_len(WAV_HEADER_SIZE + complete_pcm_bytes)?;
    file.seek(SeekFrom::Start(0))?;
    write_wav_header(&mut file, sample_rate, riff_len, data_len)?;
    file.sync_all()?;
    drop(file);
    let final_path = path.with_extension("");
    if final_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "finalized take segment already exists",
        ));
    }
    fs::rename(path, &final_path)?;
    #[cfg(unix)]
    if let Some(parent) = final_path.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(final_path)
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

struct OpenRecordingSegment {
    part_path: PathBuf,
    final_path: PathBuf,
    file: Option<File>,
    data_bytes: u64,
    preserve_partial: bool,
}

impl OpenRecordingSegment {
    fn create(
        directory: &Path,
        stem: &str,
        index: usize,
        preserve_partial: bool,
    ) -> io::Result<Self> {
        let part_path = directory.join(format!("{stem}-{index:06}.wav.part"));
        let final_path = directory.join(format!("{stem}-{index:06}.wav"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part_path)?;
        if let Err(error) = file.write_all(&[0; WAV_HEADER_SIZE as usize]) {
            let _ = fs::remove_file(&part_path);
            return Err(error);
        }
        Ok(Self {
            part_path,
            final_path,
            file: Some(file),
            data_bytes: 0,
            preserve_partial,
        })
    }

    fn finish(mut self, sample_rate: u32) -> Result<PathBuf, AudioRecordingError> {
        let data_len =
            u32::try_from(self.data_bytes).map_err(|_| AudioRecordingError::TakeTooLarge)?;
        let riff_len = 36_u32
            .checked_add(data_len)
            .ok_or(AudioRecordingError::TakeTooLarge)?;
        let mut file = self.file.take().expect("open segment retains its file");
        file.seek(SeekFrom::Start(0))?;
        write_wav_header(&mut file, sample_rate, riff_len, data_len)?;
        file.sync_all()?;
        drop(file);
        fs::hard_link(&self.part_path, &self.final_path)?;
        if let Err(error) = fs::remove_file(&self.part_path) {
            let _ = fs::remove_file(&self.final_path);
            return Err(AudioRecordingError::Io(error));
        }
        Ok(self.final_path.clone())
    }
}

impl Drop for OpenRecordingSegment {
    fn drop(&mut self) {
        if !self.preserve_partial {
            let _ = fs::remove_file(&self.part_path);
        }
    }
}

fn write_wav_header(
    file: &mut File,
    sample_rate: u32,
    riff_size: u32,
    data_bytes: u32,
) -> io::Result<()> {
    let byte_rate = sample_rate.checked_mul(6).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "WAV sample rate is too large")
    })?;
    file.write_all(b"RIFF")?;
    file.write_all(&riff_size.to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&6_u16.to_le_bytes())?;
    file.write_all(&24_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())
}

#[cfg(test)]
mod tests {
    use super::{
        AudioRecordingError, AudioRecordingWorker, RecordingRecoveryManifest,
        discard_recording_recovery, recover_recording_candidate, scan_recording_recoveries,
        write_recovery_manifest,
    };
    use crate::CaptureTimelineAnchor;
    use crate::{audio_capture_stream, prepare_audio_playback, start_audio_item_import};
    use aaadaw_core::{DawAction, Project};
    use aaadaw_media::AudioStreamDecoder;
    use aaadaw_storage::ProjectStore;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    static NEXT_DIR: AtomicU64 = AtomicU64::new(1);

    fn test_project_path() -> (PathBuf, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "aaadaw-recording-test-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).expect("test directory should be created");
        let project = directory.join("test.aaadaw");
        fs::write(&project, []).expect("placeholder project file should be created");
        (directory, project)
    }

    #[test]
    fn recording_worker_finalizes_a_decodable_stereo_wave_file() {
        let (directory, project) = test_project_path();
        let (mut producer, consumer, control) = audio_capture_stream(8);
        let worker = AudioRecordingWorker::start(&project, 48_000, consumer, control.clone())
            .expect("recording worker should start");
        control.start();
        producer.push_planar(&[0.25, -0.5], &[-0.25, 0.5]);
        control.stop();

        let recordings = worker.finish().expect("take should finalize");
        assert_eq!(recordings.len(), 1);
        let mut decoder = AudioStreamDecoder::open(&recordings[0]).expect("WAV should decode");
        let metadata = decoder.metadata();
        assert_eq!(metadata.sample_rate, Some(48_000));
        assert_eq!(metadata.channel_count, Some(2));
        assert_eq!(metadata.bits_per_sample, Some(24));
        assert_eq!(metadata.frame_count, Some(2));
        let chunk = decoder
            .next_chunk()
            .expect("audio should decode")
            .expect("take should contain frames");
        assert_eq!(chunk.samples().len(), 4);
        assert!((chunk.samples()[0] - 0.25).abs() < 0.001);
        assert!((chunk.samples()[1] + 0.25).abs() < 0.001);
        assert!(
            fs::read_dir(&directory)
                .expect("directory should be readable")
                .filter_map(Result::ok)
                .all(|entry| !entry.file_name().to_string_lossy().ends_with(".part"))
        );
        fs::remove_dir_all(directory).expect("test files should be removed");
    }

    #[test]
    fn capture_timestamp_gaps_become_silence_without_compressing_timeline_duration() {
        for sample_rate in [44_100, 48_000] {
            let (directory, project) = test_project_path();
            let (mut producer, consumer, control) = audio_capture_stream(16);
            let worker = AudioRecordingWorker::start_recoverable(
                &project,
                sample_rate,
                vec![41],
                consumer,
                control.clone(),
            )
            .expect("recoverable writer should start");
            let manifest_path = worker
                .recovery_manifest_path()
                .expect("manifest should be available")
                .to_path_buf();
            worker
                .set_start_sample(48_000)
                .expect("recording anchor should be persisted");
            control.start();
            producer.push_planar_at(100, &[0.1, 0.2], &[0.1, 0.2]);
            producer.push_planar_at(104, &[0.3], &[0.3]);
            control.stop();

            let recordings = worker
                .finish()
                .expect("take with a short gap should finalize");
            assert_eq!(recordings.len(), 1);
            let mut decoder = AudioStreamDecoder::open(&recordings[0]).expect("WAV should decode");
            assert_eq!(decoder.metadata().sample_rate, Some(sample_rate));
            assert_eq!(decoder.metadata().frame_count, Some(5));
            let chunk = decoder
                .next_chunk()
                .expect("recording should decode")
                .expect("recording should contain frames");
            let left = chunk
                .samples()
                .iter()
                .step_by(2)
                .copied()
                .collect::<Vec<_>>();
            assert_eq!(left.len(), 5);
            for (actual, expected) in left.iter().zip([0.1_f32, 0.2, 0.0, 0.0, 0.3]) {
                assert!((actual - expected).abs() < 0.001);
            }

            let manifest: RecordingRecoveryManifest = serde_json::from_slice(
                &fs::read(&manifest_path).expect("manifest should be readable"),
            )
            .expect("manifest should decode");
            assert_eq!(manifest.segments[0].frame_count, 5);
            discard_recording_recovery(&manifest_path).expect("test recovery should be discarded");
            fs::remove_dir_all(directory).expect("test directory should be removed");
        }
    }

    #[test]
    fn regressing_capture_timestamp_invalidates_the_take() {
        let (directory, project) = test_project_path();
        let (mut producer, consumer, control) = audio_capture_stream(16);
        let worker = AudioRecordingWorker::start(&project, 48_000, consumer, control.clone())
            .expect("writer should start");
        control.start();
        producer.push_planar_at(100, &[0.1, 0.2], &[0.1, 0.2]);
        producer.push_planar_at(101, &[0.3], &[0.3]);
        control.stop();

        assert!(matches!(
            worker.finish(),
            Err(AudioRecordingError::CaptureTimingInvalid)
        ));
        fs::remove_dir_all(directory).expect("test directory should be removed");
    }

    #[test]
    fn oversized_capture_gap_is_rejected_before_gap_fill_work() {
        let (directory, project) = test_project_path();
        let (mut producer, consumer, control) = audio_capture_stream(8);
        let worker = AudioRecordingWorker::start(&project, 48_000, consumer, control.clone())
            .expect("writer should start");
        control.start();
        producer.push_planar_at(100, &[0.1], &[0.1]);
        producer.push_planar_at(480_102, &[0.2], &[0.2]);
        control.stop();

        assert!(matches!(
            worker.finish(),
            Err(AudioRecordingError::CaptureTimingInvalid)
        ));
        fs::remove_dir_all(directory).expect("test directory should be removed");
    }

    #[test]
    fn interrupted_recoverable_take_restores_finalized_and_partial_rollover_segments() {
        let (directory, project) = test_project_path();
        let (mut producer, consumer, control) = audio_capture_stream(16);
        let worker = AudioRecordingWorker::start_internal(
            &project,
            48_000,
            consumer,
            control.clone(),
            12,
            Some(vec![41, 42]),
        )
        .expect("recoverable recording worker should start");
        let manifest_path = worker
            .recovery_manifest_path()
            .expect("manifest should exist before recording")
            .to_path_buf();
        worker
            .set_start_sample(96_000)
            .expect("recording start position should persist");
        control.start();
        producer.push_planar(&[0.1, 0.2, 0.3], &[-0.1, -0.2, -0.3]);
        control.fail();
        assert!(matches!(
            worker.finish(),
            Err(AudioRecordingError::CaptureFailed { .. })
        ));

        let partial_path = fs::read_dir(&directory)
            .expect("directory should be readable")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.to_string_lossy().ends_with(".wav.part"))
            .expect("crash boundary should leave the active segment partial");
        OpenOptions::new()
            .append(true)
            .open(&partial_path)
            .expect("partial segment should be appendable")
            .write_all(&[1, 2, 3, 4, 5])
            .expect("simulate a truncated final PCM frame");
        let mut manifest: super::RecordingRecoveryManifest =
            serde_json::from_slice(&fs::read(&manifest_path).expect("manifest should be readable"))
                .expect("manifest should decode");
        manifest.segments = vec![super::RecordingSegment {
            path: directory.join(format!("{}-missing.wav", manifest.stem)),
            frame_count: 2,
        }];
        write_recovery_manifest(&manifest_path, &manifest)
            .expect("simulate a crash before finalized paths reach the manifest");

        let candidates = scan_recording_recoveries(&project)
            .expect("interrupted recording should be discoverable");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].manifest.start_sample, Some(96_000));
        assert_eq!(candidates[0].manifest.track_ids, [41, 42]);
        assert_eq!(candidates[0].recorded_frames, 3);
        assert_eq!(candidates[0].discarded_frames, 2);
        assert_eq!(candidates[0].discarded_tail_bytes, 5);
        assert_eq!(candidates[0].segment_paths.len(), 2);

        let recovered = recover_recording_candidate(candidates[0].clone())
            .expect("complete PCM frames should be finalized and decoded");
        assert_eq!(recovered.recorded_frames, 3);
        assert_eq!(recovered.discarded_frames, 2);
        assert!(recovered.manifest.finalized);
        assert!(recovered.segment_paths.iter().all(|path| path.is_file()));
        let rescanned = scan_recording_recoveries(&project)
            .expect("recovery loss report should survive another restart");
        assert_eq!(rescanned[0].discarded_frames, 2);
        assert_eq!(rescanned[0].discarded_tail_bytes, 5);

        discard_recording_recovery(&manifest_path).expect("explicit discard should clean take");
        discard_recording_recovery(&manifest_path).expect("discard should be retry-safe");
        assert!(!manifest_path.exists());
        assert!(
            fs::read_dir(&directory)
                .expect("test directory should remain available")
                .all(|entry| !entry
                    .expect("directory entry should be readable")
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".wav"))
        );
        fs::remove_dir_all(directory).expect("test directory should be removed");
    }

    #[test]
    fn capture_start_position_refines_provisional_recovery_anchor_in_worker_order() {
        let (directory, project) = test_project_path();
        let (mut producer, consumer, control) = audio_capture_stream(8);
        let worker = AudioRecordingWorker::start_recoverable(
            &project,
            48_000,
            vec![41],
            consumer,
            control.clone(),
        )
        .expect("recoverable writer should start");
        let manifest_path = worker
            .recovery_manifest_path()
            .expect("manifest should be available")
            .to_path_buf();

        worker
            .set_start_sample(96_000)
            .expect("provisional position should be durable before setup work");
        // Model a delayed capture setup: the transport advances before input is enabled.
        let capture_start_sample = 96_512;
        control.start();
        worker
            .refine_start_sample(capture_start_sample)
            .expect("refined position should queue without waiting for disk I/O");
        producer.push_planar(&[0.25, 0.5], &[-0.25, -0.5]);
        control.stop();
        worker.finish().expect("take should finalize");

        let manifest: RecordingRecoveryManifest = serde_json::from_slice(
            &fs::read(&manifest_path).expect("updated manifest should be readable"),
        )
        .expect("manifest should decode");
        assert_eq!(manifest.start_sample, Some(capture_start_sample));
        assert!(!manifest.start_sample_is_estimate);

        let mut legacy_json = serde_json::to_value(&manifest).expect("manifest should serialize");
        legacy_json
            .as_object_mut()
            .expect("manifest should serialize as an object")
            .remove("start_sample_is_estimate");
        let legacy_manifest: RecordingRecoveryManifest =
            serde_json::from_value(legacy_json).expect("older manifest should remain readable");
        assert!(legacy_manifest.start_sample_is_estimate);

        let candidates =
            scan_recording_recoveries(&project).expect("finalized take should be discoverable");
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].manifest.start_sample,
            Some(capture_start_sample)
        );
        assert!(!candidates[0].manifest.start_sample_is_estimate);
        fs::remove_dir_all(directory).expect("test files should be removed");
    }

    #[test]
    fn first_capture_frame_refines_recovery_to_its_mapped_project_sample() {
        let (directory, project) = test_project_path();
        let (mut producer, consumer, control) = audio_capture_stream(8);
        let worker = AudioRecordingWorker::start_recoverable(
            &project,
            48_000,
            vec![41],
            consumer,
            control.clone(),
        )
        .expect("recoverable recording worker should start");
        let manifest_path = worker
            .recovery_manifest_path()
            .expect("manifest should exist")
            .to_path_buf();
        let anchor = CaptureTimelineAnchor::new(10_000, 96_000, 48_000, 48_000)
            .expect("JACK capture clock should map to the project rate");
        worker
            .set_start_sample(96_000)
            .expect("provisional start should be saved");
        worker
            .set_capture_timeline_anchor(anchor)
            .expect("capture clock anchor should be queued before capture starts");
        control.start();
        producer.push_planar_at(10_256, &[0.25, 0.5], &[-0.25, -0.5]);
        control.stop();

        worker.finish().expect("take should finalize");
        let manifest: RecordingRecoveryManifest = serde_json::from_slice(
            &fs::read(&manifest_path).expect("recovery manifest should be readable"),
        )
        .expect("recovery manifest should decode");
        assert_eq!(manifest.start_sample, Some(96_256));
        assert!(!manifest.start_sample_is_estimate);

        let recovered = scan_recording_recoveries(&project)
            .expect("take should remain discoverable")
            .pop()
            .expect("take should have one recovery candidate");
        assert_eq!(recovered.manifest.start_sample, Some(96_256));
        assert!(!recovered.manifest.start_sample_is_estimate);
        discard_recording_recovery(&manifest_path).expect("test recording should be discarded");
        fs::remove_dir_all(directory).expect("test files should be removed");
    }

    #[test]
    fn recording_worker_rolls_over_into_contiguous_decodable_segments() {
        let (directory, project) = test_project_path();
        let (mut producer, consumer, control) = audio_capture_stream(16);
        let worker = AudioRecordingWorker::start_with_segment_limit(
            &project,
            48_000,
            consumer,
            control.clone(),
            12,
        )
        .expect("recording worker should start");
        control.start();
        producer.push_planar(&[0.1, 0.2, 0.3, 0.4, 0.5], &[0.1, 0.2, 0.3, 0.4, 0.5]);
        control.stop();

        let recordings = worker.finish().expect("take segments should finalize");
        assert_eq!(recordings.len(), 3);
        let mut decoded = Vec::new();
        for (index, recording) in recordings.iter().enumerate() {
            let mut decoder = AudioStreamDecoder::open(recording).expect("segment should decode");
            assert_eq!(decoder.metadata().sample_rate, Some(48_000));
            assert_eq!(decoder.metadata().channel_count, Some(2));
            let chunk = decoder
                .next_chunk()
                .expect("segment should decode")
                .expect("segment should contain frames");
            assert_eq!(chunk.samples().len(), if index == 2 { 2 } else { 4 });
            decoded.extend(chunk.samples().iter().step_by(2).copied());
        }
        for (actual, expected) in decoded.iter().zip([0.1_f32, 0.2, 0.3, 0.4, 0.5]) {
            assert!((actual - expected).abs() < 0.001);
        }
        fs::remove_dir_all(directory).expect("test files should be removed");
    }

    #[test]
    fn segmented_take_survives_save_reopen_and_playback_across_the_boundary() {
        let (directory, project_path) = test_project_path();
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Long take".to_owned(),
            })
            .expect("track should be created");
        let track_id = project.tracks()[0].id();
        let mut store = ProjectStore::open(&project_path).expect("project store should open");
        store
            .save(&project)
            .expect("project should be saved before recording");
        store.close().expect("project store should close");

        let (mut producer, consumer, control) = audio_capture_stream(16);
        let writer = AudioRecordingWorker::start_with_segment_limit(
            &project_path,
            project.settings().sample_rate(),
            consumer,
            control.clone(),
            12,
        )
        .expect("recording writer should start");
        control.start();
        producer.push_planar(&[0.1, 0.2, 0.3, 0.4], &[0.1, 0.2, 0.3, 0.4]);
        control.stop();
        let recordings = writer.finish().expect("take should finalize into segments");
        assert_eq!(recordings.len(), 2);

        let mut cursor = 0;
        let mut placements = Vec::new();
        for recording in &recordings {
            let worker = start_audio_item_import(
                &project_path,
                recording,
                track_id,
                cursor,
                project.settings().sample_rate(),
            )
            .expect("recorded segment should import");
            while !worker.is_finished() {
                let _ = worker.progress().try_iter().count();
                std::thread::sleep(Duration::from_millis(1));
            }
            let action = worker.finish().expect("segment metadata should finalize");
            let DawAction::InsertAudioItem { length_samples, .. } = &action else {
                panic!("segment import should produce an audio placement");
            };
            cursor += length_samples;
            placements.push(action);
        }
        project
            .apply(DawAction::BatchTransaction {
                tx_id: 1,
                actions: placements,
            })
            .expect("segments should form one continuous edit");
        let mut store = ProjectStore::open(&project_path).expect("project store should reopen");
        store.save(&project).expect("segmented take should save");
        store.close().expect("project store should close");

        let store = ProjectStore::open(&project_path).expect("saved project should reopen");
        let reopened = store.load().expect("saved project should load");
        assert_eq!(reopened.audio_items().len(), 2);
        assert_eq!(reopened.audio_items()[0].start_sample(), 0);
        assert_eq!(reopened.audio_items()[1].start_sample(), 2);
        let prepared = prepare_audio_playback(&reopened, &store, 16, 8)
            .expect("all recorded segments should resolve for playback");
        let (mut graph, feeders) = prepared.into_parts();
        for feeder in feeders {
            feeder.join().expect("recording feeder should finish");
        }
        graph.transport_mut().start();
        let mut output = [[0.0; 2]; 4];
        let stats = graph
            .render_into(&mut output)
            .expect("segmented take should render across the join");
        assert_eq!(stats.underrun_samples, 0);
        assert!(output.iter().all(|frame| frame[0] > 0.0));
        assert!(output[0][0] < output[1][0]);
        assert!(output[1][0] < output[2][0]);
        assert!(output[2][0] < output[3][0]);
        drop(store);
        fs::remove_dir_all(directory).expect("test files should be removed");
    }

    #[test]
    fn capture_gap_crossing_segment_boundary_survives_recovery_import_and_playback() {
        let (directory, project_path) = test_project_path();
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Gap take".to_owned(),
            })
            .expect("track should be created");
        let track_id = project.tracks()[0].id();
        let sample_rate = project.settings().sample_rate();
        let project_anchor = 12_000;
        let mut store = ProjectStore::open(&project_path).expect("project store should open");
        store.save(&project).expect("project should be saved");
        store.close().expect("project store should close");

        let (mut producer, consumer, control) = audio_capture_stream(16);
        let writer = AudioRecordingWorker::start_internal(
            &project_path,
            sample_rate,
            consumer,
            control.clone(),
            12,
            Some(vec![track_id.value()]),
        )
        .expect("recoverable writer should start");
        let manifest_path = writer
            .recovery_manifest_path()
            .expect("recovery manifest should exist")
            .to_path_buf();
        writer
            .set_start_sample(project_anchor)
            .expect("capture anchor should persist");
        control.start();
        producer.push_planar_at(50_000, &[0.1, 0.2], &[0.1, 0.2]);
        producer.push_planar_at(50_003, &[0.4], &[0.4]);
        control.stop();

        let recordings = writer.finish().expect("gapped take should finalize");
        assert_eq!(recordings.len(), 2);
        let manifest: RecordingRecoveryManifest =
            serde_json::from_slice(&fs::read(&manifest_path).expect("manifest should be readable"))
                .expect("manifest should decode");
        assert_eq!(manifest.start_sample, Some(project_anchor));
        assert_eq!(
            manifest
                .segments
                .iter()
                .map(|segment| segment.frame_count)
                .collect::<Vec<_>>(),
            [2, 2]
        );
        assert!(manifest.finalized);

        let candidates = scan_recording_recoveries(&project_path)
            .expect("finalized gapped take should be discoverable");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].recorded_frames, 4);
        let recovered = recover_recording_candidate(candidates[0].clone())
            .expect("finalized take should survive recovery validation");
        assert_eq!(recovered.recorded_frames, 4);

        let mut cursor = project_anchor;
        let mut placements = Vec::new();
        for recording in &recordings {
            let worker =
                start_audio_item_import(&project_path, recording, track_id, cursor, sample_rate)
                    .expect("each recovered segment should import");
            while !worker.is_finished() {
                let _ = worker.progress().try_iter().count();
                std::thread::sleep(Duration::from_millis(1));
            }
            let action = worker.finish().expect("segment should produce a placement");
            let DawAction::InsertAudioItem { length_samples, .. } = &action else {
                panic!("segment import should create an audio item");
            };
            cursor += length_samples;
            placements.push(action);
        }
        assert_eq!(cursor - project_anchor, 4);
        project
            .apply(DawAction::BatchTransaction {
                tx_id: 1,
                actions: placements,
            })
            .expect("gapped segments should be placed as one take");
        let mut store = ProjectStore::open(&project_path).expect("project store should reopen");
        store.save(&project).expect("imported take should save");
        store.close().expect("project store should close");

        let store = ProjectStore::open(&project_path).expect("saved project should reopen");
        let reopened = store.load().expect("saved take should load");
        assert_eq!(reopened.audio_items().len(), 2);
        assert_eq!(reopened.audio_items()[0].start_sample(), project_anchor);
        assert_eq!(reopened.audio_items()[1].start_sample(), project_anchor + 2);
        let prepared = prepare_audio_playback(&reopened, &store, 16, 8)
            .expect("imported segments should resolve for playback");
        let (mut graph, feeders) = prepared.into_parts();
        for feeder in feeders {
            feeder.join().expect("take feeder should finish");
        }
        graph.transport_mut().seek_sample(project_anchor);
        graph.transport_mut().start();
        let mut output = [[0.0; 2]; 4];
        let stats = graph
            .render_into(&mut output)
            .expect("gapped take should render");
        assert_eq!(stats.underrun_samples, 0);
        assert!(output[0][0] > 0.0 && output[1][0] > 0.0);
        assert!(output[2][0].abs() < 0.001);
        assert!(output[3][0] > 0.0);

        discard_recording_recovery(&manifest_path).expect("recovery sidecar should be cleaned");
        fs::remove_dir_all(directory).expect("test files should be removed");
    }

    #[test]
    fn overflow_invalidates_the_take_and_removes_partial_audio() {
        let (directory, project) = test_project_path();
        let (mut producer, consumer, control) = audio_capture_stream(1);
        let worker = AudioRecordingWorker::start(&project, 48_000, consumer, control.clone())
            .expect("recording worker should start");
        control.start();
        producer.push_planar(&[0.1, 0.2], &[0.1, 0.2]);
        control.stop();

        assert!(matches!(
            worker.finish(),
            Err(AudioRecordingError::CaptureFailed { overflow_frames: 1 })
        ));
        assert_eq!(
            fs::read_dir(&directory)
                .expect("directory should be readable")
                .count(),
            1,
            "only the placeholder project should remain"
        );
        fs::remove_dir_all(directory).expect("test files should be removed");
    }
}
