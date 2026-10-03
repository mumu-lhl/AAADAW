//! Non-realtime recording-file ownership and finalization.
//!
//! The audio callback only writes fixed-size stereo frames to the bounded engine queue. This
//! worker drains that queue, writes bounded PCM WAV segments, and publishes them on stop.

use aaadaw_engine::{AudioCaptureConsumer, AudioCaptureControl};
use std::error::Error as StdError;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const WAV_HEADER_SIZE: u64 = 44;
const MAX_RIFF_DATA_BYTES: u64 = u32::MAX as u64 - WAV_HEADER_SIZE;
const MAX_SEGMENT_DATA_BYTES: u64 = MAX_RIFF_DATA_BYTES - MAX_RIFF_DATA_BYTES % 6;
const DRAIN_FRAMES: usize = 4096;
static NEXT_RECORDING_ID: AtomicU64 = AtomicU64::new(1);

enum Command {
    Finish,
}

/// Errors while recording or finalizing a take.
#[derive(Debug)]
pub enum AudioRecordingError {
    InvalidSampleRate,
    ProjectDirectoryUnavailable(PathBuf),
    Io(io::Error),
    CaptureFailed { overflow_frames: u64 },
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

    fn start_with_segment_limit(
        project_path: impl AsRef<Path>,
        sample_rate: u32,
        consumer: AudioCaptureConsumer,
        control: AudioCaptureControl,
        segment_data_limit: u64,
    ) -> Result<Self, AudioRecordingError> {
        if sample_rate == 0 {
            return Err(AudioRecordingError::InvalidSampleRate);
        }
        if segment_data_limit < 6 || segment_data_limit % 6 != 0 {
            return Err(AudioRecordingError::TakeTooLarge);
        }
        let project_path = project_path.as_ref();
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
            if !first_part.exists() && !first_final.exists() {
                break base;
            }
        };
        let (command, commands) = mpsc::channel();
        let worker_control = control.clone();
        let thread = thread::Builder::new()
            .name("aaadaw-recording-writer".to_owned())
            .spawn(move || {
                write_take(
                    directory,
                    stem,
                    sample_rate,
                    segment_data_limit,
                    consumer,
                    worker_control,
                    commands,
                )
            })
            .map_err(AudioRecordingError::Io)?;
        Ok(Self {
            command,
            thread: Some(thread),
            control,
        })
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

fn write_take(
    directory: PathBuf,
    stem: String,
    sample_rate: u32,
    segment_data_limit: u64,
    mut consumer: AudioCaptureConsumer,
    control: AudioCaptureControl,
    commands: Receiver<Command>,
) -> Result<Vec<PathBuf>, AudioRecordingError> {
    let mut completed_segments = Vec::new();
    let mut current_segment = None;
    let result = (|| {
        let mut frames = [[0.0_f32; 2]; DRAIN_FRAMES];
        let mut bytes = Vec::with_capacity(DRAIN_FRAMES * 6);
        let mut finishing = false;
        loop {
            if !finishing {
                match commands.recv_timeout(Duration::from_millis(5)) {
                    Ok(Command::Finish) | Err(RecvTimeoutError::Disconnected) => finishing = true,
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
            let count = consumer.pop_frames(&mut frames);
            if count == 0 {
                if finishing {
                    break;
                }
                continue;
            }
            let mut frame_offset = 0;
            while frame_offset < count {
                if current_segment.is_none() {
                    current_segment = Some(OpenRecordingSegment::create(
                        &directory,
                        &stem,
                        completed_segments.len(),
                    )?);
                }
                let segment = current_segment.as_mut().expect("segment was created");
                let remaining_frames = ((segment_data_limit - segment.data_bytes) / 6) as usize;
                let write_frames = remaining_frames.min(count - frame_offset);
                bytes.clear();
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
                        bytes.extend_from_slice(&pcm.to_le_bytes()[..3]);
                    }
                }
                segment
                    .file
                    .as_mut()
                    .expect("open segment retains its file")
                    .write_all(&bytes)?;
                segment.data_bytes += (write_frames as u64) * 6;
                frame_offset += write_frames;
                if segment.data_bytes == segment_data_limit {
                    let segment = current_segment.take().expect("full segment exists");
                    completed_segments.push(segment.finish(sample_rate)?);
                }
            }
        }
        if control.has_failed() {
            return Err(AudioRecordingError::CaptureFailed {
                overflow_frames: control.overflow_frames(),
            });
        }
        if current_segment.is_none() && completed_segments.is_empty() {
            return Err(AudioRecordingError::EmptyTake);
        }
        if let Some(segment) = current_segment.take() {
            completed_segments.push(segment.finish(sample_rate)?);
        }
        Ok(completed_segments.clone())
    })();
    if result.is_err() {
        control.fail();
        drop(current_segment);
        for path in completed_segments {
            let _ = fs::remove_file(path);
        }
    }
    result
}

struct OpenRecordingSegment {
    part_path: PathBuf,
    final_path: PathBuf,
    file: Option<File>,
    data_bytes: u64,
}

impl OpenRecordingSegment {
    fn create(directory: &Path, stem: &str, index: usize) -> io::Result<Self> {
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
        let _ = fs::remove_file(&self.part_path);
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
    use super::{AudioRecordingError, AudioRecordingWorker};
    use crate::{audio_capture_stream, prepare_audio_playback, start_audio_item_import};
    use aaadaw_core::{DawAction, Project};
    use aaadaw_media::AudioStreamDecoder;
    use aaadaw_storage::ProjectStore;
    use std::fs;
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
