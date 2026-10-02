use super::{App, Message, run_blocking};
use aaadaw_app::{ClapPluginScanReport, scan_clap_plugins};
use iced::Task;
use std::collections::HashSet;
use std::path::PathBuf;

impl App {
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
                self.start_clap_plugin_scan()
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
                self.start_clap_plugin_scan()
            }
            Err(error) => {
                self.clap_plugin_settings_feedback =
                    format!("Could not save CLAP search paths: {error}");
                Task::none()
            }
        }
    }

    pub(super) fn start_clap_plugin_scan(&mut self) -> Task<Message> {
        if self.clap_plugin_scan_busy {
            return Task::none();
        }
        if self.clap_plugin_paths.is_empty() {
            self.clap_plugin_scan = ClapPluginScanReport::default();
            self.clap_plugin_settings_feedback = "No CLAP search paths are configured".to_owned();
            return Task::none();
        }
        let paths = self.clap_plugin_paths.clone();
        self.clap_plugin_scan_busy = true;
        self.clap_plugin_settings_feedback = format!("Scanning {} search paths…", paths.len());
        Task::perform(
            run_blocking("aaadaw-clap-plugin-scan", move || {
                Ok(scan_clap_plugins(&paths))
            }),
            Message::ClapPluginsScanned,
        )
    }

    pub(super) fn finish_clap_plugin_scan(&mut self, result: Result<ClapPluginScanReport, String>) {
        self.clap_plugin_scan_busy = false;
        match result {
            Ok(report) => {
                self.clap_plugin_settings_feedback = format!(
                    "Scan complete: {} plugins found, {} entries checked, {} issues",
                    report.plugins.len(),
                    report.entries_checked,
                    report.errors.len()
                );
                self.clap_plugin_scan = report;
            }
            Err(error) => {
                self.clap_plugin_settings_feedback = format!("Plugin scan failed: {error}");
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
