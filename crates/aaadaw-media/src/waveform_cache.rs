use crate::{AudioWaveform, WaveformPeak};
use std::collections::HashSet;
use std::io::{self, Cursor, Read, Write};

const MAGIC: &[u8; 8] = b"AAAPEAKS";
const VERSION: u32 = 1;
const MAX_CACHE_BYTES: usize = 128 * 1024 * 1024;
const MAX_CACHE_ENTRIES: u32 = 100_000;
const MAX_MEDIA_REF_BYTES: u32 = 4_096;
const MAX_PEAKS_PER_ENTRY: u32 = 32_768;

/// One cached waveform tied to the exact source bytes used to decode it.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioWaveformCacheEntry {
    pub media_ref: String,
    pub source_hash: [u8; 32],
    pub waveform: AudioWaveform,
}

/// Encodes waveform entries into the versioned `.aaapeaks` binary format.
pub fn encode_waveform_cache(entries: &[AudioWaveformCacheEntry]) -> io::Result<Vec<u8>> {
    let entry_count = u32::try_from(entries.len())
        .ok()
        .filter(|count| *count <= MAX_CACHE_ENTRIES)
        .ok_or_else(|| invalid_data("too many waveform cache entries"))?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.write_all(&VERSION.to_le_bytes())?;
    bytes.write_all(&entry_count.to_le_bytes())?;
    for entry in entries {
        let media_ref = entry.media_ref.as_bytes();
        let media_ref_len = u32::try_from(media_ref.len())
            .ok()
            .filter(|len| *len <= MAX_MEDIA_REF_BYTES)
            .ok_or_else(|| invalid_data("waveform cache media reference is too long"))?;
        bytes.write_all(&media_ref_len.to_le_bytes())?;
        bytes.write_all(media_ref)?;
        bytes.write_all(&entry.source_hash)?;
        bytes.write_all(&entry.waveform.sample_rate().to_le_bytes())?;
        bytes.write_all(&entry.waveform.frame_count().to_le_bytes())?;
        let level_count = u32::try_from(entry.waveform.levels().len())
            .ok()
            .filter(|count| *count <= 32)
            .ok_or_else(|| invalid_data("waveform cache entry has too many levels"))?;
        bytes.write_all(&level_count.to_le_bytes())?;
        for level in entry.waveform.levels() {
            let peak_count = u32::try_from(level.peaks().len())
                .ok()
                .filter(|count| *count <= MAX_PEAKS_PER_ENTRY)
                .ok_or_else(|| invalid_data("waveform cache level has too many peaks"))?;
            bytes.write_all(&level.frames_per_peak().to_le_bytes())?;
            bytes.write_all(&peak_count.to_le_bytes())?;
            for peak in level.peaks() {
                bytes.write_all(&peak.min.to_le_bytes())?;
                bytes.write_all(&peak.max.to_le_bytes())?;
            }
        }
        if bytes.len() > MAX_CACHE_BYTES {
            return Err(invalid_data("waveform cache exceeds the size limit"));
        }
    }
    Ok(bytes)
}

/// Decodes and validates a `.aaapeaks` file. Invalid or unsupported data is rejected as a unit.
pub fn decode_waveform_cache(bytes: &[u8]) -> io::Result<Vec<AudioWaveformCacheEntry>> {
    decode_waveform_cache_with_limit(bytes, MAX_CACHE_BYTES)
}

