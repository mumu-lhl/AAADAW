//! CLAP plugin search paths and background-friendly entry discovery.

use aaadaw_engine::{ClapPluginDescriptor, inspect_clap_plugin_entry};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// A failure encountered while walking a CLAP search path or loading one entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClapPluginScanError {
    pub path: PathBuf,
    pub message: String,
}

/// Results from scanning configured CLAP plugin directories.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClapPluginScanReport {
    pub plugins: Vec<ClapPluginDescriptor>,
    pub errors: Vec<ClapPluginScanError>,
    pub entries_checked: usize,
}

/// Returns the standard CLAP paths for this platform plus paths from `CLAP_PATH`.
pub fn default_clap_search_paths() -> Vec<PathBuf> {
    let mut paths = env::var_os("CLAP_PATH")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = home_directory() {
            paths.push(home.join(".clap"));
        }
        paths.push(PathBuf::from("/usr/lib/clap"));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(common_files) = env::var_os("COMMONPROGRAMFILES") {
            paths.push(PathBuf::from(common_files).join("CLAP"));
        }
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            paths.push(PathBuf::from(local_app_data).join("Programs/Common/CLAP"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
        if let Some(home) = home_directory() {
            paths.push(home.join("Library/Audio/Plug-Ins/CLAP"));
        }
    }

    deduplicate_paths(paths)
}

/// Recursively scans `.clap` files and bundles. Call on a worker because plugin entry loading
/// runs native code and can take an arbitrary amount of time.
pub fn scan_clap_plugins(search_paths: &[PathBuf]) -> ClapPluginScanReport {
    scan_clap_plugins_with_inspector(search_paths, &[], |entry_path| {
        // SAFETY: these entries came from the user's configured CLAP search paths. This matches
        // the in-process plugin trust model documented in ADR 0005; scanning is always off-thread.
        unsafe { inspect_clap_plugin_entry(entry_path) }.map_err(|error| error.to_string())
    })
}

/// Scans configured paths using a caller-supplied entry inspector.
///
/// Cached scanner-process errors can be supplied in `skipped_errors`; matching entries are not
/// loaded again, and their previous error remains visible until the caller retries the scan.
pub fn scan_clap_plugins_with_inspector(
    search_paths: &[PathBuf],
    skipped_errors: &[ClapPluginScanError],
    mut inspect: impl FnMut(&Path) -> Result<Vec<ClapPluginDescriptor>, String>,
) -> ClapPluginScanReport {
    let mut report = ClapPluginScanReport::default();
    let entries = collect_entry_paths(search_paths, &mut report.errors);
    let mut plugin_ids = HashSet::new();

    for entry_path in entries {
        if let Some(error) = skipped_errors
            .iter()
            .find(|error| paths_match(&error.path, &entry_path))
        {
            report.errors.push(error.clone());
            continue;
        }
        report.entries_checked += 1;
        let plugins = match inspect(&entry_path) {
            Ok(plugins) => plugins,
            Err(message) => {
                report.errors.push(ClapPluginScanError {
                    path: entry_path,
                    message,
                });
                continue;
            }
        };

        for plugin in plugins {
            if plugin_ids.insert(plugin.plugin_id.clone()) {
                report.plugins.push(plugin);
            } else {
                report.errors.push(ClapPluginScanError {
                    path: plugin.entry_path,
                    message: format!(
                        "duplicate plugin ID {:?}; the first discovered copy is listed",
                        plugin.plugin_id
                    ),
                });
            }
        }
    }

    report.plugins.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.plugin_id.cmp(&right.plugin_id))
    });
    report.errors.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.message.cmp(&right.message))
    });
    report
}

fn paths_match(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_owned());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_owned());
    left == right
}

fn collect_entry_paths(
    search_paths: &[PathBuf],
    errors: &mut Vec<ClapPluginScanError>,
) -> Vec<PathBuf> {
    let mut pending = search_paths.to_vec();
    let mut visited_directories = HashSet::new();
    let mut seen_entries = HashSet::new();
    let mut entries = Vec::new();

    while let Some(path) = pending.pop() {
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                errors.push(ClapPluginScanError {
                    path,
                    message: error.to_string(),
                });
                continue;
            }
        };

        if has_clap_extension(&path) {
            let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if metadata.is_file() || metadata.is_dir() {
                if seen_entries.insert(canonical) {
                    entries.push(path);
                }
            } else {
                errors.push(ClapPluginScanError {
                    path,
                    message: "CLAP entry is neither a file nor a bundle directory".to_owned(),
                });
            }
            continue;
        }

        if !metadata.is_dir() {
            continue;
        }
        let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !visited_directories.insert(canonical) {
            continue;
        }
        let children = match fs::read_dir(&path) {
            Ok(children) => children,
            Err(error) => {
                errors.push(ClapPluginScanError {
                    path,
                    message: error.to_string(),
                });
                continue;
            }
        };
        let mut child_paths = Vec::new();
        for child in children {
            match child {
                Ok(child) => child_paths.push(child.path()),
                Err(error) => errors.push(ClapPluginScanError {
                    path: path.clone(),
                    message: error.to_string(),
                }),
            }
        }
        child_paths.sort();
        pending.extend(child_paths.into_iter().rev());
    }

    entries.sort();
    entries
}

