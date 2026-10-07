use crate::prepare_audio_playback;
use crate::{
    Float32WavExport, Float32WavExportError, Pcm24WavExport, Pcm24WavExportError,
    PreparedAudioPlayback,
};
use aaadaw_core::{ItemId, Project};
use aaadaw_engine::{
    AudioGraphError, AudioRenderGraph, ClapEffectOwner, ClapInstrumentOwner, MasterOutputCeiling,
    TrackFxProcessor, TrackInstrumentProcessor,
};
use aaadaw_storage::ProjectStore;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

/// Export decay allowance and minimum fallback when frozen CLAP plugins do not report longer tails.
pub const DEFAULT_EFFECT_TAIL_SECONDS: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrozenTrackRender {
    pub start_sample: u64,
    pub length_samples: u64,
}

trait OfflineWavWriter {
    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), OfflineRenderError>;
    fn finish(self) -> Result<PathBuf, OfflineRenderError>;
}

impl OfflineWavWriter for Pcm24WavExport {
    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), OfflineRenderError> {
        Pcm24WavExport::write_frames(self, frames).map_err(OfflineRenderError::Wav)
    }

    fn finish(self) -> Result<PathBuf, OfflineRenderError> {
        Pcm24WavExport::finish(self).map_err(OfflineRenderError::Wav)
    }
}

impl OfflineWavWriter for Float32WavExport {
    fn write_frames(&mut self, frames: &[[f32; 2]]) -> Result<(), OfflineRenderError> {
        Float32WavExport::write_frames(self, frames).map_err(OfflineRenderError::Float32Wav)
    }

    fn finish(self) -> Result<PathBuf, OfflineRenderError> {
        Float32WavExport::finish(self).map_err(OfflineRenderError::Float32Wav)
    }
}

fn freeze_tail_frames(
    sample_rate: u32,
    plugin_tail_frames: &[u32],
) -> Result<u64, OfflineRenderError> {
    let minimum_tail = u64::from(sample_rate)
        .checked_mul(u64::from(DEFAULT_EFFECT_TAIL_SECONDS))
        .ok_or(OfflineRenderError::TimelineRange)?;
    let reported_tail = plugin_tail_frames.iter().try_fold(0_u64, |total, frames| {
        total
            .checked_add(u64::from(*frames))
            .ok_or(OfflineRenderError::TimelineRange)
    })?;
    Ok(minimum_tail.max(reported_tail))
}

/// Failure while rendering a prepared graph to an offline PCM WAV file.
#[derive(Debug)]
pub enum OfflineRenderError {
    Wav(Pcm24WavExportError),
    Float32Wav(Float32WavExportError),
    Graph(AudioGraphError),
    Cancelled,
    InputUnderrun { samples: usize },
    Media(String),
    SourceEndedEarly { item_id: ItemId },
    TimelineRange,
}

struct RenderRequest<'a> {
    total_frames: u64,
    start_sample: u64,
    cancelled: &'a AtomicBool,
}

impl std::fmt::Display for OfflineRenderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wav(error) => error.fmt(formatter),
            Self::Float32Wav(error) => error.fmt(formatter),
            Self::Graph(error) => write!(formatter, "offline render failed: {error}"),
            Self::Cancelled => formatter.write_str("offline render was cancelled"),
            Self::InputUnderrun { samples } => write!(
                formatter,
                "offline render stopped because {samples} source samples were unavailable"
            ),
            Self::Media(error) => write!(formatter, "media decoding failed during export: {error}"),
            Self::SourceEndedEarly { item_id } => write!(
                formatter,
                "audio source for item {} ended before its timeline range was rendered",
                item_id.value()
            ),
            Self::TimelineRange => {
                formatter.write_str("project render length exceeds the sample timeline range")
            }
        }
    }
}

impl std::error::Error for OfflineRenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Wav(error) => Some(error),
            Self::Float32Wav(error) => Some(error),
            Self::Graph(error) => Some(error),
            Self::Media(_)
            | Self::Cancelled
            | Self::InputUnderrun { .. }
            | Self::SourceEndedEarly { .. }
            | Self::TimelineRange => None,
        }
    }
}

