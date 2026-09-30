//! Background-thread media import and decoding.
//!
//! Decoding is deliberately packet-based so callers can stream large files
//! without buffering the entire asset in memory. Never call this API from an
//! audio callback.

use std::error::Error as StdError;
use std::fmt;
use std::fs::File;
use std::path::Path;

mod stream;
pub use stream::{AudioFeedWorker, spawn_mono_stream};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// One decoded, interleaved floating-point PCM packet.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudioChunk {
    sample_rate: u32,
    channels: usize,
    samples: Vec<f32>,
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

/// A packet-by-packet decoder for the first decodable audio track in a file.
pub struct AudioStreamDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
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
    ChangedAudioSampleRate,
    ResampleRatioTooLarge { input: u32, output: u32 },
    AudioTooLong,
    ThreadSpawn(std::io::Error),
    WorkerPanicked,
}

impl fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "failed to open audio file: {error}"),
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
            Self::ChangedAudioSampleRate => {
                formatter.write_str("audio sample rate changed during decoding")
            }
            Self::ResampleRatioTooLarge { input, output } => write!(
                formatter,
                "resampling ratio from {input} Hz to {output} Hz exceeds the supported limit"
            ),
            Self::AudioTooLong => formatter.write_str("audio stream exceeds the supported length"),
            Self::ThreadSpawn(error) => write!(formatter, "failed to start audio worker: {error}"),
            Self::WorkerPanicked => formatter.write_str("audio decoding worker panicked"),
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
        let source = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
            hint.with_extension(extension);
        }
        let format = symphonia::default::get_probe().probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )?;

        let (track_id, codec_params) = {
            let track = format
                .default_track(TrackType::Audio)
                .ok_or(MediaError::NoAudioTrack)?;
            (
                track.id,
                track
                    .codec_params
                    .clone()
                    .ok_or(MediaError::MissingCodecParameters)?,
            )
        };
        let audio_params = codec_params
            .audio()
            .ok_or(MediaError::MissingAudioCodecParameters)?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(audio_params, &AudioDecoderOptions::default())?;

        Ok(Self {
            format,
            decoder,
            track_id,
        })
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
