//! Background-thread media import and decoding.
//!
//! Decoding is deliberately packet-based so callers can stream large files
//! without buffering the entire asset in memory. Never call this API from an
//! audio callback.

use std::error::Error as StdError;
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

mod decoded_cache;
mod stream;
mod waveform;
mod waveform_cache;
pub use decoded_cache::{
    DECODED_AUDIO_CACHE_BYTES, DECODED_AUDIO_CACHE_ENTRIES, DecodedAudioCache,
    DecodedAudioCacheKey, MAX_CACHED_AUDIO_SOURCE_BYTES,
};
pub use stream::{
    AudioFeedWorker, ResamplingQuality, StereoPcmResampler, spawn_audio_item_stream,
    spawn_audio_item_stream_at, spawn_audio_item_stream_from_reader,
    spawn_audio_item_stream_from_reader_at, spawn_cached_stereo_audio_item_stream,
    spawn_cached_stereo_audio_item_stream_with_quality, spawn_mono_stream,
    spawn_stereo_audio_item_stream, spawn_stereo_audio_item_stream_at,
    spawn_stereo_audio_item_stream_at_with_quality,
    spawn_stereo_audio_item_stream_for_transport_loop,
    spawn_stereo_audio_item_stream_for_transport_loop_at,
    spawn_stereo_audio_item_stream_from_reader, spawn_stereo_audio_item_stream_from_reader_at,
    spawn_stereo_audio_item_stream_from_reader_at_with_quality,
    spawn_stereo_audio_item_stream_from_reader_for_transport_loop,
    spawn_stereo_audio_item_stream_from_reader_with_quality,
    spawn_stereo_audio_item_stream_with_quality,
};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
pub use waveform::{AudioWaveform, WaveformLevel, WaveformPeak};
pub use waveform_cache::{AudioWaveformCacheEntry, decode_waveform_cache, encode_waveform_cache};

/// One decoded, interleaved floating-point PCM packet.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudioChunk {
    sample_rate: u32,
    channels: usize,
    samples: Vec<f32>,
}

/// Fully decoded source PCM retained only for short embedded sources.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudioSource {
    sample_rate: u32,
    channels: usize,
    samples: Vec<f32>,
}

impl DecodedAudioSource {
    /// Returns the original source sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the source channel count.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Returns interleaved source samples.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Returns decoded PCM storage in bytes.
    pub fn byte_len(&self) -> usize {
        self.samples
            .len()
            .saturating_mul(std::mem::size_of::<f32>())
    }

    pub(crate) fn chunk_at(&self, cursor: &mut usize) -> Option<DecodedAudioChunk> {
        const CHUNK_FRAMES: usize = 4_096;
        let total_frames = self.samples.len() / self.channels;
        if *cursor >= total_frames {
            return None;
        }
        let end = cursor.saturating_add(CHUNK_FRAMES).min(total_frames);
        let start_sample = *cursor * self.channels;
        let end_sample = end * self.channels;
        *cursor = end;
        Some(DecodedAudioChunk {
            sample_rate: self.sample_rate,
            channels: self.channels,
            samples: self.samples[start_sample..end_sample].to_vec(),
        })
    }

    #[cfg(test)]
    fn from_test_data(byte_len: usize) -> Self {
        Self {
            sample_rate: 48_000,
            channels: 1,
            samples: vec![0.0; byte_len.div_ceil(std::mem::size_of::<f32>())],
        }
    }
}

impl DecodedAudioChunk {
    /// Returns the sample rate of this chunk.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the number of channels in this chunk.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Returns the interleaved floating-point samples.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Returns the number of complete audio frames in this chunk.
    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.channels
    }
}

/// Header metadata for the first decodable audio track.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AudioMetadata {
    pub container: String,
    pub codec: String,
    pub sample_rate: Option<u32>,
    pub channel_count: Option<u32>,
    pub bits_per_sample: Option<u32>,
    pub frame_count: Option<u64>,
    pub duration_nanos: Option<u64>,
    pub byte_len: Option<u64>,
}