/// Computes the render length through all placed items plus the selected fixed effect tail.
pub fn project_render_length_samples(
    project: &Project,
    tail_seconds: u32,
) -> Result<u64, OfflineRenderError> {
    let mut content_end = 0_u64;
    for item in project.audio_items() {
        let end = item
            .start_sample()
            .checked_add(item.length_samples())
            .ok_or(OfflineRenderError::TimelineRange)?;
        content_end = content_end.max(end);
    }
    for item in project.midi_items() {
        let end_tick = item
            .start_tick()
            .checked_add(item.length_ticks())
            .ok_or(OfflineRenderError::TimelineRange)?;
        let end_sample = project
            .sample_at_tick(end_tick)
            .map_err(|_| OfflineRenderError::TimelineRange)?;
        content_end = content_end.max(end_sample);
    }
    let tail_frames = u64::from(project.settings().sample_rate())
        .checked_mul(u64::from(tail_seconds))
        .ok_or(OfflineRenderError::TimelineRange)?;
    content_end
        .checked_add(tail_frames)
        .ok_or(OfflineRenderError::TimelineRange)
}

/// Renders an already prepared graph to stereo PCM24 WAV using fixed-size buffers.
///
/// The graph must contain all configured instruments/effects and its media feeders must remain
/// alive while this function runs. Source underruns fail the export, so an incomplete render is
/// never published as a successful file. Progress is reported on the calling thread; invoke this
/// function on a background worker when called from the desktop UI.
pub fn render_graph_to_pcm24_wav(
    graph: &mut AudioRenderGraph,
    destination: impl AsRef<Path>,
    sample_rate: u32,
    total_frames: u64,
    cancelled: &AtomicBool,
    mut report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    let export = Pcm24WavExport::create(destination.as_ref(), sample_rate)
        .map_err(OfflineRenderError::Wav)?;
    render_graph_with_waiter(
        graph,
        RenderRequest {
            total_frames,
            start_sample: 0,
            cancelled,
        },
        export,
        |_, frames| Ok(frames),
        || Ok(()),
        &mut report_progress,
    )
}

/// Renders a prepared project's AudioItems while waiting off the audio thread for decoder queues.
pub fn render_prepared_audio_to_pcm24_wav(
    prepared: PreparedAudioPlayback,
    project: &Project,
    destination: impl AsRef<Path>,
    total_frames: u64,
    cancelled: &AtomicBool,
    mut report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    let (_graph, result) = render_prepared_inner(
        prepared,
        project,
        destination,
        total_frames,
        cancelled,
        &mut report_progress,
    );
    result
}

fn render_prepared_inner(
    prepared: PreparedAudioPlayback,
    project: &Project,
    destination: impl AsRef<Path>,
    total_frames: u64,
    cancelled: &AtomicBool,
    report_progress: &mut impl FnMut(u64, u64),
) -> (AudioRenderGraph, Result<(), OfflineRenderError>) {
    let (mut graph, feeders) = prepared.into_parts();
    let feeders = RefCell::new(feeders.into_iter().map(Some).collect::<Vec<_>>());
    let item_ids: Vec<_> = project.audio_items().iter().map(|item| item.id()).collect();
    let sample_rate = project.settings().sample_rate();
    let export =
        Pcm24WavExport::create(destination.as_ref(), sample_rate).map_err(OfflineRenderError::Wav);
    let export = match export {
        Ok(export) => export,
        Err(error) => return (graph, Err(error)),
    };
    let result = render_graph_with_waiter(
        &mut graph,
        RenderRequest {
            total_frames,
            start_sample: 0,
            cancelled,
        },
        export,
        |graph, requested_frames| loop {
            let available_frames =
                graph.audio_item_frames_available_for_next_block(requested_frames);
            if available_frames > 0 {
                break Ok(available_frames);
            }
            let Some(item_id) = graph.audio_item_missing_for_next_block(1) else {
                // A feeder can publish the first frame between the prefix query above and this
                // one-frame readiness check. Re-query the prefix instead of assuming the entire
                // requested block is ready; bounded queues may only have one frame at a time.
                thread::yield_now();
                continue;
            };
            if cancelled.load(Ordering::Acquire) {
                break Err(OfflineRenderError::Cancelled);
            }
            let Some(feeder_index) = item_ids.iter().position(|candidate| *candidate == item_id)
            else {
                break Err(OfflineRenderError::SourceEndedEarly { item_id });
            };
            let feeder_finished = {
                let mut feeders = feeders.borrow_mut();
                let Some(feeder) = feeders.get_mut(feeder_index).and_then(Option::as_mut) else {
                    break Err(OfflineRenderError::SourceEndedEarly { item_id });
                };
                if feeder.is_finished() {
                    feeders[feeder_index].take()
                } else {
                    None
                }
            };
            if let Some(feeder) = feeder_finished {
                if let Err(error) = feeder.join() {
                    break Err(OfflineRenderError::Media(error.to_string()));
                }
                break Err(OfflineRenderError::SourceEndedEarly { item_id });
            }
            thread::sleep(Duration::from_millis(1));
        },
        || {
            for feeder in feeders.borrow_mut().iter_mut().filter_map(Option::take) {
                feeder
                    .join()
                    .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
            }
            Ok(())
        },
        report_progress,
    );
    (graph, result)
}

