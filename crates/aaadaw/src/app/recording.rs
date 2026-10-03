use super::{
    ActiveRecording, App, Message, SharedAudioImportWorker, SharedRecordingStart,
    SharedRecordingStop, run_blocking,
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
        let sample_rate = self.project.settings().sample_rate();
        let backend = self.selected_playback_backend();
        self.recording_start_sample = self.playhead_sample;
        self.recording_tracks = tracks;
        self.recording_starting = true;
        self.recording_cancel_requested = false;
        self.recording_cancelled_transport_start = false;
        self.status = format!("Connecting {} input…", backend.name());
        let (producer, consumer, control) = audio_capture_stream(sample_rate as usize * 10);
        Task::perform(
            run_blocking("aaadaw-recording-start", move || {
                let writer = aaadaw_app::AudioRecordingWorker::start(
                    project_path,
                    sample_rate,
                    consumer,
                    control.clone(),
                )
                .map_err(|error| error.to_string())?;
                match open_audio_input(backend, producer, control.clone(), sample_rate) {
                    Ok(input) => {
                        control.start();
                        Ok(ActiveRecording {
                            input,
                            writer,
                            control,
                        })
                    }
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
        self.recording_starting = false;
        let result = result.0.lock().ok().and_then(|mut result| result.take());
        match result {
            Some(Ok(recording)) => {
                if self.recording_cancel_requested {
                    self.recording_cancel_requested = false;
                    self.recording_tracks.clear();
                    recording.control.stop();
                    recording.input.shutdown();
                    recording.writer.cancel();
                    self.status = "Recording setup cancelled".to_owned();
                    return Task::none();
                }
                self.recording = Some(recording);
                self.status = "Recording".to_owned();
                if self.playback.is_none() || !self.playback_playing {
                    self.start_playback()
                } else {
                    Task::none()
                }
            }
            Some(Err(error)) => {
                self.recording_cancel_requested = false;
                self.recording_tracks.clear();
                self.status = format!("Recording could not start: {error}");
                Task::none()
            }
            None => {
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
        self.stop_playback();
        self.status = "Finalizing take…".to_owned();
        Task::perform(
            run_blocking("aaadaw-recording-finish", move || {
                recording.input.shutdown();
                recording.writer.finish().map_err(|error| error.to_string())
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
            Some(Ok(path)) => {
                self.close_playback();
                let tracks = std::mem::take(&mut self.recording_tracks);
                let start_sample = self.recording_start_sample;
                let Some(project_path) = self.project_path.clone() else {
                    let _ = std::fs::remove_file(path);
                    self.status = "Project path disappeared before take import".to_owned();
                    return Task::none();
                };
                let Some(first_track) = tracks.first().copied() else {
                    let _ = std::fs::remove_file(path);
                    self.status = "No armed tracks remain for this take".to_owned();
                    return Task::none();
                };
                let sample_rate = self.project.settings().sample_rate();
                self.record_import_tracks = Some((tracks, start_sample, path.clone()));
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
                            path,
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
                self.status = format!("Take was discarded: {error}");
                Task::none()
            }
            None => {
                self.recording_tracks.clear();
                self.status = "Recording finalization result was unavailable".to_owned();
                Task::none()
            }
        }
    }
}
