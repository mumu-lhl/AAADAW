use super::messages::FreezeTrackResult;
use super::offline_job_queue::{EnqueueError, OfflineJobId};
use super::{App, Message, run_blocking};
use aaadaw_core::{DawAction, Project, ProjectSnapshot, TrackId};
use aaadaw_storage::ProjectStore;
use iced::Task;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

const JOB_HISTORY_CAPACITY: usize = 4;

#[derive(Clone)]
pub(super) struct OfflineRenderJob {
    kind: OfflineRenderKind,
    snapshot: Arc<ProjectSnapshot>,
    project_path: PathBuf,
    project_generation: u64,
    label: String,
}

#[derive(Clone)]
enum OfflineRenderKind {
    ProjectWav {
        destination: PathBuf,
        master_ceiling: aaadaw_engine::MasterOutputCeiling,
        options: aaadaw_app::WavExportOptions,
    },
    FreezeTrack {
        track_id: TrackId,
    },
}

impl OfflineRenderJob {
    pub(super) fn label(&self) -> &str {
        &self.label
    }
}

impl App {
    pub(super) fn offline_job_submission_allowed(&self) -> bool {
        #[cfg(feature = "audio-device")]
        let recording_active =
            self.recording.is_some() || self.recording_starting || self.recording_stopping;
        #[cfg(not(feature = "audio-device"))]
        let recording_active = false;
        let active_freeze_owns_io = self
            .active_offline_job
            .as_ref()
            .is_some_and(|queued| matches!(queued.job.kind, OfflineRenderKind::FreezeTrack { .. }));
        !recording_active
            && !self.path_picker_busy
            && !self.import_busy
            && !self.audio_asset_management_busy
            && (!self.io_busy || active_freeze_owns_io)
    }