/// Builds and exports a saved project snapshot, including its configured CLAP instruments and FX.
///
/// Invoke on a worker thread. Plugin libraries are trusted native code and are loaded in-process,
/// matching normal playback behavior. The project database is used for media lookup; project state
/// and live playback are not modified.
pub fn render_project_file_to_pcm24_wav(
    project_path: impl AsRef<Path>,
    project: &Project,
    destination: impl AsRef<Path>,
    master_ceiling: MasterOutputCeiling,
    cancelled: &AtomicBool,
    mut report_progress: impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(OfflineRenderError::Cancelled);
    }
    let total_frames = project_render_length_samples(project, DEFAULT_EFFECT_TAIL_SECONDS)?;
    let store = ProjectStore::open(project_path.as_ref())
        .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
    let prepared = prepare_audio_playback(project, &store, 16_384, 2_048)
        .map_err(|error| OfflineRenderError::Media(error.to_string()));
    let close = store
        .close()
        .map_err(|error| OfflineRenderError::Media(error.to_string()));
    let mut prepared = prepared?;
    close?;
    prepared.set_master_output_ceiling_dbfs(master_ceiling);

    let mut instrument_owners = Vec::new();
    let mut instrument_processors = Vec::new();
    let mut effect_owners = Vec::new();
    let mut effect_processors = Vec::new();
    let sample_rate = project.settings().sample_rate();
    let max_block_frames = prepared.graph().max_block_frames();
    for track in project.tracks() {
        if track.is_frozen() {
            continue;
        }
        if cancelled.load(Ordering::Acquire) {
            cleanup_uninstalled_processors(
                instrument_owners,
                instrument_processors,
                effect_owners,
                effect_processors,
            );
            return Err(OfflineRenderError::Cancelled);
        }
        if let Some(instrument) = track.instrument() {
            let max_events = prepared.graph().midi_event_capacity_for_track(track.id());
            // SAFETY: the user assigned this saved CLAP instrument through the normal plugin UI.
            let loaded = unsafe {
                ClapInstrumentOwner::load_with_state(
                    Path::new(instrument.bundle_path()),
                    instrument.plugin_id(),
                    instrument.state(),
                    sample_rate,
                    max_block_frames,
                    max_events,
                )
            };
            let (owner, processor) = match loaded {
                Ok(loaded) => loaded,
                Err(error) => {
                    cleanup_uninstalled_processors(
                        instrument_owners,
                        instrument_processors,
                        effect_owners,
                        effect_processors,
                    );
                    return Err(OfflineRenderError::Media(error.to_string()));
                }
            };
            let instance_id = owner.instance_id();
            instrument_owners.push((instance_id, owner));
            instrument_processors.push(TrackInstrumentProcessor::new(track.id(), processor));
        }
        for (chain_index, effect) in track.fx_chain().iter().enumerate() {
            if !effect.is_enabled() {
                continue;
            }
            let parameter_values: Vec<_> = effect.parameter_values().collect();
            // SAFETY: this saved effect was assigned through the normal plugin UI.
            let loaded = unsafe {
                ClapEffectOwner::load_with_state(
                    Path::new(effect.bundle_path()),
                    effect.plugin_id(),
                    effect.state(),
                    &parameter_values,
                    sample_rate,
                    max_block_frames,
                )
            };
            let (owner, processor) = match loaded {
                Ok(loaded) => loaded,
                Err(error) => {
                    cleanup_uninstalled_processors(
                        instrument_owners,
                        instrument_processors,
                        effect_owners,
                        effect_processors,
                    );
                    return Err(OfflineRenderError::Media(error.to_string()));
                }
            };
            let instance_id = owner.instance_id();
            effect_owners.push((instance_id, owner));
            effect_processors.push(TrackFxProcessor::new(
                track.id(),
                chain_index,
                effect.plugin_id(),
                processor,
            ));
        }
    }
    if cancelled.load(Ordering::Acquire) {
        cleanup_uninstalled_processors(
            instrument_owners,
            instrument_processors,
            effect_owners,
            effect_processors,
        );
        return Err(OfflineRenderError::Cancelled);
    }
    if let Err(error) = prepared
        .graph_mut()
        .install_instrument_processors(project, &mut instrument_processors)
    {
        cleanup_uninstalled_processors(
            instrument_owners,
            instrument_processors,
            effect_owners,
            effect_processors,
        );
        return Err(OfflineRenderError::Media(error.to_string()));
    }
    if let Err(error) = prepared
        .graph_mut()
        .install_fx_processors(project, &mut effect_processors)
    {
        let _stop_failures = prepared.graph_mut().stop_processors_after_offline_render();
        let stopped_instruments = prepared.graph_mut().take_stopped_instruments();
        deactivate_instruments(stopped_instruments, instrument_owners);
        cleanup_uninstalled_processors(Vec::new(), Vec::new(), effect_owners, effect_processors);
        return Err(OfflineRenderError::Media(error.to_string()));
    }

    let (mut graph, result) = render_prepared_inner(
        prepared,
        project,
        destination,
        total_frames,
        cancelled,
        &mut report_progress,
    );
    let _stop_failures = graph.stop_processors_after_offline_render();
    deactivate_instruments(graph.take_stopped_instruments(), instrument_owners);
    deactivate_effects(graph.take_stopped_fx_processors(), effect_owners);
    result
}

