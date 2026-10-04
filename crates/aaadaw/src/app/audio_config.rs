use super::config_paths::{config_file_path, write_atomic};
use aaadaw_engine::MasterOutputCeiling;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "audio.conf";
const MAX_RECORDING_OFFSET_US: i32 = 5_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlaybackBackendSetting {
    Jack,
    PipeWire,
    Wasapi,
}

impl PlaybackBackendSetting {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "JACK" => Some(Self::Jack),
            "PipeWire" => Some(Self::PipeWire),
            "WASAPI" => Some(Self::Wasapi),
            _ => None,
        }
    }

    fn as_config_value(self) -> &'static str {
        match self {
            Self::Jack => "JACK",
            Self::PipeWire => "PipeWire",
            Self::Wasapi => "WASAPI",
        }
    }
}

#[cfg(feature = "audio-device")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RecordingPlacementCorrection {
    user_offset_us: i32,
    capture_latency_frames: Option<u32>,
}

#[cfg(feature = "audio-device")]
impl RecordingPlacementCorrection {
    pub(super) const fn new(user_offset_us: i32) -> Self {
        Self {
            user_offset_us,
            capture_latency_frames: None,
        }
    }

    pub(super) const fn with_capture_latency_frames(mut self, frames: Option<u32>) -> Self {
        self.capture_latency_frames = frames;
        self
    }

    pub(super) const fn capture_latency_frames(self) -> Option<u32> {
        self.capture_latency_frames
    }

