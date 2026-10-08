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
    AudioFeedWorker, StereoPcmResampler, spawn_audio_item_stream, spawn_audio_item_stream_at,
    spawn_audio_item_stream_from_reader, spawn_audio_item_stream_from_reader_at,
    spawn_cached_stereo_audio_item_stream, spawn_mono_stream, spawn_stereo_audio_item_stream,
    spawn_stereo_audio_item_stream_at, spawn_stereo_audio_item_stream_from_reader,
    spawn_stereo_audio_item_stream_from_reader_at,
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
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    metadata: AudioMetadata,
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
        reader: R,
        byte_len: Option<u64>,
        extension: Option<&str>,
    ) -> Result<Self, MediaError>
    where
        R: Read + Seek + Send + Sync + 'static,
    {
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
            format,
            decoder,
            track_id,
            metadata,
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
        loop {
            let Some(packet) = self.format.next_packet()? else {
                return Ok(None);
            };
            if packet.track_id != self.track_id {
                continue;
            }

            let decoded = self.decoder.decode(&packet)?;
            let spec = decoded.spec();
            let channels = spec.channels().count();
            if spec.rate() == 0 || channels == 0 {
                return Err(MediaError::InvalidDecodedAudioSpec);
            }
            let mut samples = vec![0.0; decoded.samples_interleaved()];
            decoded.copy_to_slice_interleaved(&mut samples);
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