/// A packet-by-packet decoder for the first decodable audio track in a file.
pub struct AudioStreamDecoder {
    format: Option<Box<dyn FormatReader>>,
    decoder: Option<Box<dyn AudioDecoder>>,
    track_id: u32,
    metadata: AudioMetadata,
    fact_frames_remaining: Option<u64>,
    cadpcm_wave: Option<CadpcmWaveReader>,
}

trait ReadSeek: Read + Seek + Send + Sync {}

impl<T> ReadSeek for T where T: Read + Seek + Send + Sync {}

struct CadpcmWaveReader {
    reader: Box<dyn ReadSeek>,
    data_bytes_remaining: u64,
    frames_remaining: u64,
    sample_rate: u32,
    channels: usize,
    block_align: usize,
    samples_per_block: usize,
}

/// Errors encountered while opening or decoding an audio file.
#[derive(Debug)]
pub enum MediaError {
    Io(std::io::Error),
    Symphonia(SymphoniaError),
    NoAudioTrack,
    MissingCodecParameters,
    MissingAudioCodecParameters,
    InvalidDecodedAudioSpec,
    InvalidOutputSampleRate,
    InvalidWaveformBinSize,
    InvalidAudioItemLength,
    InvalidAudioItemSeek {
        requested: u64,
        start_sample: u64,
        end_sample: u64,
    },
    ChangedAudioSampleRate,
    ResampleRatioTooLarge {
        input: u32,
        output: u32,
    },
    AudioLoopSectionExceedsSource,
    InvalidAudioLoopSection,
    InvalidTimelineLoop,
    UnsupportedLoopResamplingPrefix,
    AudioTooLong,
    WorkerCancelled,
    ThreadSpawn(std::io::Error),
    WorkerPanicked,
    WorkerStartupFailed {
        message: String,
        io_kind: Option<std::io::ErrorKind>,
    },
    WorkerStartupAlreadyChecked,
}

impl fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "media source I/O failed: {error}"),
            Self::Symphonia(error) => write!(formatter, "audio decode failed: {error}"),
            Self::NoAudioTrack => formatter.write_str("file contains no decodable audio track"),
            Self::MissingCodecParameters => {
                formatter.write_str("audio track has no codec parameters")
            }
            Self::MissingAudioCodecParameters => {
                formatter.write_str("track codec parameters are not audio")
            }
            Self::InvalidDecodedAudioSpec => {
                formatter.write_str("decoder returned an invalid audio specification")
            }
            Self::InvalidOutputSampleRate => {
                formatter.write_str("output sample rate must be positive")
            }
            Self::InvalidWaveformBinSize => {
                formatter.write_str("waveform bin size must be positive")
            }
            Self::InvalidAudioItemLength => {
                formatter.write_str("audio item output length must be positive")
            }
            Self::InvalidAudioItemSeek {
                requested,
                start_sample,
                end_sample,
            } => write!(
                formatter,
                "timeline sample {requested} is outside audio item range {start_sample}..{end_sample}"
            ),
            Self::ChangedAudioSampleRate => {
                formatter.write_str("audio sample rate changed during decoding")
            }
            Self::ResampleRatioTooLarge { input, output } => write!(
                formatter,
                "resampling ratio from {input} Hz to {output} Hz exceeds the supported limit"
            ),
            Self::AudioLoopSectionExceedsSource => {
                formatter.write_str("looped source section extends beyond the decoded media")
            }
            Self::InvalidAudioLoopSection => {
                formatter.write_str("loop source section does not contain the item source offset")
            }
            Self::InvalidTimelineLoop => formatter.write_str(
                "transport loop and source segment do not share exact sample boundaries",
            ),
            Self::UnsupportedLoopResamplingPrefix => formatter.write_str(
                "rate-converted pre-loop source exceeds the bounded cyclic-history cache",
            ),
            Self::AudioTooLong => formatter.write_str("audio stream exceeds the supported length"),
            Self::WorkerCancelled => formatter.write_str("audio waveform decoding was cancelled"),
            Self::ThreadSpawn(error) => write!(formatter, "failed to start audio worker: {error}"),
            Self::WorkerPanicked => formatter.write_str("audio decoding worker panicked"),
            Self::WorkerStartupFailed { message, .. } => {
                write!(
                    formatter,
                    "audio decoding worker could not open its source: {message}"
                )
            }
            Self::WorkerStartupAlreadyChecked => {
                formatter.write_str("audio decoding worker startup was already checked")
            }
        }
    }
}

