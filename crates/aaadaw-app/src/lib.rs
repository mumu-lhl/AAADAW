//! Application-layer orchestration for preparing project audio playback.
//!
//! This crate resolves opaque media references through storage and connects background media
//! feeders to the realtime render graph. It also coordinates cancellable asset imports and turns
//! them into project actions. Call its APIs from a control thread, never an audio callback.

mod asset_management;
mod audio_editing;
mod audio_import;
mod midi_editing;
mod waveform;

pub use aaadaw_storage::AudioAssetSourceStatus;
pub use asset_management::{
    AudioAssetManagementOperation, AudioAssetManagementProgress, AudioAssetManagementResult,
    AudioAssetManagementWorker, AudioAssetSourceStatusEntry, relink_external_audio_source,
    start_audio_asset_management,
};
pub use audio_editing::{AudioEditError, duplicate_audio_item, set_audio_item_start_sample};
pub use audio_import::{
    AudioItemImportError, AudioItemImportProgress, AudioItemImportWorker, start_audio_item_import,
};
pub use midi_editing::{
    MidiEditError, add_quarter_note, adjust_midi_note_pitch, adjust_midi_note_velocity,
    create_four_beat_midi_item, delete_midi_note, move_midi_item_by_beat,
    move_midi_note_by_sixteenth, quantize_midi_item_to_sixteenth,
};
pub use waveform::{AudioWaveformResult, AudioWaveformWorker};

use aaadaw_core::Project;
use aaadaw_engine::{
    AudioGraphBuildError, AudioItemStream, AudioRenderGraph, PcmStreamError, pcm_stream,
};
#[cfg(feature = "jack-backend")]
use aaadaw_engine::{JackAudioOutput, JackOutputError, JackOutputStats};
use aaadaw_media::{
    AudioFeedWorker, MediaError, spawn_audio_item_stream, spawn_audio_item_stream_at,
    spawn_audio_item_stream_from_reader, spawn_audio_item_stream_from_reader_at,
};
use aaadaw_storage::{ProjectStore, ResolvedAudioAsset, StorageError};
use std::error::Error as StdError;
use std::fmt;
use std::io::ErrorKind;
use std::path::Path;

/// Errors while resolving project media and preparing its render graph.
#[derive(Debug)]
pub enum PlaybackBuildError {
    Storage(StorageError),
    Media(MediaError),
    PcmStream(PcmStreamError),
    AudioGraph(AudioGraphBuildError),
    #[cfg(feature = "jack-backend")]
    Jack(JackOutputError),
    ExternalSourceUnavailable {
        media_ref: String,
    },
}

impl fmt::Display for PlaybackBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "project media resolution failed: {error}"),
            Self::Media(error) => write!(formatter, "audio feeder could not start: {error}"),
            Self::PcmStream(error) => write!(formatter, "PCM stream setup failed: {error}"),
            Self::AudioGraph(error) => write!(formatter, "render graph setup failed: {error}"),
            #[cfg(feature = "jack-backend")]
            Self::Jack(error) => write!(formatter, "JACK output setup failed: {error}"),
            Self::ExternalSourceUnavailable { media_ref } => {
                write!(
                    formatter,
                    "external source for {media_ref:?} is unavailable"
                )
            }
        }
    }
}

impl StdError for PlaybackBuildError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Media(error) => Some(error),
            Self::PcmStream(error) => Some(error),
            Self::AudioGraph(error) => Some(error),
            #[cfg(feature = "jack-backend")]
            Self::Jack(error) => Some(error),
            Self::ExternalSourceUnavailable { .. } => None,
        }
    }
}

/// A prepared project graph together with the workers that feed its item queues.
///
/// Keep this value alive while rendering. Dropping it requests cancellation of the feeders.
pub struct PreparedAudioPlayback {
    graph: AudioRenderGraph,
    feeders: Vec<AudioFeedWorker>,
}

impl PreparedAudioPlayback {
    /// Returns the prepared realtime render graph.
    pub fn graph(&self) -> &AudioRenderGraph {
        &self.graph
    }