/// Renders one instrument track to float WAV from its first MIDI item through its last item and
/// the plugin tail. Float output preserves headroom because track and Master gain remain live
/// after freezing. Track gain, pan, routing, mute, solo, and volume automation are neutralized in
/// this temporary source render. The saved project and playback graph are never modified.
pub fn render_freeze_track_to_float32_wav(
    project_path: impl AsRef<Path>,
    project: &Project,
    track_id: aaadaw_core::TrackId,
    destination: impl AsRef<Path>,
    cancelled: &AtomicBool,
    mut report_progress: impl FnMut(u64, u64),
) -> Result<FrozenTrackRender, OfflineRenderError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(OfflineRenderError::Cancelled);
    }
    let track = project
        .tracks()
        .iter()
        .find(|track| track.id() == track_id)
        .ok_or_else(|| OfflineRenderError::Media("freeze track no longer exists".into()))?;
    if track.is_frozen() || track.is_bus() || track.instrument().is_none() {
        return Err(OfflineRenderError::Media(
            "only an unfrozen instrument track can be frozen".into(),
        ));
    }
    let midi_items: Vec<_> = project
        .midi_items()
        .iter()
        .filter(|item| item.track_id() == track_id)
        .collect();
    if !midi_items.iter().any(|item| !item.notes().is_empty()) {
        return Err(OfflineRenderError::Media(
            "track has no MIDI notes to render".into(),
        ));
    }
    let mut start_sample = u64::MAX;
    let mut content_end = 0_u64;
    for item in &midi_items {
        let end_tick = item
            .start_tick()
            .checked_add(item.length_ticks())
            .ok_or(OfflineRenderError::TimelineRange)?;
        start_sample = start_sample.min(
            project
                .sample_at_tick(item.start_tick())
                .map_err(|_| OfflineRenderError::TimelineRange)?,
        );
        content_end = content_end.max(
            project
                .sample_at_tick(end_tick)
                .map_err(|_| OfflineRenderError::TimelineRange)?,
        );
    }
    let mut snapshot = project.snapshot();
    snapshot
        .tracks
        .retain(|candidate| candidate.id == track_id.value());
    let source_track = snapshot
        .tracks
        .first_mut()
        .ok_or_else(|| OfflineRenderError::Media("freeze track snapshot is missing".into()))?;
    source_track.output_track_id = None;
    source_track.volume_db = 0.0;
    source_track.pan = 0.0;
    source_track.muted = false;
    source_track.solo = false;
    source_track.record_armed = false;
    source_track.volume_automation.clear();
    source_track.frozen_audio_item_id = None;
    snapshot.audio_items.clear();
    snapshot
        .midi_items
        .retain(|item| item.track_id == track_id.value());
    let source_project = Project::from_snapshot(snapshot)
        .map_err(|error| OfflineRenderError::Media(error.to_string()))?;

    let store = ProjectStore::open(project_path.as_ref())
        .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
    let prepared = prepare_audio_playback(&source_project, &store, 16_384, 2_048)
        .map_err(|error| OfflineRenderError::Media(error.to_string()));
    let close = store
        .close()
        .map_err(|error| OfflineRenderError::Media(error.to_string()));
    let mut prepared = prepared?;
    close?;
    prepared
        .graph()
        .master_output_safety_controller()
        .set_guard_enabled(false);

    let mut instrument_owners = Vec::new();
    let mut instrument_processors = Vec::new();
    let mut effect_owners = Vec::new();
    let mut effect_processors = Vec::new();
    let mut plugin_tail_frames = Vec::new();
    let sample_rate = source_project.settings().sample_rate();
    let max_block_frames = prepared.graph().max_block_frames();
    let source_track = &source_project.tracks()[0];
    let instrument = source_track
        .instrument()
        .expect("validated freeze source has an instrument");
    let max_events = prepared.graph().midi_event_capacity_for_track(track_id);
    // SAFETY: this saved instrument was assigned through the normal plugin UI.
    let loaded = unsafe {
        ClapInstrumentOwner::load_with_state(
            Path::new(instrument.bundle_path()),
            instrument.plugin_id(),
            instrument.state(),
            sample_rate,
            max_block_frames,
            max_events,
        )
    }
    .map_err(|error| OfflineRenderError::Media(error.to_string()))?;
    let (owner, mut processor) = loaded;
    match processor.tail_length_samples() {
        Some(frames) => plugin_tail_frames.push(frames),
        None => {
            instrument_owners.push((owner.instance_id(), owner));
            instrument_processors.push(TrackInstrumentProcessor::new(track_id, processor));
            cleanup_uninstalled_processors(
                instrument_owners,
                instrument_processors,
                effect_owners,
                effect_processors,
            );
            return Err(OfflineRenderError::Media(
                "cannot freeze a plugin that reports an infinite tail".into(),
            ));
        }
    }
    instrument_owners.push((owner.instance_id(), owner));
    instrument_processors.push(TrackInstrumentProcessor::new(track_id, processor));
    for (chain_index, effect) in source_track.fx_chain().iter().enumerate() {
        if !effect.is_enabled() {
            continue;
        }
        let parameter_values: Vec<_> = effect.parameter_values().collect();
        // SAFETY: this saved effect was assigned through the normal plugin UI.
        let loaded = unsafe {
            ClapEffectOwner::load_with_state(
                Path::new(effect.bundle_path()),
                effect.plugin_id(),
                effect.state(),
                &parameter_values,
                sample_rate,
                max_block_frames,
            )
        };
        let (owner, mut processor) = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                cleanup_uninstalled_processors(
                    instrument_owners,
                    instrument_processors,
                    effect_owners,
                    effect_processors,
                );
                return Err(OfflineRenderError::Media(error.to_string()));
            }
        };
        match processor.tail_length_samples() {
            Some(frames) => plugin_tail_frames.push(frames),
            None => {
                effect_owners.push((owner.instance_id(), owner));
                effect_processors.push(TrackFxProcessor::new(
                    track_id,
                    chain_index,
                    effect.plugin_id(),
                    processor,
                ));
                cleanup_uninstalled_processors(
                    instrument_owners,
                    instrument_processors,
                    effect_owners,
                    effect_processors,
                );
                return Err(OfflineRenderError::Media(
                    "cannot freeze a plugin that reports an infinite tail".into(),
                ));
            }
        }
        effect_owners.push((owner.instance_id(), owner));
        effect_processors.push(TrackFxProcessor::new(
            track_id,
            chain_index,
            effect.plugin_id(),
            processor,
        ));
    }
    let tail_frames = match freeze_tail_frames(sample_rate, &plugin_tail_frames) {
        Ok(frames) => frames,
        Err(error) => {
            cleanup_uninstalled_processors(
                instrument_owners,
                instrument_processors,
                effect_owners,
                effect_processors,
            );
            return Err(error);
        }
    };
    let total_frames = match content_end
        .checked_add(tail_frames)
        .and_then(|end_sample| end_sample.checked_sub(start_sample))
        .filter(|frames| *frames > 0)
    {
        Some(frames) => frames,
        None => {
            cleanup_uninstalled_processors(
                instrument_owners,
                instrument_processors,
                effect_owners,
                effect_processors,
            );
            return Err(OfflineRenderError::TimelineRange);
        }
    };
    if cancelled.load(Ordering::Acquire) {
        cleanup_uninstalled_processors(
            instrument_owners,
            instrument_processors,
            effect_owners,
            effect_processors,
        );
        return Err(OfflineRenderError::Cancelled);
    }
    if let Err(error) = prepared
        .graph_mut()
        .install_instrument_processors(&source_project, &mut instrument_processors)
    {
        cleanup_uninstalled_processors(
            instrument_owners,
            instrument_processors,
            effect_owners,
            effect_processors,
        );
        return Err(OfflineRenderError::Media(error.to_string()));
    }
    if let Err(error) = prepared
        .graph_mut()
        .install_fx_processors(&source_project, &mut effect_processors)
    {
        let _ = prepared.graph_mut().stop_processors_after_offline_render();
        deactivate_instruments(
            prepared.graph_mut().take_stopped_instruments(),
            instrument_owners,
        );
        cleanup_uninstalled_processors(Vec::new(), Vec::new(), effect_owners, effect_processors);
        return Err(OfflineRenderError::Media(error.to_string()));
    }
    let export = Float32WavExport::create(destination.as_ref(), sample_rate)
        .map_err(OfflineRenderError::Float32Wav);
    let export = match export {
        Ok(export) => export,
        Err(error) => {
            cleanup_uninstalled_processors(
                instrument_owners,
                instrument_processors,
                effect_owners,
                effect_processors,
            );
            return Err(error);
        }
    };
    let result = render_graph_with_waiter(
        prepared.graph_mut(),
        RenderRequest {
            total_frames,
            start_sample,
            cancelled,
        },
        export,
        |_, frames| Ok(frames),
        || Ok(()),
        &mut report_progress,
    );
    let graph = prepared.graph_mut();
    let _ = graph.stop_processors_after_offline_render();
    deactivate_instruments(graph.take_stopped_instruments(), instrument_owners);
    deactivate_effects(graph.take_stopped_fx_processors(), effect_owners);
    result?;
    Ok(FrozenTrackRender {
        start_sample,
        length_samples: total_frames,
    })
}