impl StdError for MediaError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) | Self::ThreadSpawn(error) => Some(error),
            Self::Symphonia(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SymphoniaError> for MediaError {
    fn from(error: SymphoniaError) -> Self {
        Self::Symphonia(error)
    }
}

impl AudioStreamDecoder {
    /// Opens a file and selects its first decodable audio track.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MediaError> {
        let path = path.as_ref();
        let file = File::open(path).map_err(MediaError::Io)?;
        let byte_len = file.metadata().map_err(MediaError::Io)?.len();
        let extension = path.extension().and_then(|extension| extension.to_str());
        Self::from_reader(file, Some(byte_len), extension)
    }

    /// Opens a seekable reader, such as an embedded SQLite asset, for decoding.
    pub fn from_reader<R>(
        mut reader: R,
        byte_len: Option<u64>,
        extension: Option<&str>,
    ) -> Result<Self, MediaError>
    where
        R: Read + Seek + Send + Sync + 'static,
    {
        if let Some(info) = read_cadpcm_wave_info(&mut reader, byte_len).map_err(MediaError::Io)? {
            reader
                .seek(SeekFrom::Start(info.data_offset))
                .map_err(MediaError::Io)?;
            let frame_count = Some(info.frame_count);
            let duration_nanos = u64::try_from(
                u128::from(info.frame_count) * 1_000_000_000 / u128::from(info.sample_rate),
            )
            .ok();
            return Ok(Self {
                format: None,
                decoder: None,
                track_id: 0,
                metadata: AudioMetadata {
                    container: "WAVE".to_owned(),
                    codec: "cADPCM".to_owned(),
                    sample_rate: Some(info.sample_rate),
                    channel_count: Some(info.channels as u32),
                    bits_per_sample: Some(2),
                    frame_count,
                    duration_nanos,
                    byte_len,
                },
                fact_frames_remaining: None,
                cadpcm_wave: Some(CadpcmWaveReader {
                    reader: Box::new(reader),
                    data_bytes_remaining: info.data_bytes,
                    frames_remaining: info.frame_count,
                    sample_rate: info.sample_rate,
                    channels: info.channels,
                    block_align: info.block_align,
                    samples_per_block: info.samples_per_block,
                }),
            });
        }
        let fact_frame_count =
            read_wave_fact_frame_count(&mut reader, byte_len).map_err(MediaError::Io)?;
        let source = SeekableMediaSource { reader, byte_len };
        let source = MediaSourceStream::new(Box::new(source), Default::default());
        let mut hint = Hint::new();
        if let Some(extension) = extension {
            hint.with_extension(extension);
        }
        let format = symphonia::default::get_probe().probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )?;

        let container = format.format_info().short_name.to_owned();
        let (track_id, codec_params, frame_count, duration_nanos) = {
            let track = format
                .default_track(TrackType::Audio)
                .ok_or(MediaError::NoAudioTrack)?;
            let duration_nanos = track
                .duration
                .and_then(|duration| {
                    track
                        .time_base
                        .and_then(|base| base.calc_duration(duration))
                })
                .and_then(|time| u64::try_from(time.as_nanos()).ok());
            (
                track.id,
                track
                    .codec_params
                    .clone()
                    .ok_or(MediaError::MissingCodecParameters)?,
                track.num_frames,
                duration_nanos,
            )
        };
        let audio_params = codec_params
            .audio()
            .ok_or(MediaError::MissingAudioCodecParameters)?;
        let sample_rate = audio_params.sample_rate;
        let frame_count = fact_frame_count.or(frame_count);
        let fact_duration_nanos = match (fact_frame_count, sample_rate) {
            (Some(frames), Some(rate)) if rate > 0 => {
                u64::try_from(u128::from(frames) * 1_000_000_000 / u128::from(rate)).ok()
            }
            _ => None,
        };
        let duration_nanos = fact_duration_nanos.or(duration_nanos);
        let channel_count = audio_params
            .channels
            .as_ref()
            .and_then(|channels| u32::try_from(channels.count()).ok());
        let bits_per_sample = audio_params.bits_per_sample;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(audio_params, &AudioDecoderOptions::default())?;
        let metadata = AudioMetadata {
            container,
            codec: decoder.codec_info().short_name.to_owned(),
            sample_rate,
            channel_count,
            bits_per_sample,
            frame_count,
            duration_nanos,
            byte_len,
        };

        Ok(Self {
            format: Some(format),
            decoder: Some(decoder),
            track_id,
            metadata,
            fact_frames_remaining: fact_frame_count,
            cadpcm_wave: None,
        })
    }

    /// Returns container and codec header metadata without decoding the whole stream.
    pub fn metadata(&self) -> &AudioMetadata {
        &self.metadata
    }

    /// Decodes the next packet from the selected track, or returns `None` at EOF.
    /// Each returned chunk owns its PCM samples, so decode and consume it on a
    /// background worker rather than on the realtime audio thread.
    pub fn next_chunk(&mut self) -> Result<Option<DecodedAudioChunk>, MediaError> {
        if let Some(cadpcm_wave) = &mut self.cadpcm_wave {
            return cadpcm_wave.next_chunk();
        }
        loop {
            if self.fact_frames_remaining == Some(0) {
                return Ok(None);
            }
            let Some(packet) = self
                .format
                .as_mut()
                .expect("Symphonia decoder retains a format reader")
                .next_packet()?
            else {
                return Ok(None);
            };
            if packet.track_id != self.track_id {
                continue;
            }

            let decoded = self
                .decoder
                .as_mut()
                .expect("Symphonia decoder retains a codec decoder")
                .decode(&packet)?;
            let spec = decoded.spec();
            let channels = spec.channels().count();
            if spec.rate() == 0 || channels == 0 {
                return Err(MediaError::InvalidDecodedAudioSpec);
            }
            let mut samples = vec![0.0; decoded.samples_interleaved()];
            decoded.copy_to_slice_interleaved(&mut samples);
            if let Some(remaining) = &mut self.fact_frames_remaining {
                let valid_frames = (*remaining).min((samples.len() / channels) as u64) as usize;
                samples.truncate(valid_frames * channels);
                *remaining -= valid_frames as u64;
            }
            return Ok(Some(DecodedAudioChunk {
                sample_rate: spec.rate(),
                channels,
                samples,
            }));
        }
    }
}

