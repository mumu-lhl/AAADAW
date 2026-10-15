use super::{App, config_paths};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct LoopTimePreferences {
    version: u32,
    pub(super) linked: bool,
    pub(super) repeat_enabled: bool,
}

impl Default for LoopTimePreferences {
    fn default() -> Self {
        Self {
            version: 1,
            linked: true,
            repeat_enabled: false,
        }
    }
}

fn load_path(path: &Path) -> Result<LoopTimePreferences, String> {
    let contents = match std::fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LoopTimePreferences::default());
        }
        Err(error) => return Err(error.to_string()),
    };
    let preferences: LoopTimePreferences =
        serde_json::from_slice(&contents).map_err(|error| error.to_string())?;
    if preferences.version != 1 {
        return Err("Unsupported loop/time selection preferences version".to_owned());
    }
    Ok(preferences)
}

pub(super) fn load() -> Result<LoopTimePreferences, String> {
    config_paths::config_file_path("loop-time-preferences.json").map_or_else(
        || Ok(LoopTimePreferences::default()),
        |path| load_path(&path),
    )
}

fn save_path(path: &Path, preferences: LoopTimePreferences) -> Result<(), String> {
    let contents = serde_json::to_vec_pretty(&preferences).map_err(|error| error.to_string())?;
    config_paths::write_atomic(path, &contents).map_err(|error| error.to_string())
}

impl App {
    pub(super) fn schedule_loop_time_preferences_save(&mut self) {
        self.loop_time_preferences_save_at = Some(Instant::now() + Duration::from_millis(500));
    }

    pub(super) fn flush_loop_time_preferences_if_due(&mut self) {
        if self
            .loop_time_preferences_save_at
            .is_some_and(|at| Instant::now() >= at)
        {
            self.flush_loop_time_preferences();
        }
    }

    pub(super) fn flush_loop_time_preferences(&mut self) {
        if self.loop_time_preferences_save_at.take().is_none() {
            return;
        }
        let preferences = LoopTimePreferences {
            linked: self.timeline.loop_points_linked,
            repeat_enabled: self.repeat_enabled,
            ..Default::default()
        };
        let result = config_paths::config_file_path("loop-time-preferences.json")
            .ok_or_else(|| "No application configuration directory".to_owned())
            .and_then(|path| save_path(&path, preferences));
        if let Err(error) = result {
            self.status = format!("Loop/time selection preference could not be saved: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_time_preferences_default_round_trip_and_reject_unknown_versions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("loop-time-preferences.json");
        assert_eq!(load_path(&path).unwrap(), LoopTimePreferences::default());

        let preferences = LoopTimePreferences {
            linked: false,
            repeat_enabled: true,
            ..Default::default()
        };
        save_path(&path, preferences).unwrap();
        assert_eq!(load_path(&path).unwrap(), preferences);

        std::fs::write(&path, r#"{"version":1,"linked":false}"#).unwrap();
        assert_eq!(
            load_path(&path).unwrap(),
            LoopTimePreferences {
                linked: false,
                ..Default::default()
            }
        );

        std::fs::write(&path, r#"{"version":2,"linked":true}"#).unwrap();
        assert!(load_path(&path).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"version":2,"linked":true}"#
        );
    }
}
