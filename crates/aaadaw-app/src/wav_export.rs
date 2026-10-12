use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RIFF_DATA_BYTES: u64 = u32::MAX as u64 - 36;
const IMA_ADPCM_SAMPLES_PER_BLOCK: usize = 2041;
const CADPCM_SAMPLES_PER_BLOCK: usize = 4081;
const IMA_ADPCM_BYTES_PER_CHANNEL_BLOCK: usize = 1024;
const ADPCM_RIFF_OVERHEAD: u64 = 52;
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
static NEXT_DITHER_SEED: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WavSampleFormat {
    Pcm8,
    Pcm16,
    #[default]
    Pcm24,
    Pcm32,
    Float32,
    Float64,
    ImaAdpcm4,
    Cadpcm2,
    MuLaw8,
}

impl WavSampleFormat {
    pub const ALL: [Self; 9] = [
        Self::Pcm8,
        Self::Pcm16,
        Self::Pcm24,
        Self::Pcm32,
        Self::Float32,
        Self::Float64,
        Self::ImaAdpcm4,
        Self::Cadpcm2,
        Self::MuLaw8,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Pcm8 => "PCM 8-bit",
            Self::Pcm16 => "PCM 16-bit",
            Self::Pcm24 => "PCM 24-bit",
            Self::Pcm32 => "PCM 32-bit",
            Self::Float32 => "Float 32-bit",
            Self::Float64 => "Float 64-bit",
            Self::MuLaw8 => "8-bit u-Law",
            Self::ImaAdpcm4 => "4-bit IMA ADPCM",
            Self::Cadpcm2 => "2-bit cADPCM",
        }
    }

    pub const fn is_integer(self) -> bool {
        matches!(self, Self::Pcm8 | Self::Pcm16 | Self::Pcm24 | Self::Pcm32)
    }
}

impl std::fmt::Display for WavSampleFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WavChannelCount {
    Mono,
    #[default]
    Stereo,
}

impl WavChannelCount {
    pub const ALL: [Self; 2] = [Self::Mono, Self::Stereo];

    pub const fn channels(self) -> u16 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }

    fn frame_sample(self, frame: &[f32; 2], channel: usize) -> f32 {
        match self {
            Self::Mono => frame[0] * 0.5 + frame[1] * 0.5,
            Self::Stereo => frame[channel],
        }
    }
}

impl std::fmt::Display for WavChannelCount {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Mono => "Mono",
            Self::Stereo => "Stereo",
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WavExportOptions {
    pub sample_format: WavSampleFormat,
    pub dither: bool,
}

#[derive(Debug)]
struct IntegerPcmWavExport {
    destination: PathBuf,
    temporary: PathBuf,
    file: Option<BufWriter<File>>,
    sample_rate: u32,
    channels: WavChannelCount,
    bits_per_sample: u16,
    data_bytes: u64,
    dither: Option<TpdfDither>,
}

/// Incremental stereo PCM24 WAV export with atomic publication on completion.
///
/// Audio is converted from interleaved f32 frames without buffering the rendered file. The final
/// path is created only by `finish`; dropping or cancelling an export removes its temporary file.
pub struct Pcm24WavExport {
    inner: IntegerPcmWavExport,
}

/// Incremental WAV PCM8 export using the unsigned sample representation required by RIFF/WAVE.
pub struct Pcm8WavExport {
    inner: IntegerPcmWavExport,
}

/// Incremental stereo PCM16 WAV export with optional TPDF dither.
pub struct Pcm16WavExport {
    inner: IntegerPcmWavExport,
}

/// Incremental WAV PCM32 export.
pub struct Pcm32WavExport {
    inner: IntegerPcmWavExport,
}

/// Incremental G.711 μ-law WAV export.
pub struct MuLaw8WavExport {
    destination: PathBuf,
    temporary: PathBuf,
    file: Option<BufWriter<File>>,
    sample_rate: u32,
    channels: WavChannelCount,
    data_bytes: u64,
}

/// Incremental block-based 4-bit IMA ADPCM WAV export.
pub struct ImaAdpcm4WavExport {
    inner: AdpcmWavExport,
}

/// Incremental block-based 2-bit cADPCM WAV export.
pub struct Cadpcm2WavExport {
    inner: AdpcmWavExport,
}

struct AdpcmWavExport {
    destination: PathBuf,
    temporary: PathBuf,
    file: Option<BufWriter<File>>,
    sample_rate: u32,
    channels: WavChannelCount,
    codec: AdpcmCodec,
    total_frames: u64,
    data_bytes: u64,
    pending_frames: Vec<[i16; 2]>,
    encoded_block: Vec<u8>,
}

#[derive(Clone, Copy)]
enum AdpcmCodec {
    Ima4,
    Cadpcm2,
}

impl AdpcmCodec {
    const fn samples_per_block(self) -> usize {
        match self {
            Self::Ima4 => IMA_ADPCM_SAMPLES_PER_BLOCK,
            Self::Cadpcm2 => CADPCM_SAMPLES_PER_BLOCK,
        }
    }

    const fn bits_per_sample(self) -> u16 {
        match self {
            Self::Ima4 => 4,
            Self::Cadpcm2 => 2,
        }
    }

    const fn format_tag(self) -> u16 {
        match self {
            Self::Ima4 => 17,
            Self::Cadpcm2 => 0xAC0C,
        }
    }
}

/// User-facing WAV export writer. Float output ignores the integer-only dither setting.
pub enum WavExport {
    Pcm8(Pcm8WavExport),
    Pcm16(Pcm16WavExport),
    Pcm24(Pcm24WavExport),
    Pcm32(Pcm32WavExport),
    Float32(Float32WavExport),
    Float64(Float64WavExport),
    MuLaw8(MuLaw8WavExport),
    ImaAdpcm4(ImaAdpcm4WavExport),
    Cadpcm2(Cadpcm2WavExport),
}

#[derive(Debug)]
pub enum WavExportError {
    Pcm8(IntegerPcmWavExportError),
    Pcm16(IntegerPcmWavExportError),
    Pcm24(IntegerPcmWavExportError),
    Pcm32(IntegerPcmWavExportError),
    Float32(Float32WavExportError),
    Float64(Float32WavExportError),
    MuLaw8(MuLaw8WavExportError),
    ImaAdpcm4(ImaAdpcmWavExportError),
    Cadpcm2(Cadpcm2WavExportError),
}

impl std::fmt::Display for WavExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pcm8(error) | Self::Pcm16(error) | Self::Pcm24(error) | Self::Pcm32(error) => {
                error.fmt(formatter)
            }
            Self::Float32(error) | Self::Float64(error) => error.fmt(formatter),
            Self::MuLaw8(error) => error.fmt(formatter),
            Self::ImaAdpcm4(error) | Self::Cadpcm2(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for WavExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pcm8(error) | Self::Pcm16(error) | Self::Pcm24(error) | Self::Pcm32(error) => {
                Some(error)
            }
            Self::Float32(error) | Self::Float64(error) => Some(error),
            Self::MuLaw8(error) => Some(error),
            Self::ImaAdpcm4(error) | Self::Cadpcm2(error) => Some(error),
        }
    }
}

/// Errors returned while creating or writing an integer PCM WAV export.
#[derive(Debug)]
pub enum IntegerPcmWavExportError {
    Io(io::Error),
    InvalidSampleRate,
    RiffSizeLimit,
}

pub type Pcm24WavExportError = IntegerPcmWavExportError;
pub type Pcm16WavExportError = IntegerPcmWavExportError;
pub type Pcm8WavExportError = IntegerPcmWavExportError;
pub type Pcm32WavExportError = IntegerPcmWavExportError;

impl std::fmt::Display for IntegerPcmWavExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "WAV export failed: {error}"),
            Self::InvalidSampleRate => formatter.write_str("WAV sample rate must be positive"),
            Self::RiffSizeLimit => formatter.write_str("WAV export exceeds the RIFF size limit"),
        }
    }
}

impl std::error::Error for IntegerPcmWavExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidSampleRate | Self::RiffSizeLimit => None,
        }
    }
}

impl From<io::Error> for IntegerPcmWavExportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl Pcm24WavExport {
    /// Begins an incremental stereo PCM24 WAV export beside its final destination.
    ///
    /// Existing destinations are rejected. Samples are clamped to [-1, 1] and rounded to the
    /// nearest signed 24-bit integer; no normalization or dither is applied.
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
    ) -> Result<Self, IntegerPcmWavExportError> {
        Self::create_with_channels(destination, sample_rate, WavChannelCount::Stereo)
    }

    fn create_with_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
    ) -> Result<Self, IntegerPcmWavExportError> {
        Ok(Self {
            inner: IntegerPcmWavExport::create(
                destination.as_ref(),
                sample_rate,
                24,
                channels,
                None,
            )?,
        })
    }

    fn create_with_dither_seed_and_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
        seed: u64,
    ) -> Result<Self, Pcm24WavExportError> {
        Ok(Self {
            inner: IntegerPcmWavExport::create(
                destination.as_ref(),
                sample_rate,
                24,
                channels,
                Some(seed),
            )?,
        })
    }

    /// Appends rendered interleaved stereo frames using bounded memory.
    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), Pcm24WavExportError> {
        self.inner.write_frames(frames)
    }

    /// Flushes the WAV header and atomically publishes the completed export.
    pub fn finish(self) -> Result<PathBuf, Pcm24WavExportError> {
        self.inner.finish()
    }
}

impl Pcm8WavExport {
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
    ) -> Result<Self, IntegerPcmWavExportError> {
        Self::create_with_channels(destination, sample_rate, WavChannelCount::Stereo)
    }

    fn create_with_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
    ) -> Result<Self, IntegerPcmWavExportError> {
        Ok(Self {
            inner: IntegerPcmWavExport::create(
                destination.as_ref(),
                sample_rate,
                8,
                channels,
                None,
            )?,
        })
    }

    fn create_with_dither_seed_and_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
        seed: u64,
    ) -> Result<Self, IntegerPcmWavExportError> {
        Ok(Self {
            inner: IntegerPcmWavExport::create(
                destination.as_ref(),
                sample_rate,
                8,
                channels,
                Some(seed),
            )?,
        })
    }

    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), IntegerPcmWavExportError> {
        self.inner.write_frames(frames)
    }

    pub fn finish(self) -> Result<PathBuf, IntegerPcmWavExportError> {
        self.inner.finish()
    }
}

