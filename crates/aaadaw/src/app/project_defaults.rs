use super::config_paths;
use aaadaw_core::{
    BpmBasis, DEFAULT_PPQ, DEFAULT_SAMPLE_RATE, DEFAULT_TEMPO_BPM, FrameRate, Project,
    ProjectSettings, TimeSignature,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ProjectDefaults {
    version: u32,
    sample_rate: u32,
    tempo_bpm: f64,
    meter_numerator: u32,
    meter_denominator: u32,
    bpm_basis: u32,
    start_measure: i32,
    base_ruler_label_spacing_on_measure: bool,
    force_tempo_changes_to_whole_samples: bool,
    project_start_time_offset_seconds: f64,
    fractional_frame_offset_allowed: bool,
    frame_rate: u32,
}

impl Default for ProjectDefaults {
    fn default() -> Self {
        Self {
            version: 1,
            sample_rate: DEFAULT_SAMPLE_RATE,
            tempo_bpm: DEFAULT_TEMPO_BPM,
            meter_numerator: 4,
            meter_denominator: 4,
            bpm_basis: BpmBasis::Quarter.storage_code(),
            start_measure: 1,
            base_ruler_label_spacing_on_measure: true,
            force_tempo_changes_to_whole_samples: false,
            project_start_time_offset_seconds: 0.0,
            fractional_frame_offset_allowed: false,
            frame_rate: FrameRate::default().storage_code(),
        }
    }
}

impl ProjectDefaults {
    pub(super) fn from_project(project: &Project) -> Self {
        let settings = project.settings();
        let meter = project.time_signature_at_tick(0);
        Self {
            version: 1,
            sample_rate: settings.sample_rate(),
            tempo_bpm: project.tempo_at_tick(0),
            meter_numerator: meter.numerator(),
            meter_denominator: meter.denominator(),
            bpm_basis: settings.bpm_basis().storage_code(),
            start_measure: settings.start_measure(),
            base_ruler_label_spacing_on_measure: settings.base_ruler_label_spacing_on_measure(),
            force_tempo_changes_to_whole_samples: settings.force_tempo_changes_to_whole_samples(),
            project_start_time_offset_seconds: settings.project_start_time_offset_seconds(),
            fractional_frame_offset_allowed: settings
                .project_start_time_fractional_frame_offset_allowed(),
            frame_rate: settings.frame_rate().storage_code(),
        }
    }

    pub(super) fn create_project(&self) -> Result<Project, String> {
        if self.version != 1 {
            return Err("Unsupported default project settings version".to_owned());
        }
        let bpm_basis = BpmBasis::from_storage_code(self.bpm_basis)
            .ok_or_else(|| "Unknown default project BPM basis".to_owned())?;
        let frame_rate = FrameRate::from_storage_code(self.frame_rate)
            .ok_or_else(|| "Unknown default project frame rate".to_owned())?;
        let meter = TimeSignature::new(self.meter_numerator, self.meter_denominator)
            .map_err(|error| format!("Invalid default project time signature: {error}"))?;
        let settings = ProjectSettings::new(self.sample_rate, DEFAULT_PPQ, self.tempo_bpm)
            .and_then(|settings| settings.with_bpm_basis(bpm_basis))
            .and_then(|settings| {
                settings
                    .with_project_start_time_offset_seconds(self.project_start_time_offset_seconds)
            })
            .map(|settings| {
                settings
                    .with_start_measure(self.start_measure)
                    .with_base_ruler_label_spacing_on_measure(
                        self.base_ruler_label_spacing_on_measure,
                    )
                    .with_force_tempo_changes_to_whole_samples(
                        self.force_tempo_changes_to_whole_samples,
                    )
                    .with_frame_rate(frame_rate)
                    .with_project_start_time_fractional_frame_offset_allowed(
                        self.fractional_frame_offset_allowed,
                    )
            })
            .map_err(|error| format!("Invalid default project settings: {error:?}"))?;
        Project::with_settings_and_initial_time_signature(settings, meter)
            .map_err(|error| format!("Invalid default project time signature: {error:?}"))
    }
}

fn load_path(path: &Path) -> Result<ProjectDefaults, String> {
    let contents = match std::fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProjectDefaults::default());
        }
        Err(error) => return Err(error.to_string()),
    };
    let defaults: ProjectDefaults =
        serde_json::from_slice(&contents).map_err(|error| error.to_string())?;
    defaults.create_project()?;
    Ok(defaults)
}

pub(super) fn load() -> Result<ProjectDefaults, String> {
    config_paths::config_file_path("default-project-settings.json")
        .map_or_else(|| Ok(ProjectDefaults::default()), |path| load_path(&path))
}

fn save_path(path: &Path, defaults: &ProjectDefaults) -> Result<(), String> {
    defaults.create_project()?;
    let contents = serde_json::to_vec_pretty(defaults).map_err(|error| error.to_string())?;
    config_paths::write_atomic(path, &contents).map_err(|error| error.to_string())
}

pub(super) fn save(defaults: &ProjectDefaults) -> Result<(), String> {
    let path = config_paths::config_file_path("default-project-settings.json")
        .ok_or_else(|| "No application configuration directory".to_owned())?;
    save_path(&path, defaults)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_core::{DawAction, ProjectNotes};

    #[test]
    fn defaults_round_trip_and_create_a_clean_project_without_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("default-project-settings.json");
        let mut source = Project::new();
        source
            .apply(DawAction::SetTempo {
                start_tick: 0,
                bpm: 96.0,
            })
            .unwrap();
        source
            .apply(DawAction::SetTimeSignature {
                start_tick: 0,
                signature: TimeSignature::new(7, 8).unwrap(),
            })
            .unwrap();
        source
            .apply(DawAction::SetProjectBpmBasis {
                basis: BpmBasis::Eighth,
            })
            .unwrap();

        let defaults = ProjectDefaults::from_project(&source);
        save_path(&path, &defaults).unwrap();
        let restored = load_path(&path).unwrap();
        let project = restored.create_project().unwrap();
        assert_eq!(ProjectDefaults::from_project(&project), defaults);
        assert_eq!(project.tempo_at_tick(0), 96.0);
        assert_eq!(
            project.time_signature_at_tick(0),
            TimeSignature::new(7, 8).unwrap()
        );
        assert_eq!(project.settings().bpm_basis(), BpmBasis::Eighth);
        assert!(!project.can_undo());
        assert!(project.tracks().is_empty());
        assert!(project.audio_items().is_empty());
        assert!(project.midi_items().is_empty());
        assert_eq!(project.project_notes(), &ProjectNotes::default());
    }

    #[test]
    fn invalid_defaults_are_rejected_without_replacing_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("default-project-settings.json");
        let invalid = ProjectDefaults {
            sample_rate: 0,
            ..ProjectDefaults::default()
        };
        std::fs::write(&path, b"existing defaults").unwrap();
        assert!(save_path(&path, &invalid).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"existing defaults");
        std::fs::write(&path, br#"{"version":2}"#).unwrap();
        assert!(load_path(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), br#"{"version":2}"#);
    }
}
