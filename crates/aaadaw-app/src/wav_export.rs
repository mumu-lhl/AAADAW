use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const CHANNELS: u16 = 2;
const BITS_PER_SAMPLE: u16 = 24;
const BYTES_PER_FRAME: u64 = CHANNELS as u64 * (BITS_PER_SAMPLE as u64 / 8);
const MAX_RIFF_DATA_BYTES: u64 = u32::MAX as u64 - 36;
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Incremental stereo PCM24 WAV export with atomic publication on completion.
///
/// Audio is converted from interleaved f32 frames without buffering the rendered file. The final
/// path is created only by `finish`; dropping or cancelling an export removes its temporary file.
pub struct Pcm24WavExport {
    destination: PathBuf,
    temporary: PathBuf,
    file: Option<BufWriter<File>>,
    sample_rate: u32,
    data_bytes: u64,
}

/// Errors returned while creating or writing a PCM24 WAV export.
#[derive(Debug)]
pub enum Pcm24WavExportError {
    Io(io::Error),
    InvalidSampleRate,
    RiffSizeLimit,
}

impl std::fmt::Display for Pcm24WavExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "WAV export failed: {error}"),
            Self::InvalidSampleRate => formatter.write_str("WAV sample rate must be positive"),
            Self::RiffSizeLimit => formatter.write_str("WAV export exceeds the RIFF size limit"),
        }
    }
}

impl std::error::Error for Pcm24WavExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidSampleRate | Self::RiffSizeLimit => None,
        }
    }
}

impl From<io::Error> for Pcm24WavExportError {
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
    ) -> Result<Self, Pcm24WavExportError> {
        if sample_rate == 0 {
            return Err(Pcm24WavExportError::InvalidSampleRate);
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
        write_header(&mut file, sample_rate, 0)?;
        export.file = Some(BufWriter::new(file));
        Ok(export)
    }

    /// Appends rendered interleaved stereo frames using bounded memory.
    pub fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), Pcm24WavExportError> {
        let added_bytes = (frames.len() as u64)
            .checked_mul(BYTES_PER_FRAME)
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
        let mut encoded = [0_u8; 3];
        for frame in frames {
            for sample in frame {
                let quantized = (sample.clamp(-1.0, 1.0) as f64 * 8_388_608.0)
                    .round()
                    .clamp(-8_388_608.0, 8_388_607.0) as i32;
                let bytes = quantized.to_le_bytes();
                encoded.copy_from_slice(&bytes[..3]);
                file.write_all(&encoded)?;
            }
        }
        self.data_bytes = data_bytes;
        Ok(())
    }

    /// Flushes the WAV header and atomically publishes the completed export.
    pub fn finish(mut self) -> Result<PathBuf, Pcm24WavExportError> {
        let mut file = self
            .file
            .take()
            .expect("unfinished export retains its file");
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        write_header(&mut file, self.sample_rate, self.data_bytes)?;
        file.flush()?;
        drop(file);

        // A same-directory hard link publishes the completed file atomically and fails rather
        // than replacing a destination created while the render was in progress.
        fs::hard_link(&self.temporary, &self.destination)?;
        let _ = fs::remove_file(&self.temporary);
        Ok(self.destination.clone())
    }
}

impl Drop for Pcm24WavExport {
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

fn write_header(file: &mut impl Write, sample_rate: u32, data_bytes: u64) -> io::Result<()> {
    let data_bytes = u32::try_from(data_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "WAV data exceeds RIFF limit"))?;
    let byte_rate = sample_rate
        .checked_mul(u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE / 8))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV byte rate overflow"))?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36_u32 + data_bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&CHANNELS.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&(CHANNELS * (BITS_PER_SAMPLE / 8)).to_le_bytes())?;
    file.write_all(&BITS_PER_SAMPLE.to_le_bytes())?;
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