impl Pcm16WavExport {
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
    ) -> Result<Self, Pcm24WavExportError> {
        Self::create_with_channels(destination, sample_rate, WavChannelCount::Stereo)
    }

    fn create_with_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
    ) -> Result<Self, Pcm24WavExportError> {
        Ok(Self {
            inner: IntegerPcmWavExport::create(
                destination.as_ref(),
                sample_rate,
                16,
                channels,
                None,
            )?,
        })
    }

    fn create_with_dither_seed_and_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
        seed: u64,
    ) -> Result<Self, Pcm24WavExportError> {
        Ok(Self {
            inner: IntegerPcmWavExport::create(
                destination.as_ref(),
                sample_rate,
                16,
                channels,
                Some(seed),
            )?,
        })
    }

    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), Pcm24WavExportError> {
        self.inner.write_frames(frames)
    }

    pub fn finish(self) -> Result<PathBuf, Pcm24WavExportError> {
        self.inner.finish()
    }
}

impl Pcm32WavExport {
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
    ) -> Result<Self, IntegerPcmWavExportError> {
        Self::create_with_channels(destination, sample_rate, WavChannelCount::Stereo)
    }

    fn create_with_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
    ) -> Result<Self, IntegerPcmWavExportError> {
        Ok(Self {
            inner: IntegerPcmWavExport::create(
                destination.as_ref(),
                sample_rate,
                32,
                channels,
                None,
            )?,
        })
    }

    fn create_with_dither_seed_and_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
        seed: u64,
    ) -> Result<Self, IntegerPcmWavExportError> {
        Ok(Self {
            inner: IntegerPcmWavExport::create(
                destination.as_ref(),
                sample_rate,
                32,
                channels,
                Some(seed),
            )?,
        })
    }

    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), IntegerPcmWavExportError> {
        self.inner.write_frames(frames)
    }

    pub fn finish(self) -> Result<PathBuf, IntegerPcmWavExportError> {
        self.inner.finish()
    }
}

/// Errors returned while creating or writing a G.711 μ-law WAV export.
#[derive(Debug)]
pub enum MuLaw8WavExportError {
    Io(io::Error),
    InvalidSampleRate,
    RiffSizeLimit,
    NonFiniteSample,
}

impl std::fmt::Display for MuLaw8WavExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "u-Law WAV export failed: {error}"),
            Self::InvalidSampleRate => formatter.write_str("WAV sample rate must be positive"),
            Self::RiffSizeLimit => formatter.write_str("WAV export exceeds the RIFF size limit"),
            Self::NonFiniteSample => formatter.write_str("u-Law WAV contains a non-finite sample"),
        }
    }
}

impl std::error::Error for MuLaw8WavExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidSampleRate | Self::RiffSizeLimit | Self::NonFiniteSample => None,
        }
    }
}

impl From<io::Error> for MuLaw8WavExportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl MuLaw8WavExport {
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
    ) -> Result<Self, MuLaw8WavExportError> {
        Self::create_with_channels(destination, sample_rate, WavChannelCount::Stereo)
    }

    fn create_with_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
    ) -> Result<Self, MuLaw8WavExportError> {
        if sample_rate == 0 {
            return Err(MuLaw8WavExportError::InvalidSampleRate);
        }
        let destination = destination.as_ref().to_path_buf();
        if destination.exists() {
            return Err(
                io::Error::new(io::ErrorKind::AlreadyExists, "destination already exists").into(),
            );
        }
        let (temporary, mut file) = create_temporary_sibling(&destination)?;
        let mut export = Self {
            destination,
            temporary,
            file: None,
            sample_rate,
            channels,
            data_bytes: 0,
        };
        write_mulaw_header(&mut file, sample_rate, channels, 0)?;
        export.file = Some(BufWriter::new(file));
        Ok(export)
    }

    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), MuLaw8WavExportError> {
        let channel_count = self.channels.channels();
        let bytes_per_frame = u64::from(channel_count);
        let added_bytes = (frames.len() as u64)
            .checked_mul(bytes_per_frame)
            .ok_or(MuLaw8WavExportError::RiffSizeLimit)?;
        let data_bytes = self
            .data_bytes
            .checked_add(added_bytes)
            .filter(|bytes| {
                bytes
                    .checked_add(38)
                    .and_then(|size| size.checked_add(bytes % 2))
                    .is_some_and(|riff_size| riff_size <= u64::from(u32::MAX))
            })
            .ok_or(MuLaw8WavExportError::RiffSizeLimit)?;
        if frames.iter().flatten().any(|sample| !sample.is_finite()) {
            return Err(MuLaw8WavExportError::NonFiniteSample);
        }
        let file = self
            .file
            .as_mut()
            .expect("unfinished export retains its file");
        for frame in frames {
            for channel in 0..channel_count as usize {
                file.write_all(&[encode_mulaw_sample(
                    self.channels.frame_sample(frame, channel),
                )])?;
            }
        }
        self.data_bytes = data_bytes;
        Ok(())
    }

    pub fn finish(mut self) -> Result<PathBuf, MuLaw8WavExportError> {
        let mut file = self
            .file
            .take()
            .expect("unfinished export retains its file");
        file.flush()?;
        if self.data_bytes & 1 != 0 {
            file.seek(SeekFrom::End(0))?;
            file.write_all(&[0])?;
        }
        file.seek(SeekFrom::Start(0))?;
        write_mulaw_header(&mut file, self.sample_rate, self.channels, self.data_bytes)?;
        file.flush()?;
        drop(file);
        fs::hard_link(&self.temporary, &self.destination)?;
        let _ = fs::remove_file(&self.temporary);
        Ok(self.destination.clone())
    }
}

impl Drop for MuLaw8WavExport {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.temporary);
    }
}

/// Errors returned while creating or writing an ADPCM WAV export.
#[derive(Debug)]
pub enum AdpcmWavExportError {
    Io(io::Error),
    InvalidSampleRate,
    RiffSizeLimit,
    NonFiniteSample,
}

pub type ImaAdpcmWavExportError = AdpcmWavExportError;
pub type Cadpcm2WavExportError = AdpcmWavExportError;

impl std::fmt::Display for AdpcmWavExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "ADPCM WAV export failed: {error}"),
            Self::InvalidSampleRate => formatter.write_str("WAV sample rate must be positive"),
            Self::RiffSizeLimit => formatter.write_str("WAV export exceeds the RIFF size limit"),
            Self::NonFiniteSample => formatter.write_str("ADPCM WAV contains a non-finite sample"),
        }
    }
}

impl std::error::Error for AdpcmWavExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidSampleRate | Self::RiffSizeLimit | Self::NonFiniteSample => None,
        }
    }
}

impl From<io::Error> for AdpcmWavExportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl ImaAdpcm4WavExport {
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
    ) -> Result<Self, ImaAdpcmWavExportError> {
        Self::create_with_channels(destination, sample_rate, WavChannelCount::Stereo)
    }

    fn create_with_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
    ) -> Result<Self, ImaAdpcmWavExportError> {
        Ok(Self {
            inner: AdpcmWavExport::create(destination, sample_rate, channels, AdpcmCodec::Ima4)?,
        })
    }

    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), ImaAdpcmWavExportError> {
        self.inner.write_frames(frames)
    }

    pub fn finish(self) -> Result<PathBuf, ImaAdpcmWavExportError> {
        self.inner.finish()
    }
}

impl Cadpcm2WavExport {
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
    ) -> Result<Self, Cadpcm2WavExportError> {
        Self::create_with_channels(destination, sample_rate, WavChannelCount::Stereo)
    }

    fn create_with_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
    ) -> Result<Self, Cadpcm2WavExportError> {
        Ok(Self {
            inner: AdpcmWavExport::create(destination, sample_rate, channels, AdpcmCodec::Cadpcm2)?,
        })
    }

    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), Cadpcm2WavExportError> {
        self.inner.write_frames(frames)
    }

    pub fn finish(self) -> Result<PathBuf, Cadpcm2WavExportError> {
        self.inner.finish()
    }
}

impl AdpcmWavExport {
    fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
        codec: AdpcmCodec,
    ) -> Result<Self, AdpcmWavExportError> {
        if sample_rate == 0 {
            return Err(AdpcmWavExportError::InvalidSampleRate);
        }
        let destination = destination.as_ref().to_path_buf();
        if destination.exists() {
            return Err(
                io::Error::new(io::ErrorKind::AlreadyExists, "destination already exists").into(),
            );
        }
        let (temporary, mut file) = create_temporary_sibling(&destination)?;
        let mut export = Self {
            destination,
            temporary,
            file: None,
            sample_rate,
            channels,
            codec,
            total_frames: 0,
            data_bytes: 0,
            pending_frames: Vec::with_capacity(codec.samples_per_block()),
            encoded_block: vec![0; adpcm_block_align(channels)],
        };
        write_adpcm_header(&mut file, sample_rate, channels, codec, 0, 0)?;
        export.file = Some(BufWriter::new(file));
        Ok(export)
    }

    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), AdpcmWavExportError> {
        if frames.iter().flatten().any(|sample| !sample.is_finite()) {
            return Err(AdpcmWavExportError::NonFiniteSample);
        }
        let added_frames =
            u64::try_from(frames.len()).map_err(|_| AdpcmWavExportError::RiffSizeLimit)?;
        let total_frames = self
            .total_frames
            .checked_add(added_frames)
            .filter(|frames| *frames <= u64::from(u32::MAX))
            .ok_or(AdpcmWavExportError::RiffSizeLimit)?;
        adpcm_data_bytes(total_frames, self.channels, self.codec)
            .filter(|bytes| {
                bytes
                    .checked_add(ADPCM_RIFF_OVERHEAD)
                    .is_some_and(|size| size <= u64::from(u32::MAX))
            })
            .ok_or(AdpcmWavExportError::RiffSizeLimit)?;

        for frame in frames {
            let left = i16_from_float(self.channels.frame_sample(frame, 0));
            let right = if self.channels == WavChannelCount::Stereo {
                i16_from_float(frame[1])
            } else {
                left
            };
            self.pending_frames.push([left, right]);
            if self.pending_frames.len() == self.codec.samples_per_block() {
                self.write_pending_block()?;
            }
        }
        self.total_frames = total_frames;
        Ok(())
    }

    fn write_pending_block(&mut self) -> Result<(), AdpcmWavExportError> {
        debug_assert_eq!(self.pending_frames.len(), self.codec.samples_per_block());
        encode_adpcm_block(
            &self.pending_frames,
            self.channels,
            self.codec,
            &mut self.encoded_block,
        );
        self.file
            .as_mut()
            .expect("unfinished export retains its file")
            .write_all(&self.encoded_block)?;
        self.data_bytes = self
            .data_bytes
            .checked_add(self.encoded_block.len() as u64)
            .ok_or(AdpcmWavExportError::RiffSizeLimit)?;
        self.pending_frames.clear();
        Ok(())
    }

    fn finish(mut self) -> Result<PathBuf, AdpcmWavExportError> {
        if !self.pending_frames.is_empty() {
            self.pending_frames
                .resize(self.codec.samples_per_block(), [0, 0]);
            self.write_pending_block()?;
        }
        let mut file = self
            .file
            .take()
            .expect("unfinished export retains its file");
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        write_adpcm_header(
            &mut file,
            self.sample_rate,
            self.channels,
            self.codec,
            self.data_bytes,
            self.total_frames,
        )?;
        file.flush()?;
        drop(file);
        fs::hard_link(&self.temporary, &self.destination)?;
        let _ = fs::remove_file(&self.temporary);
        Ok(self.destination.clone())
    }
}