/// Probes container and first-audio-track metadata from a file without decoding its samples.
///
/// Call this from a background thread because probing may perform file I/O.
pub fn probe_audio_metadata(path: impl AsRef<Path>) -> Result<AudioMetadata, MediaError> {
    Ok(AudioStreamDecoder::open(path)?.metadata().clone())
}

/// Decodes a complete source when its raw PCM stays under `max_bytes`.
///
/// Returns `Ok(None)` as soon as the source is too large to cache. Callers must reopen that source
/// and use the streaming path in that case.
pub fn decode_audio_source_for_cache<R>(
    reader: R,
    byte_len: Option<u64>,
    extension: Option<&str>,
    max_bytes: usize,
) -> Result<Option<DecodedAudioSource>, MediaError>
where
    R: Read + Seek + Send + Sync + 'static,
{
    let mut decoder = AudioStreamDecoder::from_reader(reader, byte_len, extension)?;
    let metadata = decoder.metadata();
    if let (Some(frame_count), Some(channels)) = (metadata.frame_count, metadata.channel_count) {
        let estimated_bytes = frame_count
            .checked_mul(u64::from(channels))
            .and_then(|samples| samples.checked_mul(std::mem::size_of::<f32>() as u64));
        if estimated_bytes.is_some_and(|bytes| bytes > max_bytes as u64) {
            return Ok(None);
        }
    }

    let mut decoded_sample_rate = None;
    let mut decoded_channels = None;
    let mut samples = Vec::new();
    while let Some(chunk) = decoder.next_chunk()? {
        if decoded_sample_rate.is_some_and(|rate| rate != chunk.sample_rate())
            || decoded_channels.is_some_and(|channels| channels != chunk.channels())
        {
            return Err(MediaError::InvalidDecodedAudioSpec);
        }
        decoded_sample_rate = Some(chunk.sample_rate());
        decoded_channels = Some(chunk.channels());
        let Some(next_len) = samples.len().checked_add(chunk.samples().len()) else {
            return Ok(None);
        };
        let Some(next_bytes) = next_len.checked_mul(std::mem::size_of::<f32>()) else {
            return Ok(None);
        };
        if next_bytes > max_bytes {
            return Ok(None);
        }
        samples.extend_from_slice(chunk.samples());
    }
    let Some(sample_rate) = decoded_sample_rate else {
        return Ok(None);
    };
    let Some(channels) = decoded_channels else {
        return Ok(None);
    };
    Ok(Some(DecodedAudioSource {
        sample_rate,
        channels,
        samples,
    }))
}

