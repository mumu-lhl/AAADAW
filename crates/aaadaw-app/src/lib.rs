//! Application-layer orchestration for preparing project audio playback.
//!
//! This crate resolves opaque media references through storage and connects background media
//! feeders to the realtime render graph. Call its APIs from a control thread, never an audio
//! callback.

use aaadaw_core::Project;
use aaadaw_engine::{
    AudioGraphBuildError, AudioItemStream, AudioRenderGraph, PcmStreamError, pcm_stream,
};
use aaadaw_media::{
    AudioFeedWorker, MediaError, spawn_audio_item_stream, spawn_audio_item_stream_from_reader,
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
    ExternalSourceUnavailable { media_ref: String },
}

impl fmt::Display for PlaybackBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "project media resolution failed: {error}"),
            Self::Media(error) => write!(formatter, "audio feeder could not start: {error}"),
            Self::PcmStream(error) => write!(formatter, "PCM stream setup failed: {error}"),
            Self::AudioGraph(error) => write!(formatter, "render graph setup failed: {error}"),
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
    let output_sample_rate = project.settings().sample_rate();
    let mut feeders = Vec::with_capacity(project.audio_items().len());
    let mut item_streams = Vec::with_capacity(project.audio_items().len());

    for item in project.audio_items() {
        let resolved = store
            .resolve_audio_asset(item.media_ref())
            .map_err(PlaybackBuildError::Storage)?;
        let is_linked = matches!(&resolved, ResolvedAudioAsset::LinkedFile { .. });
        let (producer, consumer) =
            pcm_stream(queue_capacity_samples).map_err(PlaybackBuildError::PcmStream)?;
        let mut feeder = match resolved {
            ResolvedAudioAsset::Embedded(reader) => {
                let byte_len = reader.byte_len();
                let extension = Path::new(reader.original_name())
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_owned);
                spawn_audio_item_stream_from_reader(
                    item,
                    reader,
                    Some(byte_len),
                    extension.as_deref(),
                    output_sample_rate,
                    producer,
                )
                .map_err(PlaybackBuildError::Media)?
            }
            ResolvedAudioAsset::LinkedFile { path, .. } => {
                spawn_audio_item_stream(item, path, output_sample_rate, producer)
                    .map_err(PlaybackBuildError::Media)?
            }
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
        item_streams.push(AudioItemStream::new(item.id(), consumer));
    }

    let graph = AudioRenderGraph::new_for_audio_items(project, item_streams, max_block_frames)
        .map_err(PlaybackBuildError::AudioGraph)?;
    Ok(PreparedAudioPlayback { graph, feeders })
}
