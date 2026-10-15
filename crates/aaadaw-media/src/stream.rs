use crate::{AudioStreamDecoder, DecodedAudioChunk, DecodedAudioSource, MediaError};
use aaadaw_core::AudioItem;
use aaadaw_engine::{
    AudioStreamPosition, PcmStreamProducer, StereoPcmStreamProducer, TransportLoopRange,
};
use std::collections::HashMap;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_UPSAMPLE_RATIO: f64 = 64.0;
const PLAYBACK_SINC_TAPS: usize = 64;
const RENDER_SINC_TAPS: usize = 192;
const SINC_PHASES: usize = 1024;
// Cache short sections to avoid decoder reopen work on every loop; the bound keeps workers small.
const MAX_CACHED_LOOP_SECTION_BYTES: usize = 1_048_576;
type WorkerStartupResult = Result<(), (Option<io::ErrorKind>, String)>;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResamplingQuality {
    /// 64-tap sinc filtering for project playback.
    Sinc64,
    /// 192-tap sinc filtering for offline project rendering and Freeze.
    Sinc192,
}

impl ResamplingQuality {
    fn taps(self) -> usize {
        match self {
            Self::Sinc64 => PLAYBACK_SINC_TAPS,
            Self::Sinc192 => RENDER_SINC_TAPS,
        }
    }
}

/// Stateful stereo sample-rate converter for control and offline-render threads.
///
/// This type allocates while processing and must not be called from the realtime audio callback.
pub struct StereoPcmResampler {
    inner: StreamingResampler<[f32; 2]>,
}

impl StereoPcmResampler {
    /// Creates a converter and prepares its filter table before processing begins.
    pub fn new(
        input_sample_rate: u32,
        output_sample_rate: u32,
        quality: ResamplingQuality,
    ) -> Result<Self, MediaError> {
        Ok(Self {
            inner: StreamingResampler::new_with_quality(
                input_sample_rate,
                output_sample_rate,
                quality,
            )?,
        })
    }

    /// Converts one interleaved stereo frame packet while retaining filter history.
    pub fn push_frames(&mut self, frames: &[[f32; 2]]) -> Result<Vec<[f32; 2]>, MediaError> {
        self.inner.push(frames)
    }

    /// Flushes the converter's remaining frames at end of input.
    pub fn finish(self) -> Vec<[f32; 2]> {
        let mut inner = self.inner;
        inner.finish()
    }
}

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
        None,
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
        source_loop_section(item),
    )
}

/// Feeds a timeline item while preserving a mono center or stereo left/right image.
pub fn spawn_stereo_audio_item_stream(
    item: &AudioItem,
    resolved_path: impl AsRef<Path>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError> {
    spawn_stereo_audio_item_stream_with_quality(
        item,
        resolved_path,
        output_sample_rate,
        producer,
        ResamplingQuality::Sinc64,
    )
}

/// Feeds a stereo item using the requested background resampling quality.
pub fn spawn_stereo_audio_item_stream_with_quality(
    item: &AudioItem,
    resolved_path: impl AsRef<Path>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
    quality: ResamplingQuality,
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
        quality,
        source_loop_section(item),
        None,
    )
}