impl Drop for AdpcmWavExport {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.temporary);
    }
}

impl WavExport {
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        options: WavExportOptions,
    ) -> Result<Self, WavExportError> {
        Self::create_with_channels(destination, sample_rate, options, WavChannelCount::Stereo)
    }

    pub fn create_with_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        options: WavExportOptions,
        channels: WavChannelCount,
    ) -> Result<Self, WavExportError> {
        let seed = options.dither.then(random_dither_seed);
        let export = match options.sample_format {
            WavSampleFormat::Pcm8 => match seed {
                Some(seed) => Pcm8WavExport::create_with_dither_seed_and_channels(
                    destination,
                    sample_rate,
                    channels,
                    seed,
                )
                .map(Self::Pcm8),
                None => Pcm8WavExport::create_with_channels(destination, sample_rate, channels)
                    .map(Self::Pcm8),
            }
            .map_err(WavExportError::Pcm8)?,
            WavSampleFormat::Pcm16 => match seed {
                Some(seed) => Pcm16WavExport::create_with_dither_seed_and_channels(
                    destination,
                    sample_rate,
                    channels,
                    seed,
                )
                .map(Self::Pcm16),
                None => Pcm16WavExport::create_with_channels(destination, sample_rate, channels)
                    .map(Self::Pcm16),
            }
            .map_err(WavExportError::Pcm16)?,
            WavSampleFormat::Pcm24 => match seed {
                Some(seed) => Pcm24WavExport::create_with_dither_seed_and_channels(
                    destination,
                    sample_rate,
                    channels,
                    seed,
                )
                .map(Self::Pcm24),
                None => Pcm24WavExport::create_with_channels(destination, sample_rate, channels)
                    .map(Self::Pcm24),
            }
            .map_err(WavExportError::Pcm24)?,
            WavSampleFormat::Pcm32 => match seed {
                Some(seed) => Pcm32WavExport::create_with_dither_seed_and_channels(
                    destination,
                    sample_rate,
                    channels,
                    seed,
                )
                .map(Self::Pcm32),
                None => Pcm32WavExport::create_with_channels(destination, sample_rate, channels)
                    .map(Self::Pcm32),
            }
            .map_err(WavExportError::Pcm32)?,
            WavSampleFormat::Float32 => {
                Float32WavExport::create_with_channels(destination, sample_rate, channels)
                    .map(Self::Float32)
                    .map_err(WavExportError::Float32)?
            }
            WavSampleFormat::Float64 => {
                Float64WavExport::create_with_channels(destination, sample_rate, channels)
                    .map(Self::Float64)
                    .map_err(WavExportError::Float64)?
            }
            WavSampleFormat::MuLaw8 => {
                MuLaw8WavExport::create_with_channels(destination, sample_rate, channels)
                    .map(Self::MuLaw8)
                    .map_err(WavExportError::MuLaw8)?
            }
            WavSampleFormat::ImaAdpcm4 => {
                ImaAdpcm4WavExport::create_with_channels(destination, sample_rate, channels)
                    .map(Self::ImaAdpcm4)
                    .map_err(WavExportError::ImaAdpcm4)?
            }
            WavSampleFormat::Cadpcm2 => {
                Cadpcm2WavExport::create_with_channels(destination, sample_rate, channels)
                    .map(Self::Cadpcm2)
                    .map_err(WavExportError::Cadpcm2)?
            }
        };
        Ok(export)
    }

    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), WavExportError> {
        match self {
            Self::Pcm8(export) => export.write_frames(frames).map_err(WavExportError::Pcm8),
            Self::Pcm16(export) => export.write_frames(frames).map_err(WavExportError::Pcm16),
            Self::Pcm24(export) => export.write_frames(frames).map_err(WavExportError::Pcm24),
            Self::Pcm32(export) => export.write_frames(frames).map_err(WavExportError::Pcm32),
            Self::Float32(export) => export.write_frames(frames).map_err(WavExportError::Float32),
            Self::Float64(export) => export.write_frames(frames).map_err(WavExportError::Float64),
            Self::MuLaw8(export) => export.write_frames(frames).map_err(WavExportError::MuLaw8),
            Self::ImaAdpcm4(export) => export
                .write_frames(frames)
                .map_err(WavExportError::ImaAdpcm4),
            Self::Cadpcm2(export) => export.write_frames(frames).map_err(WavExportError::Cadpcm2),
        }
    }

    pub fn finish(self) -> Result<PathBuf, WavExportError> {
        match self {
            Self::Pcm8(export) => export.finish().map_err(WavExportError::Pcm8),
            Self::Pcm16(export) => export.finish().map_err(WavExportError::Pcm16),
            Self::Pcm24(export) => export.finish().map_err(WavExportError::Pcm24),
            Self::Pcm32(export) => export.finish().map_err(WavExportError::Pcm32),
            Self::Float32(export) => export.finish().map_err(WavExportError::Float32),
            Self::Float64(export) => export.finish().map_err(WavExportError::Float64),
            Self::MuLaw8(export) => export.finish().map_err(WavExportError::MuLaw8),
            Self::ImaAdpcm4(export) => export.finish().map_err(WavExportError::ImaAdpcm4),
            Self::Cadpcm2(export) => export.finish().map_err(WavExportError::Cadpcm2),
        }
    }
}

impl IntegerPcmWavExport {
    fn create(
        destination: &Path,
        sample_rate: u32,
        bits_per_sample: u16,
        channels: WavChannelCount,
        seed: Option<u64>,
    ) -> Result<Self, Pcm24WavExportError> {
        if sample_rate == 0 {
            return Err(Pcm24WavExportError::InvalidSampleRate);
        }
        let destination = destination.to_path_buf();
        if destination.exists() {
            return Err(
                io::Error::new(io::ErrorKind::AlreadyExists, "destination already exists").into(),
            );
        }
        let (temporary, mut file) = create_temporary_sibling(&destination)?;
        let mut export = Self {
            destination,
            temporary,
            file: None,
            sample_rate,
            channels,
            bits_per_sample,
            data_bytes: 0,
            dither: seed.map(TpdfDither::new),
        };
        write_integer_header(&mut file, sample_rate, bits_per_sample, channels, 0)?;
        export.file = Some(BufWriter::new(file));
        Ok(export)
    }

    /// Appends rendered interleaved stereo frames using bounded memory.
    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), IntegerPcmWavExportError> {
        let bytes_per_sample = u64::from(self.bits_per_sample / 8);
        let channel_count = self.channels.channels();
        let bytes_per_frame = u64::from(channel_count) * bytes_per_sample;
        let added_bytes = (frames.len() as u64)
            .checked_mul(bytes_per_frame)
            .ok_or(Pcm24WavExportError::RiffSizeLimit)?;
        let data_bytes = self
            .data_bytes
            .checked_add(added_bytes)
            .filter(|bytes| *bytes <= MAX_RIFF_DATA_BYTES)
            .ok_or(Pcm24WavExportError::RiffSizeLimit)?;
        let file = self
            .file
            .as_mut()
            .expect("unfinished export retains its file");
        let scale = 2_f64.powi(i32::from(self.bits_per_sample - 1));
        let min = -scale;
        let max = scale - 1.0;
        for frame in frames {
            for channel in 0..channel_count as usize {
                let sample = self.channels.frame_sample(frame, channel);
                let dither = self.dither.as_mut().map_or(0.0, TpdfDither::next_lsb);
                let quantized = (sample.clamp(-1.0, 1.0) as f64 * scale + dither)
                    .round()
                    .clamp(min, max) as i32;
                if self.bits_per_sample == 8 {
                    file.write_all(&[(quantized + 128) as u8])?;
                } else {
                    let bytes = quantized.to_le_bytes();
                    file.write_all(&bytes[..bytes_per_sample as usize])?;
                }
            }
        }
        self.data_bytes = data_bytes;
        Ok(())
    }

    /// Flushes the WAV header and atomically publishes the completed export.
    fn finish(mut self) -> Result<PathBuf, IntegerPcmWavExportError> {
        let mut file = self
            .file
            .take()
            .expect("unfinished export retains its file");
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        write_integer_header(
            &mut file,
            self.sample_rate,
            self.bits_per_sample,
            self.channels,
            self.data_bytes,
        )?;
        file.flush()?;
        drop(file);

        // A same-directory hard link publishes the completed file atomically and fails rather
        // than replacing a destination created while the render was in progress.
        fs::hard_link(&self.temporary, &self.destination)?;
        let _ = fs::remove_file(&self.temporary);
        Ok(self.destination.clone())
    }
}

impl Drop for IntegerPcmWavExport {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.temporary);
    }
}

/// Incremental IEEE float32 WAV export for internal renders that must preserve headroom.
pub struct Float32WavExport {
    inner: FloatWavExport,
}

/// Incremental IEEE float64 WAV export.
pub struct Float64WavExport {
    inner: FloatWavExport,
}

struct FloatWavExport {
    destination: PathBuf,
    temporary: PathBuf,
    file: Option<BufWriter<File>>,
    sample_rate: u32,
    channels: WavChannelCount,
    bits_per_sample: u16,
    data_bytes: u64,
}

/// Errors returned while creating or writing an IEEE float32 WAV export.
#[derive(Debug)]
pub enum Float32WavExportError {
    Io(io::Error),
    InvalidSampleRate,
    RiffSizeLimit,
    NonFiniteSample,
}

pub type Float64WavExportError = Float32WavExportError;

