use super::config_paths;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct RenderWildcardPreferences {
    version: u32,
    pub(super) allow_slash_in_substitutions: bool,
}

impl Default for RenderWildcardPreferences {
    fn default() -> Self {
        Self {
            version: 1,
            allow_slash_in_substitutions: true,
        }
    }
}

fn load_path(path: &Path) -> Result<RenderWildcardPreferences, String> {
    let contents = match std::fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RenderWildcardPreferences::default());
        }
        Err(error) => return Err(error.to_string()),
    };
    let preferences: RenderWildcardPreferences =
        serde_json::from_slice(&contents).map_err(|error| error.to_string())?;
    if preferences.version != 1 {
        return Err("Unsupported Render wildcard preferences version".to_owned());
    }
    Ok(preferences)
}

pub(super) fn load() -> Result<RenderWildcardPreferences, String> {
    config_paths::config_file_path("render-wildcard-preferences.json").map_or_else(
        || Ok(RenderWildcardPreferences::default()),
        |path| load_path(&path),
    )
}

pub(super) fn save(allow_slash_in_substitutions: bool) -> Result<(), String> {
    let path = config_paths::config_file_path("render-wildcard-preferences.json")
        .ok_or_else(|| "No application configuration directory".to_owned())?;
    save_path(
        &path,
        RenderWildcardPreferences {
            allow_slash_in_substitutions,
            ..Default::default()
        },
    )
}

fn save_path(path: &Path, preferences: RenderWildcardPreferences) -> Result<(), String> {
    let contents = serde_json::to_vec_pretty(&preferences).map_err(|error| error.to_string())?;
    config_paths::write_atomic(path, &contents).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_wildcard_preferences_default_on_and_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("render-wildcard-preferences.json");
        assert_eq!(
            load_path(&path).unwrap(),
            RenderWildcardPreferences::default()
        );
        assert!(RenderWildcardPreferences::default().allow_slash_in_substitutions);

        let preferences = RenderWildcardPreferences {
            allow_slash_in_substitutions: false,
            ..Default::default()
        };
        save_path(&path, preferences).unwrap();
        assert_eq!(load_path(&path).unwrap(), preferences);
    }

    #[test]
    fn unsupported_render_wildcard_preferences_version_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("render-wildcard-preferences.json");
        std::fs::write(
            &path,
            r#"{"version":2,"allow_slash_in_substitutions":true}"#,
        )
        .unwrap();
        assert!(load_path(&path).is_err());
    }
}