/// Feeds a stereo item that exactly covers one project transport loop range.
///
/// This feeder runs until cancelled or dropped. It accepts a bounded loop-source section or a
/// normal trimmed item whose source offset plus its timeline distance to Loop start resolves to a
/// nonnegative source frame, regardless of that item's Loop source setting. The item timeline
/// must contain `timeline_loop_range`, the repeated source segment must have the same frame count,
/// and the decoded sample rate must match the output rate.
pub fn spawn_stereo_audio_item_stream_for_transport_loop(
    item: &AudioItem,
    timeline_loop_range: TransportLoopRange,
    resolved_path: impl AsRef<Path>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError> {
    spawn_stereo_audio_item_stream_for_transport_loop_at(
        item,
        timeline_loop_range,
        timeline_loop_range.start_sample(),
        resolved_path,
        output_sample_rate,
        producer,
    )
}

/// Feeds an exact-range stereo transport-loop item beginning at a sample inside the loop.
pub fn spawn_stereo_audio_item_stream_for_transport_loop_at(
    item: &AudioItem,
    timeline_loop_range: TransportLoopRange,
    timeline_sample: u64,
    resolved_path: impl AsRef<Path>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
) -> Result<AudioFeedWorker, MediaError> {
    let (samples_to_skip, source_section) =
        validate_transport_loop_item(item, timeline_loop_range, timeline_sample)?;
    spawn_stereo_stream(
        DecoderInput::Path(resolved_path.as_ref().to_owned()),
        output_sample_rate,
        0,
        None,
        samples_to_skip,
        AudioStreamPosition::new(timeline_sample),
        producer,
        ResamplingQuality::Sinc64,
        Some(source_section),
        Some(timeline_loop_range),
    )
}

/// Feeds a seekable embedded source for an exact stereo transport-loop item.
#[allow(clippy::too_many_arguments)]
pub fn spawn_stereo_audio_item_stream_from_reader_for_transport_loop<R>(
    item: &AudioItem,
    timeline_loop_range: TransportLoopRange,
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
    let (samples_to_skip, source_section) =
        validate_transport_loop_item(item, timeline_loop_range, timeline_sample)?;
    spawn_stereo_stream(
        DecoderInput::Reader {
            reader: SharedReadSeek::new(Box::new(reader)),
            byte_len,
            extension: extension_hint.map(str::to_owned),
        },
        output_sample_rate,
        0,
        None,
        samples_to_skip,
        AudioStreamPosition::new(timeline_sample),
        producer,
        ResamplingQuality::Sinc64,
        Some(source_section),
        Some(timeline_loop_range),
    )
}

fn validate_transport_loop_item(
    item: &AudioItem,
    timeline_loop_range: TransportLoopRange,
    timeline_sample: u64,
) -> Result<(u64, SourceLoopSection), MediaError> {
    let Some(item_end) = item.start_sample().checked_add(item.length_samples()) else {
        return Err(MediaError::InvalidTimelineLoop);
    };
    if item.length_samples() == 0
        || item.start_sample() > timeline_loop_range.start_sample()
        || item_end < timeline_loop_range.end_sample()
        || timeline_sample < timeline_loop_range.start_sample()
        || timeline_sample >= timeline_loop_range.end_sample()
    {
        return Err(MediaError::InvalidTimelineLoop);
    }
    let loop_offset_from_item_start = timeline_loop_range
        .start_sample()
        .checked_sub(item.start_sample())
        .ok_or(MediaError::InvalidTimelineLoop)?;
    let source_section = match (
        item.source_section_start_samples(),
        item.source_section_length_samples(),
    ) {
        (Some(_), Some(_)) => {
            if !item.is_loop_source()
                || item.source_offset_samples() != 0
                || loop_offset_from_item_start != 0
                || item_end != timeline_loop_range.end_sample()
            {
                return Err(MediaError::InvalidTimelineLoop);
            }
            source_loop_section(item)
                .filter(|section| {
                    section.looping
                        && section.length_frames == Some(timeline_loop_range.length_samples())
                })
                .ok_or(MediaError::InvalidTimelineLoop)?
        }
        (None, None) => {
            let loop_offset_from_item_start = i64::try_from(loop_offset_from_item_start)
                .map_err(|_| MediaError::InvalidTimelineLoop)?;
            let start_frame = item
                .source_offset_samples()
                .checked_add(loop_offset_from_item_start)
                .and_then(|offset| u64::try_from(offset).ok())
                .ok_or(MediaError::InvalidTimelineLoop)?;
            if start_frame
                .checked_add(timeline_loop_range.length_samples())
                .is_none()
            {
                return Err(MediaError::InvalidTimelineLoop);
            }
            SourceLoopSection {
                start_frame,
                length_frames: Some(timeline_loop_range.length_samples()),
                looping: true,
            }
        }
        _ => return Err(MediaError::InvalidTimelineLoop),
    };
    Ok((
        timeline_sample - timeline_loop_range.start_sample(),
        source_section,
    ))
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
        source_loop_section(item),
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
    spawn_stereo_audio_item_stream_at_with_quality(
        item,
        timeline_sample,
        resolved_path,
        output_sample_rate,
        producer,
        ResamplingQuality::Sinc64,
    )
}

/// Re-decodes a stereo item at the requested quality and timeline position.
pub fn spawn_stereo_audio_item_stream_at_with_quality(
    item: &AudioItem,
    timeline_sample: u64,
    resolved_path: impl AsRef<Path>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
    quality: ResamplingQuality,
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
        quality,
        source_loop_section(item),
        None,
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
    spawn_cached_stereo_audio_item_stream_with_quality(
        item,
        timeline_sample,
        source,
        output_sample_rate,
        producer,
        ResamplingQuality::Sinc64,
    )
}

/// Feeds a cached stereo source using the requested background resampling quality.
pub fn spawn_cached_stereo_audio_item_stream_with_quality(
    item: &AudioItem,
    timeline_sample: u64,
    source: Arc<DecodedAudioSource>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
    quality: ResamplingQuality,
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
        quality,
        source_loop_section(item),
        None,
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
            reader: SharedReadSeek::new(Box::new(reader)),
            byte_len,
            extension: extension_hint.map(str::to_owned),
        },
        output_sample_rate,
        item.source_offset_samples(),
        Some(item.length_samples()),
        0,
        AudioStreamPosition::new(item.start_sample()),
        producer,
        source_loop_section(item),
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
    spawn_stereo_audio_item_stream_from_reader_with_quality(
        item,
        reader,
        byte_len,
        extension_hint,
        output_sample_rate,
        producer,
        ResamplingQuality::Sinc64,
    )
}

/// Decodes a seekable reader into a stereo item stream at the requested quality.
pub fn spawn_stereo_audio_item_stream_from_reader_with_quality<R>(
    item: &AudioItem,
    reader: R,
    byte_len: Option<u64>,
    extension_hint: Option<&str>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
    quality: ResamplingQuality,
) -> Result<AudioFeedWorker, MediaError>
where
    R: Read + Seek + Send + Sync + 'static,
{
    if item.length_samples() == 0 {
        return Err(MediaError::InvalidAudioItemLength);
    }
    spawn_stereo_stream(
        DecoderInput::Reader {
            reader: SharedReadSeek::new(Box::new(reader)),
            byte_len,
            extension: extension_hint.map(str::to_owned),
        },
        output_sample_rate,
        item.source_offset_samples(),
        Some(item.length_samples()),
        0,
        AudioStreamPosition::new(item.start_sample()),
        producer,
        quality,
        source_loop_section(item),
        None,
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
            reader: SharedReadSeek::new(Box::new(reader)),
            byte_len,
            extension: extension_hint.map(str::to_owned),
        },
        output_sample_rate,
        item.source_offset_samples(),
        Some(remaining_samples),
        samples_to_skip,
        AudioStreamPosition::new(timeline_sample),
        producer,
        source_loop_section(item),
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
    spawn_stereo_audio_item_stream_from_reader_at_with_quality(
        item,
        timeline_sample,
        reader,
        byte_len,
        extension_hint,
        output_sample_rate,
        producer,
        ResamplingQuality::Sinc64,
    )
}

