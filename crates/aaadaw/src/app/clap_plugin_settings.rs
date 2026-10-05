use super::{App, Message, run_blocking};
use crate::clap_scanner::{self, PROCESS_ERROR_PREFIX};
use aaadaw_app::{ClapPluginScanError, ClapPluginScanReport};
use iced::Task;
use std::collections::HashSet;
use std::path::PathBuf;

impl App {
    pub(super) fn initialize_clap_plugin_scan(
        &mut self,
        cache_path: Option<PathBuf>,
        mut warnings: Vec<String>,
    ) -> Task<Message> {
        self.clap_plugin_cache_path = cache_path;
        if let Some(error) = self.restore_cached_clap_plugin_scan() {
            warnings.push(format!("CLAP scan cache was ignored: {error}"));
        }
        let task = self.start_clap_plugin_scan(false);
        if !warnings.is_empty() {
            self.clap_plugin_settings_feedback = format!(
                "{}; {}",
                self.clap_plugin_settings_feedback,
                warnings.join("; ")
            );
        }
        task
    }

    pub(super) fn restore_cached_clap_plugin_scan(&mut self) -> Option<String> {
        let path = self.clap_plugin_cache_path.as_deref()?;
        match super::clap_plugin_cache::load_from(path) {
            Ok(Some(cached)) => {
                self.clap_plugin_scan = cached.report;
                self.clap_plugin_scan_is_cached = true;
                self.clap_plugin_scan_paths = cached.search_paths;
                None
            }
            Ok(None) => None,
            Err(error) => Some(error),
        }
    }

    pub(super) fn add_clap_plugin_path(&mut self, path: PathBuf) -> Task<Message> {
        if !path.is_dir() {
            self.clap_plugin_settings_feedback =
                format!("Search path is not a directory: {}", path.display());
            return Task::none();
        }
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if self.clap_plugin_paths.iter().any(|existing| {
            std::fs::canonicalize(existing).unwrap_or_else(|_| existing.clone()) == path
        }) {
            self.clap_plugin_settings_feedback = "Search path is already listed".to_owned();
            return Task::none();
        }
        let mut paths = self.clap_plugin_paths.clone();
        paths.push(path);
        let custom_paths = custom_clap_plugin_paths(&paths, &self.clap_plugin_default_paths);
        match super::clap_plugin_config::save(&custom_paths) {
            Ok(()) => {
                self.clap_plugin_paths = paths;
                self.clap_plugin_settings_feedback = "Search path added".to_owned();
                self.start_clap_plugin_scan(false)
            }
            Err(error) => {
                self.clap_plugin_settings_feedback =
                    format!("Could not save CLAP search paths: {error}");
                Task::none()
            }
        }
    }

    pub(super) fn remove_clap_plugin_path(&mut self, path: PathBuf) -> Task<Message> {
        let Some(index) = self.clap_plugin_paths.iter().position(|existing| {
            if existing == &path {
                return true;
            }
            match (
                std::fs::canonicalize(existing),
                std::fs::canonicalize(&path),
            ) {
                (Ok(existing), Ok(path)) => existing == path,
                _ => false,
            }
        }) else {
            return Task::none();
        };
        if self
            .clap_plugin_default_paths
            .contains(&self.clap_plugin_paths[index])
        {
            self.clap_plugin_settings_feedback =
                "Default and CLAP_PATH entries are managed by the host".to_owned();
            return Task::none();
        }
        let mut paths = self.clap_plugin_paths.clone();
        paths.remove(index);
        let custom_paths = custom_clap_plugin_paths(&paths, &self.clap_plugin_default_paths);
        match super::clap_plugin_config::save(&custom_paths) {
            Ok(()) => {
                self.clap_plugin_paths = paths;
                self.clap_plugin_settings_feedback = "Search path removed".to_owned();
                self.start_clap_plugin_scan(false)
            }
            Err(error) => {
                self.clap_plugin_settings_feedback =
                    format!("Could not save CLAP search paths: {error}");
                Task::none()
            }
        }
    }