impl std::fmt::Display for Float32WavExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "float WAV export failed: {error}"),
            Self::InvalidSampleRate => formatter.write_str("WAV sample rate must be positive"),
            Self::RiffSizeLimit => formatter.write_str("WAV export exceeds the RIFF size limit"),
            Self::NonFiniteSample => formatter.write_str("float WAV contains a non-finite sample"),
        }
    }
}

impl std::error::Error for Float32WavExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidSampleRate | Self::RiffSizeLimit | Self::NonFiniteSample => None,
        }
    }
}

impl From<io::Error> for Float32WavExportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl Float32WavExport {
    /// Begins an incremental stereo float32 WAV export beside its final destination.
    ///
    /// Samples are written without normalization or clipping, retaining values outside [-1, 1].
    /// Existing destinations are rejected and the final path is published only by `finish`.
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
    ) -> Result<Self, Float32WavExportError> {
        Self::create_with_channels(destination, sample_rate, WavChannelCount::Stereo)
    }

    pub fn create_with_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
    ) -> Result<Self, Float32WavExportError> {
        Ok(Self {
            inner: FloatWavExport::create(destination.as_ref(), sample_rate, channels, 32)?,
        })
    }

    /// Appends finite interleaved stereo frames without changing their levels.
    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), Float32WavExportError> {
        self.inner.write_frames(frames)
    }

    /// Flushes the WAV header and atomically publishes the completed export.
    pub fn finish(self) -> Result<PathBuf, Float32WavExportError> {
        self.inner.finish()
    }
}

impl Float64WavExport {
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
    ) -> Result<Self, Float64WavExportError> {
        Self::create_with_channels(destination, sample_rate, WavChannelCount::Stereo)
    }

    pub fn create_with_channels(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        channels: WavChannelCount,
    ) -> Result<Self, Float64WavExportError> {
        Ok(Self {
            inner: FloatWavExport::create(destination.as_ref(), sample_rate, channels, 64)?,
        })
    }

    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), Float64WavExportError> {
        self.inner.write_frames(frames)
    }

    pub fn finish(self) -> Result<PathBuf, Float64WavExportError> {
        self.inner.finish()
    }
}

impl FloatWavExport {
    fn create(
        destination: &Path,
        sample_rate: u32,
        channels: WavChannelCount,
        bits_per_sample: u16,
    ) -> Result<Self, Float32WavExportError> {
        if sample_rate == 0 {
            return Err(Float32WavExportError::InvalidSampleRate);
        }
        let destination = destination.to_path_buf();
        if destination.exists() {
            return Err(
                io::Error::new(io::ErrorKind::AlreadyExists, "destination already exists").into(),
            );
        }
        let (temporary, mut file) = create_temporary_sibling(&destination)?;
        let mut export = Self {
            destination,
            temporary,
            file: None,
            sample_rate,
            channels,
            bits_per_sample,
            data_bytes: 0,
        };
        write_float_header(&mut file, sample_rate, channels, bits_per_sample, 0)?;
        export.file = Some(BufWriter::new(file));
        Ok(export)
    }

    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), Float32WavExportError> {
        let bytes_per_sample = u64::from(self.bits_per_sample / 8);
        let bytes_per_frame = u64::from(self.channels.channels()) * bytes_per_sample;
        let max_data_bytes = u64::from(u32::MAX) - if self.bits_per_sample == 64 { 38 } else { 36 };
        let added_bytes = (frames.len() as u64)
            .checked_mul(bytes_per_frame)
            .ok_or(Float32WavExportError::RiffSizeLimit)?;
        let data_bytes = self
            .data_bytes
            .checked_add(added_bytes)
            .filter(|bytes| *bytes <= max_data_bytes)
            .ok_or(Float32WavExportError::RiffSizeLimit)?;
        if frames.iter().flatten().any(|sample| !sample.is_finite()) {
            return Err(Float32WavExportError::NonFiniteSample);
        }
        let file = self
            .file
            .as_mut()
            .expect("unfinished export retains its file");
        for frame in frames {
            for channel in 0..self.channels.channels() as usize {
                let sample = self.channels.frame_sample(frame, channel);
                if self.bits_per_sample == 32 {
                    file.write_all(&sample.to_le_bytes())?;
                } else {
                    file.write_all(&f64::from(sample).to_le_bytes())?;
                }
            }
        }
        self.data_bytes = data_bytes;
        Ok(())
    }

    fn finish(mut self) -> Result<PathBuf, Float32WavExportError> {
        let mut file = self
            .file
            .take()
            .expect("unfinished export retains its file");
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        write_float_header(
            &mut file,
            self.sample_rate,
            self.channels,
            self.bits_per_sample,
            self.data_bytes,
        )?;
        file.flush()?;
        drop(file);
        fs::hard_link(&self.temporary, &self.destination)?;
        let _ = fs::remove_file(&self.temporary);
        Ok(self.destination.clone())
    }
}

impl Drop for FloatWavExport {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.temporary);
    }
}

fn create_temporary_sibling(destination: &Path) -> io::Result<(PathBuf, File)> {
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = destination.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "destination has no file name")
    })?;
    for _ in 0..16 {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = name.to_os_string();
        temporary_name.push(format!(".aaadaw-{}-{id}.partial", std::process::id()));
        let temporary = parent.join(temporary_name);
        match OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a unique temporary WAV path",
    ))
}

#[derive(Debug)]
struct TpdfDither(u64);

impl TpdfDither {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_lsb(&mut self) -> f64 {
        self.next_unit() - self.next_unit()
    }

    fn next_unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
        (value >> 11) as f64 * (1.0 / ((1_u64 << 53) as f64))
    }
}

fn random_dither_seed() -> u64 {
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64);
    clock ^ NEXT_DITHER_SEED.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed)
}

fn write_integer_header(
    file: &mut impl Write,
    sample_rate: u32,
    bits_per_sample: u16,
    channels: WavChannelCount,
    data_bytes: u64,
) -> io::Result<()> {
    let data_bytes = u32::try_from(data_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "WAV data exceeds RIFF limit"))?;
    let bytes_per_sample = bits_per_sample / 8;
    let channel_count = channels.channels();
    let block_align = channel_count * bytes_per_sample;
    let byte_rate = sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV byte rate overflow"))?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36_u32 + data_bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&channel_count.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&bits_per_sample.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())
}

fn write_float_header(
    file: &mut impl Write,
    sample_rate: u32,
    channels: WavChannelCount,
    bits_per_sample: u16,
    data_bytes: u64,
) -> io::Result<()> {
    let fmt_chunk_size = if bits_per_sample == 64 { 18_u32 } else { 16 };
    let riff_overhead = 20 + fmt_chunk_size;
    let data_bytes = u32::try_from(data_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "WAV data exceeds RIFF limit"))?;
    let channel_count = channels.channels();
    let byte_rate = sample_rate
        .checked_mul(u32::from(channel_count) * u32::from(bits_per_sample / 8))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV byte rate overflow"))?;
    file.write_all(b"RIFF")?;
    file.write_all(&(riff_overhead + data_bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&fmt_chunk_size.to_le_bytes())?;
    file.write_all(&3_u16.to_le_bytes())?;
    file.write_all(&channel_count.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&(channel_count * (bits_per_sample / 8)).to_le_bytes())?;
    file.write_all(&bits_per_sample.to_le_bytes())?;
    if fmt_chunk_size == 18 {
        file.write_all(&0_u16.to_le_bytes())?;
    }
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())
}

fn write_mulaw_header(
    file: &mut impl Write,
    sample_rate: u32,
    channels: WavChannelCount,
    data_bytes: u64,
) -> io::Result<()> {
    let data_bytes = u32::try_from(data_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "WAV data exceeds RIFF limit"))?;
    let channel_count = channels.channels();
    let byte_rate = sample_rate
        .checked_mul(u32::from(channel_count))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV byte rate overflow"))?;
    file.write_all(b"RIFF")?;
    let riff_size = 38_u32
        .checked_add(data_bytes)
        .and_then(|size| size.checked_add(data_bytes % 2))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV RIFF size overflow"))?;
    file.write_all(&riff_size.to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&18_u32.to_le_bytes())?;
    file.write_all(&7_u16.to_le_bytes())?;
    file.write_all(&channel_count.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&channel_count.to_le_bytes())?;
    file.write_all(&8_u16.to_le_bytes())?;
    file.write_all(&0_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())
}

fn write_adpcm_header(
    file: &mut impl Write,
    sample_rate: u32,
    channels: WavChannelCount,
    codec: AdpcmCodec,
    data_bytes: u64,
    frame_count: u64,
) -> io::Result<()> {
    let data_bytes = u32::try_from(data_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "WAV data exceeds RIFF limit"))?;
    let frame_count = u32::try_from(frame_count).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "WAV frame count exceeds limit")
    })?;
    let channel_count = channels.channels();
    let block_align = u16::try_from(adpcm_block_align(channels))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "ADPCM block size overflow"))?;
    let byte_rate = sample_rate
        .checked_mul(u32::from(channel_count))
        .and_then(|rate| rate.checked_mul(u32::from(codec.bits_per_sample())))
        .map(|rate| rate / 8)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV byte rate overflow"))?;
    let riff_size = u32::try_from(ADPCM_RIFF_OVERHEAD + u64::from(data_bytes))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "WAV RIFF size overflow"))?;

    file.write_all(b"RIFF")?;
    file.write_all(&riff_size.to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&20_u32.to_le_bytes())?;
    file.write_all(&codec.format_tag().to_le_bytes())?;
    file.write_all(&channel_count.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&codec.bits_per_sample().to_le_bytes())?;
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&(codec.samples_per_block() as u16).to_le_bytes())?;
    file.write_all(b"fact")?;
    file.write_all(&4_u32.to_le_bytes())?;
    file.write_all(&frame_count.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())
}

fn adpcm_block_align(channels: WavChannelCount) -> usize {
    IMA_ADPCM_BYTES_PER_CHANNEL_BLOCK * channels.channels() as usize
}

fn adpcm_data_bytes(frame_count: u64, channels: WavChannelCount, codec: AdpcmCodec) -> Option<u64> {
    let frames_per_block = codec.samples_per_block() as u64;
    let block_count = frame_count.checked_add(frames_per_block - 1)? / frames_per_block;
    block_count.checked_mul(adpcm_block_align(channels) as u64)
}

fn i16_from_float(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32_768.0)
        .round()
        .clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

#[derive(Clone, Copy)]
struct ImaAdpcmState {
    predictor: i32,
    step_index: usize,
}

const IMA_ADPCM_STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const IMA_ADPCM_INDEX_TABLE: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