/// Re-decodes a seekable stereo source at the requested quality and timeline position.
#[allow(clippy::too_many_arguments)]
pub fn spawn_stereo_audio_item_stream_from_reader_at_with_quality<R>(
    item: &AudioItem,
    timeline_sample: u64,
    reader: R,
    byte_len: Option<u64>,
    extension_hint: Option<&str>,
    output_sample_rate: u32,
    producer: StereoPcmStreamProducer,
    quality: ResamplingQuality,
) -> Result<AudioFeedWorker, MediaError>
where
    R: Read + Seek + Send + Sync + 'static,
{
    let samples_to_skip = checked_item_seek(item, timeline_sample)?;
    spawn_stereo_stream(
        DecoderInput::Reader {
            reader: SharedReadSeek::new(Box::new(reader)),
            byte_len,
            extension: extension_hint.map(str::to_owned),
        },
        output_sample_rate,
        item.source_offset_samples(),
        Some(item.length_samples() - samples_to_skip),
        samples_to_skip,
        AudioStreamPosition::new(timeline_sample),
        producer,
        quality,
        source_loop_section(item),
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_stream(
    input: DecoderInput,
    output_sample_rate: u32,
    source_offset_samples: i64,
    output_length_samples: Option<u64>,
    output_samples_to_skip: u64,
    position: AudioStreamPosition,
    producer: PcmStreamProducer,
    source_loop_section: Option<SourceLoopSection>,
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
        quality: ResamplingQuality::Sinc64,
        source_loop_section,
        timeline_loop_range: None,
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

#[allow(clippy::too_many_arguments)]
fn spawn_stereo_stream(
    input: DecoderInput,
    output_sample_rate: u32,
    source_offset_samples: i64,
    output_length_samples: Option<u64>,
    output_samples_to_skip: u64,
    position: AudioStreamPosition,
    producer: StereoPcmStreamProducer,
    quality: ResamplingQuality,
    source_loop_section: Option<SourceLoopSection>,
    timeline_loop_range: Option<TransportLoopRange>,
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
        quality,
        source_loop_section,
        timeline_loop_range,
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

    /// Waits until the worker opens its source and prepares known-rate resampling state.
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

trait ReadSeek: Read + Seek + Send + Sync {}

impl<T: Read + Seek + Send + Sync> ReadSeek for T {}

#[derive(Clone)]
struct SharedReadSeek(Arc<Mutex<Box<dyn ReadSeek>>>);

impl SharedReadSeek {
    fn new(reader: Box<dyn ReadSeek>) -> Self {
        Self(Arc::new(Mutex::new(reader)))
    }

    fn rewind(&self) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("shared audio reader mutex was poisoned"))?
            .seek(io::SeekFrom::Start(0))
            .map(|_| ())
    }
}

impl Read for SharedReadSeek {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("shared audio reader mutex was poisoned"))?
            .read(output)
    }
}

impl Seek for SharedReadSeek {
    fn seek(&mut self, position: io::SeekFrom) -> io::Result<u64> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("shared audio reader mutex was poisoned"))?
            .seek(position)
    }
}

#[derive(Clone)]
enum DecoderInput {
    Path(PathBuf),
    Reader {
        reader: SharedReadSeek,
        byte_len: Option<u64>,
        extension: Option<String>,
    },
    Cached(Arc<DecodedAudioSource>),
}

enum WorkerDecoder {
    Streaming {
        decoder: Option<Box<AudioStreamDecoder>>,
        input: DecoderInput,
    },
    Cached {
        source: Arc<DecodedAudioSource>,
        cursor_frames: usize,
    },
}

impl WorkerDecoder {
    fn open(input: DecoderInput) -> Result<(Self, usize, Option<u32>), MediaError> {
        match input.clone() {
            DecoderInput::Path(path) => {
                let decoder = AudioStreamDecoder::open(path)?;
                let channels = decoder.metadata().channel_count.unwrap_or(0) as usize;
                let sample_rate = decoder.metadata().sample_rate;
                Ok((
                    Self::Streaming {
                        decoder: Some(Box::new(decoder)),
                        input,
                    },
                    channels,
                    sample_rate,
                ))
            }
            DecoderInput::Reader {
                reader,
                byte_len,
                extension,
            } => {
                let decoder =
                    AudioStreamDecoder::from_reader(reader, byte_len, extension.as_deref())?;
                let channels = decoder.metadata().channel_count.unwrap_or(0) as usize;
                let sample_rate = decoder.metadata().sample_rate;
                Ok((
                    Self::Streaming {
                        decoder: Some(Box::new(decoder)),
                        input,
                    },
                    channels,
                    sample_rate,
                ))
            }
            DecoderInput::Cached(source) => {
                let channels = source.channels();
                let sample_rate = source.sample_rate();
                Ok((
                    Self::Cached {
                        source,
                        cursor_frames: 0,
                    },
                    channels,
                    Some(sample_rate),
                ))
            }
        }
    }

    fn next_chunk(&mut self) -> Result<Option<DecodedAudioChunk>, MediaError> {
        match self {
            Self::Streaming { decoder, .. } => decoder
                .as_mut()
                .expect("streaming decoder is open between loop restarts")
                .next_chunk(),
            Self::Cached {
                source,
                cursor_frames,
            } => Ok(source.chunk_at(cursor_frames)),
        }
    }

    fn restart(&mut self) -> Result<(usize, Option<u32>), MediaError> {
        let input = match self {
            Self::Cached {
                source,
                cursor_frames,
            } => {
                *cursor_frames = 0;
                return Ok((source.channels(), Some(source.sample_rate())));
            }
            Self::Streaming { decoder, input } => {
                decoder.take();
                input.clone()
            }
        };
        if let DecoderInput::Reader { reader, .. } = &input {
            reader.rewind().map_err(MediaError::Io)?;
        }
        let (decoder, channels, sample_rate) = Self::open(input)?;
        *self = decoder;
        Ok((channels, sample_rate))
    }
}

