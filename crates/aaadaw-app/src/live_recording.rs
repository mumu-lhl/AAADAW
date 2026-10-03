//! Non-realtime recording-file ownership and finalization.
//!
//! The audio callback only writes fixed-size stereo frames to the bounded engine queue. This
//! worker drains that queue, writes PCM WAV data, and publishes the completed file on stop.

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

/// Background WAV writer for a single stereo take.
///
/// The writer owns the queue consumer. Call `finish` only after stopping and joining the input
/// backend, so no callback can publish frames after the worker drains the queue.
pub struct AudioRecordingWorker {
    command: Sender<Command>,
    thread: Option<JoinHandle<Result<PathBuf, AudioRecordingError>>>,
    control: AudioCaptureControl,
}

impl AudioRecordingWorker {
    /// Starts a dedicated writer and creates its temporary file next to the project.
    pub fn start(
        project_path: impl AsRef<Path>,
        sample_rate: u32,
        consumer: AudioCaptureConsumer,
        control: AudioCaptureControl,
    ) -> Result<Self, AudioRecordingError> {
        if sample_rate == 0 {
            return Err(AudioRecordingError::InvalidSampleRate);
        }
        let project_path = project_path.as_ref();
        let directory = project_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !directory.is_dir() {
            return Err(AudioRecordingError::ProjectDirectoryUnavailable(
                directory.to_owned(),
            ));
        }
        let (part_path, final_path) = loop {
            let id = NEXT_RECORDING_ID.fetch_add(1, Ordering::Relaxed);
            let stem = project_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("project");
            let base = format!(".{stem}-take-{}", id);
            let part = directory.join(format!("{base}.wav.part"));
            let final_path = directory.join(format!("{base}.wav"));
            if !part.exists() && !final_path.exists() {
                break (part, final_path);
            }
        };
        let (command, commands) = mpsc::channel();
        let worker_control = control.clone();
        let thread = thread::Builder::new()
            .name("aaadaw-recording-writer".to_owned())
            .spawn(move || {
                write_take(
                    part_path,
                    final_path,
                    sample_rate,
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
    pub fn finish(mut self) -> Result<PathBuf, AudioRecordingError> {
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
    part_path: PathBuf,
    final_path: PathBuf,
    sample_rate: u32,
    mut consumer: AudioCaptureConsumer,
    control: AudioCaptureControl,
    commands: Receiver<Command>,
) -> Result<PathBuf, AudioRecordingError> {
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part_path)?;
        file.write_all(&[0; WAV_HEADER_SIZE as usize])?;
        let mut frames = [[0.0_f32; 2]; DRAIN_FRAMES];
        let mut bytes = Vec::with_capacity(DRAIN_FRAMES * 6);
        let mut frame_count = 0_u64;
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
            let chunk_bytes = (count as u64) * 6;
            if frame_count.saturating_mul(6).saturating_add(chunk_bytes) > MAX_RIFF_DATA_BYTES {
                control.fail();
                return Err(AudioRecordingError::TakeTooLarge);
            }
            bytes.clear();
            for frame in &frames[..count] {
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
            file.write_all(&bytes)?;
            frame_count += count as u64;
        }
        if control.has_failed() {
            return Err(AudioRecordingError::CaptureFailed {
                overflow_frames: control.overflow_frames(),
            });
        }
        if frame_count == 0 {
            return Err(AudioRecordingError::EmptyTake);
        }
        let data_bytes =
            u32::try_from(frame_count * 6).map_err(|_| AudioRecordingError::TakeTooLarge)?;
        let riff_size = 36_u32
            .checked_add(data_bytes)
            .ok_or(AudioRecordingError::TakeTooLarge)?;
        file.seek(SeekFrom::Start(0))?;
        write_wav_header(&mut file, sample_rate, riff_size, data_bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&part_path, &final_path)?;
        Ok(final_path.clone())
    })();
    if result.is_err() {
        control.fail();
        let _ = fs::remove_file(part_path);
        let _ = fs::remove_file(final_path);
    }
    result
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
    use crate::audio_capture_stream;
    use aaadaw_media::AudioStreamDecoder;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

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

        let recording = worker.finish().expect("take should finalize");
        let mut decoder = AudioStreamDecoder::open(&recording).expect("WAV should decode");
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