    pub(super) fn start_clap_plugin_scan(&mut self, retry_failed_entries: bool) -> Task<Message> {
        if self.clap_plugin_scan_busy {
            return Task::none();
        }
        if self.clap_plugin_paths.is_empty() {
            self.clap_plugin_scan = ClapPluginScanReport::default();
            self.clap_plugin_scan_is_cached = false;
            self.clap_plugin_scan_paths.clear();
            self.clap_plugin_settings_feedback = "No CLAP search paths are configured".to_owned();
            if let Some(path) = &self.clap_plugin_cache_path
                && let Err(error) = super::clap_plugin_cache::save_to(
                    path,
                    &self.clap_plugin_paths,
                    &self.clap_plugin_scan,
                )
            {
                self.clap_plugin_settings_feedback =
                    format!("No CLAP search paths are configured; cache update failed: {error}");
            }
            return Task::none();
        }
        let paths = self.clap_plugin_paths.clone();
        let skipped_errors = if retry_failed_entries {
            Vec::new()
        } else {
            self.clap_plugin_scan
                .errors
                .iter()
                .filter(|error| error.message.starts_with(PROCESS_ERROR_PREFIX))
                .cloned()
                .collect::<Vec<ClapPluginScanError>>()
        };
        self.clap_plugin_scan_busy = true;
        self.clap_plugin_settings_feedback = if self.clap_plugin_scan_is_cached {
            format!(
                "Refreshing {} search paths; cached results remain available",
                paths.len()
            )
        } else {
            format!("Scanning {} search paths…", paths.len())
        };
        Task::perform(
            run_blocking("aaadaw-clap-plugin-scan", move || {
                Ok(clap_scanner::scan_plugins(&paths, &skipped_errors))
            }),
            Message::ClapPluginsScanned,
        )
    }

    pub(super) fn finish_clap_plugin_scan(&mut self, result: Result<ClapPluginScanReport, String>) {
        self.clap_plugin_scan_busy = false;
        match result {
            Ok(report) => {
                let summary = format!(
                    "Scan complete: {} plugins found, {} entries checked, {} issues",
                    report.plugins.len(),
                    report.entries_checked,
                    report.errors.len()
                );
                if let Some(path) = &self.clap_plugin_cache_path
                    && let Err(error) =
                        super::clap_plugin_cache::save_to(path, &self.clap_plugin_paths, &report)
                {
                    self.clap_plugin_settings_feedback =
                        format!("{summary}; cache update failed: {error}");
                } else {
                    self.clap_plugin_settings_feedback = summary;
                }
                self.clap_plugin_scan = report;
                self.clap_plugin_scan_is_cached = false;
                self.clap_plugin_scan_paths = self.clap_plugin_paths.clone();
            }
            Err(error) => {
                self.clap_plugin_settings_feedback = if self.clap_plugin_scan_is_cached {
                    format!(
                        "Refresh failed; cached results from {} paths retained: {error}",
                        self.clap_plugin_scan_paths.len()
                    )
                } else {
                    format!("Plugin scan failed; previous results remain available: {error}")
                };
            }
        }
    }
}

fn custom_clap_plugin_paths(paths: &[PathBuf], defaults: &HashSet<PathBuf>) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|path| !defaults.contains(*path))
        .cloned()
        .collect()
}

pub(super) fn merge_clap_plugin_paths(
    defaults: Vec<PathBuf>,
    custom: Vec<PathBuf>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for path in defaults.into_iter().chain(custom) {
        let normalized = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.insert(normalized.clone()) {
            paths.push(normalized);
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_paths_are_merged_with_custom_paths_once() {
        let defaults = vec![PathBuf::from("/standard/clap")];
        let configured = vec![
            PathBuf::from("/custom/clap"),
            PathBuf::from("/standard/clap"),
        ];
        let paths = merge_clap_plugin_paths(defaults.clone(), configured);

        assert_eq!(
            paths,
            vec![
                PathBuf::from("/standard/clap"),
                PathBuf::from("/custom/clap")
            ]
        );
        assert_eq!(
            custom_clap_plugin_paths(&paths, &defaults.into_iter().collect()),
            vec![PathBuf::from("/custom/clap")]
        );
    }
}