struct CadpcmWaveInfo {
    data_offset: u64,
    data_bytes: u64,
    frame_count: u64,
    sample_rate: u32,
    channels: usize,
    block_align: usize,
    samples_per_block: usize,
}

fn read_cadpcm_wave_info<R: Read + Seek>(
    reader: &mut R,
    byte_len: Option<u64>,
) -> io::Result<Option<CadpcmWaveInfo>> {
    let original_position = reader.stream_position()?;
    let info = (|| {
        let mut header = [0_u8; 12];
        if reader.seek(SeekFrom::Start(0)).is_err() || reader.read_exact(&mut header).is_err() {
            return Ok(None);
        }
        if &header[..4] != b"RIFF" || &header[8..12] != b"WAVE" {
            return Ok(None);
        }
        let file_end = match byte_len {
            Some(length) => length,
            None => match reader.seek(SeekFrom::End(0)) {
                Ok(length) => length,
                Err(_) => return Ok(None),
            },
        };
        let riff_end = 8_u64 + u64::from(u32::from_le_bytes(header[4..8].try_into().unwrap()));
        let limit = file_end.min(riff_end);
        let mut offset = 12_u64;
        let mut format = None;
        let mut fact_frames = None;
        let mut data = None;

        for _ in 0..256 {
            if offset.checked_add(8).is_none_or(|end| end > limit) {
                break;
            }
            reader.seek(SeekFrom::Start(offset))?;
            let mut chunk_header = [0_u8; 8];
            reader.read_exact(&mut chunk_header)?;
            let size = u64::from(u32::from_le_bytes(chunk_header[4..8].try_into().unwrap()));
            let chunk_data = offset + 8;
            let chunk_end = chunk_data.checked_add(size).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "WAVE chunk offset overflow")
            })?;
            if chunk_end > limit {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "WAVE chunk exceeds RIFF bounds",
                ));
            }

            match &chunk_header[..4] {
                b"fmt " if format.is_none() => {
                    if size < 2 {
                        return Ok(None);
                    }
                    reader.seek(SeekFrom::Start(chunk_data))?;
                    let mut format_tag = [0_u8; 2];
                    reader.read_exact(&mut format_tag)?;
                    if u16::from_le_bytes(format_tag) == 0xAC0C {
                        if size < 20 {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "cADPCM fmt chunk is too short",
                            ));
                        }
                        reader.seek(SeekFrom::Start(chunk_data))?;
                        let mut fmt = [0_u8; 20];
                        reader.read_exact(&mut fmt)?;
                        let channels = usize::from(u16::from_le_bytes([fmt[2], fmt[3]]));
                        let sample_rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
                        let block_align = usize::from(u16::from_le_bytes([fmt[12], fmt[13]]));
                        let bits_per_sample = u16::from_le_bytes([fmt[14], fmt[15]]);
                        let extra_size = u16::from_le_bytes([fmt[16], fmt[17]]);
                        let samples_per_block = usize::from(u16::from_le_bytes([fmt[18], fmt[19]]));
                        format = Some((
                            channels,
                            sample_rate,
                            block_align,
                            bits_per_sample,
                            extra_size,
                            samples_per_block,
                        ));
                    }
                }
                b"fact" if size >= 4 => {
                    reader.seek(SeekFrom::Start(chunk_data))?;
                    let mut frames = [0_u8; 4];
                    reader.read_exact(&mut frames)?;
                    fact_frames = Some(u64::from(u32::from_le_bytes(frames)));
                }
                b"data" if data.is_none() => data = Some((chunk_data, size)),
                _ => {}
            }
            offset = chunk_end.checked_add(size & 1).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "WAVE chunk offset overflow")
            })?;
        }

        let Some((channels, sample_rate, block_align, bits, extra_size, samples_per_block)) =
            format
        else {
            return Ok(None);
        };
        if channels == 0 || channels > 2 || sample_rate == 0 || bits != 2 || extra_size < 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported cADPCM WAVE parameters",
            ));
        }
        let expected_block_align = 1024_usize.checked_mul(channels).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "cADPCM block size overflow")
        })?;
        let expected_samples_per_block = 1 + (expected_block_align / channels - 4) * 4;
        if block_align != expected_block_align || samples_per_block != expected_samples_per_block {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported cADPCM block layout",
            ));
        }
        let Some((data_offset, data_bytes)) = data else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cADPCM WAVE has no data chunk",
            ));
        };
        if data_bytes % block_align as u64 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cADPCM data is not block aligned",
            ));
        }
        let block_count = data_bytes / block_align as u64;
        let capacity = block_count
            .checked_mul(samples_per_block as u64)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "cADPCM frame count overflow")
            })?;
        let frame_count = fact_frames.unwrap_or(capacity);
        if frame_count > capacity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cADPCM fact count exceeds encoded data",
            ));
        }
        Ok(Some(CadpcmWaveInfo {
            data_offset,
            data_bytes,
            frame_count,
            sample_rate,
            channels,
            block_align,
            samples_per_block,
        }))
    })();
    reader.seek(SeekFrom::Start(original_position))?;
    info
}

