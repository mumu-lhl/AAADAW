use aaadaw_app::{ClapPluginDescriptor, ClapPluginScanError, ClapPluginScanReport};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "clap-scan-cache.json";
const SCHEMA_VERSION: u32 = 1;
const MAX_CACHE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug)]
pub(super) struct CachedScan {
    pub report: ClapPluginScanReport,
    pub search_paths: Vec<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheFile {
    schema_version: u32,
    search_paths: Vec<PathBuf>,
    report: CachedReport,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedReport {
    plugins: Vec<CachedPlugin>,
    errors: Vec<CachedError>,
    entries_checked: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedPlugin {
    entry_path: PathBuf,
    plugin_id: String,
    name: String,
    vendor: Option<String>,
    features: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedError {
    path: PathBuf,
    message: String,
}

pub(super) fn default_path() -> Option<PathBuf> {
    super::config_paths::config_file_path(FILE_NAME)
}

pub(super) fn load_from(path: &Path) -> Result<Option<CachedScan>, String> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not read CLAP scan cache: {error}")),
    };
    if metadata.len() > MAX_CACHE_BYTES {
        return Err("CLAP scan cache exceeds the 32 MiB limit".to_owned());
    }
    let contents =
        std::fs::read(path).map_err(|error| format!("Could not read CLAP scan cache: {error}"))?;
    let cache: CacheFile = serde_json::from_slice(&contents)
        .map_err(|error| format!("Invalid CLAP scan cache: {error}"))?;
    if cache.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "Unsupported CLAP scan cache version {}",
            cache.schema_version
        ));
    }
    Ok(Some(CachedScan {
        report: cache.report.into_report(),
        search_paths: cache.search_paths,
    }))
}

pub(super) fn save_to(
    path: &Path,
    search_paths: &[PathBuf],
    report: &ClapPluginScanReport,
) -> Result<(), String> {
    let cache = CacheFile::from_report(search_paths, report);
    let contents = serde_json::to_vec(&cache)
        .map_err(|error| format!("Could not encode CLAP scan cache: {error}"))?;
    if contents.len() as u64 > MAX_CACHE_BYTES {
        return Err("CLAP scan cache exceeds the 32 MiB limit".to_owned());
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create the CLAP cache directory: {error}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("Could not create a temporary CLAP cache: {error}"))?;
    temporary
        .write_all(&contents)
        .map_err(|error| format!("Could not write the CLAP scan cache: {error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("Could not sync the CLAP scan cache: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("Could not replace the CLAP scan cache: {error}"))?;
    Ok(())
}

impl CacheFile {
    fn from_report(search_paths: &[PathBuf], report: &ClapPluginScanReport) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            search_paths: search_paths.to_vec(),
            report: CachedReport::from_report(report),
        }
    }
}

impl CachedReport {
    fn from_report(report: &ClapPluginScanReport) -> Self {
        Self {
            plugins: report.plugins.iter().map(CachedPlugin::from).collect(),
            errors: report.errors.iter().map(CachedError::from).collect(),
            entries_checked: report.entries_checked,
        }
    }

    fn into_report(self) -> ClapPluginScanReport {
        ClapPluginScanReport {
            plugins: self.plugins.into_iter().map(Into::into).collect(),
            errors: self.errors.into_iter().map(Into::into).collect(),
            entries_checked: self.entries_checked,
        }
    }
}

impl From<&ClapPluginDescriptor> for CachedPlugin {
    fn from(plugin: &ClapPluginDescriptor) -> Self {
        Self {
            entry_path: plugin.entry_path.clone(),
            plugin_id: plugin.plugin_id.clone(),
            name: plugin.name.clone(),
            vendor: plugin.vendor.clone(),
            features: plugin.features.clone(),
        }
    }
}

impl From<CachedPlugin> for ClapPluginDescriptor {
    fn from(plugin: CachedPlugin) -> Self {
        Self {
            entry_path: plugin.entry_path,
            plugin_id: plugin.plugin_id,
            name: plugin.name,
            vendor: plugin.vendor,
            features: plugin.features,
        }
    }
}

impl From<&ClapPluginScanError> for CachedError {
    fn from(error: &ClapPluginScanError) -> Self {
        Self {
            path: error.path.clone(),
            message: error.message.clone(),
        }
    }
}

impl From<CachedError> for ClapPluginScanError {
    fn from(error: CachedError) -> Self {
        Self {
            path: error.path,
            message: error.message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "aaadaw-clap-cache-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("temporary directory should be created");
            Self(path)
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn report(name: &str) -> ClapPluginScanReport {
        ClapPluginScanReport {
            plugins: vec![ClapPluginDescriptor {
                entry_path: PathBuf::from("/plugins/test.clap"),
                plugin_id: "org.example.test".to_owned(),
                name: name.to_owned(),
                vendor: Some("Example".to_owned()),
                features: vec!["instrument".to_owned(), "audio-effect".to_owned()],
            }],
            errors: vec![ClapPluginScanError {
                path: PathBuf::from("/plugins/broken.clap"),
                message: "could not load entry".to_owned(),
            }],
            entries_checked: 2,
        }
    }

    #[test]
    fn scan_cache_round_trips_catalog_paths_and_metadata() {
        let directory = TempDirectory::new();
        let path = directory.0.join(FILE_NAME);
        let search_paths = vec![PathBuf::from("/plugins"), PathBuf::from("/more-plugins")];
        let expected = report("Test Instrument");

        save_to(&path, &search_paths, &expected).unwrap();

        let cached = load_from(&path).unwrap().unwrap();
        assert_eq!(cached.report, expected);
        assert_eq!(cached.search_paths, search_paths);
    }

    #[test]
    fn scan_cache_replaces_the_previous_catalog_atomically() {
        let directory = TempDirectory::new();
        let path = directory.0.join(FILE_NAME);
        let search_paths = vec![PathBuf::from("/plugins")];

        save_to(&path, &search_paths, &report("Old name")).unwrap();
        save_to(&path, &search_paths, &report("New name")).unwrap();

        assert_eq!(
            load_from(&path).unwrap().unwrap().report.plugins[0].name,
            "New name"
        );
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn missing_malformed_and_incompatible_caches_do_not_load() {
        let directory = TempDirectory::new();
        let path = directory.0.join(FILE_NAME);
        assert!(load_from(&path).unwrap().is_none());

        std::fs::write(&path, b"not json").unwrap();
        assert!(load_from(&path).is_err());

        std::fs::write(
            &path,
            br#"{"schema_version":99,"search_paths":[],"report":{"plugins":[],"errors":[],"entries_checked":0}}"#,
        )
        .unwrap();
        assert!(
            load_from(&path)
                .unwrap_err()
                .contains("Unsupported CLAP scan cache version 99")
        );
    }
}