fn encode_ima_adpcm_block(frames: &[[i16; 2]], channels: WavChannelCount, output: &mut [u8]) {
    debug_assert_eq!(frames.len(), IMA_ADPCM_SAMPLES_PER_BLOCK);
    debug_assert_eq!(output.len(), adpcm_block_align(channels));
    output.fill(0);
    let channel_count = channels.channels() as usize;
    let mut states = [ImaAdpcmState {
        predictor: 0,
        step_index: 0,
    }; 2];
    for channel in 0..channel_count {
        let predictor = i32::from(frames[0][channel]);
        states[channel].predictor = predictor;
        let header_offset = channel * 4;
        output[header_offset..header_offset + 2].copy_from_slice(&(predictor as i16).to_le_bytes());
        output[header_offset + 2] = 0;
    }

    let header_bytes = channel_count * 4;
    for group in 0..((IMA_ADPCM_SAMPLES_PER_BLOCK - 1) / 8) {
        for channel in 0..channel_count {
            let channel_group_offset = header_bytes + group * channel_count * 4 + channel * 4;
            for sample_in_group in 0..8 {
                let frame_index = 1 + group * 8 + sample_in_group;
                let nibble =
                    encode_ima_adpcm_sample(frames[frame_index][channel], &mut states[channel]);
                let byte_offset = channel_group_offset + sample_in_group / 2;
                output[byte_offset] |= nibble << ((sample_in_group & 1) * 4);
            }
        }
    }
}

fn encode_ima_adpcm_sample(sample: i16, state: &mut ImaAdpcmState) -> u8 {
    let step = IMA_ADPCM_STEP_TABLE[state.step_index];
    let mut difference = i32::from(sample) - state.predictor;
    let mut nibble = 0_u8;
    if difference < 0 {
        nibble = 8;
        difference = -difference;
    }

    let mut reconstructed_difference = step >> 3;
    if difference > step {
        nibble |= 4;
        difference -= step;
        reconstructed_difference += step;
    }
    if difference > step >> 1 {
        nibble |= 2;
        difference -= step >> 1;
        reconstructed_difference += step >> 1;
    }
    if difference > step >> 2 {
        nibble |= 1;
        reconstructed_difference += step >> 2;
    }

    if nibble & 8 != 0 {
        state.predictor -= reconstructed_difference;
    } else {
        state.predictor += reconstructed_difference;
    }
    state.predictor = state
        .predictor
        .clamp(i32::from(i16::MIN), i32::from(i16::MAX));
    let next_index = state.step_index as i32 + IMA_ADPCM_INDEX_TABLE[usize::from(nibble & 7)];
    state.step_index = next_index.clamp(0, IMA_ADPCM_STEP_TABLE.len() as i32 - 1) as usize;
    nibble
}

fn encode_cadpcm2_block(frames: &[[i16; 2]], channels: WavChannelCount, output: &mut [u8]) {
    debug_assert_eq!(frames.len(), CADPCM_SAMPLES_PER_BLOCK);
    debug_assert_eq!(output.len(), adpcm_block_align(channels));
    output.fill(0);
    let channel_count = channels.channels() as usize;
    let mut states = [ImaAdpcmState {
        predictor: 0,
        step_index: 0,
    }; 2];
    for channel in 0..channel_count {
        let predictor = i32::from(frames[0][channel]);
        states[channel].predictor = predictor;
        let header_offset = channel * 4;
        output[header_offset..header_offset + 2].copy_from_slice(&(predictor as i16).to_le_bytes());
        output[header_offset + 2] = 0;
        output[header_offset + 3] = 0;
    }

    let header_bytes = channel_count * 4;
    for group in 0..((CADPCM_SAMPLES_PER_BLOCK - 1) / 16) {
        for channel in 0..channel_count {
            let channel_group_offset = header_bytes + group * channel_count * 4 + channel * 4;
            for sample_in_group in 0..16 {
                let frame_index = 1 + group * 16 + sample_in_group;
                let code =
                    encode_cadpcm2_sample(frames[frame_index][channel], &mut states[channel]);
                let byte_offset = channel_group_offset + sample_in_group / 4;
                output[byte_offset] |= code << ((sample_in_group & 3) * 2);
            }
        }
    }
}

fn encode_cadpcm2_sample(sample: i16, state: &mut ImaAdpcmState) -> u8 {
    let step = IMA_ADPCM_STEP_TABLE[state.step_index];
    let mut difference = i32::from(sample) - state.predictor;
    let mut code = 0_u8;
    if difference < 0 {
        code = 2;
        difference = -difference;
    }
    if difference > step {
        code |= 1;
    }

    let mut reconstructed_difference = step >> 3;
    if code & 1 != 0 {
        reconstructed_difference += step;
    }
    if code & 2 != 0 {
        state.predictor -= reconstructed_difference;
    } else {
        state.predictor += reconstructed_difference;
    }
    state.predictor = state
        .predictor
        .clamp(i32::from(i16::MIN), i32::from(i16::MAX));
    let index_delta = IMA_ADPCM_INDEX_TABLE[usize::from((code << 2) & 7)];
    let next_index = state.step_index as i32 + index_delta;
    state.step_index = next_index.clamp(0, IMA_ADPCM_STEP_TABLE.len() as i32 - 1) as usize;
    code
}

fn encode_adpcm_block(
    frames: &[[i16; 2]],
    channels: WavChannelCount,
    codec: AdpcmCodec,
    output: &mut [u8],
) {
    match codec {
        AdpcmCodec::Ima4 => encode_ima_adpcm_block(frames, channels, output),
        AdpcmCodec::Cadpcm2 => encode_cadpcm2_block(frames, channels, output),
    }
}