    pub(super) fn start_freeze_track(&mut self, track_id: TrackId) -> Task<Message> {
        if !self.offline_job_submission_allowed() {
            self.status = "Finish the current project operation before freezing a track".to_owned();
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
        let track_name = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            .map_or_else(
                || format!("track {}", track_id.value()),
                |track| track.name().to_owned(),
            );
        let job = OfflineRenderJob {
            kind: OfflineRenderKind::FreezeTrack { track_id },
            snapshot: Arc::new(self.project.snapshot()),
            project_path,
            project_generation: self.project_generation,
            label: format!("Freeze {track_name}"),
        };
        self.enqueue_offline_job(job)
    }

    pub(super) fn start_offline_render(&mut self, destination: PathBuf) -> Task<Message> {
        #[cfg(feature = "audio-device")]
        if self.recording.is_some() || self.recording_starting || self.recording_stopping {
            self.status = "Stop recording before rendering audio".to_owned();
            return Task::none();
        }
        if !self.offline_job_submission_allowed() {
            self.status = "Wait for the current project operation to finish".to_owned();
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

        let file_name = destination.file_name().map_or_else(
            || destination.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let job = OfflineRenderJob {
            kind: OfflineRenderKind::ProjectWav {
                destination,
                master_ceiling: self.audio_settings.master_output_ceiling,
                options: self.wav_export_options,
            },
            snapshot: Arc::new(self.project.snapshot()),
            project_path,
            project_generation: self.project_generation,
            label: format!("Render {file_name}"),
        };
        self.enqueue_offline_job(job)
    }

    fn enqueue_offline_job(&mut self, job: OfflineRenderJob) -> Task<Message> {
        match self.offline_job_queue.enqueue(job) {
            Ok(_) => {
                if self.offline_render_busy {
                    self.status = format!(
                        "Queued offline job ({} waiting)",
                        self.offline_job_queue.len()
                    );
                    Task::none()
                } else {
                    self.start_next_offline_job()
                }
            }
            Err(EnqueueError::Full) => {
                self.status = format!(
                    "Offline render queue is full (maximum {} waiting jobs)",
                    super::offline_job_queue::MAX_PENDING_OFFLINE_JOBS
                );
                Task::none()
            }
            Err(EnqueueError::IdExhausted) => {
                self.status = "No more offline render queue IDs are available".to_owned();
                Task::none()
            }
        }
    }

    fn start_next_offline_job(&mut self) -> Task<Message> {
        if !self.offline_job_queue.is_empty() && !self.offline_job_submission_allowed() {
            self.status = "Offline jobs are waiting for the current project operation".to_owned();
            return Task::none();
        }
        while let Some(queued) = self.offline_job_queue.pop_front() {
            if let OfflineRenderKind::ProjectWav { destination, .. } = &queued.job.kind
                && destination == &queued.job.project_path
            {
                self.record_offline_job_result(format!(
                    "{} failed: destination is the project file",
                    queued.job.label
                ));
                continue;
            }

            let job = &queued.job;
            let snapshot = Arc::clone(&job.snapshot);
            let project_path = job.project_path.clone();
            let worker_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let cancel = Arc::clone(&worker_cancel);
            let progress = Arc::new(Mutex::new((0, 1)));
            let worker_progress = Arc::clone(&progress);
            self.offline_render_busy = true;
            self.offline_render_is_freeze =
                matches!(job.kind, OfflineRenderKind::FreezeTrack { .. });
            self.io_busy = self.offline_render_is_freeze;
            self.offline_render_cancel = Some(cancel);
            self.offline_render_progress = Some(progress);
            self.status = format!("{}…", job.label);
            let task = match &job.kind {
                OfflineRenderKind::ProjectWav {
                    destination,
                    master_ceiling,
                    options,
                } => {
                    let destination = destination.clone();
                    let result_path = destination.clone();
                    let master_ceiling = *master_ceiling;
                    let options = *options;
                    Task::perform(
                        run_blocking("aaadaw-offline-render", move || {
                            let project = Project::from_snapshot((*snapshot).clone())
                                .map_err(|error| error.to_string())?;
                            aaadaw_app::render_project_file_to_wav(
                                project_path,
                                &project,
                                &destination,
                                master_ceiling,
                                options,
                                &worker_cancel,
                                |done, total| {
                                    *worker_progress
                                        .lock()
                                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                        (done, total);
                                },
                            )
                            .map_err(|error| error.to_string())?;
                            #[cfg(target_os = "android")]
                            crate::android_platform::sync_saf_document(&destination)?;
                            Ok(result_path)
                        }),
                        Message::OfflineRenderFinished,
                    )
                }
                OfflineRenderKind::FreezeTrack { track_id } => {
                    let track_id = *track_id;
                    Task::perform(
                        run_blocking("aaadaw-track-freeze", move || {
                            let project = Project::from_snapshot((*snapshot).clone())
                                .map_err(|error| error.to_string())?;
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
                                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                        (done, total);
                                },
                            )
                            .map_err(|error| error.to_string())?;
                            if worker_cancel.load(Ordering::Acquire) {
                                return Err("freeze was cancelled".to_owned());
                            }
                            let mut store = ProjectStore::open(&project_path)
                                .map_err(|error| error.to_string())?;
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
            };
            self.active_offline_job = Some(queued);
            return task;
        }
        self.active_offline_job = None;
        self.offline_render_busy = false;
        self.offline_render_is_freeze = false;
        self.io_busy = false;
        self.offline_render_cancel = None;
        self.offline_render_progress = None;
        Task::none()
    }

    pub(super) fn resume_offline_job_queue(&mut self) -> Task<Message> {
        if self.offline_render_busy
            || self.offline_job_queue.is_empty()
            || !self.offline_job_submission_allowed()
        {
            return Task::none();
        }
        self.start_next_offline_job()
    }

    pub(super) fn remove_queued_offline_job(&mut self, id: u64) {
        if let Some(job) = self.offline_job_queue.remove(OfflineJobId::from_value(id)) {
            self.record_offline_job_result(format!("Removed queued {}", job.label));
            self.status = format!("Removed queued {}", job.label);
        }
    }

    pub(super) fn cancel_offline_render(&mut self) {
        if let Some(cancel) = &self.offline_render_cancel {
            cancel.store(true, Ordering::Release);
            self.status = "Cancelling active offline job…".to_owned();
        }
    }

    pub(super) fn finish_freeze_track(
        &mut self,
        result: Result<FreezeTrackResult, String>,
    ) -> Task<Message> {
        let Some(active) = self.active_offline_job.take() else {
            return Task::none();
        };
        let was_cancelled = self
            .offline_render_cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Acquire));
        self.clear_active_offline_job();
        let OfflineRenderKind::FreezeTrack { track_id } = &active.job.kind else {
            self.record_offline_job_result(format!(
                "{} failed: unexpected completion",
                active.job.label
            ));
            self.status = "Offline job returned an unexpected completion type".to_owned();
            return self.start_next_offline_job();
        };
        if was_cancelled {
            let cleanup = result
                .as_ref()
                .map(|frozen| remove_freeze_media(&active.job.project_path, &frozen.media_ref))
                .unwrap_or(Ok(()));
            let message = match cleanup {
                Ok(()) => format!("{} cancelled", active.job.label),
                Err(error) => format!(
                    "{} cancelled; unused media cleanup failed: {error}",
                    active.job.label
                ),
            };
            self.record_offline_job_result(message.clone());
            self.status = message;
            return self.start_next_offline_job();
        }
        let FreezeTrackResult {
            track_id: result_track_id,
            media_ref,
            start_sample,
            length_samples,
        } = match result {
            Ok(frozen) => frozen,
            Err(error) => {
                let message = if error.contains("cancelled") {
                    format!("{} cancelled", active.job.label)
                } else {
                    format!("{} failed: {error}", active.job.label)
                };
                self.record_offline_job_result(message.clone());
                self.status = message;
                return self.start_next_offline_job();
            }
        };
        if result_track_id != *track_id
            || self.project_generation != active.job.project_generation
            || self.project_path.as_ref() != Some(&active.job.project_path)
            || !freeze_source_matches(&self.project.snapshot(), &active.job.snapshot, *track_id)
        {
            let cleanup = remove_freeze_media(&active.job.project_path, &media_ref);
            let message = match cleanup {
                Ok(()) => format!(
                    "{} skipped: its source or project changed since submission",
                    active.job.label
                ),
                Err(error) => format!(
                    "{} skipped because its source changed; unused media cleanup failed: {error}",
                    active.job.label
                ),
            };
            self.record_offline_job_result(message.clone());
            self.status = message;
            return self.start_next_offline_job();
        }

        let previous_revision = self.revision;
        self.apply_action(
            DawAction::FreezeTrack {
                track_id: *track_id,
                media_ref: media_ref.clone(),
                start_sample,
                length_samples,
            },
            "Track frozen; source remains available for unfreeze",
        );
        if self.revision == previous_revision {
            let cleanup = remove_freeze_media(&active.job.project_path, &media_ref);
            if let Err(error) = cleanup {
                self.status = format!(
                    "{}; unused freeze media cleanup failed: {error}",
                    self.status
                );
            }
            self.record_offline_job_result(format!(
                "{} failed: project rejected the freeze",
                active.job.label
            ));
        } else {
            self.record_offline_job_result(format!("{} complete", active.job.label));
        }
        self.start_next_offline_job()
    }

    pub(super) fn finish_offline_render(
        &mut self,
        result: Result<PathBuf, String>,
    ) -> Task<Message> {
        let Some(active) = self.active_offline_job.take() else {
            return Task::none();
        };
        self.clear_active_offline_job();
        let message = match result {
            Ok(path) => format!("WAV render complete: {}", path.display()),
            Err(error) if error.contains("cancelled") => {
                "WAV render cancelled; no output file was published".to_owned()
            }
            Err(error) => format!("WAV render failed: {error}"),
        };
        self.record_offline_job_result(format!("{}: {message}", active.job.label));
        self.status = message;
        self.start_next_offline_job()
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
            let label = self
                .active_offline_job
                .as_ref()
                .map_or(operation, |job| job.job.label());
            self.status = format!("{label}… {percent}% ({done}/{total} frames)");
        }
    }

    fn clear_active_offline_job(&mut self) {
        self.offline_render_busy = false;
        self.offline_render_is_freeze = false;
        self.io_busy = false;
        self.offline_render_cancel = None;
        self.offline_render_progress = None;
    }

    fn record_offline_job_result(&mut self, message: String) {
        self.offline_job_history.push_front(message);
        self.offline_job_history.truncate(JOB_HISTORY_CAPACITY);
    }
}

fn remove_freeze_media(project_path: &std::path::Path, media_ref: &str) -> Result<(), String> {
    let mut store = ProjectStore::open(project_path).map_err(|error| error.to_string())?;
    let cleanup = store
        .remove_unreferenced_audio_asset(media_ref)
        .map_err(|error| error.to_string());
    let close = store.close().map_err(|error| error.to_string());
    cleanup.and(close)
}

fn freeze_source_matches(
    current: &ProjectSnapshot,
    captured: &ProjectSnapshot,
    track_id: TrackId,
) -> bool {
    let track_value = track_id.value();
    current.settings == captured.settings
        && current.tempo_points == captured.tempo_points
        && current.meter_points == captured.meter_points
        && current.tracks.iter().find(|track| track.id == track_value)
            == captured.tracks.iter().find(|track| track.id == track_value)
        && current
            .midi_items
            .iter()
            .filter(|item| item.track_id == track_value)
            .eq(captured
                .midi_items
                .iter()
                .filter(|item| item.track_id == track_value))
}

#[cfg(test)]
mod tests {
    use super::super::offline_job_queue::OfflineJobQueue;
    use super::super::offline_job_queue::QueuedOfflineJob;
    use super::*;
    use aaadaw_core::DawAction;

    fn wav_job(label: &str, destination: &str) -> OfflineRenderJob {
        OfflineRenderJob {
            kind: OfflineRenderKind::ProjectWav {
                destination: PathBuf::from(destination),
                master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
                options: aaadaw_app::WavExportOptions::default(),
            },
            snapshot: Arc::new(Project::default().snapshot()),
            project_path: PathBuf::from("session.aaadaw"),
            project_generation: 0,
            label: label.to_owned(),
        }
    }

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
        assert_eq!(app.status, "Cancelling active offline job…");
    }

    #[test]
    fn background_progress_updates_the_visible_job_status() {
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
    fn queue_state_is_empty_by_default() {
        let queue: OfflineJobQueue<&str> = OfflineJobQueue::default();
        assert!(queue.is_empty());
    }

    #[test]
    fn offline_jobs_panel_can_be_opened_and_dismissed_with_escape() {
        let mut app = App::default();
        let _open_task = app.update(Message::ToggleOfflineJobsPanel);
        assert!(app.offline_jobs_panel_open);

        let _close_task = app.update(Message::Escape);
        assert!(!app.offline_jobs_panel_open);
    }

    #[test]
    fn failed_render_hands_off_to_the_next_queued_job_in_fifo_order() {
        let mut app = App {
            offline_render_busy: true,
            active_offline_job: Some(QueuedOfflineJob {
                id: OfflineJobId::from_value(1),
                job: wav_job("Render active.wav", "active.wav"),
            }),
            ..App::default()
        };
        app.offline_job_queue
            .enqueue(wav_job("Render first.wav", "first.wav"))
            .unwrap();
        app.offline_job_queue
            .enqueue(wav_job("Render second.wav", "second.wav"))
            .unwrap();

        let _first_task = app.finish_offline_render(Err("expected failure".to_owned()));
        assert_eq!(
            app.active_offline_job.as_ref().map(|job| job.job.label()),
            Some("Render first.wav")
        );
        assert!(app.offline_render_busy);
        assert_eq!(app.offline_job_queue.len(), 1);

        let _second_task = app.finish_offline_render(Err("expected failure".to_owned()));
        assert_eq!(
            app.active_offline_job.as_ref().map(|job| job.job.label()),
            Some("Render second.wav")
        );
        assert!(app.offline_job_history[0].contains("Render first.wav"));
    }

    #[test]
    fn full_pending_queue_keeps_existing_jobs_and_explains_its_limit() {
        let mut app = App {
            offline_render_busy: true,
            active_offline_job: Some(QueuedOfflineJob {
                id: OfflineJobId::from_value(1),
                job: wav_job("Render active.wav", "active.wav"),
            }),
            ..App::default()
        };
        for index in 0..super::super::offline_job_queue::MAX_PENDING_OFFLINE_JOBS {
            app.offline_job_queue
                .enqueue(wav_job(&format!("Render {index}.wav"), "pending.wav"))
                .unwrap();
        }

        let _task = app.enqueue_offline_job(wav_job("Render rejected.wav", "rejected.wav"));

        assert_eq!(
            app.offline_job_queue.len(),
            super::super::offline_job_queue::MAX_PENDING_OFFLINE_JOBS
        );
        assert!(app.status.contains("maximum 4 waiting jobs"));
        assert_eq!(
            app.active_offline_job.as_ref().map(|job| job.job.label()),
            Some("Render active.wav")
        );
    }

    #[test]
    fn queued_jobs_wait_for_project_io_and_resume_when_it_finishes() {
        let mut app = App {
            io_busy: true,
            ..App::default()
        };
        app.offline_job_queue
            .enqueue(wav_job("Render queued.wav", "queued.wav"))
            .unwrap();

        let _blocked_task = app.resume_offline_job_queue();
        assert!(!app.offline_render_busy);
        assert_eq!(app.offline_job_queue.len(), 1);

        app.io_busy = false;
        let _started_task = app.resume_offline_job_queue();
        assert!(app.offline_render_busy);
        assert_eq!(
            app.active_offline_job.as_ref().map(|job| job.job.label()),
            Some("Render queued.wav")
        );
        assert!(app.offline_job_queue.is_empty());
    }

    #[test]
    fn freeze_and_wav_requests_queue_behind_an_active_render() {
        let project_path = PathBuf::from("session.aaadaw");
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Instrument".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        let mut app = App {
            project,
            project_path: Some(project_path.clone()),
            offline_render_busy: true,
            active_offline_job: Some(QueuedOfflineJob {
                id: OfflineJobId::from_value(1),
                job: wav_job("Render active.wav", "active.wav"),
            }),
            ..App::default()
        };

        let _freeze_task = app.start_freeze_track(track_id);
        let _render_task = app.start_offline_render(PathBuf::from("second.wav"));

        let labels: Vec<_> = app
            .offline_job_queue
            .iter()
            .map(|(_, job)| job.label())
            .collect();
        assert_eq!(labels, ["Freeze Instrument", "Render second.wav"]);
        assert_eq!(
            app.active_offline_job.as_ref().map(|job| job.job.label()),
            Some("Render active.wav")
        );
        assert!(app.offline_render_busy);
    }

    #[test]
    fn freeze_source_validation_ignores_unrelated_track_edits() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Source".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "Other".to_owned(),
            })
            .unwrap();
        let source_id = project.tracks()[0].id();
        let other_id = project.tracks()[1].id();
        let captured = project.snapshot();

        project
            .apply(DawAction::SetTrackMute {
                track_id: other_id,
                muted: true,
            })
            .unwrap();

        assert!(freeze_source_matches(
            &project.snapshot(),
            &captured,
            source_id
        ));
    }

    #[test]
    fn freeze_source_validation_rejects_edits_to_the_frozen_track() {
        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Source".to_owned(),
            })
            .unwrap();
        let source_id = project.tracks()[0].id();
        let captured = project.snapshot();

        project
            .apply(DawAction::SetTrackVolume {
                track_id: source_id,
                volume_db: -3.0,
            })
            .unwrap();

        assert!(!freeze_source_matches(
            &project.snapshot(),
            &captured,
            source_id
        ));
    }

    #[test]
    fn stale_freeze_completion_keeps_the_track_live_and_removes_rendered_media() {
        let directory = tempfile::tempdir().unwrap();
        let project_path = directory.path().join("session.aaadaw");
        let rendered_file = directory.path().join("freeze.wav");
        std::fs::write(&rendered_file, b"rendered audio bytes").unwrap();
        let mut store = ProjectStore::open(&project_path).unwrap();
        let media_ref = store.import_audio_file(&rendered_file).unwrap();
        store.close().unwrap();

        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Source".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        let captured = Arc::new(project.snapshot());
        project
            .apply(DawAction::SetTrackVolume {
                track_id,
                volume_db: -3.0,
            })
            .unwrap();
        let live_project_snapshot = project.snapshot();
        let job = OfflineRenderJob {
            kind: OfflineRenderKind::FreezeTrack { track_id },
            snapshot: captured,
            project_path: project_path.clone(),
            project_generation: 0,
            label: "Freeze Source".to_owned(),
        };
        let mut app = App {
            project,
            offline_render_busy: true,
            offline_render_is_freeze: true,
            io_busy: true,
            active_offline_job: Some(QueuedOfflineJob {
                id: OfflineJobId::from_value(1),
                job,
            }),
            ..App::default()
        };

        let _next_task = app.finish_freeze_track(Ok(FreezeTrackResult {
            track_id,
            media_ref: media_ref.clone(),
            start_sample: 0,
            length_samples: 128,
        }));

        assert_eq!(app.project.snapshot(), live_project_snapshot);
        assert!(
            app.status
                .contains("skipped: its source or project changed")
        );
        let store = ProjectStore::open(&project_path).unwrap();
        assert!(store.resolve_audio_asset(&media_ref).is_err());
    }

    #[test]
    fn cancelled_successful_freeze_does_not_apply_or_leave_rendered_media() {
        let directory = tempfile::tempdir().unwrap();
        let project_path = directory.path().join("session.aaadaw");
        let rendered_file = directory.path().join("freeze.wav");
        std::fs::write(&rendered_file, b"rendered audio bytes").unwrap();
        let mut store = ProjectStore::open(&project_path).unwrap();
        let media_ref = store.import_audio_file(&rendered_file).unwrap();
        store.close().unwrap();

        let mut project = Project::default();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Source".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        let snapshot = Arc::new(project.snapshot());
        let job = OfflineRenderJob {
            kind: OfflineRenderKind::FreezeTrack { track_id },
            snapshot: Arc::clone(&snapshot),
            project_path: project_path.clone(),
            project_generation: 0,
            label: "Freeze Source".to_owned(),
        };
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let mut app = App {
            project,
            offline_render_busy: true,
            offline_render_is_freeze: true,
            io_busy: true,
            active_offline_job: Some(QueuedOfflineJob {
                id: OfflineJobId::from_value(1),
                job,
            }),
            offline_render_cancel: Some(cancel),
            ..App::default()
        };

        let _next_task = app.finish_freeze_track(Ok(FreezeTrackResult {
            track_id,
            media_ref: media_ref.clone(),
            start_sample: 0,
            length_samples: 128,
        }));

        assert_eq!(app.project.snapshot(), snapshot.as_ref().clone());
        assert!(app.status.contains("cancelled"));
        let store = ProjectStore::open(&project_path).unwrap();
        assert!(store.resolve_audio_asset(&media_ref).is_err());
    }
}
