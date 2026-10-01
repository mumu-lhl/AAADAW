use crate::{AudioStreamDecoder, DecodedAudioChunk, MediaError};
use aaadaw_core::AudioItem;
use aaadaw_engine::PcmStreamProducer;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_UPSAMPLE_RATIO: f64 = 64.0;
type WorkerStartupResult = Result<(), (Option<io::ErrorKind>, String)>;

/// A cancellable decoder/resampler worker feeding one mono engine stream.
pub struct AudioFeedWorker {
    cancelled: Arc<AtomicBool>,
    startup: Option<Receiver<WorkerStartupResult>>,
    thread: Option<JoinHandle<Result<(), MediaError>>>,
}

/// Decodes an audio file off-thread, downmixes it to mono, resamples to the
/// project/device rate, then feeds the bounded engine queue with backpressure.
///
/// The producer is owned by the worker. Call [`AudioFeedWorker::cancel`] or
/// drop the worker to stop it; call [`AudioFeedWorker::join`] to wait for EOF.
/// Joining without consuming the stream can block once its queue fills.
pub fn spawn_mono_stream(
    path: impl AsRef<Path>,
    output_sample_rate: u32,
    producer: PcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError> {
    spawn_stream(
        DecoderInput::Path(path.as_ref().to_owned()),
        output_sample_rate,
        0,
        None,
        0,
        producer,
    )
}

/// Feeds a timeline item from a resolved media file, applying its source trim
/// and limiting output to its project-sample duration.
pub fn spawn_audio_item_stream(
    item: &AudioItem,
    resolved_path: impl AsRef<Path>,
    output_sample_rate: u32,
    producer: PcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError> {
    if item.length_samples() == 0 {
        return Err(MediaError::InvalidAudioItemLength);
    }
    spawn_stream(
        DecoderInput::Path(resolved_path.as_ref().to_owned()),
        output_sample_rate,
        item.source_offset_samples(),
        Some(item.length_samples()),
        0,
        producer,
    )
}

/// Decodes an embedded or otherwise seekable media source into an AudioItem stream.
///
/// The reader is moved to the background worker; `byte_len` and `extension_hint`
/// help the container probe without requiring the source to be a filesystem path.
pub fn spawn_audio_item_stream_from_reader<R>(
    item: &AudioItem,
    reader: R,
    byte_len: Option<u64>,
    extension_hint: Option<&str>,
    output_sample_rate: u32,
    producer: PcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError>
where
    R: Read + Seek + Send + Sync + 'static,
{
    if item.length_samples() == 0 {
        return Err(MediaError::InvalidAudioItemLength);
    }
    spawn_stream(
        DecoderInput::Reader {
            reader: Box::new(reader),
            byte_len,
            extension: extension_hint.map(str::to_owned),
        },
        output_sample_rate,
        item.source_offset_samples(),
        Some(item.length_samples()),
        0,
        producer,
    )
}

/// Re-decodes an item and discards PCM before the requested timeline sample.
pub fn spawn_audio_item_stream_from_reader_at<R>(
    item: &AudioItem,
    timeline_sample: u64,
    reader: R,
    byte_len: Option<u64>,
    extension_hint: Option<&str>,
    output_sample_rate: u32,
    producer: PcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError>
where
    R: Read + Seek + Send + Sync + 'static,
{
    let samples_to_skip = timeline_sample.checked_sub(item.start_sample()).ok_or(
        MediaError::InvalidAudioItemSeek {
            requested: timeline_sample,
            start_sample: item.start_sample(),
            end_sample: item.end_sample(),
        },
    )?;
    if samples_to_skip >= item.length_samples() {
        return Err(MediaError::InvalidAudioItemSeek {
            requested: timeline_sample,
            start_sample: item.start_sample(),
            end_sample: item.end_sample(),
        });
    }
    let remaining_samples = item.length_samples() - samples_to_skip;
    spawn_stream(
        DecoderInput::Reader {
            reader: Box::new(reader),
            byte_len,
            extension: extension_hint.map(str::to_owned),
        },
        output_sample_rate,
        item.source_offset_samples(),
        Some(remaining_samples),
        samples_to_skip,
        producer,
    )
}

fn spawn_stream(
    input: DecoderInput,
    output_sample_rate: u32,
    source_offset_samples: u64,
    output_length_samples: Option<u64>,
    output_samples_to_skip: u64,
    producer: PcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError> {
    if output_sample_rate == 0 {
        return Err(MediaError::InvalidOutputSampleRate);
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let (startup_sender, startup) = mpsc::sync_channel(1);
    let config = WorkerConfig {
        output_sample_rate,
        source_offset_remaining: source_offset_samples,
        output_samples_remaining: output_length_samples,
        output_samples_to_skip,
    };
    let thread = thread::Builder::new()
        .name("aaadaw-media-decode".to_owned())
        .spawn(move || run_worker(input, config, producer, worker_cancelled, startup_sender))
        .map_err(MediaError::ThreadSpawn)?;

    Ok(AudioFeedWorker {
        cancelled,
        startup: Some(startup),
        thread: Some(thread),
    })
}

impl AudioFeedWorker {
    /// Waits until the worker opens and probes its source successfully.
    ///
    /// The source I/O remains on the worker thread; this caller only waits for its result.
    pub fn wait_ready(&mut self) -> Result<(), MediaError> {
        let startup = self
            .startup
            .take()
            .ok_or(MediaError::WorkerStartupAlreadyChecked)?;
        match startup.recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err((io_kind, message))) => {
                Err(MediaError::WorkerStartupFailed { message, io_kind })
            }
            Err(_) => Err(MediaError::WorkerPanicked),
        }
    }

    /// Waits for decoding to reach EOF and returns any worker error.
    /// The stream consumer must continue draining if the queue is bounded.
    pub fn join(mut self) -> Result<(), MediaError> {
        self.join_worker()
    }

    /// Requests cancellation and waits for the worker to exit.
    pub fn cancel(mut self) -> Result<(), MediaError> {
        self.cancelled.store(true, Ordering::Release);
        self.join_worker()
    }

    fn join_worker(&mut self) -> Result<(), MediaError> {
        let worker = self.thread.take().expect("worker thread is joined once");
        worker.join().map_err(|_| MediaError::WorkerPanicked)?
    }
}

impl Drop for AudioFeedWorker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

trait ReadSeek: Read + Seek {}

impl<T: Read + Seek> ReadSeek for T {}

enum DecoderInput {
    Path(PathBuf),
    Reader {
        reader: Box<dyn ReadSeek + Send + Sync>,
        byte_len: Option<u64>,
        extension: Option<String>,
    },
}

struct WorkerConfig {
    output_sample_rate: u32,
    source_offset_remaining: u64,
    output_samples_remaining: Option<u64>,
    output_samples_to_skip: u64,
}

fn run_worker(
    input: DecoderInput,
    config: WorkerConfig,
    mut producer: PcmStreamProducer,
    cancelled: Arc<AtomicBool>,
    startup: SyncSender<WorkerStartupResult>,
) -> Result<(), MediaError> {
    let WorkerConfig {
        output_sample_rate,
        mut source_offset_remaining,
        mut output_samples_remaining,
        mut output_samples_to_skip,
    } = config;
    let decoder_result = match input {
        DecoderInput::Path(path) => AudioStreamDecoder::open(path),
        DecoderInput::Reader {
            reader,
            byte_len,
            extension,
        } => AudioStreamDecoder::from_reader(reader, byte_len, extension.as_deref()),
    };
    let mut decoder = match decoder_result {
        Ok(decoder) => {
            let _ = startup.send(Ok(()));
            decoder
        }
        Err(error) => {
            let io_kind = match &error {
                MediaError::Io(error) => Some(error.kind()),
                _ => None,
            };
            let _ = startup.send(Err((io_kind, error.to_string())));
            return Err(error);
        }
    };
    let mut resampler: Option<StreamingMonoResampler> = None;
    let mut mono_input = Vec::new();

    loop {
        if cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        match decoder.next_chunk()? {
            Some(chunk) => {
                let current = match &mut resampler {
                    Some(resampler) if resampler.input_sample_rate() != chunk.sample_rate() => {
                        return Err(MediaError::ChangedAudioSampleRate);
                    }
                    Some(resampler) => resampler,
                    None => {
                        resampler = Some(StreamingMonoResampler::new(
                            chunk.sample_rate(),
                            output_sample_rate,
                        )?);
                        resampler.as_mut().expect("resampler was just initialized")
                    }
                };
                chunk.copy_mono_downmix(&mut mono_input);
                let skipped = source_offset_remaining.min(mono_input.len() as u64) as usize;
                source_offset_remaining -= skipped as u64;
                let mut output = current.push(&mono_input[skipped..])?;
                skip_output(&mut output, &mut output_samples_to_skip);
                limit_output(&mut output, &mut output_samples_remaining);
                push_with_backpressure(&mut producer, &output, &cancelled);
                if output_samples_remaining == Some(0) {
                    return Ok(());
                }
            }
            None => {
                if let Some(resampler) = &mut resampler {
                    let mut output = resampler.finish();
                    skip_output(&mut output, &mut output_samples_to_skip);
                    limit_output(&mut output, &mut output_samples_remaining);
                    push_with_backpressure(&mut producer, &output, &cancelled);
                }
                return Ok(());
            }
        }
    }
}

fn skip_output(samples: &mut Vec<f32>, remaining: &mut u64) {
    let skipped = usize::try_from(*remaining)
        .unwrap_or(usize::MAX)
        .min(samples.len());
    if skipped > 0 {
        let retained = samples.len() - skipped;
        samples.copy_within(skipped.., 0);
        samples.truncate(retained);
        *remaining -= skipped as u64;
    }
}

fn limit_output(samples: &mut Vec<f32>, remaining: &mut Option<u64>) {
    if let Some(remaining) = remaining {
        let allowed = usize::try_from(*remaining)
            .unwrap_or(usize::MAX)
            .min(samples.len());
        samples.truncate(allowed);
        *remaining -= allowed as u64;
    }
}

fn push_with_backpressure(
    producer: &mut PcmStreamProducer,
    samples: &[f32],
    cancelled: &AtomicBool,
) {
    let mut offset = 0;
    while offset < samples.len() && !cancelled.load(Ordering::Acquire) {
        offset += producer.push_samples(&samples[offset..]);
        if offset < samples.len() {
            thread::sleep(Duration::from_millis(1));
        }
    }
}

struct StreamingMonoResampler {
    input_sample_rate: u32,
    output_sample_rate: u32,
    next_position_numerator: u128,
    input_frames: u64,
    pending_start_frame: u64,
    pending_samples: Vec<f32>,
}

impl StreamingMonoResampler {
    fn new(input_sample_rate: u32, output_sample_rate: u32) -> Result<Self, MediaError> {
        if input_sample_rate == 0 {
            return Err(MediaError::InvalidDecodedAudioSpec);
        }
        if output_sample_rate == 0 {
            return Err(MediaError::InvalidOutputSampleRate);
        }
        if output_sample_rate as f64 / input_sample_rate as f64 > MAX_UPSAMPLE_RATIO {
            return Err(MediaError::ResampleRatioTooLarge {
                input: input_sample_rate,
                output: output_sample_rate,
            });
        }
        Ok(Self {
            input_sample_rate,
            output_sample_rate,
            next_position_numerator: 0,
            input_frames: 0,
            pending_start_frame: 0,
            pending_samples: Vec::new(),
        })
    }

    fn input_sample_rate(&self) -> u32 {
        self.input_sample_rate
    }

    fn push(&mut self, input: &[f32]) -> Result<Vec<f32>, MediaError> {
        self.input_frames = self
            .input_frames
            .checked_add(input.len() as u64)
            .ok_or(MediaError::AudioTooLong)?;
        self.pending_samples.extend_from_slice(input);
        Ok(self.drain(false))
    }

    fn finish(&mut self) -> Vec<f32> {
        self.drain(true)
    }

    fn drain(&mut self, at_eof: bool) -> Vec<f32> {
        let mut output = Vec::new();
        let denominator = self.output_sample_rate as u128;
        let input_end = self.input_frames as u128 * denominator;

        while self.next_position_numerator < input_end {
            let left_frame = (self.next_position_numerator / denominator) as u64;
            let right_frame = left_frame + 1;
            if !at_eof && right_frame >= self.input_frames {
                break;
            }

            let left_index = (left_frame - self.pending_start_frame) as usize;
            let left_sample = self.pending_samples[left_index];
            let right_sample = if right_frame < self.input_frames {
                self.pending_samples[(right_frame - self.pending_start_frame) as usize]
            } else {
                left_sample
            };
            let fraction = (self.next_position_numerator % denominator) as f32 / denominator as f32;
            output.push(left_sample + (right_sample - left_sample) * fraction);
            self.next_position_numerator += self.input_sample_rate as u128;
        }

        let discard_until =
            ((self.next_position_numerator / denominator) as u64).min(self.input_frames);
        let discard_count = (discard_until - self.pending_start_frame) as usize;
        if discard_count > 0 {
            self.pending_samples.drain(..discard_count);
            self.pending_start_frame = discard_until;
        }
        output
    }
}

impl DecodedAudioChunk {
    fn copy_mono_downmix(&self, output: &mut Vec<f32>) {
        output.clear();
        output.reserve(self.frame_count());
        for frame in self.samples().chunks_exact(self.channels()) {
            let sum = frame.iter().copied().sum::<f32>();
            output.push(sum / self.channels() as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StreamingMonoResampler;

    #[test]
    fn streaming_resampler_preserves_interpolation_across_chunk_boundaries() {
        let mut resampler = StreamingMonoResampler::new(24_000, 48_000)
            .expect("valid sample rates should create a resampler");
        let first = resampler.push(&[0.0]).expect("first packet should process");
        let second = resampler
            .push(&[1.0])
            .expect("second packet should process");
        let final_samples = resampler.finish();

        assert!(first.is_empty());
        assert_eq!(second, [0.0, 0.5]);
        assert_eq!(final_samples, [1.0, 1.0]);
    }

    #[test]
    fn streaming_resampler_downsamples_without_packet_local_phase_reset() {
        let mut resampler = StreamingMonoResampler::new(48_000, 24_000)
            .expect("valid sample rates should create a resampler");
        let first = resampler
            .push(&[0.0, 0.25])
            .expect("first packet should process");
        let second = resampler
            .push(&[0.5, 0.75])
            .expect("second packet should process");
        let final_samples = resampler.finish();

        assert_eq!(first, [0.0]);
        assert_eq!(second, [0.5]);
        assert_eq!(final_samples, []);
    }
}