fn encode_mulaw_sample(sample: f32) -> u8 {
    const SEGMENT_ENDS: [i32; 8] = [0x3f, 0x7f, 0xff, 0x1ff, 0x3ff, 0x7ff, 0xfff, 0x1fff];
    let pcm = (sample.clamp(-1.0, 1.0) * 32_768.0).round() as i32;
    let pcm = pcm >> 2;
    let (mask, magnitude) = if pcm < 0 {
        (0x7f_u8, -pcm)
    } else {
        (0xff_u8, pcm)
    };
    let magnitude = (magnitude + 33).min(0x1fff);
    let segment = SEGMENT_ENDS
        .iter()
        .position(|segment_end| magnitude <= *segment_end)
        .unwrap_or(8);
    if segment >= 8 {
        return 0x7f ^ mask;
    }
    (((segment as i32) << 4) | ((magnitude >> (segment + 1)) & 0x0f)) as u8 ^ mask
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn find_wav_data_offset(bytes: &[u8]) -> Option<usize> {
        let mut chunk_offset = 12_usize;
        while chunk_offset.checked_add(8)? <= bytes.len() {
            let chunk_size =
                u32::from_le_bytes(bytes[chunk_offset + 4..chunk_offset + 8].try_into().ok()?)
                    as usize;
            let data_offset = chunk_offset.checked_add(8)?;
            if &bytes[chunk_offset..chunk_offset + 4] == b"data" {
                return Some(data_offset);
            }
            chunk_offset = data_offset.checked_add(chunk_size + (chunk_size & 1))?;
        }
        None
    }

    fn temp_destination(name: &str) -> PathBuf {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "aaadaw-wav-export-{}-{id}-{name}.wav",
            std::process::id()
        ))
    }

    #[test]
    fn pcm24_export_writes_stereo_header_and_clamped_samples() {
        let path = temp_destination("roundtrip");
        let mut export = Pcm24WavExport::create(&path, 48_000).expect("writer should open");
        export
            .write_frames(&[[0.5, -0.5], [2.0, f32::NAN]])
            .expect("frames should be written");
        export.finish().expect("export should publish");

        let mut bytes = Vec::new();
        File::open(&path)
            .expect("export should exist")
            .read_to_end(&mut bytes)
            .expect("export should be readable");
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 48);
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 2);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            48_000
        );
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 24);
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 12);
        assert_eq!(&bytes[44..50], &[0, 0, 0x40, 0, 0, 0xc0]);
        assert_eq!(&bytes[50..56], &[0xff, 0xff, 0x7f, 0, 0, 0]);
        fs::remove_file(path).expect("test export should be removed");
    }

    #[test]
    fn selected_pcm16_format_writes_stereo_integer_samples() {
        let path = temp_destination("pcm16");
        let mut export = WavExport::create(
            &path,
            48_000,
            WavExportOptions {
                sample_format: WavSampleFormat::Pcm16,
                dither: false,
            },
        )
        .expect("PCM16 writer should open");
        export
            .write_frames(&[[0.5, -0.5], [2.0, -2.0]])
            .expect("frames should be written");
        export.finish().expect("export should publish");

        let bytes = fs::read(&path).expect("export should be readable");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 44);
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 16);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 8);
        assert_eq!(
            i16::from_le_bytes(bytes[44..46].try_into().unwrap()),
            16_384
        );
        assert_eq!(
            i16::from_le_bytes(bytes[46..48].try_into().unwrap()),
            -16_384
        );
        assert_eq!(
            i16::from_le_bytes(bytes[48..50].try_into().unwrap()),
            32_767
        );
        assert_eq!(
            i16::from_le_bytes(bytes[50..52].try_into().unwrap()),
            -32_768
        );
        let mut decoder = aaadaw_media::AudioStreamDecoder::open(&path)
            .expect("audio importer should recognize PCM16 WAV");
        let chunk = decoder
            .next_chunk()
            .expect("PCM16 WAV should decode")
            .expect("PCM16 WAV contains one packet");
        assert_eq!(chunk.channels(), 2);
        assert_eq!(chunk.samples().len(), 4);
        assert!((chunk.samples()[0] - 0.5).abs() < 0.0001);
        assert!((chunk.samples()[1] + 0.5).abs() < 0.0001);
        fs::remove_file(path).expect("test export should be removed");
    }

    #[test]
    fn tpdf_dither_is_seeded_bounded_and_zero_mean() {
        let silence = vec![[0.0, 0.0]; 16_384];
        for bits_per_sample in [8, 16, 24, 32] {
            let path = temp_destination(&format!("pcm{bits_per_sample}-dither"));
            let mut export = IntegerPcmWavExport::create(
                &path,
                48_000,
                bits_per_sample,
                WavChannelCount::Stereo,
                Some(17),
            )
            .expect("dithered writer should open");
            export
                .write_frames(&silence)
                .expect("silence frames should be written");
            export.finish().expect("export should publish");

            let bytes = fs::read(&path).expect("export should be readable");
            let bytes_per_sample = usize::from(bits_per_sample / 8);
            let samples = bytes[44..]
                .chunks_exact(bytes_per_sample)
                .map(|sample| match bits_per_sample {
                    8 => i32::from(sample[0]) - 128,
                    16 => i16::from_le_bytes(sample.try_into().unwrap()) as i32,
                    24 => {
                        let sign = if sample[2] & 0x80 == 0 { 0 } else { 0xff };
                        i32::from_le_bytes([sample[0], sample[1], sample[2], sign])
                    }
                    32 => i32::from_le_bytes(sample.try_into().unwrap()),
                    _ => unreachable!("test covers supported integer PCM formats"),
                })
                .collect::<Vec<_>>();
            assert!(samples.iter().all(|sample| (-1..=1).contains(sample)));
            let mean =
                samples.iter().map(|sample| f64::from(*sample)).sum::<f64>() / samples.len() as f64;
            assert!(
                mean.abs() < 0.02,
                "{bits_per_sample}-bit dither mean was {mean}"
            );

            let repeat_path = temp_destination(&format!("pcm{bits_per_sample}-dither-repeat"));
            let mut repeated = IntegerPcmWavExport::create(
                &repeat_path,
                48_000,
                bits_per_sample,
                WavChannelCount::Stereo,
                Some(17),
            )
            .expect("repeat writer should open");
            repeated
                .write_frames(&silence)
                .expect("repeat silence should be written");
            repeated.finish().expect("repeat export should publish");
            assert_eq!(fs::read(&path).unwrap(), fs::read(&repeat_path).unwrap());
            fs::remove_file(path).expect("test export should be removed");
            fs::remove_file(repeat_path).expect("repeat test export should be removed");
        }
    }

    #[test]
    fn every_selected_format_keeps_cancellation_rate_and_destination_failures_atomic() {
        for sample_format in WavSampleFormat::ALL {
            let options = WavExportOptions {
                sample_format,
                dither: true,
            };
            let invalid_rate_path = temp_destination("invalid-rate");
            assert!(WavExport::create(&invalid_rate_path, 0, options).is_err());
            assert!(!invalid_rate_path.exists());

            let cancelled_path = temp_destination("cancel-selected-format");
            let mut cancelled = WavExport::create(&cancelled_path, 48_000, options)
                .expect("selected writer should open");
            cancelled
                .write_frames(&[[0.25, -0.25]])
                .expect("sample frame should be written");
            drop(cancelled);
            assert!(!cancelled_path.exists());

            let write_error_path = temp_destination("write-error-selected-format");
            let mut write_error = WavExport::create(&write_error_path, 48_000, options)
                .expect("selected writer should open");
            match &mut write_error {
                WavExport::Pcm8(export) => export.inner.data_bytes = MAX_RIFF_DATA_BYTES,
                WavExport::Pcm16(export) => export.inner.data_bytes = MAX_RIFF_DATA_BYTES,
                WavExport::Pcm24(export) => export.inner.data_bytes = MAX_RIFF_DATA_BYTES,
                WavExport::Pcm32(export) => export.inner.data_bytes = MAX_RIFF_DATA_BYTES,
                WavExport::Float32(export) => export.inner.data_bytes = MAX_RIFF_DATA_BYTES,
                WavExport::Float64(export) => export.inner.data_bytes = MAX_RIFF_DATA_BYTES,
                WavExport::MuLaw8(export) => export.data_bytes = u64::MAX,
                WavExport::ImaAdpcm4(export) => export.inner.total_frames = u64::MAX,
                WavExport::Cadpcm2(export) => export.inner.total_frames = u64::MAX,
            }
            assert!(write_error.write_frames(&[[0.25, -0.25]]).is_err());
            drop(write_error);
            assert!(!write_error_path.exists());

            let conflict_path = temp_destination("conflict-selected-format");
            let mut conflict = WavExport::create(&conflict_path, 48_000, options)
                .expect("selected writer should open");
            conflict
                .write_frames(&[[0.25, -0.25]])
                .expect("sample frame should be written");
            fs::write(&conflict_path, b"existing destination")
                .expect("destination should appear during render");
            assert!(conflict.finish().is_err());
            assert_eq!(
                fs::read(&conflict_path).unwrap(),
                b"existing destination",
                "{sample_format} export must not replace a destination"
            );
            fs::remove_file(conflict_path).expect("existing destination should be removed");
        }
    }

    #[test]
    fn float_format_remains_bit_exact_when_dither_is_requested() {
        let path = temp_destination("float32-no-dither");
        let mut export = WavExport::create(
            &path,
            48_000,
            WavExportOptions {
                sample_format: WavSampleFormat::Float32,
                dither: true,
            },
        )
        .expect("float writer should open");
        export
            .write_frames(&[[1.25, -1.5]])
            .expect("float samples should remain unmodified");
        export.finish().expect("export should publish");
        let mut decoder = aaadaw_media::AudioStreamDecoder::open(&path)
            .expect("audio importer should recognize IEEE float WAV");
        let chunk = decoder
            .next_chunk()
            .expect("float WAV should decode")
            .expect("float WAV contains one packet");
        assert_eq!(chunk.samples(), &[1.25, -1.5]);
        fs::remove_file(path).expect("test export should be removed");
    }

    #[test]
    fn mono_wav_outputs_average_the_stereo_mix_for_every_supported_format() {
        for sample_format in WavSampleFormat::ALL {
            let path = temp_destination(&format!("mono-{sample_format}"));
            let mut export = WavExport::create_with_channels(
                &path,
                48_000,
                WavExportOptions {
                    sample_format,
                    dither: false,
                },
                WavChannelCount::Mono,
            )
            .expect("mono writer should open");
            export
                .write_frames(&[[0.125, 0.375]])
                .expect("mono frame should be written");
            export.finish().expect("mono output should publish");

            let bytes = fs::read(&path).expect("mono output should be readable");
            assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1);
            let bytes_per_sample = match sample_format {
                WavSampleFormat::Pcm8 => 1,
                WavSampleFormat::Pcm16 => 2,
                WavSampleFormat::Pcm24 => 3,
                WavSampleFormat::Pcm32 | WavSampleFormat::Float32 => 4,
                WavSampleFormat::Float64 => 8,
                WavSampleFormat::MuLaw8 => 1,
                WavSampleFormat::ImaAdpcm4 => 1,
                WavSampleFormat::Cadpcm2 => 1,
            };
            let expected_format_tag = match sample_format {
                WavSampleFormat::MuLaw8 => 7,
                WavSampleFormat::ImaAdpcm4 => 17,
                WavSampleFormat::Cadpcm2 => 0xAC0C,
                WavSampleFormat::Float32 | WavSampleFormat::Float64 => 3,
                _ => 1,
            };
            assert_eq!(
                u16::from_le_bytes(bytes[20..22].try_into().unwrap()),
                expected_format_tag
            );
            let data_offset = find_wav_data_offset(&bytes).expect("mono data chunk should exist");
            assert_eq!(
                u32::from_le_bytes(bytes[data_offset - 4..data_offset].try_into().unwrap()),
                if matches!(
                    sample_format,
                    WavSampleFormat::ImaAdpcm4 | WavSampleFormat::Cadpcm2
                ) {
                    IMA_ADPCM_BYTES_PER_CHANNEL_BLOCK as u32
                } else {
                    bytes_per_sample as u32
                }
            );
            let mut decoder =
                aaadaw_media::AudioStreamDecoder::open(&path).expect("mono output should decode");
            let chunk = decoder
                .next_chunk()
                .expect("mono output should read")
                .expect("mono output should contain a frame");
            assert_eq!(chunk.channels(), 1);
            assert_eq!(chunk.samples().len(), 1);
            let tolerance = if matches!(
                sample_format,
                WavSampleFormat::MuLaw8 | WavSampleFormat::ImaAdpcm4 | WavSampleFormat::Cadpcm2
            ) {
                0.02
            } else {
                1.0e-6
            };
            assert!(
                (chunk.samples()[0] - 0.25).abs() < tolerance,
                "{sample_format} mono output decoded to {}",
                chunk.samples()[0]
            );
            if sample_format == WavSampleFormat::MuLaw8 {
                assert_eq!(bytes.len() & 1, 0, "odd u-Law data should be RIFF padded");
                assert_eq!(bytes.last(), Some(&0), "u-Law RIFF pad byte should be zero");
                assert_eq!(
                    u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
                    bytes.len() - 8
                );
            }
            fs::remove_file(path).expect("test output should be removed");
        }
    }

    #[test]
    fn ima_adpcm_wav_matches_native_block_layout_and_round_trips() {
        let path = temp_destination("ima-adpcm-native-block");
        let frames = vec![[0.25, 0.75]; IMA_ADPCM_SAMPLES_PER_BLOCK];
        let mut export = WavExport::create(
            &path,
            48_000,
            WavExportOptions {
                sample_format: WavSampleFormat::ImaAdpcm4,
                dither: false,
            },
        )
        .expect("IMA ADPCM writer should open");
        export
            .write_frames(&frames[..997])
            .expect("first partial block should be accepted");
        export
            .write_frames(&frames[997..])
            .expect("remaining block samples should be accepted");
        export.finish().expect("IMA ADPCM WAV should publish");

        let bytes = fs::read(&path).expect("IMA ADPCM WAV should be readable");
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            52 + 2048
        );
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 20);
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 17);
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 2);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            48_000
        );
        assert_eq!(
            u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
            48_000
        );
        assert_eq!(u16::from_le_bytes(bytes[32..34].try_into().unwrap()), 2048);
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 4);
        assert_eq!(u16::from_le_bytes(bytes[36..38].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(bytes[38..40].try_into().unwrap()), 2041);
        assert_eq!(&bytes[40..44], b"fact");
        assert_eq!(u32::from_le_bytes(bytes[48..52].try_into().unwrap()), 2041);
        assert_eq!(&bytes[52..56], b"data");
        assert_eq!(u32::from_le_bytes(bytes[56..60].try_into().unwrap()), 2048);
        assert_eq!(&bytes[60..68], &[0, 0x20, 0, 0, 0, 0x60, 0, 0]);
        assert!(bytes[68..].iter().all(|byte| *byte == 0));

        let mut decoder = aaadaw_media::AudioStreamDecoder::open(&path)
            .expect("AAADAW should open IMA ADPCM WAV");
        assert_eq!(decoder.metadata().sample_rate, Some(48_000));
        assert_eq!(decoder.metadata().channel_count, Some(2));
        let chunk = decoder
            .next_chunk()
            .expect("IMA block should decode")
            .expect("IMA block should contain samples");
        assert_eq!(chunk.channels(), 2);
        assert!((chunk.samples()[0] - 0.25).abs() < 1.0e-4);
        assert!((chunk.samples()[1] - 0.75).abs() < 1.0e-4);
        fs::remove_file(path).expect("test WAV should be removed");
    }

    #[test]
    fn ima_adpcm_first_block_predictors_match_native_level_vectors() {
        let path = temp_destination("ima-adpcm-native-vectors");
        let frames = vec![[-0.999, -0.999]; IMA_ADPCM_SAMPLES_PER_BLOCK];
        let mut export = WavExport::create(
            &path,
            48_000,
            WavExportOptions {
                sample_format: WavSampleFormat::ImaAdpcm4,
                dither: false,
            },
        )
        .expect("IMA ADPCM writer should open");
        export
            .write_frames(&frames)
            .expect("native level vector should write");
        export.finish().expect("IMA ADPCM WAV should publish");
        let bytes = fs::read(&path).expect("IMA ADPCM WAV should be readable");
        assert_eq!(&bytes[60..68], &[0x21, 0x80, 0, 0, 0x21, 0x80, 0, 0]);
        assert!(bytes[68..].iter().all(|byte| *byte == 0));
        fs::remove_file(path).expect("test WAV should be removed");
    }

    #[test]
    fn ima_adpcm_matches_native_render_vector_headers_and_decoded_levels() {
        let path = temp_destination("ima-adpcm-native-render-vectors");
        let levels = [
            -0.999_f32, -0.75, -0.5, -0.25, -0.125, -0.1, -0.05, -0.01, 0.0, 0.01, 0.05, 0.1,
            0.125, 0.25, 0.5, 0.75,
        ];
        let mut frames = Vec::with_capacity(48_000);
        for level in levels {
            let source_sample = ((level * 0.5) * 8_388_608.0).round() as i32;
            let master_sample = (source_sample as f32 / 8_388_608.0) * 2.0;
            frames.extend(std::iter::repeat_n([master_sample, master_sample], 3_000));
        }
        assert_eq!(frames.len(), 48_000);

        let mut export = WavExport::create(
            &path,
            48_000,
            WavExportOptions {
                sample_format: WavSampleFormat::ImaAdpcm4,
                dither: false,
            },
        )
        .expect("IMA ADPCM writer should open");
        for chunk in frames.chunks(1_237) {
            export
                .write_frames(chunk)
                .expect("native level source should encode");
        }
        export.finish().expect("IMA ADPCM WAV should publish");

        let bytes = fs::read(&path).expect("IMA ADPCM WAV should be readable");
        let data_offset = find_wav_data_offset(&bytes).expect("IMA data chunk should exist");
        let data_size =
            u32::from_le_bytes(bytes[data_offset - 4..data_offset].try_into().unwrap()) as usize;
        assert_eq!(data_size, 49_152);
        let native_predictors = [
            -32_735_i16,
            -32_735,
            -24_576,
            -16_384,
            -16_384,
            -8_192,
            -4_096,
            -4_096,
            -3_277,
            -1_638,
            -1_638,
            -328,
            0,
            0,
            328,
            1_638,
            1_638,
            3_277,
            4_096,
            4_096,
            8_192,
            16_384,
            16_384,
            24_576,
        ];
        for (block, expected_predictor) in native_predictors.into_iter().enumerate() {
            let offset = data_offset + block * 2048;
            assert_eq!(
                i16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
                expected_predictor,
                "left predictor for native block {block}"
            );
            assert_eq!(
                i16::from_le_bytes(bytes[offset + 4..offset + 6].try_into().unwrap()),
                expected_predictor,
                "right predictor for native block {block}"
            );
            assert_eq!(&bytes[offset + 2..offset + 4], &[0, 0]);
            assert_eq!(&bytes[offset + 6..offset + 8], &[0, 0]);
        }

        let native_levels = [
            -0.998_992_9_f32,
            -0.75,
            -0.5,
            -0.25,
            -0.125,
            -0.100_006_1,
            -0.049_987_8,
            -0.010_009_8,
            0.0,
            0.010_040_3,
            0.049_987_8,
            0.100_097_7,
            0.125,
            0.250_061,
            0.5,
            0.750_091_6,
        ];
        let mut decoder = aaadaw_media::AudioStreamDecoder::open(&path)
            .expect("AAADAW should decode the generated IMA ADPCM WAV");
        assert_eq!(decoder.metadata().frame_count, Some(48_000));
        let mut decoded_left = Vec::new();
        while let Some(chunk) = decoder
            .next_chunk()
            .expect("IMA ADPCM vector should decode")
        {
            decoded_left.extend(
                chunk
                    .samples()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|frame| frame[0]),
            );
        }
        assert_eq!(decoded_left.len(), 48_000);
        for (level_index, expected) in native_levels.into_iter().enumerate() {
            let frame = level_index * 3_000 + 1_500;
            assert!(
                (decoded_left[frame] - expected).abs() < 0.001,
                "IMA decoded sample at level {level_index} was {}",
                decoded_left[frame]
            );
        }
        fs::remove_file(path).expect("test WAV should be removed");
    }

    #[test]
    fn cadpcm2_wav_matches_native_block_layout_and_round_trips() {
        let path = temp_destination("cadpcm2-native-block");
        let frames = vec![[0.25, 0.75]; CADPCM_SAMPLES_PER_BLOCK];
        let mut export = WavExport::create(
            &path,
            48_000,
            WavExportOptions {
                sample_format: WavSampleFormat::Cadpcm2,
                dither: false,
            },
        )
        .expect("cADPCM writer should open");
        for chunk in frames.chunks(1_117) {
            export
                .write_frames(chunk)
                .expect("cADPCM frames should write");
        }
        export.finish().expect("cADPCM WAV should publish");

        let bytes = fs::read(&path).expect("cADPCM WAV should be readable");
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            52 + 2048
        );
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 20);
        assert_eq!(
            u16::from_le_bytes(bytes[20..22].try_into().unwrap()),
            0xac0c
        );
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 2);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            48_000
        );
        assert_eq!(
            u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
            24_000
        );
        assert_eq!(u16::from_le_bytes(bytes[32..34].try_into().unwrap()), 2048);
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(bytes[36..38].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(bytes[38..40].try_into().unwrap()), 4081);
        assert_eq!(&bytes[40..44], b"fact");
        assert_eq!(u32::from_le_bytes(bytes[48..52].try_into().unwrap()), 4081);
        assert_eq!(&bytes[52..56], b"data");
        assert_eq!(u32::from_le_bytes(bytes[56..60].try_into().unwrap()), 2048);
        assert_eq!(&bytes[60..68], &[0, 0x20, 0, 0, 0, 0x60, 0, 0]);
        assert!(bytes[68..].iter().all(|byte| *byte == 0));

        let mut decoder = aaadaw_media::AudioStreamDecoder::open(&path)
            .expect("media decoder should recognize native cADPCM WAVE");
        assert_eq!(decoder.metadata().codec, "cADPCM");
        assert_eq!(decoder.metadata().sample_rate, Some(48_000));
        assert_eq!(decoder.metadata().channel_count, Some(2));
        assert_eq!(decoder.metadata().bits_per_sample, Some(2));
        assert_eq!(decoder.metadata().frame_count, Some(4081));
        let chunk = decoder
            .next_chunk()
            .expect("cADPCM block should decode")
            .expect("cADPCM block should contain frames");
        assert_eq!(chunk.frame_count(), CADPCM_SAMPLES_PER_BLOCK);
        assert!(
            chunk
                .samples()
                .as_chunks::<2>()
                .0
                .iter()
                .all(|frame| (frame[0] - 0.25).abs() < 1.0e-6 && (frame[1] - 0.75).abs() < 1.0e-6)
        );
        assert!(
            decoder
                .next_chunk()
                .expect("decoder should honor the fact frame count")
                .is_none()
        );
        fs::remove_file(path).expect("test WAV should be removed");
    }

    #[test]
    fn cadpcm2_vectors_match_native_block_predictors_and_decoded_levels() {
        const NATIVE_REFERENCE: &[u8] = include_bytes!(
            "../../../docs/verification/reaper-parity/fixtures/render-cadpcm2-native-vectors.wav"
        );
        let path = temp_destination("cadpcm2-native-render-vectors");
        let levels = [
            -0.999_f32, -0.75, -0.5, -0.25, -0.125, -0.1, -0.05, -0.01, 0.0, 0.01, 0.05, 0.1,
            0.125, 0.25, 0.5, 0.75,
        ];
        let mut frames = Vec::with_capacity(48_000);
        for level in levels {
            let source_sample = ((level * 0.5) * 8_388_608.0).round() as i32;
            let master_sample = (source_sample as f32 / 8_388_608.0) * 2.0;
            frames.extend(std::iter::repeat_n([master_sample, master_sample], 3_000));
        }

        let mut export = WavExport::create(
            &path,
            48_000,
            WavExportOptions {
                sample_format: WavSampleFormat::Cadpcm2,
                dither: false,
            },
        )
        .expect("cADPCM writer should open");
        for chunk in frames.chunks(1_237) {
            export
                .write_frames(chunk)
                .expect("native level source should encode");
        }
        export.finish().expect("cADPCM WAV should publish");

        let bytes = fs::read(&path).expect("cADPCM WAV should be readable");
        let data_offset = find_wav_data_offset(&bytes).expect("cADPCM data should exist");
        let native_data_offset =
            find_wav_data_offset(NATIVE_REFERENCE).expect("native cADPCM data should exist");
        let native_data_bytes = u32::from_le_bytes(
            NATIVE_REFERENCE[native_data_offset - 4..native_data_offset]
                .try_into()
                .unwrap(),
        ) as usize;
        let native_data =
            &NATIVE_REFERENCE[native_data_offset..native_data_offset + native_data_bytes];
        let encoded_data = &bytes[data_offset..data_offset + native_data_bytes];
        let matching_bytes = encoded_data
            .iter()
            .zip(native_data)
            .filter(|(encoded, native)| encoded == native)
            .count();
        assert!(
            matching_bytes * 100 >= native_data.len() * 99,
            "only {matching_bytes}/{} cADPCM bytes match the native REAPER render",
            native_data.len()
        );
        let native_predictors = [
            -32_735_i16,
            -24_576,
            -16_384,
            -4_096,
            -3_277,
            -1_638,
            0,
            328,
            1_638,
            4_096,
            8_192,
            16_384,
        ];
        for (block, expected_predictor) in native_predictors.into_iter().enumerate() {
            let offset = data_offset + block * 2048;
            assert_eq!(
                i16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
                expected_predictor,
                "left predictor for native cADPCM block {block}"
            );
            assert_eq!(
                i16::from_le_bytes(bytes[offset + 4..offset + 6].try_into().unwrap()),
                expected_predictor,
                "right predictor for native cADPCM block {block}"
            );
        }

        let native_levels = [
            -0.998_992_9_f32,
            -0.75,
            -0.5,
            -0.25,
            -0.125,
            -0.100_006_1,
            -0.049_987_8,
            -0.009_765_6,
            0.0,
            0.010_009_8,
            0.049_987_8,
            0.100_036_6,
            0.125,
            0.25,
            0.5,
            0.75,
        ];
        let mut decoder = aaadaw_media::AudioStreamDecoder::open(&path)
            .expect("media decoder should read encoded cADPCM WAVE");
        assert_eq!(decoder.metadata().frame_count, Some(48_000));
        let mut decoded_left = Vec::new();
        while let Some(chunk) = decoder.next_chunk().expect("cADPCM vector should decode") {
            decoded_left.extend(
                chunk
                    .samples()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|frame| frame[0]),
            );
        }
        assert_eq!(decoded_left.len(), 48_000);
        for (level_index, expected) in native_levels.into_iter().enumerate() {
            let frame = level_index * 3_000 + 1_500;
            assert!(
                (decoded_left[frame] - expected).abs() < 0.001,
                "cADPCM decoded sample at level {level_index} was {}",
                decoded_left[frame]
            );
        }

        let mut native_decoder = aaadaw_media::AudioStreamDecoder::from_reader(
            std::io::Cursor::new(NATIVE_REFERENCE),
            Some(NATIVE_REFERENCE.len() as u64),
            Some("wav"),
        )
        .expect("media decoder should read REAPER's BWF cADPCM file");
        assert_eq!(native_decoder.metadata().frame_count, Some(48_000));
        let mut native_decoded_left = Vec::new();
        while let Some(chunk) = native_decoder
            .next_chunk()
            .expect("native cADPCM block should decode")
        {
            native_decoded_left.extend(
                chunk
                    .samples()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|frame| frame[0]),
            );
        }
        assert_eq!(native_decoded_left.len(), 48_000);
        for (level_index, expected) in native_levels.into_iter().enumerate() {
            let frame = level_index * 3_000 + 1_500;
            assert!(
                (native_decoded_left[frame] - expected).abs() < 0.001,
                "native cADPCM decoded sample at level {level_index} was {}",
                native_decoded_left[frame]
            );
        }
        fs::remove_file(path).expect("test WAV should be removed");
    }

    #[test]
    fn mulaw_wav_matches_native_g711_codes_and_decodes() {
        let path = temp_destination("mulaw-native-vectors");
        let frames = [
            [-0.999, -0.75],
            [-0.5, -0.25],
            [-0.125, -0.1],
            [-0.05, -0.01],
            [0.0, 0.01],
            [0.05, 0.1],
            [0.125, 0.25],
            [0.5, 0.75],
        ];
        let mut export = WavExport::create(
            &path,
            48_000,
            WavExportOptions {
                sample_format: WavSampleFormat::MuLaw8,
                dither: false,
            },
        )
        .expect("u-Law writer should open");
        export
            .write_frames(&frames)
            .expect("finite u-Law samples should write");
        export.finish().expect("u-Law WAV should publish");

        let bytes = fs::read(&path).expect("u-Law WAV should be readable");
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 7);
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 2);
        assert_eq!(
            u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
            96_000
        );
        assert_eq!(u16::from_le_bytes(bytes[32..34].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 8);
        assert_eq!(u16::from_le_bytes(bytes[36..38].try_into().unwrap()), 0);
        let data_offset = find_wav_data_offset(&bytes).expect("u-Law data chunk should exist");
        assert_eq!(
            &bytes[data_offset..data_offset + 16],
            &[
                0, 7, 15, 31, 47, 53, 68, 99, 255, 227, 196, 181, 175, 159, 143, 135
            ]
        );

        let mut decoder = aaadaw_media::AudioStreamDecoder::open(&path)
            .expect("AAADAW should decode G.711 μ-law WAV");
        let chunk = decoder
            .next_chunk()
            .expect("u-Law WAV should decode")
            .unwrap();
        assert_eq!(chunk.channels(), 2);
        assert_eq!(chunk.samples().len(), frames.len() * 2);
        for (actual, expected) in chunk.samples().iter().zip(frames.into_iter().flatten()) {
            assert!((*actual - expected).abs() < 0.04);
        }
        fs::remove_file(path).expect("test WAV should be removed");
    }

    #[test]
    fn pcm8_is_unsigned_and_pcm32_preserves_full_scale_integer_samples() {
        let path = temp_destination("pcm8-and-pcm32");
        for (sample_format, bits_per_sample, expected_samples) in [
            (WavSampleFormat::Pcm8, 8, vec![160_u8, 224]),
            (
                WavSampleFormat::Pcm32,
                32,
                [
                    536_870_912_i32.to_le_bytes(),
                    1_610_612_736_i32.to_le_bytes(),
                ]
                .concat(),
            ),
        ] {
            let mut export = WavExport::create(
                &path,
                48_000,
                WavExportOptions {
                    sample_format,
                    dither: false,
                },
            )
            .expect("PCM writer should open");
            export
                .write_frames(&[[0.25, 0.75]])
                .expect("samples should write");
            export.finish().expect("WAV should publish");
            let bytes = fs::read(&path).expect("WAV should be readable");
            assert_eq!(
                u16::from_le_bytes(bytes[34..36].try_into().unwrap()),
                bits_per_sample
            );
            let data_offset = find_wav_data_offset(&bytes).expect("data chunk should exist");
            let sample_bytes = usize::from(bits_per_sample / 8) * 2;
            assert_eq!(
                &bytes[data_offset..data_offset + sample_bytes],
                expected_samples
            );
            let mut decoder = aaadaw_media::AudioStreamDecoder::open(&path)
                .expect("AAADAW should decode its PCM output");
            let chunk = decoder.next_chunk().expect("WAV should decode").unwrap();
            assert_eq!(chunk.samples().len(), 2);
            let tolerance = if bits_per_sample == 8 {
                1.0 / 128.0
            } else {
                1.0e-7
            };
            assert!((chunk.samples()[0] - 0.25).abs() <= tolerance);
            assert!((chunk.samples()[1] - 0.75).abs() <= tolerance);
            fs::remove_file(&path).expect("test output should be removed");
        }
    }

    #[test]
    fn float64_wav_preserves_headroom_and_round_trips_through_media_decoder() {
        let path = temp_destination("float64-headroom");
        let mut export = WavExport::create(
            &path,
            48_000,
            WavExportOptions {
                sample_format: WavSampleFormat::Float64,
                dither: true,
            },
        )
        .expect("float64 writer should open");
        export
            .write_frames(&[[1.25, -1.5]])
            .expect("float samples should preserve headroom");
        export.finish().expect("WAV should publish");

        let bytes = fs::read(&path).expect("WAV should be readable");
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            (bytes.len() - 8) as u32
        );
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 3);
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 64);
        let data_offset = find_wav_data_offset(&bytes).expect("data chunk should exist");
        assert_eq!(
            [
                f64::from_le_bytes(bytes[data_offset..data_offset + 8].try_into().unwrap()),
                f64::from_le_bytes(bytes[data_offset + 8..data_offset + 16].try_into().unwrap()),
            ],
            [1.25, -1.5]
        );
        let mut decoder = aaadaw_media::AudioStreamDecoder::open(&path)
            .expect("AAADAW should decode IEEE float64 WAV");
        let chunk = decoder.next_chunk().expect("WAV should decode").unwrap();
        assert_eq!(chunk.samples(), &[1.25, -1.5]);
        fs::remove_file(path).expect("test output should be removed");
    }

    #[test]
    fn float32_export_preserves_samples_above_full_scale_and_decodes_as_float_wav() {
        let path = temp_destination("float32-headroom");
        let mut export = Float32WavExport::create(&path, 48_000).expect("writer should open");
        export
            .write_frames(&[[1.25, -1.5], [-2.25, 2.5]])
            .expect("finite float samples should be written without clipping");
        export.finish().expect("export should publish");

        let bytes = fs::read(&path).expect("export should be readable");
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 3);
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 32);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 16);

        let mut decoder = aaadaw_media::AudioStreamDecoder::open(&path)
            .expect("audio importer should recognize IEEE float WAV");
        let chunk = decoder
            .next_chunk()
            .expect("float WAV should decode")
            .expect("float WAV contains one audio packet");
        assert_eq!(chunk.channels(), 2);
        assert_eq!(chunk.samples(), &[1.25, -1.5, -2.25, 2.5]);
        fs::remove_file(path).expect("test export should be removed");
    }

    #[test]
    fn float32_export_rejects_non_finite_samples_without_publishing_a_file() {
        let path = temp_destination("float32-non-finite");
        {
            let mut export = Float32WavExport::create(&path, 48_000).expect("writer should open");
            assert!(matches!(
                export.write_frames(&[[f32::NAN, 0.0]]),
                Err(Float32WavExportError::NonFiniteSample)
            ));
        }
        assert!(!path.exists());
    }

    #[test]
    fn dropping_or_rejecting_an_export_never_publishes_a_partial_file() {
        let path = temp_destination("cancel");
        {
            let mut export = Pcm24WavExport::create(&path, 44_100).expect("writer should open");
            export
                .write_frames(&[[0.25, -0.25]])
                .expect("frame should be written");
        }
        assert!(!path.exists());
        let export = Pcm24WavExport::create(&path, 44_100).expect("path should be reusable");
        export.finish().expect("empty export should publish");
        assert!(matches!(
            Pcm24WavExport::create(&path, 44_100),
            Err(Pcm24WavExportError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists
        ));
        fs::remove_file(path).expect("test export should be removed");
    }

    #[test]
    fn invalid_destination_fails_before_publishing_any_file() {
        let parent = std::env::temp_dir().join(format!(
            "aaadaw-wav-export-missing-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let path = parent.join("render.wav");
        assert!(matches!(
            Pcm24WavExport::create(&path, 48_000),
            Err(Pcm24WavExportError::Io(error)) if error.kind() == io::ErrorKind::NotFound
        ));
        assert!(!parent.exists());
        assert!(!path.exists());
    }
}