fn decode_waveform_cache_with_limit(
    bytes: &[u8],
    max_bytes: usize,
) -> io::Result<Vec<AudioWaveformCacheEntry>> {
    if bytes.len() > max_bytes {
        return Err(invalid_data("waveform cache exceeds the size limit"));
    }
    let mut reader = Cursor::new(bytes);
    let mut magic = [0; 8];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid_data("invalid waveform cache signature"));
    }
    let version = read_u32(&mut reader)?;
    if version != VERSION {
        return Err(invalid_data("unsupported waveform cache version"));
    }
    let entry_count = read_u32(&mut reader)?;
    if entry_count > MAX_CACHE_ENTRIES {
        return Err(invalid_data("too many waveform cache entries"));
    }
    let mut entries = Vec::with_capacity(entry_count as usize);
    let mut media_refs = HashSet::with_capacity(entry_count as usize);
    for _ in 0..entry_count {
        let media_ref_len = read_u32(&mut reader)?;
        if media_ref_len == 0 || media_ref_len > MAX_MEDIA_REF_BYTES {
            return Err(invalid_data(
                "invalid waveform cache media reference length",
            ));
        }
        let mut media_ref = vec![0; media_ref_len as usize];
        reader.read_exact(&mut media_ref)?;
        let media_ref = String::from_utf8(media_ref)
            .map_err(|_| invalid_data("waveform cache media reference is not UTF-8"))?;
        if !media_refs.insert(media_ref.clone()) {
            return Err(invalid_data("duplicate waveform cache media reference"));
        }
        let mut source_hash = [0; 32];
        reader.read_exact(&mut source_hash)?;
        let sample_rate = read_u32(&mut reader)?;
        let frame_count = read_u64(&mut reader)?;
        let level_count = read_u32(&mut reader)?;
        if level_count == 0 || level_count > 32 {
            return Err(invalid_data("invalid waveform cache level count"));
        }
        let mut levels = Vec::with_capacity(level_count as usize);
        for _ in 0..level_count {
            let frames_per_peak = read_u32(&mut reader)?;
            let peak_count = read_u32(&mut reader)?;
            if peak_count > MAX_PEAKS_PER_ENTRY {
                return Err(invalid_data("waveform cache level has too many peaks"));
            }
            let mut peaks = Vec::with_capacity(peak_count as usize);
            for _ in 0..peak_count {
                peaks.push(WaveformPeak {
                    min: read_f32(&mut reader)?,
                    max: read_f32(&mut reader)?,
                });
            }
            levels.push((frames_per_peak, peaks));
        }
        let Some((frames_per_peak, peaks)) = levels.first().cloned() else {
            return Err(invalid_data("waveform cache entry has no base level"));
        };
        let waveform = AudioWaveform::from_base(sample_rate, frames_per_peak, frame_count, peaks)
            .ok_or_else(|| invalid_data("invalid waveform cache peak data"))?;
        if waveform.levels().len() != levels.len()
            || waveform.levels().iter().zip(levels.iter()).any(
                |(actual, (frames_per_peak, peaks))| {
                    actual.frames_per_peak() != *frames_per_peak || actual.peaks() != peaks
                },
            )
        {
            return Err(invalid_data("waveform cache levels are inconsistent"));
        }
        entries.push(AudioWaveformCacheEntry {
            media_ref,
            source_hash,
            waveform,
        });
    }
    if reader.position() != bytes.len() as u64 {
        return Err(invalid_data("trailing bytes in waveform cache"));
    }
    Ok(entries)
}

fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_f32(reader: &mut impl Read) -> io::Result<f32> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(f32::from_le_bytes(bytes))
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_round_trip_preserves_waveform_and_source_identity() {
        let waveform = AudioWaveform::from_base(
            48_000,
            256,
            512,
            vec![
                WaveformPeak {
                    min: -0.5,
                    max: 0.25,
                },
                WaveformPeak {
                    min: -1.0,
                    max: 1.0,
                },
            ],
        )
        .unwrap();
        let entry = AudioWaveformCacheEntry {
            media_ref: "asset://melody".to_owned(),
            source_hash: [42; 32],
            waveform,
        };
        let decoded =
            decode_waveform_cache(&encode_waveform_cache(&[entry.clone()]).unwrap()).unwrap();
        assert_eq!(decoded, vec![entry]);
        assert_eq!(decoded[0].waveform.levels().len(), 2);
    }

    #[test]
    fn corrupt_cache_is_rejected_without_partial_entries() {
        let mut bytes = encode_waveform_cache(&[]).unwrap();
        bytes[0] = b'X';
        assert_eq!(
            decode_waveform_cache(&bytes).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn cache_rejects_waveforms_over_the_peak_limit() {
        let peaks = vec![WaveformPeak { min: 0.0, max: 0.0 }; MAX_PEAKS_PER_ENTRY as usize + 1];
        let waveform =
            AudioWaveform::from_base(48_000, 256, u64::from(MAX_PEAKS_PER_ENTRY + 1) * 256, peaks)
                .unwrap();
        let entry = AudioWaveformCacheEntry {
            media_ref: "asset://oversized".to_owned(),
            source_hash: [0; 32],
            waveform,
        };
        assert_eq!(
            encode_waveform_cache(&[entry]).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn cache_rejects_files_over_the_byte_limit() {
        assert_eq!(
            decode_waveform_cache_with_limit(&[0; 9], 8)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}
