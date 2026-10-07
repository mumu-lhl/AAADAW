use super::{App, Message, RecordImportTarget, SharedAudioImportWorker, run_blocking};
use aaadaw_app::{recover_recording_candidate, start_audio_item_import};
use iced::Task;
use std::sync::{Arc, Mutex};

impl App {
    pub(super) fn recover_recording(&mut self, manifest_path: std::path::PathBuf) -> Task<Message> {
        if self.recording_recovery_busy || self.import_busy || self.io_busy {
            self.status = "Wait for the current operation to finish".to_owned();
            return Task::none();
        }
        let Some(candidate) = self
            .recording_recovery_candidates
            .iter()
            .find(|candidate| candidate.manifest_path == manifest_path)
            .cloned()
        else {
            return Task::none();
        };
        if candidate.segment_paths.is_empty() {
            self.status = "This take has no complete audio frames to recover".to_owned();
            return Task::none();
        }
        if candidate.manifest.start_sample.is_none() {
            self.status = "This take has audio but no saved timeline position; it was kept in the recovery folder".to_owned();
            return Task::none();
        }
        if candidate.manifest.track_ids.is_empty()
            || candidate
                .manifest
                .track_ids
                .iter()
                .enumerate()
                .any(|(index, raw_id)| {
                    self.project
                        .tracks()
                        .iter()
                        .all(|track| track.id().value() != *raw_id)
                        && candidate
                            .manifest
                            .track_names
                            .get(index)
                            .is_none_or(|name| name.trim().is_empty())
                })
        {
            self.status = "Recovery is missing the original track metadata".to_owned();
            return Task::none();
        }
        self.recording_recovery_busy = true;
        self.status = "Validating recoverable audio segments…".to_owned();
        Task::perform(
            run_blocking("aaadaw-recording-recovery-prepare", move || {
                recover_recording_candidate(candidate).map_err(|error| error.to_string())
            }),
            Message::RecordingRecoveryPrepared,
        )
    }

    pub(super) fn recording_recovery_prepared(
        &mut self,
        result: Result<aaadaw_app::RecordingRecoveryCandidate, String>,
    ) -> Task<Message> {
        self.recording_recovery_busy = false;
        let candidate = match result {
            Ok(candidate) => candidate,
            Err(error) => {
                tracing::error!(error = %error, "recording recovery validation failed");
                self.status = format!("Recording recovery validation failed: {error}");
                return Task::none();
            }
        };
        if candidate.segment_paths.is_empty() {
            self.status = "No decoder-readable audio remains in this take".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.project_path.clone() else {
            self.status = "Open the original project before recovering this take".to_owned();
            return Task::none();
        };
        let mut track_ids = Vec::with_capacity(candidate.manifest.track_ids.len());
        let mut recreated_track_ids = Vec::new();
        for (index, raw_id) in candidate.manifest.track_ids.iter().enumerate() {
            if let Some(track) = self
                .project
                .tracks()
                .iter()
                .find(|track| track.id().value() == *raw_id)
            {
                track_ids.push(track.id());
                continue;
            }
            let Some(name) = candidate.manifest.track_names.get(index) else {
                self.status = "Recovery is missing the original track metadata".to_owned();
                return Task::none();
            };
            let create_index = self.project.tracks().len();
            self.apply_action(
                aaadaw_core::DawAction::CreateTrack {
                    index: create_index,
                    name: name.clone(),
                },
                "Restored a recorded track",
            );
            let Some(track_id) = self
                .project
                .tracks()
                .get(create_index)
                .map(|track| track.id())
            else {
                self.status = "Could not restore a missing recorded track".to_owned();
                return Task::none();
            };
            track_ids.push(track_id);
            recreated_track_ids.push(track_id);
        }
        let Some(first_track) = track_ids.first().copied() else {
            self.status = "No recorded tracks are available for recovery".to_owned();
            return Task::none();
        };
        let Some(first_path) = candidate.segment_paths.first().cloned() else {
            return Task::none();
        };
        let start_sample = candidate
            .manifest
            .start_sample
            .expect("recovery candidate position was checked before preparation");
        let start_is_estimate = candidate.manifest.start_sample_is_estimate;
        let sample_rate = self.project.settings().sample_rate();
        if let Some(current) = self
            .recording_recovery_candidates
            .iter_mut()
            .find(|current| current.manifest_path == candidate.manifest_path)
        {
            *current = candidate.clone();
        }
        self.record_import_tracks = Some(RecordImportTarget {
            track_ids,
            recreated_track_ids,
            source_paths: candidate.segment_paths,
            next_segment_index: 0,
            next_start_sample: start_sample,
            imported_actions: Vec::new(),
            project_path: project_path.clone(),
            sample_rate,
            recovery_manifest_path: candidate.manifest_path,
            project_generation: self.project_generation,
            recovery_discarded_frames: candidate.discarded_frames,
            recovery_discarded_tail_bytes: candidate.discarded_tail_bytes,
            recovery_start_sample_is_estimate: start_is_estimate,
        });
        self.import_busy = true;
        self.import_finalizing = false;
        self.import_cancel_requested = false;
        self.import_bytes = 0;
        self.import_total_bytes = None;
        self.status = if start_is_estimate {
            "Restoring take at its last saved approximate position…".to_owned()
        } else {
            "Restoring recorded take into the arrangement…".to_owned()
        };
        Task::perform(
            run_blocking("aaadaw-recording-recovery-import", move || {
                start_audio_item_import(
                    project_path,
                    first_path,
                    first_track,
                    start_sample,
                    sample_rate,
                )
                .map_err(|error| error.to_string())
            }),
            |result| {
                Message::AudioImportStarted(SharedAudioImportWorker(Arc::new(Mutex::new(Some(
                    result,
                )))))
            },
        )
    }

    pub(super) fn discard_recording_recovery(
        &mut self,
        manifest_path: std::path::PathBuf,
    ) -> Task<Message> {
        if self.recording_recovery_busy || self.import_busy || self.io_busy {
            self.status = "Wait for the current operation to finish".to_owned();
            return Task::none();
        }
        let Some(candidate) = self
            .recording_recovery_candidates
            .iter()
            .find(|candidate| candidate.manifest_path == manifest_path)
        else {
            return Task::none();
        };
        let path = candidate.manifest_path.clone();
        self.recording_recovery_busy = true;
        self.status = "Discarding incomplete recording…".to_owned();
        Task::perform(
            run_blocking("aaadaw-recording-recovery-discard", move || {
                let result = aaadaw_app::discard_recording_recovery(&path)
                    .map_err(|error| error.to_string());
                Ok((path, result))
            }),
            |result| match result {
                Ok((path, result)) => Message::RecordingRecoveryDiscarded(path, result),
                Err(error) => {
                    tracing::error!(error = %error, "recording recovery discard worker failed");
                    Message::RecordingRecoveryDiscarded(std::path::PathBuf::new(), Err(error))
                }
            },
        )
    }
}