struct WorkerConfig {
    output_sample_rate: u32,
    source_offset_remaining: i64,
    output_samples_remaining: Option<u64>,
    output_samples_to_skip: u64,
    quality: ResamplingQuality,
    source_loop_section: Option<SourceLoopSection>,
    timeline_loop_range: Option<TransportLoopRange>,
}

#[derive(Clone, Copy, Debug)]
struct SourceLoopSection {
    start_frame: u64,
    length_frames: Option<u64>,
    looping: bool,
}

fn source_loop_section(item: &AudioItem) -> Option<SourceLoopSection> {
    match (
        item.source_section_start_samples(),
        item.source_section_length_samples(),
    ) {
        (Some(start_frame), Some(length_frames)) => Some(SourceLoopSection {
            start_frame,
            length_frames: Some(length_frames),
            looping: item.is_loop_source(),
        }),
        (None, None) if item.is_loop_source() => Some(SourceLoopSection {
            start_frame: 0,
            length_frames: None,
            looping: true,
        }),
        (None, None) => None,
        _ => None,
    }
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
        source_offset_remaining,
        mut output_samples_remaining,
        mut output_samples_to_skip,
        quality,
        source_loop_section,
        timeline_loop_range,
    } = config;
    let (negative_source_offset_frames, mut source_offset_remaining) =
        if source_offset_remaining < 0 {
            (source_offset_remaining.unsigned_abs(), 0)
        } else {
            (
                0,
                u64::try_from(source_offset_remaining)
                    .expect("nonnegative signed source offset fits u64"),
            )
        };
    let (loop_base_source_offset, loop_section_length, mut loop_source_frames_remaining, looping) =
        match source_loop_section {
            Some(section) => {
                let section_end = if let Some(length) = section.length_frames {
                    let Some(end) = section.start_frame.checked_add(length) else {
                        let error = MediaError::InvalidAudioLoopSection;
                        let _ = startup.send(Err(worker_startup_error(&error)));
                        return Err(error);
                    };
                    if length == 0 {
                        let error = MediaError::InvalidAudioLoopSection;
                        let _ = startup.send(Err(worker_startup_error(&error)));
                        return Err(error);
                    }
                    Some(end)
                } else {
                    None
                };
                let remaining = if negative_source_offset_frames > 0 {
                    if section.looping || section.length_frames.is_none() {
                        let error = MediaError::InvalidAudioLoopSection;
                        let _ = startup.send(Err(worker_startup_error(&error)));
                        return Err(error);
                    }
                    // A disabled bounded section still starts at its section boundary after
                    // the signed take pre-roll. The section length remains a one-pass cap.
                    source_offset_remaining = section.start_frame;
                    section.length_frames
                } else if let (Some(length), Some(section_end)) =
                    (section.length_frames, section_end)
                {
                    if source_offset_remaining >= length {
                        if section.looping {
                            let error = MediaError::InvalidAudioLoopSection;
                            let _ = startup.send(Err(worker_startup_error(&error)));
                            return Err(error);
                        }
                        source_offset_remaining = section_end;
                        Some(0)
                    } else {
                        let Some(source_start) =
                            section.start_frame.checked_add(source_offset_remaining)
                        else {
                            let error = MediaError::InvalidAudioLoopSection;
                            let _ = startup.send(Err(worker_startup_error(&error)));
                            return Err(error);
                        };
                        source_offset_remaining = source_start;
                        Some(length - (source_start - section.start_frame))
                    }
                } else if section.length_frames.is_none() {
                    if negative_source_offset_frames > 0
                        || source_offset_remaining < section.start_frame
                    {
                        let error = MediaError::InvalidAudioLoopSection;
                        let _ = startup.send(Err(worker_startup_error(&error)));
                        return Err(error);
                    }
                    None
                } else {
                    unreachable!("bounded sections have a checked source end")
                };
                (
                    Some(section.start_frame),
                    section.length_frames,
                    remaining,
                    section.looping,
                )
            }
            None => (None, None, None, false),
        };
    let initial_loop_source_frames = loop_source_frames_remaining;
    let mut source_frames_in_current_cycle = 0_u64;
    let mut initial_loop_pass = true;
    let mut output_timeline_sample = position.requested_sample();
    let decoder_result = WorkerDecoder::open(input);
    let (mut decoder, initial_sample_rate) = match decoder_result {
        Ok((decoder, channels, sample_rate)) => {
            producer.set_source_channels(channels);
            (decoder, sample_rate)
        }
        Err(error) => {
            let _ = startup.send(Err(worker_startup_error(&error)));
            return Err(error);
        }
    };
    if let Some(timeline_loop_range) = timeline_loop_range
        && (initial_sample_rate != Some(output_sample_rate)
            || !source_loop_section.is_some_and(|section| {
                section.looping
                    && section.length_frames == Some(timeline_loop_range.length_samples())
            }))
    {
        let error = MediaError::InvalidTimelineLoop;
        let _ = startup.send(Err(worker_startup_error(&error)));
        return Err(error);
    }
    let mut resampler = match initial_sample_rate {
        Some(sample_rate) => match StreamingResampler::<P::Frame>::new_with_quality(
            sample_rate,
            output_sample_rate,
            quality,
        ) {
            Ok(resampler) => Some(resampler),
            Err(error) => {
                let _ = startup.send(Err(worker_startup_error(&error)));
                return Err(error);
            }
        },
        None => None,
    };
    let mut leading_silence_samples = 0_u64;
    if negative_source_offset_frames > 0 {
        let Some(source_sample_rate) = initial_sample_rate else {
            let error = MediaError::InvalidDecodedAudioSpec;
            let _ = startup.send(Err(worker_startup_error(&error)));
            return Err(error);
        };
        let Some(leading_silence_total) = scale_frames_rounded(
            negative_source_offset_frames,
            source_sample_rate,
            output_sample_rate,
        ) else {
            let error = MediaError::AudioTooLong;
            let _ = startup.send(Err(worker_startup_error(&error)));
            return Err(error);
        };
        leading_silence_samples = leading_silence_total.saturating_sub(output_samples_to_skip);
        output_samples_to_skip = output_samples_to_skip.saturating_sub(leading_silence_total);
    }
    if looping
        && let (Some(source_rate), Some(loop_frames), Some(initial_frames)) = (
            initial_sample_rate,
            loop_section_length,
            initial_loop_source_frames,
        )
    {
        let output_loop_frames = scale_frames_rounded(loop_frames, source_rate, output_sample_rate);
        let output_initial_frames =
            scale_frames_rounded(initial_frames, source_rate, output_sample_rate);
        if let (Some(output_loop_frames), Some(output_initial_frames)) =
            (output_loop_frames, output_initial_frames)
            && output_loop_frames > 0
            && output_samples_to_skip >= output_initial_frames
        {
            output_samples_to_skip = output_initial_frames
                + (output_samples_to_skip - output_initial_frames) % output_loop_frames;
        }
    }
    let _ = startup.send(Ok(()));
    push_silence_frames(
        &mut producer,
        leading_silence_samples,
        &mut output_samples_remaining,
        &cancelled,
        &position,
        &mut output_timeline_sample,
        timeline_loop_range,
    );
    if output_samples_remaining == Some(0) {
        return Ok(());
    }
    let mut decoded_frames = Vec::new();
    let mut source_channels = None;
    let mut loop_buffer = looping
        .then_some(loop_section_length)
        .flatten()
        .and_then(|frame_count| usize::try_from(frame_count).ok())
        .filter(|frame_count| {
            frame_count
                .checked_mul(std::mem::size_of::<P::Frame>())
                .is_some_and(|bytes| bytes <= MAX_CACHED_LOOP_SECTION_BYTES)
        })
        .map(Vec::<P::Frame>::with_capacity);
    let mut loop_buffer_ready = false;
    let mut loop_buffer_cursor = 0_usize;

    loop {
        if cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        if loop_buffer_ready {
            let buffer = loop_buffer
                .as_ref()
                .expect("ready loop buffer should be present");
            let end = loop_buffer_cursor.saturating_add(4_096).min(buffer.len());
            let mut output = resampler
                .as_mut()
                .expect("loop buffer needs an initialized resampler")
                .push(&buffer[loop_buffer_cursor..end])?;
            loop_buffer_cursor = if end == buffer.len() { 0 } else { end };
            skip_output(&mut output, &mut output_samples_to_skip);
            limit_output(&mut output, &mut output_samples_remaining);
            push_worker_output(
                &mut producer,
                &output,
                &cancelled,
                &position,
                &mut output_timeline_sample,
                timeline_loop_range,
            );
            if output_samples_remaining == Some(0) {
                return Ok(());
            }
            continue;
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
                        resampler = Some(StreamingResampler::<P::Frame>::new_with_quality(
                            chunk.sample_rate(),
                            output_sample_rate,
                            quality,
                        )?);
                        resampler.as_mut().expect("resampler was just initialized")
                    }
                };
                P::Frame::copy_from_chunk(&chunk, &mut decoded_frames);
                let skipped = source_offset_remaining.min(decoded_frames.len() as u64) as usize;
                source_offset_remaining -= skipped as u64;
                let available_frames = decoded_frames.len().saturating_sub(skipped);
                let frames_to_process =
                    loop_source_frames_remaining.map_or(available_frames, |remaining| {
                        usize::try_from(remaining)
                            .unwrap_or(usize::MAX)
                            .min(available_frames)
                    });
                let source_section = &decoded_frames[skipped..skipped + frames_to_process];
                source_frames_in_current_cycle =
                    source_frames_in_current_cycle.saturating_add(frames_to_process as u64);
                let mut output = if source_offset_remaining == 0 && frames_to_process > 0 {
                    if !initial_loop_pass && let Some(buffer) = &mut loop_buffer {
                        buffer.extend_from_slice(source_section);
                    }
                    current.push(source_section)?
                } else {
                    Vec::new()
                };
                skip_output(&mut output, &mut output_samples_to_skip);
                limit_output(&mut output, &mut output_samples_remaining);
                push_worker_output(
                    &mut producer,
                    &output,
                    &cancelled,
                    &position,
                    &mut output_timeline_sample,
                    timeline_loop_range,
                );
                if output_samples_remaining == Some(0) {
                    return Ok(());
                }
                let loop_boundary = if let Some(remaining) = &mut loop_source_frames_remaining {
                    *remaining = remaining.saturating_sub(frames_to_process as u64);
                    *remaining == 0
                } else {
                    false
                };
                if loop_boundary {
                    if !looping {
                        if let Some(resampler) = &mut resampler {
                            let mut output = resampler.finish();
                            skip_output(&mut output, &mut output_samples_to_skip);
                            limit_output(&mut output, &mut output_samples_remaining);
                            push_worker_output(
                                &mut producer,
                                &output,
                                &cancelled,
                                &position,
                                &mut output_timeline_sample,
                                timeline_loop_range,
                            );
                        }
                        push_remaining_item_silence(
                            &mut producer,
                            &mut output_samples_remaining,
                            &cancelled,
                            &position,
                            &mut output_timeline_sample,
                            timeline_loop_range,
                        );
                        return Ok(());
                    }
                    if initial_loop_pass {
                        initial_loop_pass = false;
                        restart_loop_decoder::<P>(
                            &mut decoder,
                            &mut source_offset_remaining,
                            &mut loop_source_frames_remaining,
                            loop_base_source_offset,
                            loop_section_length,
                            source_channels,
                            &resampler,
                        )?;
                        source_frames_in_current_cycle = 0;
                    } else if loop_buffer.is_some() {
                        loop_buffer_ready = true;
                        loop_buffer_cursor = 0;
                    } else {
                        restart_loop_decoder::<P>(
                            &mut decoder,
                            &mut source_offset_remaining,
                            &mut loop_source_frames_remaining,
                            loop_base_source_offset,
                            loop_section_length,
                            source_channels,
                            &resampler,
                        )?;
                        source_frames_in_current_cycle = 0;
                    }
                }
            }
            None => {
                if loop_source_frames_remaining.is_some_and(|remaining| remaining > 0) {
                    return Err(MediaError::AudioLoopSectionExceedsSource);
                }
                if looping && loop_base_source_offset.is_some() && loop_section_length.is_none() {
                    if source_offset_remaining > 0 || source_frames_in_current_cycle == 0 {
                        return Err(MediaError::AudioLoopSectionExceedsSource);
                    }
                    restart_loop_decoder::<P>(
                        &mut decoder,
                        &mut source_offset_remaining,
                        &mut loop_source_frames_remaining,
                        loop_base_source_offset,
                        loop_section_length,
                        source_channels,
                        &resampler,
                    )?;
                    source_frames_in_current_cycle = 0;
                    initial_loop_pass = false;
                    continue;
                }
                if let Some(resampler) = &mut resampler {
                    let mut output = resampler.finish();
                    skip_output(&mut output, &mut output_samples_to_skip);
                    limit_output(&mut output, &mut output_samples_remaining);
                    push_worker_output(
                        &mut producer,
                        &output,
                        &cancelled,
                        &position,
                        &mut output_timeline_sample,
                        timeline_loop_range,
                    );
                }
                push_remaining_item_silence(
                    &mut producer,
                    &mut output_samples_remaining,
                    &cancelled,
                    &position,
                    &mut output_timeline_sample,
                    timeline_loop_range,
                );
                return Ok(());
            }
        }
    }
}