    pub(super) fn apply(self, start_sample: u64, sample_rate: u32) -> Option<u64> {
        apply_recording_placement_correction(
            start_sample,
            sample_rate,
            self.user_offset_us,
            self.capture_latency_frames,
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct AudioSettings {
    pub(super) master_output_ceiling: MasterOutputCeiling,
    pub(super) recording_offset_us: i32,
    pub(super) playback_backend: Option<PlaybackBackendSetting>,
    pub(super) wasapi_output_device_id: Option<String>,
    pub(super) wasapi_input_device_id: Option<String>,
}

pub(super) fn load() -> Result<AudioSettings, String> {
    let Some(path) = config_path() else {
        return Ok(AudioSettings::default());
    };
    match std::fs::read_to_string(path) {
        Ok(contents) => parse(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AudioSettings::default()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn save(settings: &AudioSettings) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Err("no platform config directory is available".to_owned());
    };
    save_to(&path, settings).map_err(|error| error.to_string())
}

fn config_path() -> Option<PathBuf> {
    config_file_path(FILE_NAME)
}

fn parse(contents: &str) -> Result<AudioSettings, String> {
    let mut settings = AudioSettings::default();
    let mut found_ceiling = false;
    let mut found_recording_offset = false;
    let mut found_playback_backend = false;
    let mut found_wasapi_output_device = false;
    let mut found_wasapi_input_device = false;
    for (line_number, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("invalid audio config line {}", line_number + 1));
        };
        match key {
            "master_output_ceiling_dbfs" if !found_ceiling => {
                let ceiling_dbfs = value.parse::<i8>().map_err(|_| {
                    format!("invalid Master output ceiling on line {}", line_number + 1)
                })?;
                settings.master_output_ceiling =
                    MasterOutputCeiling::new(ceiling_dbfs).map_err(|_| {
                        format!(
                            "unsupported Master output ceiling on line {}",
                            line_number + 1
                        )
                    })?;
                found_ceiling = true;
            }
            "recording_placement_offset_ms" if !found_recording_offset => {
                settings.recording_offset_us =
                    parse_recording_offset_ms(value).ok_or_else(|| {
                        format!("invalid recording offset on line {}", line_number + 1)
                    })?;
                found_recording_offset = true;
            }
            "playback_backend" if !found_playback_backend => {
                settings.playback_backend = PlaybackBackendSetting::parse(value);
                found_playback_backend = true;
            }
            "wasapi_output_device" if !found_wasapi_output_device => {
                if value.len() > 4096 {
                    return Err(format!(
                        "WASAPI output device ID is too long on line {}",
                        line_number + 1
                    ));
                }
                settings.wasapi_output_device_id = (!value.is_empty()).then(|| value.to_owned());
                found_wasapi_output_device = true;
            }
            "wasapi_input_device" if !found_wasapi_input_device => {
                if value.len() > 4096 {
                    return Err(format!(
                        "WASAPI input device ID is too long on line {}",
                        line_number + 1
                    ));
                }
                settings.wasapi_input_device_id = (!value.is_empty()).then(|| value.to_owned());
                found_wasapi_input_device = true;
            }
            _ => return Err(format!("invalid audio config line {}", line_number + 1)),
        }
    }
    Ok(settings)
}

fn save_to(path: &Path, settings: &AudioSettings) -> io::Result<()> {
    let mut contents = Vec::new();
    writeln!(
        &mut contents,
        "master_output_ceiling_dbfs={}",
        settings.master_output_ceiling.as_dbfs()
    )?;
    writeln!(
        &mut contents,
        "recording_placement_offset_ms={}",
        format_recording_offset_ms(settings.recording_offset_us)
    )?;
    if let Some(playback_backend) = settings.playback_backend {
        writeln!(
            &mut contents,
            "playback_backend={}",
            playback_backend.as_config_value()
        )?;
    }
    if let Some(device_id) = &settings.wasapi_output_device_id {
        writeln!(&mut contents, "wasapi_output_device={device_id}")?;
    }
    if let Some(device_id) = &settings.wasapi_input_device_id {
        writeln!(&mut contents, "wasapi_input_device={device_id}")?;
    }
    write_atomic(path, &contents)
}

pub(super) fn parse_recording_offset_ms(value: &str) -> Option<i32> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let (negative, magnitude) = match value.as_bytes()[0] {
        b'-' => (true, &value[1..]),
        b'+' => (false, &value[1..]),
        _ => (false, value),
    };
    if magnitude.is_empty() {
        return None;
    }
    let mut pieces = magnitude.split('.');
    let whole = pieces.next()?.parse::<u32>().ok()?;
    let fraction = pieces.next().unwrap_or("");
    if pieces.next().is_some()
        || fraction.len() > 3
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let fraction = if fraction.is_empty() {
        0
    } else {
        let padded = format!("{fraction:0<3}");
        padded.parse::<u32>().ok()?
    };
    let magnitude_us = whole.checked_mul(1_000)?.checked_add(fraction)?;
    if magnitude_us > MAX_RECORDING_OFFSET_US as u32 {
        return None;
    }
    let signed = i32::try_from(magnitude_us).ok()?;
    Some(if negative { -signed } else { signed })
}

pub(super) fn format_recording_offset_ms(offset_us: i32) -> String {
    let sign = if offset_us < 0 { "-" } else { "" };
    let magnitude = offset_us.unsigned_abs();
    format!("{sign}{}.{:03}", magnitude / 1_000, magnitude % 1_000)
}

#[cfg(test)]
pub(super) fn apply_recording_offset(
    start_sample: u64,
    sample_rate: u32,
    offset_us: i32,
) -> Option<u64> {
    apply_recording_placement_correction(start_sample, sample_rate, offset_us, None)
}

#[cfg(any(feature = "audio-device", test))]
pub(super) fn apply_recording_placement_correction(
    start_sample: u64,
    sample_rate: u32,
    user_offset_us: i32,
    capture_latency_frames: Option<u32>,
) -> Option<u64> {
    if sample_rate == 0 || user_offset_us.unsigned_abs() > MAX_RECORDING_OFFSET_US as u32 {
        return None;
    }
    let user_offset_numerator = i128::from(user_offset_us).checked_mul(i128::from(sample_rate))?;
    let capture_latency_numerator =
        i128::from(capture_latency_frames.unwrap_or(0)).checked_mul(1_000_000)?;
    let numerator = user_offset_numerator.checked_sub(capture_latency_numerator)?;
    let rounded_numerator = if numerator < 0 {
        numerator.checked_sub(500_000)?
    } else {
        numerator.checked_add(500_000)?
    };
    let offset_frames = rounded_numerator / 1_000_000;
    let adjusted = i128::from(start_sample).checked_add(offset_frames)?;
    u64::try_from(adjusted).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_settings_round_trip() {
        let settings = AudioSettings {
            master_output_ceiling: MasterOutputCeiling::new(-6).unwrap(),
            recording_offset_us: -125_500,
            playback_backend: Some(PlaybackBackendSetting::PipeWire),
            wasapi_output_device_id: Some("wasapi:device/endpoint-01".to_owned()),
            wasapi_input_device_id: Some("wasapi:device/endpoint-02".to_owned()),
        };
        let path = std::env::temp_dir().join(format!(
            "aaadaw-audio-{}-{}.conf",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));

        save_to(&path, &settings).unwrap();

        assert_eq!(
            parse(&std::fs::read_to_string(&path).unwrap()).unwrap(),
            settings
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unsupported_audio_output_ceilings_are_rejected() {
        for contents in [
            "master_output_ceiling_dbfs=1\n",
            "master_output_ceiling_dbfs=-13\n",
            "master_output_ceiling_dbfs=-1\nmaster_output_ceiling_dbfs=0\n",
            "other=0\n",
        ] {
            assert!(parse(contents).is_err(), "accepted {contents:?}");
        }
    }

    #[test]
    fn old_audio_config_defaults_the_recording_offset_to_zero() {
        assert_eq!(
            parse("master_output_ceiling_dbfs=-1\n").unwrap(),
            AudioSettings::default()
        );
    }

    #[test]
    fn unknown_saved_backend_falls_back_without_invalidating_other_audio_settings() {
        let settings = parse(
            "master_output_ceiling_dbfs=-6\nrecording_placement_offset_ms=1.250\nplayback_backend=CoreAudio\n",
        )
        .unwrap();
        assert_eq!(
            settings.master_output_ceiling,
            MasterOutputCeiling::new(-6).unwrap()
        );
        assert_eq!(settings.recording_offset_us, 1_250);
        assert_eq!(settings.playback_backend, None);
    }

    #[test]
    fn saved_wasapi_output_device_id_round_trips_and_old_settings_default_to_system_device() {
        let id = "wasapi:\\\\?\\SWD#MMDEVAPI#endpoint";
        let settings = AudioSettings {
            wasapi_output_device_id: Some(id.to_owned()),
            ..AudioSettings::default()
        };
        let path = std::env::temp_dir().join(format!(
            "aaadaw-wasapi-audio-{}-{}.conf",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        save_to(&path, &settings).unwrap();
        assert_eq!(
            parse(&std::fs::read_to_string(&path).unwrap()).unwrap(),
            settings
        );
        assert_eq!(
            parse("master_output_ceiling_dbfs=-1\n").unwrap(),
            AudioSettings::default()
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn saved_wasapi_input_device_id_round_trips_and_old_settings_default_to_system_device() {
        let id = "wasapi:\\\\?\\SWD#MMDEVAPI#input-endpoint";
        let settings = AudioSettings {
            wasapi_input_device_id: Some(id.to_owned()),
            ..AudioSettings::default()
        };
        let path = std::env::temp_dir().join(format!(
            "aaadaw-wasapi-input-audio-{}-{}.conf",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        save_to(&path, &settings).unwrap();
        assert_eq!(
            parse(&std::fs::read_to_string(&path).unwrap()).unwrap(),
            settings
        );
        assert_eq!(
            parse("master_output_ceiling_dbfs=-1\n").unwrap(),
            AudioSettings::default()
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recording_offset_config_is_signed_bounded_and_has_microsecond_precision() {
        assert_eq!(parse_recording_offset_ms("-0.125"), Some(-125));
        assert_eq!(parse_recording_offset_ms("+12.5"), Some(12_500));
        assert_eq!(parse_recording_offset_ms("5000"), Some(5_000_000));
        for value in ["", "-", "1.0001", "5000.001", "-5000.001", "1.2.3", "nan"] {
            assert_eq!(parse_recording_offset_ms(value), None, "accepted {value:?}");
        }
        assert_eq!(format_recording_offset_ms(-125), "-0.125");
        assert_eq!(format_recording_offset_ms(12_500), "12.500");
    }

    #[test]
    fn recording_offset_maps_milliseconds_to_signed_project_samples_safely() {
        for (sample_rate, expected) in [(44_100, 22), (48_000, 24)] {
            assert_eq!(
                apply_recording_offset(100_000, sample_rate, 500),
                Some(100_000 + expected)
            );
            assert_eq!(
                apply_recording_offset(100_000, sample_rate, -500),
                Some(100_000 - expected)
            );
        }
        assert_eq!(apply_recording_offset(0, 48_000, -1_000), None);
        assert_eq!(apply_recording_offset(u64::MAX - 1, 48_000, 1_000), None);
        assert_eq!(apply_recording_offset(1, 0, 1_000), None);
        assert_eq!(apply_recording_offset(1, 48_000, i32::MAX), None);
    }

    #[test]
    fn reported_capture_latency_moves_take_earlier_before_user_calibration() {
        assert_eq!(
            apply_recording_placement_correction(48_000, 48_000, 0, Some(240)),
            Some(47_760)
        );
        assert_eq!(
            apply_recording_placement_correction(48_000, 48_000, 5_000, Some(240)),
            Some(48_000)
        );
        assert_eq!(
            apply_recording_placement_correction(48_000, 48_000, 0, None),
            Some(48_000)
        );
    }

    #[test]
    fn reported_capture_latency_rejects_timeline_underflow_and_overflow() {
        assert_eq!(
            apply_recording_placement_correction(239, 48_000, 0, Some(240)),
            None
        );
        assert_eq!(
            apply_recording_placement_correction(u64::MAX, 48_000, 5_000, Some(1)),
            None
        );
    }
}
