use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const CHANNELS: u16 = 2;
const FLOAT32_BITS_PER_SAMPLE: u16 = 32;
const FLOAT32_BYTES_PER_FRAME: u64 = CHANNELS as u64 * (FLOAT32_BITS_PER_SAMPLE as u64 / 8);
const MAX_RIFF_DATA_BYTES: u64 = u32::MAX as u64 - 36;
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
static NEXT_DITHER_SEED: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WavSampleFormat {
    Pcm16,
    #[default]
    Pcm24,
    Float32,
}

impl WavSampleFormat {
    pub const ALL: [Self; 3] = [Self::Pcm16, Self::Pcm24, Self::Float32];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Pcm16 => "PCM 16-bit",
            Self::Pcm24 => "PCM 24-bit",
            Self::Float32 => "Float 32-bit",
        }
    }

    pub const fn is_integer(self) -> bool {
        matches!(self, Self::Pcm16 | Self::Pcm24)
    }
}

impl std::fmt::Display for WavSampleFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
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

/// Incremental stereo PCM16 WAV export with optional TPDF dither.
pub struct Pcm16WavExport {
    inner: IntegerPcmWavExport,
}

/// User-facing WAV export writer. Float output ignores the integer-only dither setting.
pub enum WavExport {
    Pcm16(Pcm16WavExport),
    Pcm24(Pcm24WavExport),
    Float32(Float32WavExport),
}

#[derive(Debug)]
pub enum WavExportError {
    Pcm16(IntegerPcmWavExportError),
    Pcm24(IntegerPcmWavExportError),
    Float32(Float32WavExportError),
}

impl std::fmt::Display for WavExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pcm16(error) | Self::Pcm24(error) => error.fmt(formatter),
            Self::Float32(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for WavExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pcm16(error) | Self::Pcm24(error) => Some(error),
            Self::Float32(error) => Some(error),
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
        Ok(Self {
            inner: IntegerPcmWavExport::create(destination.as_ref(), sample_rate, 24, None)?,
        })
    }

    fn create_with_dither_seed(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        seed: u64,
    ) -> Result<Self, Pcm24WavExportError> {
        Ok(Self {
            inner: IntegerPcmWavExport::create(destination.as_ref(), sample_rate, 24, Some(seed))?,
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

impl Pcm16WavExport {
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
    ) -> Result<Self, Pcm24WavExportError> {
        Ok(Self {
            inner: IntegerPcmWavExport::create(destination.as_ref(), sample_rate, 16, None)?,
        })
    }

    fn create_with_dither_seed(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        seed: u64,
    ) -> Result<Self, Pcm24WavExportError> {
        Ok(Self {
            inner: IntegerPcmWavExport::create(destination.as_ref(), sample_rate, 16, Some(seed))?,
        })
    }

    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), Pcm24WavExportError> {
        self.inner.write_frames(frames)
    }

    pub fn finish(self) -> Result<PathBuf, Pcm24WavExportError> {
        self.inner.finish()
    }
}

impl WavExport {
    pub fn create(
        destination: impl AsRef<Path>,
        sample_rate: u32,
        options: WavExportOptions,
    ) -> Result<Self, WavExportError> {
        let seed = options.dither.then(random_dither_seed);
        let export = match options.sample_format {
            WavSampleFormat::Pcm16 => match seed {
                Some(seed) => {
                    Pcm16WavExport::create_with_dither_seed(destination, sample_rate, seed)
                        .map(Self::Pcm16)
                }
                None => Pcm16WavExport::create(destination, sample_rate).map(Self::Pcm16),
            }
            .map_err(WavExportError::Pcm16)?,
            WavSampleFormat::Pcm24 => match seed {
                Some(seed) => {
                    Pcm24WavExport::create_with_dither_seed(destination, sample_rate, seed)
                        .map(Self::Pcm24)
                }
                None => Pcm24WavExport::create(destination, sample_rate).map(Self::Pcm24),
            }
            .map_err(WavExportError::Pcm24)?,
            WavSampleFormat::Float32 => Float32WavExport::create(destination, sample_rate)
                .map(Self::Float32)
                .map_err(WavExportError::Float32)?,
        };
        Ok(export)
    }

    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), WavExportError> {
        match self {
            Self::Pcm16(export) => export.write_frames(frames).map_err(WavExportError::Pcm16),
            Self::Pcm24(export) => export.write_frames(frames).map_err(WavExportError::Pcm24),
            Self::Float32(export) => export.write_frames(frames).map_err(WavExportError::Float32),
        }
    }

    pub fn finish(self) -> Result<PathBuf, WavExportError> {
        match self {
            Self::Pcm16(export) => export.finish().map_err(WavExportError::Pcm16),
            Self::Pcm24(export) => export.finish().map_err(WavExportError::Pcm24),
            Self::Float32(export) => export.finish().map_err(WavExportError::Float32),
        }
    }
}