/// Fills the remainder of a finite timeline item with silence after its source ends.
/// This runs on the feeder thread; the realtime callback only consumes already queued frames.
fn push_remaining_item_silence<P: FeedProducer>(
    producer: &mut P,
    remaining: &mut Option<u64>,
    cancelled: &AtomicBool,
    position: &AudioStreamPosition,
    timeline_sample: &mut u64,
    timeline_loop_range: Option<TransportLoopRange>,
) {
    let Some(remaining_frames) = *remaining else {
        return;
    };
    push_silence_frames(
        producer,
        remaining_frames,
        remaining,
        cancelled,
        position,
        timeline_sample,
        timeline_loop_range,
    );
}

fn push_silence_frames<P: FeedProducer>(
    producer: &mut P,
    requested_frames: u64,
    remaining: &mut Option<u64>,
    cancelled: &AtomicBool,
    position: &AudioStreamPosition,
    timeline_sample: &mut u64,
    timeline_loop_range: Option<TransportLoopRange>,
) {
    const SILENCE_CHUNK_FRAMES: usize = 1_024;
    let silence = [P::Frame::zero(); SILENCE_CHUNK_FRAMES];
    let mut frames_left = if let Some(remaining_frames) = remaining {
        let frames = requested_frames.min(*remaining_frames);
        *remaining_frames -= frames;
        frames
    } else {
        requested_frames
    };
    while frames_left > 0 {
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        let frame_count = usize::try_from(frames_left.min(SILENCE_CHUNK_FRAMES as u64))
            .expect("silence chunk length fits usize");
        push_worker_output(
            producer,
            &silence[..frame_count],
            cancelled,
            position,
            timeline_sample,
            timeline_loop_range,
        );
        frames_left -= frame_count as u64;
    }
}

