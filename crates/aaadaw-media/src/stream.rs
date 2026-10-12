use crate::{AudioStreamDecoder, DecodedAudioChunk, DecodedAudioSource, MediaError};
use aaadaw_core::AudioItem;
use aaadaw_engine::{AudioStreamPosition, PcmStreamProducer, StereoPcmStreamProducer};
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_UPSAMPLE_RATIO: f64 = 64.0;
const SINC_TAPS: usize = 64;
const SINC_LEFT_TAPS: i32 = 31;
const SINC_RIGHT_TAPS: i32 = (SINC_TAPS as i32) - SINC_LEFT_TAPS - 1;
const SINC_PHASES: usize = 1024;
type WorkerStartupResult = Result<(), (Option<io::ErrorKind>, String)>;

/// A cancellable decoder/resampler worker feeding one mono engine stream.
pub struct AudioFeedWorker {
    cancelled: Arc<AtomicBool>,
    position: AudioStreamPosition,
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
        AudioStreamPosition::new(0),
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
        AudioStreamPosition::new(item.start_sample()),
        producer,
    )
}

/// Feeds a timeline item while preserving a mono center or stereo left/right image.
pub fn spawn_stereo_audio_item_stream(
    item: &AudioItem,
    resolved_path: impl AsRef<Path>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError> {
    if item.length_samples() == 0 {
        return Err(MediaError::InvalidAudioItemLength);
    }
    spawn_stereo_stream(
        DecoderInput::Path(resolved_path.as_ref().to_owned()),
        output_sample_rate,
        item.source_offset_samples(),
        Some(item.length_samples()),
        0,
        AudioStreamPosition::new(item.start_sample()),
        producer,
    )
}

/// Re-decodes a file-backed item and discards output before the requested timeline sample.
pub fn spawn_audio_item_stream_at(
    item: &AudioItem,
    timeline_sample: u64,
    resolved_path: impl AsRef<Path>,
    output_sample_rate: u32,
    producer: PcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError> {
    let samples_to_skip = checked_item_seek(item, timeline_sample)?;
    spawn_stream(
        DecoderInput::Path(resolved_path.as_ref().to_owned()),
        output_sample_rate,
        item.source_offset_samples(),
        Some(item.length_samples() - samples_to_skip),
        samples_to_skip,
        AudioStreamPosition::new(timeline_sample),
        producer,
    )
}

/// Re-decodes a stereo-capable item and discards output before the requested timeline sample.
pub fn spawn_stereo_audio_item_stream_at(
    item: &AudioItem,
    timeline_sample: u64,
    resolved_path: impl AsRef<Path>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError> {
    let samples_to_skip = checked_item_seek(item, timeline_sample)?;
    spawn_stereo_stream(
        DecoderInput::Path(resolved_path.as_ref().to_owned()),
        output_sample_rate,
        item.source_offset_samples(),
        Some(item.length_samples() - samples_to_skip),
        samples_to_skip,
        AudioStreamPosition::new(timeline_sample),
        producer,
    )
}

/// Feeds a cached immutable source while applying the item's trim and seek.
pub fn spawn_cached_stereo_audio_item_stream(
    item: &AudioItem,
    timeline_sample: u64,
    source: Arc<DecodedAudioSource>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError> {
    if item.length_samples() == 0 {
        return Err(MediaError::InvalidAudioItemLength);
    }
    let samples_to_skip = if timeline_sample > item.start_sample() {
        checked_item_seek(item, timeline_sample)?
    } else {
        0
    };
    spawn_stereo_stream(
        DecoderInput::Cached(source),
        output_sample_rate,
        item.source_offset_samples(),
        Some(item.length_samples() - samples_to_skip),
        samples_to_skip,
        AudioStreamPosition::new(item.start_sample() + samples_to_skip),
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
        AudioStreamPosition::new(item.start_sample()),
        producer,
    )
}

/// Decodes a seekable reader into a stereo-capable AudioItem stream.
pub fn spawn_stereo_audio_item_stream_from_reader<R>(
    item: &AudioItem,
    reader: R,
    byte_len: Option<u64>,
    extension_hint: Option<&str>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError>
where
    R: Read + Seek + Send + Sync + 'static,
{
    if item.length_samples() == 0 {
        return Err(MediaError::InvalidAudioItemLength);
    }
    spawn_stereo_stream(
        DecoderInput::Reader {
            reader: Box::new(reader),
            byte_len,
            extension: extension_hint.map(str::to_owned),
        },
        output_sample_rate,
        item.source_offset_samples(),
        Some(item.length_samples()),
        0,
        AudioStreamPosition::new(item.start_sample()),
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
    let samples_to_skip = checked_item_seek(item, timeline_sample)?;
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
        AudioStreamPosition::new(timeline_sample),
        producer,
    )
}

/// Re-decodes a seekable reader and discards stereo frames before the requested sample.
pub fn spawn_stereo_audio_item_stream_from_reader_at<R>(
    item: &AudioItem,
    timeline_sample: u64,
    reader: R,
    byte_len: Option<u64>,
    extension_hint: Option<&str>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError>
where
    R: Read + Seek + Send + Sync + 'static,
{
    let samples_to_skip = checked_item_seek(item, timeline_sample)?;
    spawn_stereo_stream(
        DecoderInput::Reader {
            reader: Box::new(reader),
            byte_len,
            extension: extension_hint.map(str::to_owned),
        },
        output_sample_rate,
        item.source_offset_samples(),
        Some(item.length_samples() - samples_to_skip),
        samples_to_skip,
        AudioStreamPosition::new(timeline_sample),
        producer,
    )
}

fn spawn_stream(
    input: DecoderInput,
    output_sample_rate: u32,
    source_offset_samples: u64,
    output_length_samples: Option<u64>,
    output_samples_to_skip: u64,
    position: AudioStreamPosition,
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
    let worker_position = position.clone();
    let thread = thread::Builder::new()
        .name("aaadaw-media-decode".to_owned())
        .spawn(move || {
            run_worker(
                input,
                config,
                producer,
                worker_cancelled,
                startup_sender,
                worker_position,
            )
        })
        .map_err(MediaError::ThreadSpawn)?;

    Ok(AudioFeedWorker {
        cancelled,
        position,
        startup: Some(startup),
        thread: Some(thread),
    })
}

fn spawn_stereo_stream(
    input: DecoderInput,
    output_sample_rate: u32,
    source_offset_samples: u64,
    output_length_samples: Option<u64>,
    output_samples_to_skip: u64,
    position: AudioStreamPosition,
    producer: StereoPcmStreamProducer,
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
    let worker_position = position.clone();
    let thread = thread::Builder::new()
        .name("aaadaw-stereo-media-decode".to_owned())
        .spawn(move || {
            run_worker(
                input,
                config,
                producer,
                worker_cancelled,
                startup_sender,
                worker_position,
            )
        })
        .map_err(MediaError::ThreadSpawn)?;
    Ok(AudioFeedWorker {
        cancelled,
        position,
        startup: Some(startup),
        thread: Some(thread),
    })
}

impl AudioFeedWorker {
    /// Returns the callback/worker sample position shared by this audio item stream.
    pub fn timeline_position(&self) -> AudioStreamPosition {
        self.position.clone()
    }

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

    /// Returns whether decoding and queue feeding have completed.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
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

fn checked_item_seek(item: &AudioItem, timeline_sample: u64) -> Result<u64, MediaError> {
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
    Ok(samples_to_skip)
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
    Cached(Arc<DecodedAudioSource>),
}

enum WorkerDecoder {
    Streaming(AudioStreamDecoder),
    Cached {
        source: Arc<DecodedAudioSource>,
        cursor_frames: usize,
    },
}

impl WorkerDecoder {
    fn open(input: DecoderInput) -> Result<(Self, usize), MediaError> {
        match input {
            DecoderInput::Path(path) => {
                let decoder = AudioStreamDecoder::open(path)?;
                let channels = decoder.metadata().channel_count.unwrap_or(0) as usize;
                Ok((Self::Streaming(decoder), channels))
            }
            DecoderInput::Reader {
                reader,
                byte_len,
                extension,
            } => {
                let decoder =
                    AudioStreamDecoder::from_reader(reader, byte_len, extension.as_deref())?;
                let channels = decoder.metadata().channel_count.unwrap_or(0) as usize;
                Ok((Self::Streaming(decoder), channels))
            }
            DecoderInput::Cached(source) => {
                let channels = source.channels();
                Ok((
                    Self::Cached {
                        source,
                        cursor_frames: 0,
                    },
                    channels,
                ))
            }
        }
    }

    fn next_chunk(&mut self) -> Result<Option<DecodedAudioChunk>, MediaError> {
        match self {
            Self::Streaming(decoder) => decoder.next_chunk(),
            Self::Cached {
                source,
                cursor_frames,
            } => Ok(source.chunk_at(cursor_frames)),
        }
    }
}

struct WorkerConfig {
    output_sample_rate: u32,
    source_offset_remaining: u64,
    output_samples_remaining: Option<u64>,
    output_samples_to_skip: u64,
}

trait FeedFrame: Copy + Send + 'static {
    fn copy_from_chunk(chunk: &DecodedAudioChunk, output: &mut Vec<Self>);
    fn zero() -> Self;
    fn add_scaled(&mut self, frame: Self, scale: f32);
}

impl FeedFrame for f32 {
    fn copy_from_chunk(chunk: &DecodedAudioChunk, output: &mut Vec<Self>) {
        chunk.copy_mono_downmix(output);
    }

    fn zero() -> Self {
        0.0
    }

    fn add_scaled(&mut self, frame: Self, scale: f32) {
        *self += frame * scale;
    }
}

impl FeedFrame for [f32; 2] {
    fn copy_from_chunk(chunk: &DecodedAudioChunk, output: &mut Vec<Self>) {
        chunk.copy_stereo(output);
    }

    fn zero() -> Self {
        [0.0, 0.0]
    }

    fn add_scaled(&mut self, frame: Self, scale: f32) {
        self[0] += frame[0] * scale;
        self[1] += frame[1] * scale;
    }
}

trait FeedProducer: Send + 'static {
    type Frame: FeedFrame;

    fn set_source_channels(&self, channels: usize);
    fn push_frames_at(&mut self, start_sample: u64, frames: &[Self::Frame]) -> usize;
}

impl FeedProducer for PcmStreamProducer {
    type Frame = f32;

    fn set_source_channels(&self, _channels: usize) {}

    fn push_frames_at(&mut self, start_sample: u64, frames: &[Self::Frame]) -> usize {
        PcmStreamProducer::push_samples_at(self, start_sample, frames)
    }
}

impl FeedProducer for StereoPcmStreamProducer {
    type Frame = [f32; 2];

    fn set_source_channels(&self, channels: usize) {
        self.set_stereo_content(channels == 2);
    }

    fn push_frames_at(&mut self, start_sample: u64, frames: &[Self::Frame]) -> usize {
        StereoPcmStreamProducer::push_frames_at(self, start_sample, frames)
    }
}

fn run_worker<P: FeedProducer>(
    input: DecoderInput,
    config: WorkerConfig,
    mut producer: P,
    cancelled: Arc<AtomicBool>,
    startup: SyncSender<WorkerStartupResult>,
    position: AudioStreamPosition,
) -> Result<(), MediaError> {
    let WorkerConfig {
        output_sample_rate,
        mut source_offset_remaining,
        mut output_samples_remaining,
        mut output_samples_to_skip,
    } = config;
    let mut output_timeline_sample = position.requested_sample();
    let decoder_result = WorkerDecoder::open(input);
    let mut decoder = match decoder_result {
        Ok((decoder, channels)) => {
            producer.set_source_channels(channels);
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
    let mut resampler: Option<StreamingResampler<P::Frame>> = None;
    let mut decoded_frames = Vec::new();
    let mut source_channels = None;

    loop {
        if cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        match decoder.next_chunk()? {
            Some(chunk) => {
                if source_channels.is_some_and(|channels| channels != chunk.channels()) {
                    return Err(MediaError::InvalidDecodedAudioSpec);
                }
                source_channels = Some(chunk.channels());
                producer.set_source_channels(chunk.channels());
                let current = match &mut resampler {
                    Some(resampler) if resampler.input_sample_rate() != chunk.sample_rate() => {
                        return Err(MediaError::ChangedAudioSampleRate);
                    }
                    Some(resampler) => resampler,
                    None => {
                        resampler = Some(StreamingResampler::<P::Frame>::new(
                            chunk.sample_rate(),
                            output_sample_rate,
                        )?);
                        resampler.as_mut().expect("resampler was just initialized")
                    }
                };
                P::Frame::copy_from_chunk(&chunk, &mut decoded_frames);
                let skipped = source_offset_remaining.min(decoded_frames.len() as u64) as usize;
                source_offset_remaining -= skipped as u64;
                let mut output = current.push(&decoded_frames[skipped..])?;
                skip_output(&mut output, &mut output_samples_to_skip);
                limit_output(&mut output, &mut output_samples_remaining);
                push_with_backpressure(
                    &mut producer,
                    &output,
                    &cancelled,
                    &position,
                    &mut output_timeline_sample,
                );
                if output_samples_remaining == Some(0) {
                    return Ok(());
                }
            }
            None => {
                if let Some(resampler) = &mut resampler {
                    let mut output = resampler.finish();
                    skip_output(&mut output, &mut output_samples_to_skip);
                    limit_output(&mut output, &mut output_samples_remaining);
                    push_with_backpressure(
                        &mut producer,
                        &output,
                        &cancelled,
                        &position,
                        &mut output_timeline_sample,
                    );
                }
                return Ok(());
            }
        }
    }
}

fn skip_output<T: Copy>(samples: &mut Vec<T>, remaining: &mut u64) {
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

fn limit_output<T: Copy>(samples: &mut Vec<T>, remaining: &mut Option<u64>) {
    if let Some(remaining) = remaining {
        let allowed = usize::try_from(*remaining)
            .unwrap_or(usize::MAX)
            .min(samples.len());
        samples.truncate(allowed);
        *remaining -= allowed as u64;
    }
}

fn push_with_backpressure<P: FeedProducer>(
    producer: &mut P,
    samples: &[P::Frame],
    cancelled: &AtomicBool,
    position: &AudioStreamPosition,
    timeline_sample: &mut u64,
) {
    let mut offset = 0;
    while offset < samples.len() && !cancelled.load(Ordering::Acquire) {
        let requested_sample = position.requested_sample();
        let behind = requested_sample.saturating_sub(*timeline_sample);
        let skip = usize::try_from(behind)
            .unwrap_or(usize::MAX)
            .min(samples.len() - offset);
        offset += skip;
        *timeline_sample = timeline_sample.saturating_add(skip as u64);
        if offset == samples.len() {
            break;
        }
        let pushed = producer.push_frames_at(*timeline_sample, &samples[offset..]);
        offset += pushed;
        *timeline_sample = timeline_sample.saturating_add(pushed as u64);
        if offset < samples.len() {
            thread::sleep(Duration::from_millis(1));
        }
    }
}

struct StreamingResampler<F> {
    input_sample_rate: u32,
    output_sample_rate: u32,
    coefficients: Vec<[f32; SINC_TAPS]>,
    next_position_numerator: u128,
    input_frames: u64,
    pending_start_frame: u64,
    pending_frames: Vec<F>,
}

impl<F: FeedFrame> StreamingResampler<F> {
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
        let coefficients = if input_sample_rate == output_sample_rate {
            Vec::new()
        } else {
            build_sinc_coefficients(input_sample_rate, output_sample_rate)
        };
        Ok(Self {
            input_sample_rate,
            output_sample_rate,
            coefficients,
            next_position_numerator: 0,
            input_frames: 0,
            pending_start_frame: 0,
            pending_frames: Vec::new(),
        })
    }

    fn input_sample_rate(&self) -> u32 {
        self.input_sample_rate
    }

    fn push(&mut self, input: &[F]) -> Result<Vec<F>, MediaError> {
        self.input_frames = self
            .input_frames
            .checked_add(input.len() as u64)
            .ok_or(MediaError::AudioTooLong)?;
        self.pending_frames.extend_from_slice(input);
        Ok(self.drain(false))
    }

    fn finish(&mut self) -> Vec<F> {
        self.drain(true)
    }

    fn drain(&mut self, at_eof: bool) -> Vec<F> {
        let mut output = Vec::new();
        if self.input_frames == 0 {
            return output;
        }
        let denominator = self.output_sample_rate as u128;
        let input_end = self.input_frames as u128 * denominator;

        while self.next_position_numerator < input_end {
            let center_frame = (self.next_position_numerator / denominator) as u64;
            if self.input_sample_rate == self.output_sample_rate {
                let sample =
                    self.pending_frames[(center_frame - self.pending_start_frame) as usize];
                output.push(sample);
                self.next_position_numerator += self.input_sample_rate as u128;
                continue;
            }
            let last_required_frame = center_frame.saturating_add(SINC_RIGHT_TAPS as u64);
            if !at_eof && last_required_frame >= self.input_frames {
                break;
            }

            let phase_numerator = (self.next_position_numerator % denominator) as u64;
            let phase_position = phase_numerator * SINC_PHASES as u64;
            let phase = (phase_position / self.output_sample_rate as u64) as usize;
            let phase_remainder = phase_position % self.output_sample_rate as u64;
            let phase_fraction = phase_remainder as f32 / self.output_sample_rate as f32;
            let first_coefficients = &self.coefficients[phase];
            let second_coefficients = &self.coefficients[phase + 1];
            let mut frame = F::zero();
            for tap in 0..SINC_TAPS {
                let offset = tap as i32 - SINC_LEFT_TAPS;
                let source_frame = if offset < 0 {
                    center_frame.saturating_sub((-offset) as u64)
                } else {
                    center_frame.saturating_add(offset as u64)
                }
                .min(self.input_frames - 1);
                let sample =
                    self.pending_frames[(source_frame - self.pending_start_frame) as usize];
                let coefficient = first_coefficients[tap]
                    + (second_coefficients[tap] - first_coefficients[tap]) * phase_fraction;
                frame.add_scaled(sample, coefficient);
            }
            output.push(frame);
            self.next_position_numerator += self.input_sample_rate as u128;
        }

        let next_center = (self.next_position_numerator / denominator) as u64;
        let discard_until = if self.input_sample_rate == self.output_sample_rate {
            next_center.min(self.input_frames)
        } else {
            next_center
                .saturating_sub(SINC_LEFT_TAPS as u64)
                .min(self.input_frames)
        };
        let discard_count = (discard_until - self.pending_start_frame) as usize;
        if discard_count > 0 {
            self.pending_frames.drain(..discard_count);
            self.pending_start_frame = discard_until;
        }
        output
    }
}

fn build_sinc_coefficients(
    input_sample_rate: u32,
    output_sample_rate: u32,
) -> Vec<[f32; SINC_TAPS]> {
    let cutoff = (output_sample_rate as f64 / input_sample_rate as f64).min(1.0);
    (0..=SINC_PHASES)
        .map(|phase| {
            let fraction = phase as f64 / SINC_PHASES as f64;
            let mut coefficients = [0.0; SINC_TAPS];
            let mut sum = 0.0;
            for (tap, coefficient) in coefficients.iter_mut().enumerate() {
                let distance = tap as f64 - SINC_LEFT_TAPS as f64 - fraction;
                let normalized_distance = distance / (SINC_TAPS as f64 / 2.0);
                if normalized_distance.abs() >= 1.0 {
                    continue;
                }
                let window = 0.42
                    + 0.5 * (std::f64::consts::PI * normalized_distance).cos()
                    + 0.08 * (2.0 * std::f64::consts::PI * normalized_distance).cos();
                let argument = std::f64::consts::PI * cutoff * distance;
                let sinc = if argument.abs() < f64::EPSILON {
                    1.0
                } else {
                    argument.sin() / argument
                };
                let value = cutoff * sinc * window;
                *coefficient = value as f32;
                sum += value;
            }
            if sum != 0.0 {
                for coefficient in &mut coefficients {
                    *coefficient = (*coefficient as f64 / sum) as f32;
                }
            }
            coefficients
        })
        .collect()
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

    fn copy_stereo(&self, output: &mut Vec<[f32; 2]>) {
        output.clear();
        output.reserve(self.frame_count());
        match self.channels() {
            1 => output.extend(self.samples().iter().map(|sample| [*sample, *sample])),
            2 => output.extend(
                self.samples()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|frame| [frame[0], frame[1]]),
            ),
            channels => {
                for frame in self.samples().chunks_exact(channels) {
                    let mono = frame.iter().copied().sum::<f32>() / channels as f32;
                    output.push([mono, mono]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StreamingResampler;

    #[test]
    fn streaming_resampler_preserves_constant_level_and_output_duration() {
        let mut resampler = StreamingResampler::<f32>::new(24_000, 48_000)
            .expect("valid sample rates should create a resampler");
        let mut output = resampler.push(&[0.25; 200]).expect("input should process");
        output.extend(resampler.finish());

        assert_eq!(output.len(), 400);
        assert!(output.iter().all(|sample| (*sample - 0.25).abs() < 1e-5));
    }

    #[test]
    fn streaming_resampler_is_independent_of_packet_boundaries() {
        let input = (0..1021)
            .map(|index| (index as f32 * 0.037).sin())
            .collect::<Vec<_>>();
        let mut whole = StreamingResampler::<f32>::new(48_000, 44_100)
            .expect("valid sample rates should create a resampler");
        let mut whole_output = whole.push(&input).expect("input should process");
        whole_output.extend(whole.finish());

        let mut chunked = StreamingResampler::<f32>::new(48_000, 44_100)
            .expect("valid sample rates should create a resampler");
        let mut chunked_output = Vec::new();
        for chunk in input.chunks(37) {
            chunked_output.extend(chunked.push(chunk).expect("packet should process"));
        }
        chunked_output.extend(chunked.finish());

        assert_eq!(chunked_output.len(), whole_output.len());
        assert!(
            chunked_output
                .iter()
                .zip(&whole_output)
                .all(|(chunked, whole)| (chunked - whole).abs() < 1e-6)
        );
    }

    #[test]
    fn sinc_resampler_suppresses_aliasing_when_downsampling() {
        fn resample_tone(frequency: f32) -> Vec<f32> {
            let input_rate = 48_000.0;
            let input = (0..48_000)
                .map(|index| {
                    (2.0 * std::f32::consts::PI * frequency * index as f32 / input_rate).sin()
                })
                .collect::<Vec<_>>();
            let mut resampler = StreamingResampler::<f32>::new(48_000, 24_000)
                .expect("valid sample rates should create a resampler");
            let mut output = resampler.push(&input).expect("input should process");
            output.extend(resampler.finish());
            output
        }

        fn measured_amplitude(samples: &[f32], frequency: f32, sample_rate: f32) -> f32 {
            let margin = 128;
            let stable = &samples[margin..samples.len() - margin];
            let projection = stable
                .iter()
                .enumerate()
                .map(|(index, sample)| {
                    let absolute_index = index + margin;
                    sample
                        * (2.0 * std::f32::consts::PI * frequency * absolute_index as f32
                            / sample_rate)
                            .sin()
                })
                .sum::<f32>();
            2.0 * projection.abs() / stable.len() as f32
        }

        let passband = measured_amplitude(&resample_tone(3_000.0), 3_000.0, 24_000.0);
        let aliased = measured_amplitude(&resample_tone(18_000.0), 6_000.0, 24_000.0);

        assert!(passband > 0.97, "passband amplitude was {passband}");
        assert!(aliased < 0.01, "aliased amplitude was {aliased}");
    }

    #[test]
    fn stereo_resampler_preserves_channels_across_packet_boundaries() {
        let input = (0..129)
            .map(|index| {
                let sample = (index as f32 * 0.091).sin();
                [sample, -sample]
            })
            .collect::<Vec<_>>();
        let mut resampler = StreamingResampler::<[f32; 2]>::new(24_000, 48_000)
            .expect("valid rates should create a stereo resampler");
        let mut output = Vec::new();
        for chunk in input.chunks(13) {
            output.extend(resampler.push(chunk).expect("packet should process"));
        }
        output.extend(resampler.finish());

        assert_eq!(output.len(), input.len() * 2);
        assert!(
            output
                .iter()
                .all(|frame| (frame[0] + frame[1]).abs() < 1e-6)
        );
    }
}
