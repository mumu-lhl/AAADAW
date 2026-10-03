use super::{
    ActiveRecording, App, Message, RecordImportTarget, SharedAudioImportWorker,
    SharedRecordingStart, SharedRecordingStop, run_blocking,
};
use aaadaw_app::{audio_capture_stream, open_audio_input, start_audio_item_import};
use iced::Task;
use std::sync::{Arc, Mutex};

impl App {
    pub(super) fn start_recording(&mut self) -> Task<Message> {
        if self.recording.is_some() || self.recording_starting || self.recording_stopping {
            self.status = "A recording is already active".to_owned();
            return Task::none();
        }
        if self.io_busy
            || self.import_busy
            || self.audio_asset_management_busy
            || self.playback_busy
        {
            self.status = "Wait for the current operation to finish before recording".to_owned();
            return Task::none();
        }
        if self.is_dirty() {
            self.status = "Save the project before recording".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.project_path.clone() else {
            self.status = "Save the project before recording".to_owned();
            return Task::none();
        };
        let tracks = self
            .project
            .tracks()
            .iter()
            .filter(|track| track.is_record_armed())
            .map(|track| track.id())
            .collect::<Vec<_>>();
        if tracks.is_empty() {
            self.status = "Arm at least one track before recording".to_owned();
            return Task::none();
        }
        let recovery_track_ids = tracks.iter().map(|track| track.value()).collect();
        let sample_rate = self.project.settings().sample_rate();
        let backend = self.selected_playback_backend();
        self.recording_tracks = tracks;
        self.recording_starting = true;
        self.recording_cancel_requested = false;
        self.recording_cancelled_transport_start = false;
        self.status = format!("Connecting {} input…", backend.name());
        let (producer, consumer, control) = audio_capture_stream(sample_rate as usize * 10);
        Task::perform(
            run_blocking("aaadaw-recording-start", move || {
                let writer = aaadaw_app::AudioRecordingWorker::start_recoverable(
                    project_path,
                    sample_rate,
                    recovery_track_ids,
                    consumer,
                    control.clone(),
                )
                .map_err(|error| error.to_string())?;
                let recovery_manifest_path = writer
                    .recovery_manifest_path()
                    .expect("recoverable recording has a manifest")
                    .to_path_buf();
                match open_audio_input(backend, producer, control.clone(), sample_rate) {
                    Ok(input) => Ok(ActiveRecording {
                        input,
                        writer,
                        control,
                        recovery_manifest_path,
                    }),
                    Err(error) => {
                        writer.cancel();
                        Err(error)
                    }
                }
            }),
            |result| {
                Message::RecordingStarted(SharedRecordingStart(Arc::new(Mutex::new(Some(result)))))
            },
        )
    }

    pub(super) fn finish_recording_start(&mut self, result: SharedRecordingStart) -> Task<Message> {
        let result = result.0.lock().ok().and_then(|mut result| result.take());
        match result {
            Some(Ok(recording)) => {
                if self.recording_cancel_requested {
                    self.recording_cancel_requested = false;
                    self.recording_starting = false;
                    self.recording_tracks.clear();
                    discard_recording(recording);
                    self.status = "Recording setup cancelled".to_owned();
                    return Task::none();
                }
                if self.playback.is_none() {
                    self.pending_recording = Some(recording);
                    self.status = format!("Starting {} transport…", self.playback_name());
                    self.start_playback()
                } else {
                    if !self.playback_playing {
                        if let Err(error) = self.playback.as_mut().expect("playback exists").play()
                        {
                            self.recording_starting = false;
                            self.recording_tracks.clear();
                            discard_recording(recording);
                            self.status =
                                format!("Playback could not start for recording: {error}");
                            return Task::none();
                        }
                        self.playback_playing = true;
                    }
                    self.begin_recording(recording)
                }
            }
            Some(Err(error)) => {
                self.recording_starting = false;
                self.recording_cancel_requested = false;
                self.recording_tracks.clear();
                self.status = format!("Recording could not start: {error}");
                Task::none()
            }
            None => {
                self.recording_starting = false;
                self.recording_cancel_requested = false;
                self.recording_tracks.clear();
                self.status = "Recording setup result was unavailable".to_owned();
                Task::none()
            }
        }
    }

    pub(super) fn stop_recording(&mut self) -> Task<Message> {
        let Some(recording) = self.recording.take() else {
            if self.recording_starting {
                if let Some(recording) = self.pending_recording.take() {
                    self.recording_starting = false;
                    self.recording_cancel_requested = false;
                    self.recording_cancelled_transport_start = true;
                    self.recording_tracks.clear();
                    discard_recording(recording);
                    self.status = "Recording setup cancelled".to_owned();
                    return Task::none();
                }
                self.recording_cancel_requested = true;
                self.status = "Cancelling input setup…".to_owned();
            } else {
                self.stop_playback();
            }
            return Task::none();
        };
        self.recording_stopping = true;
        self.recording_cancelled_transport_start = true;
        recording.control.stop();
        let manifest_path = recording.recovery_manifest_path.clone();
        self.stop_playback();
        self.status = "Finalizing take…".to_owned();
        Task::perform(
            run_blocking("aaadaw-recording-finish", move || {
                recording.input.shutdown();
                recording
                    .writer
                    .finish()
                    .map(|paths| (paths, manifest_path))
                    .map_err(|error| error.to_string())
            }),
            |result| {
                Message::RecordingStopped(SharedRecordingStop(Arc::new(Mutex::new(Some(result)))))
            },
        )
    }

    pub(super) fn finish_recording_stop(&mut self, result: SharedRecordingStop) -> Task<Message> {
        self.recording_stopping = false;
        let result = result.0.lock().ok().and_then(|mut result| result.take());
        match result {
            Some(Ok((paths, recovery_manifest_path))) => {
                self.close_playback();
                let tracks = std::mem::take(&mut self.recording_tracks);
                let start_sample = self.recording_start_sample;
                let Some(project_path) = self.project_path.clone() else {
                    self.status = "Project path disappeared; the finalized take remains available for recovery".to_owned();
                    return Task::none();
                };
                let Some(first_track) = tracks.first().copied() else {
                    self.status =
                        "No armed tracks remain; the finalized take is available for recovery"
                            .to_owned();
                    return Task::none();
                };
                if paths.is_empty() {
                    self.status = "No finalized audio frames were recorded".to_owned();
                    return Task::none();
                }
                let first_path = paths[0].clone();
                let sample_rate = self.project.settings().sample_rate();
                self.record_import_tracks = Some(RecordImportTarget {
                    track_ids: tracks,
                    source_paths: paths,
                    next_segment_index: 0,
                    next_start_sample: start_sample,
                    imported_actions: Vec::new(),
                    project_path: project_path.clone(),
                    sample_rate,
                    recovery_manifest_path,
                    project_generation: self.project_generation,
                    recovery_discarded_frames: 0,
                    recovery_discarded_tail_bytes: 0,
                    recovery_start_sample_is_estimate: false,
                });
                self.import_busy = true;
                self.import_finalizing = false;
                self.import_cancel_requested = false;
                self.import_bytes = 0;
                self.import_total_bytes = None;
                self.status = "Embedding recorded take…".to_owned();
                Task::perform(
                    run_blocking("aaadaw-recording-import-start", move || {
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
                        Message::AudioImportStarted(SharedAudioImportWorker(Arc::new(Mutex::new(
                            Some(result),
                        ))))
                    },
                )
            }
            Some(Err(error)) => {
                self.recording_tracks.clear();
                self.status =
                    format!("Recording stopped with an error; recoverable take retained: {error}");
                Task::none()
            }
            None => {
                self.recording_tracks.clear();
                self.status = "Recording finalization result was unavailable".to_owned();
                Task::none()
            }
        }
    }

    pub(super) fn begin_pending_recording(&mut self) -> Task<Message> {
        let Some(recording) = self.pending_recording.take() else {
            return Task::none();
        };
        if self.recording_cancel_requested {
            self.recording_cancel_requested = false;
            self.recording_starting = false;
            self.recording_tracks.clear();
            discard_recording(recording);
            self.status = "Recording setup cancelled".to_owned();
            return Task::none();
        }
        if !self.playback_playing {
            self.recording_starting = false;
            self.recording_tracks.clear();
            discard_recording(recording);
            self.status = format!(
                "{} output could not start for recording",
                self.playback_name()
            );
            return Task::none();
        }
        self.begin_recording(recording)
    }

    fn begin_recording(&mut self, recording: ActiveRecording) -> Task<Message> {
        self.recording_start_sample = self
            .playback
            .as_ref()
            .map_or(self.playhead_sample, |playback| {
                playback.stats().playhead_sample
            });
        let start_sample = self.recording_start_sample;
        self.status = "Preparing recording recovery metadata…".to_owned();
        Task::perform(
            run_blocking("aaadaw-recording-position", move || {
                if let Err(error) = recording.writer.set_start_sample(start_sample) {
                    let error = error.to_string();
                    discard_recording(recording);
                    return Err(error);
                }
                Ok((recording, start_sample))
            }),
            |result| {
                Message::RecordingPositionSaved(super::SharedRecordingPositionSaved(Arc::new(
                    Mutex::new(Some(result)),
                )))
            },
        )
    }

    pub(super) fn finish_recording_position_saved(
        &mut self,
        result: super::SharedRecordingPositionSaved,
    ) {
        let result = result.0.lock().ok().and_then(|mut result| result.take());
        match result {
            Some(Ok((recording, _start_sample))) if self.recording_cancel_requested => {
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                discard_recording(recording);
                self.status = "Recording setup cancelled".to_owned();
            }
            Some(Ok((recording, _provisional_start_sample))) => {
                let start_sample = self
                    .playback
                    .as_ref()
                    .map_or(self.playhead_sample, |playback| {
                        playback.stats().playhead_sample
                    });
                // The provisional sample was persisted before this callback. Refresh the
                // playhead now so slow recovery-file sync time is not included in the take.
                recording.control.start();
                if let Err(error) = recording.writer.refine_start_sample(start_sample) {
                    recording.control.fail();
                    discard_recording(recording);
                    self.recording_starting = false;
                    self.recording_tracks.clear();
                    self.status = format!("Could not update recording recovery position: {error}");
                    return;
                }
                self.recording_start_sample = start_sample;
                self.recording = Some(recording);
                self.recording_starting = false;
                self.status = "Recording".to_owned();
            }
            Some(Err(error)) => {
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                self.status = format!("Could not persist recording recovery metadata: {error}");
            }
            None => {
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                self.status = "Recording recovery metadata result was unavailable".to_owned();
            }
        }
    }
}

fn discard_recording(recording: ActiveRecording) {
    recording.control.stop();
    recording.input.shutdown();
    recording.writer.cancel();
}