fn cleanup_uninstalled_processors(
    mut instrument_owners: Vec<(u64, ClapInstrumentOwner)>,
    instruments: Vec<TrackInstrumentProcessor>,
    mut effect_owners: Vec<(u64, ClapEffectOwner)>,
    effects: Vec<TrackFxProcessor>,
) {
    for processor in instruments {
        let instance_id = processor.instance_id();
        let (track_id, processor) = processor.into_parts();
        if let Some(index) = instrument_owners
            .iter()
            .position(|(candidate, _)| *candidate == instance_id)
        {
            let (_, owner) = instrument_owners.swap_remove(index);
            owner.deactivate(processor.stop());
        } else {
            let _ = track_id;
        }
    }
    for processor in effects {
        let (instance_id, _, _, _, processor) = processor.into_parts();
        if let Some(index) = effect_owners
            .iter()
            .position(|(candidate, _)| *candidate == instance_id)
        {
            let (_, owner) = effect_owners.swap_remove(index);
            owner.deactivate(processor.stop());
        }
    }
    for (_, mut owner) in instrument_owners {
        let _ = owner.try_deactivate_unused();
    }
    for (_, mut owner) in effect_owners {
        let _ = owner.try_deactivate_unused();
    }
}

fn deactivate_instruments(
    processors: Vec<aaadaw_engine::StoppedTrackInstrument>,
    mut owners: Vec<(u64, ClapInstrumentOwner)>,
) {
    for processor in processors {
        let instance_id = processor.instance_id();
        let (_, processor) = processor.into_parts();
        if let Some(index) = owners
            .iter()
            .position(|(candidate, _)| *candidate == instance_id)
        {
            let (_, owner) = owners.swap_remove(index);
            owner.deactivate(processor);
        }
    }
    for (_, mut owner) in owners {
        let _ = owner.try_deactivate_unused();
    }
}

