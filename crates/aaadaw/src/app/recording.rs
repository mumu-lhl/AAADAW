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
        let recording_offset_us = self.audio_settings.recording_offset_us;
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
                        recording_offset_us,
                        capture_timeline_anchor: None,
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
        let fallback_start_sample = self.recording_start_sample;
        self.stop_playback();
        self.status = "Finalizing take…".to_owned();
        Task::perform(
            run_blocking("aaadaw-recording-finish", move || {
                recording.input.shutdown();
                let start_sample = match (
                    recording.capture_timeline_anchor,
                    recording.control.first_capture_frame(),
                ) {
                    (Some(anchor), Some(first_frame)) => {
                        anchor.project_sample_at(first_frame).ok_or_else(|| {
                            "JACK capture frame maps outside the project sample range".to_owned()
                        })?
                    }
                    _ => fallback_start_sample,
                };
                recording
                    .writer
                    .finish()
                    .map(|paths| (paths, manifest_path, start_sample))
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
            Some(Ok((paths, recovery_manifest_path, start_sample))) => {
                self.close_playback();
                let tracks = std::mem::take(&mut self.recording_tracks);
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
        let transport_sample = self
            .playback
            .as_ref()
            .map_or(self.playhead_sample, |playback| {
                playback.stats().playhead_sample
            });
        let Some(start_sample) = super::audio_config::apply_recording_offset(
            transport_sample,
            self.project.settings().sample_rate(),
            recording.recording_offset_us,
        ) else {
            self.recording_starting = false;
            self.recording_tracks.clear();
            discard_recording(recording);
            self.status =
                "Recording offset moves the take outside the supported project sample range"
                    .to_owned();
            return Task::none();
        };
        self.recording_start_sample = start_sample;
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
    ) -> Task<Message> {
        let result = result.0.lock().ok().and_then(|mut result| result.take());
        match result {
            Some(Ok((recording, _start_sample))) if self.recording_cancel_requested => {
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                discard_recording(recording);
                self.status = "Recording setup cancelled".to_owned();
                Task::none()
            }
            Some(Ok((recording, _provisional_start_sample))) => {
                let playback_stats = self.playback.as_ref().map(|playback| playback.stats());
                let jack_clock_anchor =
                    playback_stats.and_then(|stats| stats.transport_clock_anchor);
                let frame_clock_mapping = recording
                    .input
                    .map_shared_frame_time(jack_clock_anchor.map(|anchor| anchor.backend_frame));
                let transport_sample = match frame_clock_mapping {
                    aaadaw_app::SharedFrameClockMapping::Mapped(_) => {
                        jack_clock_anchor
                            .expect("a mapped clock requires an output anchor")
                            .project_sample
                    }
                    aaadaw_app::SharedFrameClockMapping::Unavailable => {
                        recording.control.fail();
                        discard_recording(recording);
                        self.recording_starting = false;
                        self.recording_tracks.clear();
                        self.status =
                            "JACK transport/input clock is unavailable; recording was not started"
                                .to_owned();
                        return Task::none();
                    }
                    aaadaw_app::SharedFrameClockMapping::Unsupported => jack_clock_anchor
                        .map_or_else(
                            || {
                                playback_stats
                                    .map_or(self.playhead_sample, |stats| stats.playhead_sample)
                            },
                            |anchor| anchor.project_sample,
                        ),
                };
                // The provisional sample was persisted before this callback. Refresh the
                // playhead now so slow recovery-file sync time is not included in the take.
                let Some(start_sample) = super::audio_config::apply_recording_offset(
                    transport_sample,
                    self.project.settings().sample_rate(),
                    recording.recording_offset_us,
                ) else {
                    discard_recording(recording);
                    self.recording_starting = false;
                    self.recording_tracks.clear();
                    self.status = "Recording offset moves the take outside the supported project sample range".to_owned();
                    return Task::none();
                };
                if let aaadaw_app::SharedFrameClockMapping::Mapped(capture_frame) =
                    frame_clock_mapping
                {
                    let Some(anchor) = aaadaw_app::CaptureTimelineAnchor::new(
                        capture_frame,
                        start_sample,
                        self.project.settings().sample_rate(),
                        self.project.settings().sample_rate(),
                    ) else {
                        recording.control.fail_timing();
                        discard_recording(recording);
                        self.recording_starting = false;
                        self.recording_tracks.clear();
                        self.status =
                            "JACK capture clock could not be mapped to the project sample rate"
                                .to_owned();
                        return Task::none();
                    };
                    self.status = "Aligning JACK capture clock…".to_owned();
                    return Task::perform(
                        run_blocking("aaadaw-recording-clock-anchor", move || {
                            if let Err(error) = recording.writer.set_capture_timeline_anchor(anchor)
                            {
                                recording.control.fail();
                                discard_recording(recording);
                                return Err(error.to_string());
                            }
                            let mut recording = recording;
                            recording.capture_timeline_anchor = Some(anchor);
                            Ok((recording, start_sample))
                        }),
                        |result| {
                            Message::RecordingClockAnchorReady(
                                super::SharedRecordingClockAnchorReady(Arc::new(Mutex::new(Some(
                                    result,
                                )))),
                            )
                        },
                    );
                } else if let Err(error) = recording.writer.refine_start_sample(start_sample) {
                    recording.control.fail();
                    discard_recording(recording);
                    self.recording_starting = false;
                    self.recording_tracks.clear();
                    self.status = format!("Could not update recording recovery position: {error}");
                    return Task::none();
                }
                recording.control.start();
                self.recording_start_sample = start_sample;
                self.recording = Some(recording);
                self.recording_starting = false;
                self.status = "Recording".to_owned();
                Task::none()
            }
            Some(Err(error)) => {
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                self.status = format!("Could not persist recording recovery metadata: {error}");
                Task::none()
            }
            None => {
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                self.status = "Recording recovery metadata result was unavailable".to_owned();
                Task::none()
            }
        }
    }

    pub(super) fn finish_recording_clock_anchor_ready(
        &mut self,
        result: super::SharedRecordingClockAnchorReady,
    ) -> Task<Message> {
        let result = result.0.lock().ok().and_then(|mut result| result.take());
        match result {
            Some(Ok((recording, _start_sample))) if self.recording_cancel_requested => {
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                discard_recording(recording);
                self.status = "Recording setup cancelled".to_owned();
                Task::none()
            }
            Some(Ok((recording, start_sample))) => {
                recording.control.start();
                self.recording_start_sample = start_sample;
                self.recording = Some(recording);
                self.recording_starting = false;
                self.status = "Recording".to_owned();
                Task::none()
            }
            Some(Err(error)) => {
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                self.status = format!("Could not initialize JACK recording timeline: {error}");
                Task::none()
            }
            None => {
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                self.status = "JACK recording timeline result was unavailable".to_owned();
                Task::none()
            }
        }
    }
}

fn discard_recording(recording: ActiveRecording) {
    recording.control.stop();
    recording.input.shutdown();
    recording.writer.cancel();
}
