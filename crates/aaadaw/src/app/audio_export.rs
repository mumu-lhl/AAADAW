use super::{App, Message, run_blocking};
use aaadaw_core::Project;
use iced::Task;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

impl App {
    pub(super) fn start_offline_render(&mut self, destination: PathBuf) -> Task<Message> {
        #[cfg(feature = "audio-device")]
        if self.recording.is_some() || self.recording_starting || self.recording_stopping {
            self.status = "Stop recording before rendering audio".to_owned();
            return Task::none();
        }
        if self.offline_render_busy {
            self.status = "A WAV render is already running".to_owned();
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
            self.status = "Cancelling WAV render…".to_owned();
        }
    }

    pub(super) fn finish_offline_render(&mut self, result: Result<PathBuf, String>) {
        self.offline_render_busy = false;
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
            self.status = format!("Rendering WAV… {percent}% ({done}/{total} frames)");
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
        assert_eq!(app.status, "Cancelling WAV render…");
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
        assert_eq!(app.status, "Rendering WAV… 25% (25/100 frames)");
    }
}
