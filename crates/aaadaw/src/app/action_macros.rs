use super::config_paths::{config_file_path, write_atomic};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "action-macros.json";
pub(super) const MAX_MACRO_NAME_CHARS: usize = 64;
pub(super) const MAX_MACRO_STEPS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActionMacro {
    pub(super) id: u64,
    pub(super) name: String,
    pub(super) steps: Vec<String>,
}

pub(super) fn load() -> Result<Vec<ActionMacro>, String> {
    let Some(path) = config_path() else {
        return Ok(Vec::new());
    };
    match std::fs::read_to_string(path) {
        Ok(contents) => parse(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn save(macros: &[ActionMacro]) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Err("no platform config directory is available".to_owned());
    };
    save_to(&path, macros).map_err(|error| error.to_string())
}

pub(super) fn validate(
    mut macros: Vec<ActionMacro>,
    supported_steps: &HashSet<String>,
) -> Result<Vec<ActionMacro>, String> {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for action_macro in &mut macros {
        if action_macro.id == 0 || !ids.insert(action_macro.id) {
            return Err("macro IDs must be unique positive integers".to_owned());
        }
        let name = action_macro.name.trim();
        if name.is_empty() || name.chars().count() > MAX_MACRO_NAME_CHARS {
            return Err(format!(
                "macro names must contain 1 to {MAX_MACRO_NAME_CHARS} characters"
            ));
        }
        if !names.insert(name.to_lowercase()) {
            return Err(format!("duplicate macro name: {name}"));
        }
        action_macro.name = name.to_owned();
        if action_macro.steps.is_empty() || action_macro.steps.len() > MAX_MACRO_STEPS {
            return Err(format!("macros must contain 1 to {MAX_MACRO_STEPS} steps"));
        }
        for step in &action_macro.steps {
            if !supported_steps.contains(step) {
                return Err(format!("unsupported macro step: {step}"));
            }
        }
    }
    Ok(macros)
}

fn config_path() -> Option<PathBuf> {
    config_file_path(FILE_NAME)
}

fn parse(contents: &str) -> Result<Vec<ActionMacro>, String> {
    serde_json::from_str(contents).map_err(|error| format!("invalid macro config: {error}"))
}

fn save_to(path: &Path, macros: &[ActionMacro]) -> io::Result<()> {
    let mut contents = serde_json::to_vec_pretty(macros).map_err(io::Error::other)?;
    contents.push(b'\n');
    write_atomic(path, &contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supported_steps() -> HashSet<String> {
        HashSet::from([
            "view.arrangement-workspace".to_owned(),
            "view.mixer-workspace".to_owned(),
        ])
    }

    fn action_macro(id: u64, name: &str, steps: &[&str]) -> ActionMacro {
        ActionMacro {
            id,
            name: name.to_owned(),
            steps: steps.iter().map(|step| (*step).to_owned()).collect(),
        }
    }

    #[test]
    fn macros_validate_ids_names_steps_and_supported_action_ids() {
        let supported = supported_steps();
        let valid = vec![action_macro(
            1,
            "Arrange and mix",
            &["view.arrangement-workspace", "view.mixer-workspace"],
        )];
        assert_eq!(validate(valid.clone(), &supported).unwrap(), valid);
        assert!(
            validate(
                vec![action_macro(0, "A", &["view.arrangement-workspace"])],
                &supported
            )
            .is_err()
        );
        assert!(
            validate(
                vec![
                    action_macro(1, "A", &["view.arrangement-workspace"]),
                    action_macro(1, "B", &["view.mixer-workspace"])
                ],
                &supported
            )
            .is_err()
        );
        assert!(
            validate(
                vec![action_macro(1, " ", &["view.arrangement-workspace"])],
                &supported
            )
            .is_err()
        );
        assert!(
            validate(
                vec![action_macro(1, "A", &["file.open-project"])],
                &supported
            )
            .is_err()
        );
        assert!(validate(vec![action_macro(1, "A", &[])], &supported).is_err());
        assert!(
            validate(
                vec![
                    action_macro(1, "A", &["view.arrangement-workspace"]),
                    action_macro(2, "a", &["view.mixer-workspace"])
                ],
                &supported
            )
            .is_err()
        );
    }

    #[test]
    fn macro_config_round_trips_and_malformed_config_is_rejected() {
        let path = std::env::temp_dir().join(format!(
            "aaadaw-macros-{}-{}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let macros = vec![action_macro(
            12,
            "Arrange and mix",
            &["view.arrangement-workspace", "view.mixer-workspace"],
        )];
        save_to(&path, &macros).unwrap();
        assert_eq!(
            parse(&std::fs::read_to_string(&path).unwrap()).unwrap(),
            macros
        );
        assert!(parse("[{broken json]").is_err());
        assert!(parse(r#"[{"id":1,"name":"A","steps":[],"other":true}]"#).is_err());
        let _ = std::fs::remove_file(path);
    }
}
