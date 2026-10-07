use super::{
    ActiveRecording, App, Message, RecordImportTarget, SharedAudioImportWorker,
    SharedRecordingStart, SharedRecordingStop, SharedStandbyInput, run_blocking,
};
use aaadaw_app::{
    StandbyAudioInput, audio_capture_stream, open_audio_input, start_audio_item_import,
};
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
            || self.standby_monitor_starting
            || self.offline_render_busy
        {
            self.status = "Wait for the current operation to finish before recording".to_owned();
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
        if tracks
            .iter()
            .any(|track_id| !self.saved_track_ids.contains(track_id))
        {
            self.status = "Save the project before recording on a new track".to_owned();
            return Task::none();
        }
        let recovery_track_ids = tracks.iter().map(|track| track.value()).collect();
        let sample_rate = self.project.settings().sample_rate();
        let backend = self.selected_playback_backend();
        let recording_offset_us = self.audio_settings.recording_offset_us;
        let cpal_input_device_id = self.audio_settings.cpal_input_device_id.clone();
        if self.playback.is_none() {
            self.recording_starting = true;
            self.recording_cancel_requested = false;
            self.status = format!("Starting {} transport for recording…", backend.name());
            return self.prepare_playback(self.playhead_sample, true);
        }
        let standby_input = self
            .playback
            .as_mut()
            .and_then(aaadaw_app::RunningAudioPlayback::take_standby_input);
        let monitor_producer = if standby_input.is_none() {
            self.playback
                .as_ref()
                .and_then(aaadaw_app::RunningAudioPlayback::input_monitor_producer)
        } else {
            None
        };
        if standby_input.is_none() && monitor_producer.is_none() {
            let target_sample = self
                .playback
                .as_ref()
                .map_or(self.playhead_sample, |playback| {
                    playback.stats().playhead_sample
                });
            let resume_playback = self.playback_playing;
            let close_task = self.close_playback();
            self.recording_starting = true;
            self.recording_cancel_requested = false;
            self.status = "Refreshing the playback input-monitor route…".to_owned();
            return Task::batch([
                close_task,
                self.prepare_playback(target_sample, resume_playback),
            ]);
        }
        self.recording_tracks = tracks;
        self.recording_starting = true;
        self.recording_cancel_requested = false;
        self.recording_cancelled_transport_start = false;
        self.status = format!("Connecting {} input…", backend.name());
        let (reused_input, producer, consumer, control) = if let Some(standby_input) = standby_input
        {
            let (input, consumer, control) = standby_input.into_recording_parts();
            (Some(input), None, consumer, control)
        } else {
            let (producer, consumer, control) = audio_capture_stream(sample_rate as usize * 10);
            (None, Some(producer), consumer, control)
        };
        Task::perform(
            run_blocking("aaadaw-recording-start", move || {
                let writer = aaadaw_app::AudioRecordingWorker::start_recoverable(
                    project_path,
                    sample_rate,
                    recovery_track_ids,
                    consumer,
                    control.clone(),
                );
                let writer = match writer {
                    Ok(writer) => writer,
                    Err(error) => {
                        if let Some(input) = reused_input {
                            input.shutdown();
                        }
                        return Err(error.to_string());
                    }
                };
                let recovery_manifest_path = writer
                    .recovery_manifest_path()
                    .expect("recoverable recording has a manifest")
                    .to_path_buf();
                let input = if let Some(input) = reused_input {
                    Ok(input)
                } else {
                    let Some(producer) = producer else {
                        writer.cancel();
                        return Err("Input capture queue is unavailable".to_owned());
                    };
                    let Some(monitor_producer) = monitor_producer else {
                        writer.cancel();
                        return Err("Input monitor queue is unavailable".to_owned());
                    };
                    open_audio_input(
                        backend,
                        producer,
                        Some(monitor_producer),
                        control.clone(),
                        sample_rate,
                        cpal_input_device_id.as_deref(),
                    )
                };
                match input {
                    Ok(input) => Ok(ActiveRecording {
                        input,
                        writer,
                        control,
                        placement_correction:
                            super::audio_config::RecordingPlacementCorrection::new(
                                recording_offset_us,
                            ),
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

    pub(super) fn toggle_input_monitor(&mut self, track_id: aaadaw_core::TrackId) -> Task<Message> {
        let armed = self
            .project
            .tracks()
            .iter()
            .any(|track| track.id() == track_id && track.is_record_armed());
        if !armed {
            return Task::none();
        }
        if self.recording.is_some() {
            if !self.playback_playing {
                return Task::none();
            }
            if let Some(playback) = self.playback.as_ref() {
                let enabled = !playback.input_monitor_enabled(track_id);
                if playback.set_track_input_monitor(track_id, enabled) {
                    self.status = if enabled {
                        "Input monitoring enabled for armed track".to_owned()
                    } else {
                        "Input monitoring disabled for armed track".to_owned()
                    };
                }
            }
            return Task::none();
        }

        if self.standby_monitor_starting && self.standby_monitor_track == Some(track_id) {
            self.standby_monitor_track = None;
            self.standby_monitor_generation = self.standby_monitor_generation.wrapping_add(1);
            self.status = "Standby input opening cancelled".to_owned();
            return Task::none();
        }
        if self.standby_monitor_starting {
            self.standby_monitor_track = Some(track_id);
            self.status = "Waiting for the current input operation to finish…".to_owned();
            return Task::none();
        }
        if let Some(playback) = self.playback.as_ref() {
            if playback.input_monitor_enabled(track_id) {
                playback.set_track_input_monitor(track_id, false);
                self.status = "Input monitoring disabled for armed track".to_owned();
                return Task::none();
            }
            if !playback.can_monitor_track_input(track_id) {
                let target_sample = playback.stats().playhead_sample;
                let resume_playback = self.playback_playing;
                self.standby_monitor_track = Some(track_id);
                self.status = "Refreshing the armed-track monitor route…".to_owned();
                return self.prepare_playback(target_sample, resume_playback);
            }
            if playback.has_standby_input() {
                if playback.set_track_input_monitor(track_id, true) {
                    self.status = "Input monitoring enabled for armed track".to_owned();
                } else {
                    self.status = "Input monitor route is unavailable".to_owned();
                }
                return Task::none();
            }
            if self.standby_monitor_starting {
                self.standby_monitor_track = Some(track_id);
                self.status = "Opening input for armed-track monitoring…".to_owned();
                return Task::none();
            }
        } else {
            self.standby_monitor_track = Some(track_id);
            self.status = "Opening audio output for input monitoring…".to_owned();
            return self.prepare_playback(self.playhead_sample, false);
        }
        self.standby_monitor_track = Some(track_id);
        self.start_standby_input(track_id)
    }

    pub(super) fn start_standby_input(&mut self, track_id: aaadaw_core::TrackId) -> Task<Message> {
        if self.standby_monitor_starting {
            return Task::none();
        }
        let Some(playback) = self.playback.as_ref() else {
            self.standby_monitor_track = Some(track_id);
            return self.prepare_playback(self.playhead_sample, false);
        };
        let Some(monitor_producer) = playback.input_monitor_producer() else {
            self.status = "Input monitor route is unavailable".to_owned();
            self.standby_monitor_track = None;
            return Task::none();
        };
        let backend = self.selected_playback_backend();
        let sample_rate = self.project.settings().sample_rate();
        let cpal_input_device_id = self.audio_settings.cpal_input_device_id.clone();
        self.standby_monitor_track = Some(track_id);
        self.standby_monitor_starting = true;
        self.standby_monitor_generation = self.standby_monitor_generation.wrapping_add(1);
        let generation = self.standby_monitor_generation;
        self.status = format!("Opening {} input for monitoring…", backend.name());
        let result = Arc::new(Mutex::new(None));
        let message_result = Arc::clone(&result);
        Task::perform(
            run_blocking("aaadaw-standby-input-open", move || {
                let (producer, consumer, control) = audio_capture_stream(sample_rate as usize * 10);
                open_audio_input(
                    backend,
                    producer,
                    Some(monitor_producer),
                    control.clone(),
                    sample_rate,
                    cpal_input_device_id.as_deref(),
                )
                .map(|input| StandbyAudioInput::new(input, consumer, control))
            }),
            move |opened| {
                *message_result
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(opened);
                Message::StandbyInputStarted(track_id, generation, SharedStandbyInput(result))
            },
        )
    }

    pub(super) fn finish_standby_input_start(
        &mut self,
        track_id: aaadaw_core::TrackId,
        generation: u64,
        result: SharedStandbyInput,
    ) -> Task<Message> {
        let opened = result.0.lock().ok().and_then(|mut result| result.take());
        if generation != self.standby_monitor_generation {
            self.standby_monitor_starting = false;
            return match opened {
                Some(Ok(standby)) => self.close_standby_input_async(standby),
                Some(Err(_)) | None => self
                    .standby_monitor_track
                    .filter(|_| self.playback.is_some())
                    .map_or_else(Task::none, |track_id| self.start_standby_input(track_id)),
            };
        }
        self.standby_monitor_starting = false;
        let standby = match opened {
            Some(Ok(input)) => input,
            Some(Err(error)) => {
                self.standby_monitor_track = None;
                tracing::error!(error = %error, "input monitor setup failed");
                self.status = format!("Input monitoring could not start: {error}");
                return Task::none();
            }
            None => {
                self.standby_monitor_track = None;
                self.status = "Input monitoring setup result was unavailable".to_owned();
                return Task::none();
            }
        };
        let route_track = self.standby_monitor_track;
        let armed = route_track.is_some_and(|route_track| {
            self.project
                .tracks()
                .iter()
                .any(|track| track.id() == route_track && track.is_record_armed())
        });
        if !armed {
            self.standby_monitor_track = None;
            return self.close_standby_input_async(standby);
        }
        let Some(playback) = self.playback.as_mut() else {
            self.standby_monitor_track = None;
            return self.close_standby_input_async(standby);
        };
        playback.install_standby_input(standby);
        if playback.set_track_input_monitor(route_track.unwrap_or(track_id), true) {
            self.standby_monitor_track = None;
            self.status = "Input monitoring enabled for armed track".to_owned();
        } else {
            self.standby_monitor_track = None;
            self.status = "Input monitor route is unavailable".to_owned();
        }
        Task::none()
    }

    pub(super) fn release_standby_input(&mut self) -> Task<Message> {
        self.standby_monitor_track = None;
        self.standby_monitor_generation = self.standby_monitor_generation.wrapping_add(1);
        let standby = self
            .playback
            .as_mut()
            .and_then(aaadaw_app::RunningAudioPlayback::take_standby_input);
        let Some(standby) = standby else {
            return Task::none();
        };
        if let Some(playback) = self.playback.as_ref() {
            playback.disable_input_monitoring();
        }
        self.close_standby_input_async(standby)
    }

    pub(super) fn close_standby_input_async(
        &mut self,
        standby: StandbyAudioInput,
    ) -> Task<Message> {
        self.standby_monitor_starting = true;
        Task::perform(
            run_blocking("aaadaw-standby-input-close", move || {
                standby.shutdown();
                Ok(())
            }),
            |_| Message::StandbyInputClosed,
        )
    }

    fn discard_recording_async(&mut self, recording: ActiveRecording) -> Task<Message> {
        if let Some(playback) = self.playback.as_ref() {
            playback.disable_input_monitoring();
        }
        self.standby_monitor_starting = true;
        Task::perform(
            run_blocking("aaadaw-recording-input-discard", move || {
                recording.control.stop();
                recording.input.shutdown();
                recording.writer.cancel();
                Ok(())
            }),
            |_| Message::RecordingInputDiscarded,
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
                    self.status = "Recording setup cancelled".to_owned();
                    return self.discard_recording_async(recording);
                }
                if self.playback.is_none() {
                    self.pending_recording = Some(recording);
                    self.status = format!("Starting {} transport…", self.playback_name());
                    self.start_playback()
                } else {
                    if !self.playback_playing {
                        self.playback_start_sample = self
                            .playback
                            .as_ref()
                            .expect("playback exists")
                            .stats()
                            .playhead_sample;
                        if let Err(error) = self.playback.as_mut().expect("playback exists").play()
                        {
                            tracing::error!(backend = self.playback_name(), error = %error, "playback could not start for recording");
                            self.recording_starting = false;
                            self.recording_tracks.clear();
                            self.status =
                                format!("Playback could not start for recording: {error}");
                            return self.discard_recording_async(recording);
                        }
                        self.playback_playing = true;
                        self.playback_paused = false;
                    }
                    self.begin_recording(recording)
                }
            }
            Some(Err(error)) => {
                self.recording_starting = false;
                self.recording_cancel_requested = false;
                self.recording_tracks.clear();
                if let Some(playback) = self.playback.as_ref() {
                    playback.disable_input_monitoring();
                }
                tracing::error!(error = %error, "recording setup failed");
                self.status = format!("Recording could not start: {error}");
                Task::none()
            }
            None => {
                tracing::error!("recording setup result was unavailable");
                self.recording_starting = false;
                self.recording_cancel_requested = false;
                self.recording_tracks.clear();
                if let Some(playback) = self.playback.as_ref() {
                    playback.disable_input_monitoring();
                }
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
                    self.status = "Recording setup cancelled".to_owned();
                    return self.discard_recording_async(recording);
                }
                self.recording_cancel_requested = true;
                self.status = "Cancelling input setup…".to_owned();
            } else {
                self.pause_playback();
            }
            return Task::none();
        };
        self.recording_stopping = true;
        self.recording_cancelled_transport_start = true;
        recording.control.stop();
        if let Some(playback) = self.playback.as_ref() {
            playback.disable_input_monitoring();
        }
        let manifest_path = recording.recovery_manifest_path.clone();
        let fallback_start_sample = self.recording_start_sample;
        self.pause_playback();
        self.playback_paused = false;
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
                let _close_task = self.close_playback();
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
                tracing::error!(error = %error, "recording finalization failed; recoverable take retained");
                self.recording_tracks.clear();
                self.status =
                    format!("Recording stopped with an error; recoverable take retained: {error}");
                Task::none()
            }
            None => {
                tracing::error!("recording finalization result was unavailable");
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
            self.status = "Recording setup cancelled".to_owned();
            return self.discard_recording_async(recording);
        }
        if !self.playback_playing {
            self.recording_starting = false;
            self.recording_tracks.clear();
            self.status = format!(
                "{} output could not start for recording",
                self.playback_name()
            );
            return self.discard_recording_async(recording);
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
        let Some(start_sample) = recording
            .placement_correction
            .apply(transport_sample, self.project.settings().sample_rate())
        else {
            self.recording_starting = false;
            self.recording_tracks.clear();
            self.status =
                "Recording offset moves the take outside the supported project sample range"
                    .to_owned();
            return self.discard_recording_async(recording);
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
                self.status = "Recording setup cancelled".to_owned();
                self.discard_recording_async(recording)
            }
            Some(Ok((recording, _provisional_start_sample))) => {
                let mut recording = recording;
                recording.placement_correction = recording
                    .placement_correction
                    .with_capture_latency_frames(recording.input.reported_capture_latency_frames());
                let placement_correction = recording.placement_correction;
                let playback_stats = self.playback.as_ref().map(|playback| playback.stats());
                let jack_clock_anchor =
                    playback_stats.and_then(|stats| stats.transport_clock_anchor);
                let frame_clock_mapping = recording
                    .input
                    .map_shared_frame_time(jack_clock_anchor.map(|anchor| anchor.backend_frame));
                let (transport_sample, capture_frame) = match frame_clock_mapping {
                    aaadaw_app::SharedFrameClockMapping::Mapped(frame) => (
                        jack_clock_anchor
                            .expect("a mapped clock requires an output anchor")
                            .project_sample,
                        Some(frame),
                    ),
                    aaadaw_app::SharedFrameClockMapping::Unavailable => {
                        tracing::error!("JACK transport/input clock mapping unavailable");
                        recording.control.fail();
                        self.recording_starting = false;
                        self.recording_tracks.clear();
                        self.status =
                            "JACK transport/input clock is unavailable; recording was not started"
                                .to_owned();
                        return self.discard_recording_async(recording);
                    }
                    aaadaw_app::SharedFrameClockMapping::Unsupported => (
                        jack_clock_anchor.map_or_else(
                            || {
                                playback_stats
                                    .map_or(self.playhead_sample, |stats| stats.playhead_sample)
                            },
                            |anchor| anchor.project_sample,
                        ),
                        None,
                    ),
                };
                // The provisional sample was persisted before this callback. Refresh the
                // playhead now so slow recovery-file sync time is not included in the take.
                let Some(start_sample) = placement_correction
                    .apply(transport_sample, self.project.settings().sample_rate())
                else {
                    tracing::error!(
                        "recording placement correction exceeded the project sample range"
                    );
                    self.recording_starting = false;
                    self.recording_tracks.clear();
                    self.status = "Recording offset moves the take outside the supported project sample range".to_owned();
                    return self.discard_recording_async(recording);
                };
                if let Some(capture_frame) = capture_frame {
                    let Some(anchor) = aaadaw_app::CaptureTimelineAnchor::new(
                        capture_frame,
                        start_sample,
                        self.project.settings().sample_rate(),
                        self.project.settings().sample_rate(),
                    ) else {
                        tracing::error!(
                            "JACK capture clock could not be mapped to the project sample rate"
                        );
                        recording.control.fail_timing();
                        self.recording_starting = false;
                        self.recording_tracks.clear();
                        self.status =
                            "JACK capture clock could not be mapped to the project sample rate"
                                .to_owned();
                        return self.discard_recording_async(recording);
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
                    tracing::error!(error = %error, "recording recovery position update failed");
                    recording.control.fail();
                    self.recording_starting = false;
                    self.recording_tracks.clear();
                    self.status = format!("Could not update recording recovery position: {error}");
                    return self.discard_recording_async(recording);
                }
                recording.control.start();
                self.recording_start_sample = start_sample;
                self.recording = Some(recording);
                self.recording_starting = false;
                self.status = recording_status(placement_correction.capture_latency_frames());
                Task::none()
            }
            Some(Err(error)) => {
                tracing::error!(error = %error, "recording recovery metadata persistence failed");
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                if let Some(playback) = self.playback.as_ref() {
                    playback.disable_input_monitoring();
                }
                self.status = format!("Could not persist recording recovery metadata: {error}");
                Task::none()
            }
            None => {
                tracing::error!("recording recovery metadata result was unavailable");
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                if let Some(playback) = self.playback.as_ref() {
                    playback.disable_input_monitoring();
                }
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
                self.status = "Recording setup cancelled".to_owned();
                self.discard_recording_async(recording)
            }
            Some(Ok((recording, start_sample))) => {
                let capture_latency_frames =
                    recording.placement_correction.capture_latency_frames();
                recording.control.start();
                self.recording_start_sample = start_sample;
                self.recording = Some(recording);
                self.recording_starting = false;
                self.status = recording_status(capture_latency_frames);
                Task::none()
            }
            Some(Err(error)) => {
                tracing::error!(error = %error, "JACK recording timeline initialization failed");
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                if let Some(playback) = self.playback.as_ref() {
                    playback.disable_input_monitoring();
                }
                self.status = format!("Could not initialize JACK recording timeline: {error}");
                Task::none()
            }
            None => {
                tracing::error!("JACK recording timeline initialization result was unavailable");
                self.recording_cancel_requested = false;
                self.recording_starting = false;
                self.recording_tracks.clear();
                if let Some(playback) = self.playback.as_ref() {
                    playback.disable_input_monitoring();
                }
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

#[cfg(feature = "audio-device")]
fn recording_status(capture_latency_frames: Option<u32>) -> String {
    match capture_latency_frames {
        Some(frames) => format!("Recording · capture latency compensated ({frames} frames)"),
        None => "Recording · automatic capture latency unavailable; user calibration only".into(),
    }
}