    /// Returns mutable graph access for transport control and rendering.
    pub fn graph_mut(&mut self) -> &mut AudioRenderGraph {
        &mut self.graph
    }

    /// Returns the number of background media feeders owned by this playback.
    pub fn feeder_count(&self) -> usize {
        self.feeders.len()
    }

    /// Transfers the graph and feeder workers to a device-backend owner.
    ///
    /// The caller must keep the workers alive for as long as the graph consumes their queues.
    pub fn into_parts(self) -> (AudioRenderGraph, Vec<AudioFeedWorker>) {
        (self.graph, self.feeders)
    }

    /// Opens JACK output and retains feeder workers for the lifetime of playback.
    #[cfg(feature = "jack-backend")]
    pub fn into_jack_output(self) -> Result<RunningJackPlayback, PlaybackBuildError> {
        let (graph, feeders) = self.into_parts();
        let output = JackAudioOutput::open(graph).map_err(PlaybackBuildError::Jack)?;
        Ok(RunningJackPlayback {
            output,
            feeders,
            retired_feeders: None,
            is_playing: false,
        })
    }
}

/// A JACK client and the background feeders that supply its render graph.
#[cfg(feature = "jack-backend")]
pub struct RunningJackPlayback {
    output: JackAudioOutput,
    feeders: Vec<AudioFeedWorker>,
    retired_feeders: Option<Vec<AudioFeedWorker>>,
    is_playing: bool,
}

#[cfg(feature = "jack-backend")]
impl RunningJackPlayback {
    /// Queues playback for the next JACK callback.
    pub fn play(&mut self) -> Result<(), JackOutputError> {
        self.output.play()?;
        self.is_playing = true;
        Ok(())
    }

    /// Queues a stop for the next JACK callback.
    pub fn stop(&mut self) -> Result<(), JackOutputError> {
        self.output.stop()?;
        self.is_playing = false;
        Ok(())
    }

    /// Prepares and queues a seek without interrupting the active graph during media refill.
    ///
    /// Call from a control worker: asset resolution and decoder startup can block. If preparation
    /// fails, current graph and playback position remain unchanged.
    pub fn seek_to_sample(
        &mut self,
        project: &Project,
        store: &ProjectStore,
        timeline_sample: u64,
        queue_capacity_samples: usize,
        max_block_frames: usize,
    ) -> Result<(), PlaybackBuildError> {
        self.collect_retired_graphs();
        if self.retired_feeders.is_some() {
            return Err(PlaybackBuildError::Jack(
                JackOutputError::GraphReplacementInFlight,
            ));
        }
        let prepared = prepare_audio_playback_at(
            project,
            store,
            timeline_sample,
            queue_capacity_samples,
            max_block_frames,
        )?;
        self.replace_graph(prepared)
    }

    /// Replaces active graph and retains old feeders until callback retires old graph.
    pub fn replace_graph(
        &mut self,
        prepared: PreparedAudioPlayback,
    ) -> Result<(), PlaybackBuildError> {
        self.collect_retired_graphs();
        if self.retired_feeders.is_some() {
            return Err(PlaybackBuildError::Jack(
                JackOutputError::GraphReplacementInFlight,
            ));
        }
        let (graph, feeders) = prepared.into_parts();
        self.output
            .replace_graph(graph, self.is_playing)
            .map_err(PlaybackBuildError::Jack)?;
        self.retired_feeders = Some(std::mem::replace(&mut self.feeders, feeders));
        Ok(())
    }

    /// Reclaims replaced graphs and stops their feeder workers after the callback swap.
    pub fn collect_retired_graphs(&mut self) -> bool {
        let collected = self.output.collect_retired_graphs();
        if collected > 0 {
            drop(self.retired_feeders.take());
            true
        } else {
            false
        }
    }

    /// Returns lock-free callback counters.
    pub fn stats(&self) -> JackOutputStats {
        self.output.stats()
    }
}

