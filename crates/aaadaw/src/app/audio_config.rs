use super::config_paths::config_file_path;
use aaadaw_engine::MasterOutputCeiling;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "audio.conf";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct AudioOutputSettings {
    pub(super) master_output_ceiling: MasterOutputCeiling,
}

pub(super) fn load() -> Result<AudioOutputSettings, String> {
    let Some(path) = config_path() else {
        return Ok(AudioOutputSettings::default());
    };
    match std::fs::read_to_string(path) {
        Ok(contents) => parse(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AudioOutputSettings::default()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn save(settings: AudioOutputSettings) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Err("no platform config directory is available".to_owned());
    };
    save_to(&path, settings).map_err(|error| error.to_string())
}

fn config_path() -> Option<PathBuf> {
    config_file_path(FILE_NAME)
}

fn parse(contents: &str) -> Result<AudioOutputSettings, String> {
    let mut settings = AudioOutputSettings::default();
    let mut found_ceiling = false;
    for (line_number, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("invalid audio config line {}", line_number + 1));
        };
        if key != "master_output_ceiling_dbfs" || found_ceiling {
            return Err(format!("invalid audio config line {}", line_number + 1));
        }
        let ceiling_dbfs = value
            .parse::<i8>()
            .map_err(|_| format!("invalid Master output ceiling on line {}", line_number + 1))?;
        settings.master_output_ceiling = MasterOutputCeiling::new(ceiling_dbfs).map_err(|_| {
            format!(
                "unsupported Master output ceiling on line {}",
                line_number + 1
            )
        })?;
        found_ceiling = true;
    }
    Ok(settings)
}

fn save_to(path: &Path, settings: AudioOutputSettings) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let temporary = path.with_extension("conf.tmp");
    let mut file = std::fs::File::create(&temporary)?;
    writeln!(
        file,
        "master_output_ceiling_dbfs={}",
        settings.master_output_ceiling.as_dbfs()
    )?;
    file.sync_all()?;
    drop(file);
    #[cfg(target_os = "windows")]
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_output_settings_round_trip() {
        let settings = AudioOutputSettings {
            master_output_ceiling: MasterOutputCeiling::new(-6).unwrap(),
        };
        let path = std::env::temp_dir().join(format!(
            "aaadaw-audio-{}-{}.conf",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));

        save_to(&path, settings).unwrap();

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
}