fn deactivate_effects(
    processors: Vec<aaadaw_engine::StoppedTrackFxProcessor>,
    mut owners: Vec<(u64, ClapEffectOwner)>,
) {
    for processor in processors {
        let instance_id = processor.instance_id();
        let (_, _, _, processor) = processor.into_parts();
        if let Some(index) = owners
            .iter()
            .position(|(candidate, _)| *candidate == instance_id)
        {
            let (_, owner) = owners.swap_remove(index);
            owner.deactivate(processor);
        }
    }
    for (_, mut owner) in owners {
        let _ = owner.try_deactivate_unused();
    }
}

fn render_graph_with_waiter<W: OfflineWavWriter>(
    graph: &mut AudioRenderGraph,
    request: RenderRequest<'_>,
    mut export: W,
    mut wait_for_input: impl FnMut(&mut AudioRenderGraph, usize) -> Result<usize, OfflineRenderError>,
    before_finish: impl FnOnce() -> Result<(), OfflineRenderError>,
    report_progress: &mut impl FnMut(u64, u64),
) -> Result<(), OfflineRenderError> {
    let RenderRequest {
        total_frames,
        start_sample,
        cancelled,
    } = request;
    let block_capacity = graph.max_block_frames();
    debug_assert!(
        block_capacity > 0,
        "render graphs reject zero block capacity"
    );
    let mut output = vec![[0.0_f32; 2]; block_capacity];
    graph.transport_mut().seek_sample(start_sample);
    graph.transport_mut().start();
    let mut rendered = 0_u64;
    report_progress(rendered, total_frames);

    while rendered < total_frames {
        if cancelled.load(Ordering::Acquire) {
            return Err(OfflineRenderError::Cancelled);
        }
        let requested_frames =
            usize::try_from((total_frames - rendered).min(block_capacity as u64))
                .expect("block frame count is bounded by usize capacity");
        let frames = wait_for_input(graph, requested_frames)?;
        if frames == 0 || frames > requested_frames {
            return Err(OfflineRenderError::InputUnderrun {
                samples: requested_frames,
            });
        }
        let stats = graph
            .render_into(&mut output[..frames])
            .map_err(OfflineRenderError::Graph)?;
        if stats.underrun_samples != 0 {
            return Err(OfflineRenderError::InputUnderrun {
                samples: stats.underrun_samples,
            });
        }
        export.write_frames(&output[..frames])?;
        rendered += frames as u64;
        report_progress(rendered, total_frames);
    }

    before_finish()?;
    export.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_core::{DawAction, Project};
    use aaadaw_engine::{AudioRenderGraph, pcm_stream};
    use std::fs;
    use std::io::Read;
    use std::sync::atomic::AtomicU64;

    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

    fn destination() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "aaadaw-offline-render-{}-{}.wav",
            std::process::id(),
            NEXT_PATH.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn project_with_one_track() -> Project {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Render".to_owned(),
            })
            .expect("track creation should succeed");
        project
    }

    #[test]
    fn project_render_length_includes_midi_end_and_the_fixed_effect_tail() {
        let mut project = project_with_one_track();
        project
            .apply(DawAction::InsertMidiItem {
                track_id: project.tracks()[0].id(),
                start_tick: 960,
                length_ticks: 960,
            })
            .expect("MIDI item should be inserted");
        assert_eq!(
            project_render_length_samples(&project, DEFAULT_EFFECT_TAIL_SECONDS)
                .expect("render length should be representable"),
            48_000 + 2 * 48_000
        );
    }

    #[test]
    fn freeze_tail_uses_the_longer_of_default_allowance_and_serial_plugin_tails() {
        assert_eq!(
            freeze_tail_frames(48_000, &[24_000, 120_000]).unwrap(),
            144_000
        );
        assert_eq!(freeze_tail_frames(48_000, &[24_000]).unwrap(), 96_000);
    }

    #[test]
    fn render_graph_to_wav_writes_exact_requested_frames_and_reports_progress() {
        let project = project_with_one_track();
        let (mut producer, consumer) = pcm_stream(8).expect("queue should be created");
        assert_eq!(producer.push_samples(&[0.5; 4]), 4);
        let mut graph = AudioRenderGraph::new(&project, vec![consumer], 4)
            .expect("graph should match the track count");
        let path = destination();
        let cancelled = AtomicBool::new(false);
        let mut progress = Vec::new();
        render_graph_to_pcm24_wav(
            &mut graph,
            &path,
            project.settings().sample_rate(),
            4,
            &cancelled,
            |done, total| progress.push((done, total)),
        )
        .expect("render should complete");
        assert_eq!(progress, [(0, 4), (4, 4)]);
        let mut bytes = Vec::new();
        fs::File::open(&path)
            .expect("export should exist")
            .read_to_end(&mut bytes)
            .expect("export should be readable");
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 24);
        let expected = (0.5_f64 * std::f64::consts::FRAC_1_SQRT_2 * 8_388_608.0).round() as i32;
        let sample = &bytes[44..47];
        assert_eq!(sample, &expected.to_le_bytes()[..3]);
        fs::remove_file(path).expect("test export should be removed");
    }

    #[test]
    fn render_graph_exports_the_stereo_mix_of_multiple_tracks() {
        let mut project = project_with_one_track();
        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "Second".to_owned(),
            })
            .expect("second track should be created");
        let (mut first_producer, first_consumer) = pcm_stream(8).expect("queue should be created");
        let (mut second_producer, second_consumer) =
            pcm_stream(8).expect("queue should be created");
        assert_eq!(first_producer.push_samples(&[0.1; 4]), 4);
        assert_eq!(second_producer.push_samples(&[0.1; 4]), 4);
        let mut graph = AudioRenderGraph::new(&project, vec![first_consumer, second_consumer], 4)
            .expect("graph should have one source per track");
        let path = destination();
        let cancelled = AtomicBool::new(false);
        render_graph_to_pcm24_wav(
            &mut graph,
            &path,
            project.settings().sample_rate(),
            4,
            &cancelled,
            |_, _| {},
        )
        .expect("both tracks should render");

        let bytes = fs::read(&path).expect("export should exist");
        let sample = i32::from_le_bytes([
            bytes[44],
            bytes[45],
            bytes[46],
            if bytes[46] & 0x80 == 0 { 0 } else { 0xff },
        ]);
        let expected = (0.1_f64 * std::f64::consts::SQRT_2 * 8_388_608.0).round() as i32;
        assert!((sample - expected).abs() <= 1);
        assert_eq!(&bytes[47..50], &bytes[44..47]);
        fs::remove_file(path).expect("test export should be removed");
    }

    #[test]
    fn underrun_and_cancellation_leave_no_published_export() {
        let project = project_with_one_track();
        let (_, consumer) = pcm_stream(8).expect("queue should be created");
        let mut graph = AudioRenderGraph::new(&project, vec![consumer], 4)
            .expect("graph should match the track count");
        let path = destination();
        let cancelled = AtomicBool::new(false);
        assert!(matches!(
            render_graph_to_pcm24_wav(
                &mut graph,
                &path,
                project.settings().sample_rate(),
                4,
                &cancelled,
                |_, _| {},
            ),
            Err(OfflineRenderError::InputUnderrun { .. })
        ));
        assert!(!path.exists());

        cancelled.store(true, Ordering::Release);
        assert!(matches!(
            render_graph_to_pcm24_wav(
                &mut graph,
                &path,
                project.settings().sample_rate(),
                4,
                &cancelled,
                |_, _| {},
            ),
            Err(OfflineRenderError::Cancelled)
        ));
        assert!(!path.exists());
    }
}
