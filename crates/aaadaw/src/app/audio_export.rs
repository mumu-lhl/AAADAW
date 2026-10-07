use super::messages::FreezeTrackResult;
use super::{App, Message, run_blocking};
use aaadaw_core::{DawAction, Project, TrackId};
use aaadaw_storage::ProjectStore;
use iced::Task;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

impl App {
    pub(super) fn start_freeze_track(&mut self, track_id: TrackId) -> Task<Message> {
        if self.offline_render_busy || self.io_busy || self.project_path.is_none() {
            self.status = if self.project_path.is_none() {
                "Save the project before freezing a track".to_owned()
            } else {
                "Finish the current project operation before freezing a track".to_owned()
            };
            return Task::none();
        }
        #[cfg(feature = "audio-device")]
        if self.recording.is_some() || self.recording_starting || self.recording_stopping {
            self.status = "Stop recording before freezing a track".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.project_path.clone() else {
            self.status = "Save the project before freezing a track".to_owned();
            return Task::none();
        };
        let snapshot = self.project.snapshot();
        let worker_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel = Arc::clone(&worker_cancel);
        let progress = Arc::new(Mutex::new((0, 1)));
        let worker_progress = Arc::clone(&progress);
        self.offline_render_busy = true;
        self.offline_render_is_freeze = true;
        self.io_busy = true;
        self.offline_render_cancel = Some(cancel);
        self.offline_render_progress = Some(progress);
        self.status = "Freezing instrument track…".to_owned();

        Task::perform(
            run_blocking("aaadaw-track-freeze", move || {
                let project =
                    Project::from_snapshot(snapshot).map_err(|error| error.to_string())?;
                let tempdir = tempfile::tempdir().map_err(|error| error.to_string())?;
                let wav_path = tempdir.path().join("freeze.wav");
                let render = aaadaw_app::render_freeze_track_to_float32_wav(
                    &project_path,
                    &project,
                    track_id,
                    &wav_path,
                    &worker_cancel,
                    |done, total| {
                        *worker_progress
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = (done, total);
                    },
                )
                .map_err(|error| error.to_string())?;
                if worker_cancel.load(Ordering::Acquire) {
                    return Err("freeze was cancelled".to_owned());
                }
                let mut store =
                    ProjectStore::open(&project_path).map_err(|error| error.to_string())?;
                let media_ref = store
                    .import_audio_file(&wav_path)
                    .map_err(|error| error.to_string())?;
                if worker_cancel.load(Ordering::Acquire) {
                    store
                        .remove_unreferenced_audio_asset(&media_ref)
                        .map_err(|error| error.to_string())?;
                    store.close().map_err(|error| error.to_string())?;
                    return Err("freeze was cancelled".to_owned());
                }
                store.close().map_err(|error| error.to_string())?;
                Ok(FreezeTrackResult {
                    track_id,
                    media_ref,
                    start_sample: render.start_sample,
                    length_samples: render.length_samples,
                })
            }),
            Message::FreezeTrackFinished,
        )
    }

    pub(super) fn finish_freeze_track(&mut self, result: Result<FreezeTrackResult, String>) {
        let was_cancelled = self
            .offline_render_cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Acquire));
        self.offline_render_busy = false;
        self.offline_render_is_freeze = false;
        self.io_busy = false;
        self.offline_render_cancel = None;
        self.offline_render_progress = None;
        let FreezeTrackResult {
            track_id,
            media_ref,
            start_sample,
            length_samples,
        } = match result {
            Ok(frozen) => frozen,
            Err(error) => {
                self.status = if error.contains("cancelled") {
                    "Track freeze cancelled; the project is unchanged".to_owned()
                } else {
                    format!("Track freeze failed: {error}")
                };
                return;
            }
        };
        let Some(project_path) = self.project_path.clone() else {
            self.status = "Track freeze finished after the project was closed".to_owned();
            return;
        };
        if was_cancelled {
            let cleanup = ProjectStore::open(&project_path).and_then(|mut store| {
                let cleanup = store.remove_unreferenced_audio_asset(&media_ref);
                let close = store.close();
                cleanup.and(close)
            });
            self.status = match cleanup {
                Ok(()) => "Track freeze cancelled; the project is unchanged".to_owned(),
                Err(error) => format!(
                    "Track freeze cancelled; project unchanged, media cleanup failed: {error}"
                ),
            };
            return;
        }
        let previous_revision = self.revision;
        self.apply_action(
            DawAction::FreezeTrack {
                track_id,
                media_ref: media_ref.clone(),
                start_sample,
                length_samples,
            },
            "Track frozen; source remains available for unfreeze",
        );
        if self.revision == previous_revision {
            let cleanup = ProjectStore::open(&project_path).and_then(|mut store| {
                let cleanup = store.remove_unreferenced_audio_asset(&media_ref);
                let close = store.close();
                cleanup.and(close)
            });
            if let Err(error) = cleanup {
                self.status = format!(
                    "{}; unused freeze media cleanup failed: {error}",
                    self.status
                );
            }
        }
    }

    pub(super) fn start_offline_render(&mut self, destination: PathBuf) -> Task<Message> {
        #[cfg(feature = "audio-device")]
        if self.recording.is_some() || self.recording_starting || self.recording_stopping {
            self.status = "Stop recording before rendering audio".to_owned();
            return Task::none();
        }
        if self.offline_render_busy {
            self.status = "An offline render is already running".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.project_path.clone() else {
            self.status = "Save the project before rendering audio".to_owned();
            return Task::none();
        };
        if destination == project_path {
            self.status = "Choose a WAV destination separate from the project file".to_owned();
            return Task::none();
        }

        let snapshot = self.project.snapshot();
        let ceiling = self.audio_settings.master_output_ceiling;
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let progress = Arc::new(Mutex::new((0, 1)));
        let worker_cancel = Arc::clone(&cancel);
        let worker_progress = Arc::clone(&progress);
        let result_path = destination.clone();
        self.offline_render_busy = true;
        self.offline_render_is_freeze = false;
        self.offline_render_cancel = Some(cancel);
        self.offline_render_progress = Some(progress);
        self.status = format!("Rendering WAV to {}…", destination.display());

        Task::perform(
            run_blocking("aaadaw-offline-render", move || {
                let project =
                    Project::from_snapshot(snapshot).map_err(|error| error.to_string())?;
                aaadaw_app::render_project_file_to_pcm24_wav(
                    project_path,
                    &project,
                    &destination,
                    ceiling,
                    &worker_cancel,
                    |done, total| {
                        *worker_progress
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = (done, total);
                    },
                )
                .map_err(|error| error.to_string())?;
                Ok(result_path)
            }),
            Message::OfflineRenderFinished,
        )
    }

    pub(super) fn cancel_offline_render(&mut self) {
        if let Some(cancel) = &self.offline_render_cancel {
            cancel.store(true, Ordering::Release);
            self.status = "Cancelling offline render…".to_owned();
        }
    }

    pub(super) fn finish_offline_render(&mut self, result: Result<PathBuf, String>) {
        self.offline_render_busy = false;
        self.offline_render_is_freeze = false;
        self.offline_render_cancel = None;
        self.offline_render_progress = None;
        self.status = match result {
            Ok(path) => format!("WAV render complete: {}", path.display()),
            Err(error) if error.contains("cancelled") => {
                "WAV render cancelled; no output file was published".to_owned()
            }
            Err(error) => format!("WAV render failed: {error}"),
        };
    }

    pub(super) fn update_offline_render_progress(&mut self) {
        if !self.offline_render_busy {
            return;
        }
        let Some(progress) = &self.offline_render_progress else {
            return;
        };
        let (done, total) = *progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(percent) = done.saturating_mul(100).checked_div(total) {
            let operation = if self.offline_render_is_freeze {
                "Freezing track"
            } else {
                "Rendering offline"
            };
            self.status = format!("{operation}… {percent}% ({done}/{total} frames)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_a_render_requests_worker_cancellation() {
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut app = App {
            offline_render_busy: true,
            offline_render_cancel: Some(Arc::clone(&cancel)),
            ..App::default()
        };
        app.cancel_offline_render();
        assert!(cancel.load(Ordering::Acquire));
        assert_eq!(app.status, "Cancelling offline render…");
    }

    #[test]
    fn background_progress_updates_the_visible_render_status() {
        let progress = Arc::new(Mutex::new((25, 100)));
        let mut app = App {
            offline_render_busy: true,
            offline_render_progress: Some(progress),
            ..App::default()
        };
        app.update_offline_render_progress();
        assert_eq!(app.status, "Rendering offline… 25% (25/100 frames)");
    }

    #[test]
    fn background_freeze_progress_names_the_track_operation() {
        let progress = Arc::new(Mutex::new((25, 100)));
        let mut app = App {
            offline_render_busy: true,
            offline_render_is_freeze: true,
            offline_render_progress: Some(progress),
            ..App::default()
        };
        app.update_offline_render_progress();
        assert_eq!(app.status, "Freezing track… 25% (25/100 frames)");
    }
}