impl CadpcmWaveReader {
    fn next_chunk(&mut self) -> Result<Option<DecodedAudioChunk>, MediaError> {
        if self.frames_remaining == 0 {
            return Ok(None);
        }
        if self.data_bytes_remaining < self.block_align as u64 {
            return Err(MediaError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "cADPCM data ended before the fact frame count",
            )));
        }
        let mut block = vec![0_u8; self.block_align];
        self.reader.read_exact(&mut block).map_err(MediaError::Io)?;
        self.data_bytes_remaining -= self.block_align as u64;
        let frames = self.frames_remaining.min(self.samples_per_block as u64) as usize;
        let mut samples = vec![0.0_f32; frames * self.channels];
        let mut predictors = [0_i32; 2];
        let mut indices = [0_usize; 2];
        for channel in 0..self.channels {
            let header = channel * 4;
            predictors[channel] = i32::from(i16::from_le_bytes([block[header], block[header + 1]]));
            indices[channel] = usize::from(block[header + 2]).min(88);
            samples[channel] = predictors[channel] as f32 / 32_768.0;
        }

        for group in 0..((self.samples_per_block - 1) / 16) {
            for channel in 0..self.channels {
                let channel_offset = self.channels * 4 + group * self.channels * 4 + channel * 4;
                for sample_in_group in 0..16 {
                    let frame = 1 + group * 16 + sample_in_group;
                    if frame >= frames {
                        break;
                    }
                    let byte = block[channel_offset + sample_in_group / 4];
                    let code = (byte >> ((sample_in_group & 3) * 2)) & 3;
                    let step = CADPCM_STEP_TABLE[indices[channel]];
                    let mut difference = step >> 3;
                    if code & 1 != 0 {
                        difference += step;
                    }
                    if code & 2 != 0 {
                        predictors[channel] -= difference;
                    } else {
                        predictors[channel] += difference;
                    }
                    predictors[channel] =
                        predictors[channel].clamp(i32::from(i16::MIN), i32::from(i16::MAX));
                    let adjustment = if code & 1 != 0 { 2 } else { -1 };
                    indices[channel] = (indices[channel] as i32 + adjustment).clamp(0, 88) as usize;
                    samples[frame * self.channels + channel] =
                        predictors[channel] as f32 / 32_768.0;
                }
            }
        }
        self.frames_remaining -= frames as u64;
        Ok(Some(DecodedAudioChunk {
            sample_rate: self.sample_rate,
            channels: self.channels,
            samples,
        }))
    }
}

