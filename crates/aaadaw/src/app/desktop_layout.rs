use super::{App, config_paths};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct DesktopLayout {
    pub(super) version: u32,
    pub(super) media_open: bool,
    pub(super) mixer_open: bool,
    pub(super) media_ratio: f32,
    pub(super) arrange_ratio: f32,
}

impl Default for DesktopLayout {
    fn default() -> Self {
        Self {
            version: 1,
            media_open: false,
            mixer_open: true,
            media_ratio: 0.72,
            arrange_ratio: 0.62,
        }
    }
}

fn load_path(path: &Path) -> Result<DesktopLayout, String> {
    let text = match std::fs::read(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(error) => return Err(error.to_string()),
    };
    let layout: DesktopLayout = serde_json::from_slice(&text).map_err(|error| error.to_string())?;
    if layout.version != 1
        || !layout.media_ratio.is_finite()
        || !layout.arrange_ratio.is_finite()
        || !(0.55..=0.86).contains(&layout.media_ratio)
        || !(0.20..=0.90).contains(&layout.arrange_ratio)
    {
        return Err("Unsupported or invalid desktop layout".to_owned());
    }
    Ok(layout)
}

pub(super) fn load() -> Result<DesktopLayout, String> {
    config_paths::config_file_path("desktop-layout.json")
        .map_or_else(|| Ok(Default::default()), |path| load_path(&path))
}

fn save_path(path: &Path, layout: DesktopLayout) -> Result<(), String> {
    let contents = serde_json::to_vec_pretty(&layout).map_err(|error| error.to_string())?;
    config_paths::write_atomic(path, &contents).map_err(|error| error.to_string())
}

impl App {
    pub(super) fn schedule_desktop_layout_save(&mut self) {
        if !self.is_mobile_main_window() {
            self.desktop_layout_save_at = Some(Instant::now() + Duration::from_millis(500));
        }
    }
    pub(super) fn flush_desktop_layout_if_due(&mut self) {
        if self
            .desktop_layout_save_at
            .is_some_and(|at| Instant::now() >= at)
        {
            self.flush_desktop_layout();
        }
    }
    pub(super) fn flush_desktop_layout(&mut self) {
        if self.desktop_layout_save_at.take().is_none() {
            return;
        }
        let result = config_paths::config_file_path("desktop-layout.json")
            .ok_or_else(|| "No application configuration directory".to_owned())
            .and_then(|path| save_path(&path, self.media_panel_dock.layout()));
        if let Err(error) = result {
            self.status = format!("Desktop layout could not be saved: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::MediaPanelDock;

    #[test]
    fn desktop_layout_round_trip_and_invalid_values_preserve_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("desktop-layout.json");
        assert_eq!(load_path(&path).unwrap(), DesktopLayout::default());
        let layout = DesktopLayout {
            media_open: true,
            mixer_open: false,
            media_ratio: 0.63,
            arrange_ratio: 0.48,
            ..Default::default()
        };
        save_path(&path, layout).unwrap();
        assert_eq!(load_path(&path).unwrap(), layout);
        let dock = MediaPanelDock::from_layout(load_path(&path).unwrap());
        assert_eq!(dock.layout(), layout);
        for bad in [
            "{",
            "{\"version\":2}",
            "{\"media_ratio\":0.1}",
            "{\"arrange_ratio\":1.0}",
        ] {
            std::fs::write(&path, bad).unwrap();
            assert!(load_path(&path).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), bad);
        }
    }
}