/// Resolves every project AudioItem and prepares a fixed-topology streaming graph.
///
/// Queue capacity and callback block size are specified in mono samples/frames. Embedded assets
/// stream directly from independent SQLite readers; linked files are opened and probed on background
/// workers. This call waits for each worker's source-open result, so invoke it on a background
/// control thread when the UI must remain responsive. Partial setup is cancelled if a source fails.
pub fn prepare_audio_playback(
    project: &Project,
    store: &ProjectStore,
    queue_capacity_samples: usize,
    max_block_frames: usize,
) -> Result<PreparedAudioPlayback, PlaybackBuildError> {
    prepare_audio_playback_at(project, store, 0, queue_capacity_samples, max_block_frames)
}

/// Prepares playback with the graph transport positioned at an arbitrary project sample.
///
/// Items containing the seek sample are re-decoded off-thread and their earlier output is discarded;
/// items already behind the playhead receive empty queues until a later graph replacement.
pub fn prepare_audio_playback_at(
    project: &Project,
    store: &ProjectStore,
    timeline_sample: u64,
    queue_capacity_samples: usize,
    max_block_frames: usize,
) -> Result<PreparedAudioPlayback, PlaybackBuildError> {
    let output_sample_rate = project.settings().sample_rate();
    let mut feeders = Vec::with_capacity(project.audio_items().len());
    let mut item_streams = Vec::with_capacity(project.audio_items().len());

    for item in project.audio_items() {
        let (producer, consumer) =
            pcm_stream(queue_capacity_samples).map_err(PlaybackBuildError::PcmStream)?;
        if timeline_sample >= item.end_sample() {
            drop(producer);
            item_streams.push(AudioItemStream::new_at_sample(
                item.id(),
                item.end_sample(),
                consumer,
            ));
            continue;
        }
        let needs_refill = timeline_sample > item.start_sample();
        let resolved = store
            .resolve_audio_asset(item.media_ref())
            .map_err(PlaybackBuildError::Storage)?;
        let is_linked = matches!(&resolved, ResolvedAudioAsset::LinkedFile { .. });
        let mut feeder = match resolved {
            ResolvedAudioAsset::Embedded(reader) => {
                let byte_len = reader.byte_len();
                let extension = Path::new(reader.original_name())
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_owned);
                if needs_refill {
                    spawn_audio_item_stream_from_reader_at(
                        item,
                        timeline_sample,
                        reader,
                        Some(byte_len),
                        extension.as_deref(),
                        output_sample_rate,
                        producer,
                    )
                } else {
                    spawn_audio_item_stream_from_reader(
                        item,
                        reader,
                        Some(byte_len),
                        extension.as_deref(),
                        output_sample_rate,
                        producer,
                    )
                }
                .map_err(PlaybackBuildError::Media)?
            }
            ResolvedAudioAsset::LinkedFile { path, .. } => if needs_refill {
                spawn_audio_item_stream_at(
                    item,
                    timeline_sample,
                    path,
                    output_sample_rate,
                    producer,
                )
            } else {
                spawn_audio_item_stream(item, path, output_sample_rate, producer)
            }
            .map_err(PlaybackBuildError::Media)?,
        };
        if let Err(error) = feeder.wait_ready() {
            if is_linked
                && matches!(
                    &error,
                    MediaError::WorkerStartupFailed {
                        io_kind: Some(ErrorKind::NotFound),
                        ..
                    }
                )
            {
                return Err(PlaybackBuildError::ExternalSourceUnavailable {
                    media_ref: item.media_ref().to_owned(),
                });
            }
            return Err(PlaybackBuildError::Media(error));
        }
        feeders.push(feeder);
        if needs_refill {
            item_streams.push(AudioItemStream::new_at_sample(
                item.id(),
                timeline_sample,
                consumer,
            ));
        } else {
            item_streams.push(AudioItemStream::new(item.id(), consumer));
        }
    }

    let mut graph = AudioRenderGraph::new_for_audio_items(project, item_streams, max_block_frames)
        .map_err(PlaybackBuildError::AudioGraph)?;
    graph.transport_mut().seek_sample(timeline_sample);
    Ok(PreparedAudioPlayback { graph, feeders })
}
