use super::{App, config_paths};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct MidiHumanizePreferences {
    pub(super) version: u32,
    pub(super) all_notes: bool,
    pub(super) timing_amount_percent: u8,
    pub(super) velocity_amount_percent: u8,
    pub(super) timing_bias_percent: i16,
}

impl Default for MidiHumanizePreferences {
    fn default() -> Self {
        Self {
            version: 1,
            all_notes: false,
            timing_amount_percent: 0,
            velocity_amount_percent: 0,
            timing_bias_percent: 0,
        }
    }
}

fn validate(preferences: MidiHumanizePreferences) -> Result<MidiHumanizePreferences, String> {
    if preferences.version != 1 {
        return Err("Unsupported MIDI Humanize preferences version".to_owned());
    }
    if preferences.timing_amount_percent > 100
        || preferences.velocity_amount_percent > 100
        || !(-100..=100).contains(&preferences.timing_bias_percent)
    {
        return Err("MIDI Humanize preferences are outside their supported ranges".to_owned());
    }
    Ok(preferences)
}

fn load_path(path: &Path) -> Result<MidiHumanizePreferences, String> {
    let contents = match std::fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(MidiHumanizePreferences::default());
        }
        Err(error) => return Err(error.to_string()),
    };
    let preferences: MidiHumanizePreferences =
        serde_json::from_slice(&contents).map_err(|error| error.to_string())?;
    validate(preferences)
}

pub(super) fn load() -> Result<MidiHumanizePreferences, String> {
    config_paths::config_file_path("midi-humanize-preferences.json").map_or_else(
        || Ok(MidiHumanizePreferences::default()),
        |path| load_path(&path),
    )
}

fn save_path(path: &Path, preferences: MidiHumanizePreferences) -> Result<(), String> {
    let preferences = validate(preferences)?;
    let contents = serde_json::to_vec_pretty(&preferences).map_err(|error| error.to_string())?;
    config_paths::write_atomic(path, &contents).map_err(|error| error.to_string())
}

impl App {
    pub(super) fn schedule_midi_humanize_preferences_save(&mut self) {
        self.midi_humanize_preferences_save_at = Some(Instant::now() + Duration::from_millis(500));
    }

    pub(super) fn flush_midi_humanize_preferences_if_due(&mut self) {
        if self
            .midi_humanize_preferences_save_at
            .is_some_and(|at| Instant::now() >= at)
        {
            self.flush_midi_humanize_preferences();
        }
    }

    pub(super) fn flush_midi_humanize_preferences(&mut self) {
        if self.midi_humanize_preferences_save_at.take().is_none() {
            return;
        }
        let result = config_paths::config_file_path("midi-humanize-preferences.json")
            .ok_or_else(|| "No application configuration directory".to_owned())
            .and_then(|path| save_path(&path, self.midi_humanize_preferences));
        if let Err(error) = result {
            self.status = format!("MIDI Humanize preferences could not be saved: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_default_and_round_trip_persisted_dialog_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("midi-humanize-preferences.json");
        assert_eq!(
            load_path(&path).unwrap(),
            MidiHumanizePreferences::default()
        );

        let preferences = MidiHumanizePreferences {
            all_notes: true,
            timing_amount_percent: 37,
            velocity_amount_percent: 61,
            timing_bias_percent: -42,
            ..Default::default()
        };
        save_path(&path, preferences).unwrap();
        assert_eq!(load_path(&path).unwrap(), preferences);
    }

    #[test]
    fn invalid_preferences_are_rejected_without_overwriting_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("midi-humanize-preferences.json");
        let invalid = br#"{"version":1,"all_notes":false,"timing_amount_percent":101,"velocity_amount_percent":0,"timing_bias_percent":0}"#;
        std::fs::write(&path, invalid).unwrap();

        assert!(load_path(&path).is_err());
        assert!(
            save_path(
                &path,
                MidiHumanizePreferences {
                    timing_bias_percent: 101,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), invalid);
    }
}