fn has_clap_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("clap"))
}

fn deduplicate_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| {
            let normalized = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            seen.insert(normalized)
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn home_directory() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn home_directory() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new() -> Self {
            let path = env::temp_dir().join(format!(
                "aaadaw-clap-scan-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("temporary directory should be created");
            Self(path)
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn recursively_finds_files_and_bundle_directories_but_not_other_files() {
        let temp = TempDirectory::new();
        let nested = temp.0.join("nested");
        let bundle = nested.join("bundle.clap");
        fs::create_dir_all(bundle.join("Contents")).unwrap();
        fs::write(temp.0.join("synth.clap"), b"test entry").unwrap();
        fs::write(bundle.join("Contents/library.so"), b"bundle contents").unwrap();
        fs::write(bundle.join("Contents/inner.clap"), b"not a separate entry").unwrap();
        fs::write(temp.0.join("ignored.so"), b"not a CLAP extension").unwrap();

        let mut errors = Vec::new();
        let entries = collect_entry_paths(std::slice::from_ref(&temp.0), &mut errors);

        assert_eq!(entries, vec![bundle, temp.0.join("synth.clap")]);
        assert!(errors.is_empty());
    }

    #[test]
    fn invalid_clap_entry_is_reported_without_hiding_other_scan_results() {
        let temp = TempDirectory::new();
        fs::write(temp.0.join("invalid.clap"), b"not a shared library").unwrap();

        let report = scan_clap_plugins(std::slice::from_ref(&temp.0));

        assert_eq!(report.entries_checked, 1);
        assert!(report.plugins.is_empty());
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].path, temp.0.join("invalid.clap"));
    }

    #[test]
    fn duplicate_search_roots_are_removed() {
        let temp = TempDirectory::new();
        assert_eq!(
            deduplicate_paths(vec![temp.0.clone(), temp.0.clone()]),
            vec![temp.0.clone()]
        );
    }

    #[test]
    fn cached_scanner_process_failures_are_skipped_until_retry() {
        let temp = TempDirectory::new();
        let entry = temp.0.join("unstable.clap");
        fs::write(&entry, b"test entry").unwrap();
        let cached_error = ClapPluginScanError {
            path: entry.clone(),
            message: "CLAP scanner process: child exited unexpectedly".to_owned(),
        };

        let cached = scan_clap_plugins_with_inspector(
            std::slice::from_ref(&temp.0),
            std::slice::from_ref(&cached_error),
            |_| panic!("cached failure should not be loaded again"),
        );
        assert_eq!(cached.entries_checked, 0);
        assert_eq!(cached.errors, vec![cached_error]);

        let retried = scan_clap_plugins_with_inspector(std::slice::from_ref(&temp.0), &[], |_| {
            Ok(vec![ClapPluginDescriptor {
                entry_path: entry.clone(),
                plugin_id: "org.example.retry".to_owned(),
                name: "Retry".to_owned(),
                vendor: None,
                features: vec!["audio-effect".to_owned()],
            }])
        });
        assert_eq!(retried.entries_checked, 1);
        assert_eq!(retried.plugins[0].plugin_id, "org.example.retry");
        assert!(retried.errors.is_empty());
    }

    #[test]
    fn one_entry_failure_does_not_hide_later_plugins() {
        let temp = TempDirectory::new();
        let broken = temp.0.join("broken.clap");
        let working = temp.0.join("working.clap");
        fs::write(&broken, b"broken").unwrap();
        fs::write(&working, b"working").unwrap();

        let report =
            scan_clap_plugins_with_inspector(std::slice::from_ref(&temp.0), &[], |entry| {
                if entry.ends_with("broken.clap") {
                    return Err("scanner child exited".to_owned());
                }
                Ok(vec![ClapPluginDescriptor {
                    entry_path: entry.to_owned(),
                    plugin_id: "org.example.working".to_owned(),
                    name: "Working".to_owned(),
                    vendor: None,
                    features: vec!["audio-effect".to_owned()],
                }])
            });

        assert_eq!(report.entries_checked, 2);
        assert_eq!(report.plugins.len(), 1);
        assert_eq!(report.plugins[0].plugin_id, "org.example.working");
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].path, broken);
    }
}
