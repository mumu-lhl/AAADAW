use super::{App, MidiQuantizeDialogState, config_paths};
use crate::app::messages::{MidiQuantizeOperation, MidiQuantizeScope};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Duration, Instant};

const PREFERENCES_FILE: &str = "midi-quantize-preferences.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct MidiQuantizePreferences {
    version: u32,
    pub(super) scope: MidiQuantizeScope,
    pub(super) operation: MidiQuantizeOperation,
    pub(super) strength_percent: u8,
    pub(super) bypass: bool,
    pub(super) fix_overlaps: bool,
}

impl Default for MidiQuantizePreferences {
    fn default() -> Self {
        Self {
            version: 1,
            scope: MidiQuantizeScope::SelectedNotes,
            operation: MidiQuantizeOperation::Position,
            strength_percent: 100,
            bypass: false,
            fix_overlaps: true,
        }
    }
}

impl From<MidiQuantizeDialogState> for MidiQuantizePreferences {
    fn from(dialog: MidiQuantizeDialogState) -> Self {
        Self {
            scope: dialog.scope,
            operation: dialog.operation,
            strength_percent: dialog.strength_percent,
            bypass: dialog.bypass,
            fix_overlaps: dialog.fix_overlaps,
            ..Self::default()
        }
    }
}

fn validate(preferences: MidiQuantizePreferences) -> Result<MidiQuantizePreferences, String> {
    if preferences.version != 1 {
        return Err("Unsupported MIDI Quantize preferences version".to_owned());
    }
    if preferences.strength_percent > 100 {
        return Err("MIDI Quantize strength must be in 0..=100".to_owned());
    }
    Ok(preferences)
}

fn load_path(path: &Path) -> Result<MidiQuantizePreferences, String> {
    let contents = match std::fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(MidiQuantizePreferences::default());
        }
        Err(error) => return Err(error.to_string()),
    };
    let preferences: MidiQuantizePreferences =
        serde_json::from_slice(&contents).map_err(|error| error.to_string())?;
    validate(preferences)
}

pub(super) fn load() -> Result<MidiQuantizePreferences, String> {
    config_paths::config_file_path(PREFERENCES_FILE).map_or_else(
        || Ok(MidiQuantizePreferences::default()),
        |path| load_path(&path),
    )
}

fn save_path(path: &Path, preferences: MidiQuantizePreferences) -> Result<(), String> {
    let preferences = validate(preferences)?;
    let contents = serde_json::to_vec_pretty(&preferences).map_err(|error| error.to_string())?;
    config_paths::write_atomic(path, &contents).map_err(|error| error.to_string())
}

impl App {
    pub(super) fn remember_midi_quantize_dialog(&mut self) {
        let Some(dialog) = self.midi_quantize_dialog else {
            return;
        };
        self.midi_quantize_preferences = dialog.into();
        self.midi_quantize_preferences_save_at = Some(Instant::now() + Duration::from_millis(500));
    }

    pub(super) fn flush_midi_quantize_preferences_if_due(&mut self) {
        if self
            .midi_quantize_preferences_save_at
            .is_some_and(|at| Instant::now() >= at)
        {
            self.flush_midi_quantize_preferences();
        }
    }

    pub(super) fn flush_midi_quantize_preferences(&mut self) {
        if self.midi_quantize_preferences_save_at.take().is_none() {
            return;
        }
        let result = config_paths::config_file_path(PREFERENCES_FILE)
            .ok_or_else(|| "No application configuration directory".to_owned())
            .and_then(|path| save_path(&path, self.midi_quantize_preferences));
        if let Err(error) = result {
            self.status = format!("MIDI Quantize preferences could not be saved: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_default_and_round_trip_persisted_dialog_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(PREFERENCES_FILE);
        assert_eq!(
            load_path(&path).unwrap(),
            MidiQuantizePreferences::default()
        );

        let preferences = MidiQuantizePreferences {
            scope: MidiQuantizeScope::SelectedEvents,
            operation: MidiQuantizeOperation::PositionAndNoteEnd,
            strength_percent: 60,
            bypass: false,
            fix_overlaps: true,
            ..Default::default()
        };
        save_path(&path, preferences).unwrap();
        assert_eq!(load_path(&path).unwrap(), preferences);
    }

    #[test]
    fn invalid_preferences_are_rejected_without_overwriting_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(PREFERENCES_FILE);
        let invalid = br#"{"version":1,"scope":"all_notes","operation":"position","strength_percent":101,"bypass":false,"fix_overlaps":true}"#;
        std::fs::write(&path, invalid).unwrap();

        assert!(load_path(&path).is_err());
        assert!(
            save_path(
                &path,
                MidiQuantizePreferences {
                    strength_percent: 101,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), invalid);
    }
}