fn worker_startup_error(error: &MediaError) -> (Option<io::ErrorKind>, String) {
    let io_kind = match error {
        MediaError::Io(error) => Some(error.kind()),
        _ => None,
    };
    (io_kind, error.to_string())
}

fn scale_frames_rounded(frames: u64, input_rate: u32, output_rate: u32) -> Option<u64> {
    u64::try_from(
        (u128::from(frames) * u128::from(output_rate) + u128::from(input_rate) / 2)
            / u128::from(input_rate),
    )
    .ok()
}

fn restart_loop_decoder<P: FeedProducer>(
    decoder: &mut WorkerDecoder,
    source_offset_remaining: &mut u64,
    loop_source_frames_remaining: &mut Option<u64>,
    loop_base_source_offset: Option<u64>,
    loop_section_length: Option<u64>,
    source_channels: Option<usize>,
    resampler: &Option<StreamingResampler<P::Frame>>,
) -> Result<(), MediaError> {
    let Some(base) = loop_base_source_offset else {
        return Err(MediaError::InvalidAudioLoopSection);
    };
    let (channels, sample_rate) = decoder.restart()?;
    if source_channels != Some(channels) {
        return Err(MediaError::InvalidDecodedAudioSpec);
    }
    if sample_rate.is_some_and(|sample_rate| {
        resampler
            .as_ref()
            .is_some_and(|resampler| resampler.input_sample_rate() != sample_rate)
    }) {
        return Err(MediaError::ChangedAudioSampleRate);
    }
    *source_offset_remaining = base;
    *loop_source_frames_remaining = loop_section_length;
    Ok(())
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

fn push_worker_output<P: FeedProducer>(
    producer: &mut P,
    samples: &[P::Frame],
    cancelled: &AtomicBool,
    position: &AudioStreamPosition,
    timeline_sample: &mut u64,
    timeline_loop_range: Option<TransportLoopRange>,
) {
    let Some(loop_range) = timeline_loop_range else {
        push_with_backpressure(producer, samples, cancelled, position, timeline_sample);
        return;
    };

    let loop_start = loop_range.start_sample();
    let loop_end = loop_range.end_sample();
    let mut offset = 0;
    while offset < samples.len() && !cancelled.load(Ordering::Acquire) {
        if *timeline_sample < loop_start || *timeline_sample >= loop_end {
            *timeline_sample = loop_start;
        }
        let frames_to_boundary = usize::try_from(loop_end - *timeline_sample)
            .unwrap_or(usize::MAX)
            .min(samples.len() - offset);
        let pushed = producer.push_frames_at(
            *timeline_sample,
            &samples[offset..offset + frames_to_boundary],
        );
        offset += pushed;
        *timeline_sample = timeline_sample.saturating_add(pushed as u64);
        if pushed < frames_to_boundary {
            thread::sleep(Duration::from_millis(1));
        }
    }
}

struct StreamingResampler<F> {
    input_sample_rate: u32,
    output_sample_rate: u32,
    filter_taps: usize,
    left_taps: i32,
    right_taps: i32,
    coefficients: Arc<[f32]>,
    next_position_numerator: u128,
    input_frames: u64,
    pending_start_frame: u64,
    pending_frames: Vec<F>,
}

impl<F: FeedFrame> StreamingResampler<F> {
    fn new_with_quality(
        input_sample_rate: u32,
        output_sample_rate: u32,
        quality: ResamplingQuality,
    ) -> Result<Self, MediaError> {
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
        let filter_taps = quality.taps();
        let left_taps = (filter_taps / 2) as i32 - 1;
        let right_taps = filter_taps as i32 - left_taps - 1;
        let coefficients = if input_sample_rate == output_sample_rate {
            Arc::from([])
        } else {
            cached_sinc_coefficients(input_sample_rate, output_sample_rate, quality)
        };
        Ok(Self {
            input_sample_rate,
            output_sample_rate,
            filter_taps,
            left_taps,
            right_taps,
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
            let last_required_frame = center_frame.saturating_add(self.right_taps as u64);
            if !at_eof && last_required_frame >= self.input_frames {
                break;
            }

            let phase_numerator = (self.next_position_numerator % denominator) as u64;
            let phase_position = phase_numerator * SINC_PHASES as u64;
            let phase = (phase_position / self.output_sample_rate as u64) as usize;
            let phase_remainder = phase_position % self.output_sample_rate as u64;
            let phase_fraction = phase_remainder as f32 / self.output_sample_rate as f32;
            let first_coefficients_start = phase * self.filter_taps;
            let second_coefficients_start = (phase + 1) * self.filter_taps;
            let mut frame = F::zero();
            for tap in 0..self.filter_taps {
                let offset = tap as i32 - self.left_taps;
                let source_frame = if offset < 0 {
                    center_frame.saturating_sub((-offset) as u64)
                } else {
                    center_frame.saturating_add(offset as u64)
                }
                .min(self.input_frames - 1);
                let sample =
                    self.pending_frames[(source_frame - self.pending_start_frame) as usize];
                let first_coefficient = self.coefficients[first_coefficients_start + tap];
                let second_coefficient = self.coefficients[second_coefficients_start + tap];
                let coefficient =
                    first_coefficient + (second_coefficient - first_coefficient) * phase_fraction;
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
                .saturating_sub(self.left_taps as u64)
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

fn cached_sinc_coefficients(
    input_sample_rate: u32,
    output_sample_rate: u32,
    quality: ResamplingQuality,
) -> Arc<[f32]> {
    type CoefficientCache = HashMap<(u32, u32, ResamplingQuality), Weak<[f32]>>;
    static CACHE: OnceLock<Mutex<CoefficientCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.retain(|_, coefficients| coefficients.strong_count() > 0);
    let key = (input_sample_rate, output_sample_rate, quality);
    if let Some(coefficients) = cache.get(&key).and_then(Weak::upgrade) {
        return coefficients;
    }
    if cache.len() >= 32 {
        cache.clear();
    }
    let coefficients = Arc::<[f32]>::from(build_sinc_coefficients(
        input_sample_rate,
        output_sample_rate,
        quality,
    ));
    cache.insert(key, Arc::downgrade(&coefficients));
    coefficients
}

fn build_sinc_coefficients(
    input_sample_rate: u32,
    output_sample_rate: u32,
    quality: ResamplingQuality,
) -> Vec<f32> {
    let cutoff = (output_sample_rate as f64 / input_sample_rate as f64).min(1.0);
    let filter_taps = quality.taps();
    let left_taps = (filter_taps / 2) as i32 - 1;
    let mut table = Vec::with_capacity((SINC_PHASES + 1) * filter_taps);
    for phase in 0..=SINC_PHASES {
        let fraction = phase as f64 / SINC_PHASES as f64;
        let mut row = vec![0.0; filter_taps];
        let mut sum = 0.0;
        for (tap, coefficient) in row.iter_mut().enumerate() {
            let distance = tap as f64 - left_taps as f64 - fraction;
            let normalized_distance = distance / (filter_taps as f64 / 2.0);
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
            *coefficient = value;
            sum += value;
        }
        if sum != 0.0 {
            table.extend(
                row.into_iter()
                    .map(|coefficient| (coefficient / sum) as f32),
            );
        } else {
            table.extend(row.into_iter().map(|coefficient| coefficient as f32));
        }
    }
    table
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
    use super::{ResamplingQuality, StreamingResampler};

    #[test]
    fn streaming_resampler_preserves_constant_level_and_output_duration() {
        for quality in [ResamplingQuality::Sinc64, ResamplingQuality::Sinc192] {
            let mut resampler =
                StreamingResampler::<f32>::new_with_quality(24_000, 48_000, quality)
                    .expect("valid sample rates should create a resampler");
            let mut output = resampler.push(&[0.25; 200]).expect("input should process");
            output.extend(resampler.finish());

            assert_eq!(output.len(), 400);
            assert!(output.iter().all(|sample| (*sample - 0.25).abs() < 1e-5));
        }
    }

    #[test]
    fn public_stereo_resampler_preserves_rate_duration_and_packet_boundaries() {
        use super::StereoPcmResampler;

        let input = (0..4_800)
            .map(|index| {
                let sample = (index as f32 * 0.031).sin() * 0.25;
                [sample, -sample]
            })
            .collect::<Vec<_>>();
        let mut whole = StereoPcmResampler::new(48_000, 44_100, ResamplingQuality::Sinc192)
            .expect("valid stereo rates should create a converter");
        let mut whole_output = whole
            .push_frames(&input)
            .expect("all input frames should convert");
        whole_output.extend(whole.finish());

        let mut chunked = StereoPcmResampler::new(48_000, 44_100, ResamplingQuality::Sinc192)
            .expect("valid stereo rates should create a converter");
        let mut chunked_output = Vec::new();
        for frames in input.chunks(73) {
            chunked_output.extend(chunked.push_frames(frames).expect("packet should convert"));
        }
        chunked_output.extend(chunked.finish());

        assert_eq!(whole_output.len(), 4_410);
        assert_eq!(chunked_output, whole_output);
        assert!(
            whole_output
                .iter()
                .all(|frame| (frame[0] + frame[1]).abs() < 1.0e-6)
        );
    }

    #[test]
    fn streaming_resampler_is_independent_of_packet_boundaries() {
        let input = (0..1021)
            .map(|index| (index as f32 * 0.037).sin())
            .collect::<Vec<_>>();
        let mut whole =
            StreamingResampler::<f32>::new_with_quality(48_000, 44_100, ResamplingQuality::Sinc64)
                .expect("valid sample rates should create a resampler");
        let mut whole_output = whole.push(&input).expect("input should process");
        whole_output.extend(whole.finish());

        let mut chunked =
            StreamingResampler::<f32>::new_with_quality(48_000, 44_100, ResamplingQuality::Sinc64)
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
        fn resample_tone(frequency: f32, quality: ResamplingQuality) -> Vec<f32> {
            let input_rate = 48_000.0;
            let input = (0..48_000)
                .map(|index| {
                    (2.0 * std::f32::consts::PI * frequency * index as f32 / input_rate).sin()
                })
                .collect::<Vec<_>>();
            let mut resampler =
                StreamingResampler::<f32>::new_with_quality(48_000, 24_000, quality)
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

        let passband = measured_amplitude(
            &resample_tone(3_000.0, ResamplingQuality::Sinc64),
            3_000.0,
            24_000.0,
        );
        let aliased = measured_amplitude(
            &resample_tone(18_000.0, ResamplingQuality::Sinc64),
            6_000.0,
            24_000.0,
        );

        assert!(passband > 0.97, "passband amplitude was {passband}");
        assert!(aliased < 0.01, "aliased amplitude was {aliased}");
    }

    #[test]
    fn render_sinc_has_a_sharper_downsampling_transition_than_playback_sinc() {
        fn resample_tone(frequency: f32, quality: ResamplingQuality) -> Vec<f32> {
            let input = (0..48_000)
                .map(|index| {
                    (2.0 * std::f32::consts::PI * frequency * index as f32 / 48_000.0).sin()
                })
                .collect::<Vec<_>>();
            let mut resampler =
                StreamingResampler::<f32>::new_with_quality(48_000, 24_000, quality)
                    .expect("valid sample rates should create a resampler");
            let mut output = resampler.push(&input).expect("input should process");
            output.extend(resampler.finish());
            output
        }

        fn amplitude(samples: &[f32], frequency: f32) -> f32 {
            let stable = &samples[128..samples.len() - 128];
            let projection = stable
                .iter()
                .enumerate()
                .map(|(index, sample)| {
                    let absolute_index = index + 128;
                    sample
                        * (2.0 * std::f32::consts::PI * frequency * absolute_index as f32
                            / 24_000.0)
                            .sin()
                })
                .sum::<f32>();
            2.0 * projection.abs() / stable.len() as f32
        }

        let playback_alias = amplitude(
            &resample_tone(13_000.0, ResamplingQuality::Sinc64),
            11_000.0,
        );
        let render_alias = amplitude(
            &resample_tone(13_000.0, ResamplingQuality::Sinc192),
            11_000.0,
        );

        assert!(
            render_alias < playback_alias * 0.7,
            "render alias {render_alias} should be below 70% of playback alias {playback_alias}"
        );
    }

    #[test]
    fn stereo_resampler_preserves_channels_across_packet_boundaries() {
        let input = (0..129)
            .map(|index| {
                let sample = (index as f32 * 0.091).sin();
                [sample, -sample]
            })
            .collect::<Vec<_>>();
        let mut resampler = StreamingResampler::<[f32; 2]>::new_with_quality(
            24_000,
            48_000,
            ResamplingQuality::Sinc64,
        )
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