const CADPCM_STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

fn read_wave_fact_frame_count<R: Read + Seek>(
    reader: &mut R,
    byte_len: Option<u64>,
) -> io::Result<Option<u64>> {
    let original_position = reader.stream_position()?;
    let frame_count = (|| {
        let mut header = [0_u8; 12];
        reader.seek(SeekFrom::Start(0)).ok()?;
        reader.read_exact(&mut header).ok()?;
        if &header[..4] != b"RIFF" || &header[8..12] != b"WAVE" {
            return None;
        }
        let file_end = byte_len.or_else(|| reader.seek(SeekFrom::End(0)).ok())?;
        let riff_end =
            8_u64.checked_add(u64::from(u32::from_le_bytes(header[4..8].try_into().ok()?)))?;
        let limit = file_end.min(riff_end);
        let mut offset = 12_u64;

        for _ in 0..256 {
            if offset.checked_add(8)? > limit {
                return None;
            }
            reader.seek(SeekFrom::Start(offset)).ok()?;
            let mut chunk_header = [0_u8; 8];
            reader.read_exact(&mut chunk_header).ok()?;
            let chunk_size = u64::from(u32::from_le_bytes(chunk_header[4..8].try_into().ok()?));
            let chunk_data = offset.checked_add(8)?;
            let chunk_end = chunk_data.checked_add(chunk_size)?;
            if chunk_end > limit {
                return None;
            }
            if &chunk_header[..4] == b"fact" {
                if chunk_size < 4 {
                    return None;
                }
                let mut frames = [0_u8; 4];
                reader.read_exact(&mut frames).ok()?;
                return Some(u64::from(u32::from_le_bytes(frames)));
            }
            offset = chunk_end.checked_add(chunk_size & 1)?;
        }
        None
    })();
    reader.seek(SeekFrom::Start(original_position))?;
    Ok(frame_count)
}

struct SeekableMediaSource<R> {
    reader: R,
    byte_len: Option<u64>,
}

impl<R: Read> Read for SeekableMediaSource<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buffer)
    }
}

impl<R: Seek> Seek for SeekableMediaSource<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.reader.seek(position)
    }
}

impl<R: Read + Seek + Send + Sync> MediaSource for SeekableMediaSource<R> {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }
}

#[cfg(test)]
mod decoded_source_tests {
    use super::decode_audio_source_for_cache;
    use std::io::Cursor;

    fn pcm_wav() -> Vec<u8> {
        let samples = [0_i16, 8192, -8192, 16_384];
        let data_len = (samples.len() * 2) as u32;
        let mut bytes = Vec::with_capacity(44 + data_len as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&48_000_u32.to_le_bytes());
        bytes.extend_from_slice(&96_000_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn short_source_decodes_with_source_metadata() {
        let bytes = pcm_wav();
        let source = decode_audio_source_for_cache(
            Cursor::new(bytes.clone()),
            Some(bytes.len() as u64),
            Some("wav"),
            64,
        )
        .unwrap()
        .unwrap();
        assert_eq!(source.sample_rate(), 48_000);
        assert_eq!(source.channels(), 1);
        assert_eq!(source.samples().len(), 4);
        assert_eq!(source.byte_len(), 16);
    }

    #[test]
    fn oversized_source_is_rejected_without_full_decode() {
        let bytes = pcm_wav();
        assert!(
            decode_audio_source_for_cache(
                Cursor::new(bytes.clone()),
                Some(bytes.len() as u64),
                Some("wav"),
                8,
            )
            .unwrap()
            .is_none()
        );
    }
}