impl IntegerPcmWavExport {
    fn create(
        destination: &Path,
        sample_rate: u32,
        bits_per_sample: u16,
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
            bits_per_sample,
            data_bytes: 0,
            dither: seed.map(TpdfDither::new),
        };
        write_integer_header(&mut file, sample_rate, bits_per_sample, 0)?;
        export.file = Some(BufWriter::new(file));
        Ok(export)
    }

    /// Appends rendered interleaved stereo frames using bounded memory.
    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), IntegerPcmWavExportError> {
        let bytes_per_sample = u64::from(self.bits_per_sample / 8);
        let bytes_per_frame = u64::from(CHANNELS) * bytes_per_sample;
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
            for sample in frame {
                let dither = self.dither.as_mut().map_or(0.0, TpdfDither::next_lsb);
                let quantized = (sample.clamp(-1.0, 1.0) as f64 * scale + dither)
                    .round()
                    .clamp(min, max) as i32;
                let bytes = quantized.to_le_bytes();
                file.write_all(&bytes[..bytes_per_sample as usize])?;
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

/// Incremental stereo IEEE float32 WAV export for internal renders that must preserve headroom.
pub struct Float32WavExport {
    destination: PathBuf,
    temporary: PathBuf,
    file: Option<BufWriter<File>>,
    sample_rate: u32,
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
        if sample_rate == 0 {
            return Err(Float32WavExportError::InvalidSampleRate);
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
            data_bytes: 0,
        };
        write_float32_header(&mut file, sample_rate, 0)?;
        export.file = Some(BufWriter::new(file));
        Ok(export)
    }

    /// Appends finite interleaved stereo frames without changing their levels.
    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), Float32WavExportError> {
        let added_bytes = (frames.len() as u64)
            .checked_mul(FLOAT32_BYTES_PER_FRAME)
            .ok_or(Float32WavExportError::RiffSizeLimit)?;
        let data_bytes = self
            .data_bytes
            .checked_add(added_bytes)
            .filter(|bytes| *bytes <= MAX_RIFF_DATA_BYTES)
            .ok_or(Float32WavExportError::RiffSizeLimit)?;
        if frames.iter().flatten().any(|sample| !sample.is_finite()) {
            return Err(Float32WavExportError::NonFiniteSample);
        }
        let file = self
            .file
            .as_mut()
            .expect("unfinished export retains its file");
        for frame in frames {
            for sample in frame {
                file.write_all(&sample.to_le_bytes())?;
            }
        }
        self.data_bytes = data_bytes;
        Ok(())
    }

    /// Flushes the WAV header and atomically publishes the completed export.
    pub fn finish(mut self) -> Result<PathBuf, Float32WavExportError> {
        let mut file = self
            .file
            .take()
            .expect("unfinished export retains its file");
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        write_float32_header(&mut file, self.sample_rate, self.data_bytes)?;
        file.flush()?;
        drop(file);
        fs::hard_link(&self.temporary, &self.destination)?;
        let _ = fs::remove_file(&self.temporary);
        Ok(self.destination.clone())
    }
}

impl Drop for Float32WavExport {
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
    data_bytes: u64,
) -> io::Result<()> {
    let data_bytes = u32::try_from(data_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "WAV data exceeds RIFF limit"))?;
    let bytes_per_sample = bits_per_sample / 8;
    let block_align = CHANNELS * bytes_per_sample;
    let byte_rate = sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV byte rate overflow"))?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36_u32 + data_bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&CHANNELS.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&bits_per_sample.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())
}

fn write_float32_header(
    file: &mut impl Write,
    sample_rate: u32,
    data_bytes: u64,
) -> io::Result<()> {
    let data_bytes = u32::try_from(data_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "WAV data exceeds RIFF limit"))?;
    let byte_rate = sample_rate
        .checked_mul(u32::from(CHANNELS) * u32::from(FLOAT32_BITS_PER_SAMPLE / 8))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV byte rate overflow"))?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36_u32 + data_bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&3_u16.to_le_bytes())?;
    file.write_all(&CHANNELS.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&(CHANNELS * (FLOAT32_BITS_PER_SAMPLE / 8)).to_le_bytes())?;
    file.write_all(&FLOAT32_BITS_PER_SAMPLE.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

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
        for bits_per_sample in [16, 24] {
            let path = temp_destination(&format!("pcm{bits_per_sample}-dither"));
            let mut export = IntegerPcmWavExport::create(&path, 48_000, bits_per_sample, Some(17))
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
                    16 => i16::from_le_bytes(sample.try_into().unwrap()) as i32,
                    24 => {
                        let sign = if sample[2] & 0x80 == 0 { 0 } else { 0xff };
                        i32::from_le_bytes([sample[0], sample[1], sample[2], sign])
                    }
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
            let mut repeated =
                IntegerPcmWavExport::create(&repeat_path, 48_000, bits_per_sample, Some(17))
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
                WavExport::Pcm16(export) => export.inner.data_bytes = MAX_RIFF_DATA_BYTES,
                WavExport::Pcm24(export) => export.inner.data_bytes = MAX_RIFF_DATA_BYTES,
                WavExport::Float32(export) => export.data_bytes = MAX_RIFF_DATA_BYTES,
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
