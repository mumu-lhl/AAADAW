//! Background source scanning and external-asset packing for project files.

use aaadaw_storage::{
    AudioAssetPackProgress, AudioAssetPackWorker, AudioAssetSourceScanProgress,
    AudioAssetSourceScanWorker, AudioAssetSourceStatus, ProjectStore, ResolvedAudioAsset,
};
use std::path::PathBuf;

/// A project media maintenance operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioAssetManagementOperation {
    /// Hash embedded sources and check whether live external links still exist.
    ScanSources,
    /// Embed every live external asset while preserving its media reference.
    PackExternalAssets,
}

/// Progress from a project media maintenance worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioAssetManagementProgress {
    /// Progress after checking an embedded source or external link.
    SourceScan(AudioAssetSourceScanProgress),
    /// Chunk progress while packing a live external source.
    Pack(AudioAssetPackProgress),
}

/// A scanned source state, including whether the media reference is a live external link.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioAssetSourceStatusEntry {
    /// Stable project media reference.
    pub media_ref: String,
    /// Result of the source check.
    pub status: AudioAssetSourceStatus,
    /// Whether the reference currently resolves to a live external link.
    pub is_external_link: bool,
}

/// Result of a completed project media maintenance operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioAssetManagementResult {
    /// Source status entries collected by a completed scan.
    SourceScan(Vec<AudioAssetSourceStatusEntry>),
    /// References of external assets successfully embedded by the operation.
    Packed(Vec<String>),
}

enum Worker {
    SourceScan(AudioAssetSourceScanWorker),
    Pack(AudioAssetPackWorker),
}

/// A cancellable scan or pack worker that owns its project-store connection.
pub struct AudioAssetManagementWorker {
    store: ProjectStore,
    worker: Worker,
}

impl AudioAssetManagementWorker {
    /// Requests cancellation at the worker's next safe checkpoint.
    pub fn cancel(&self) {
        match &self.worker {
            Worker::SourceScan(worker) => worker.cancel(),
            Worker::Pack(worker) => worker.cancel(),
        }
    }

    /// Drains queued progress without blocking.
    pub fn progress(&self) -> Vec<AudioAssetManagementProgress> {
        match &self.worker {
            Worker::SourceScan(worker) => worker
                .progress()
                .try_iter()
                .map(AudioAssetManagementProgress::SourceScan)
                .collect(),
            Worker::Pack(worker) => worker
                .progress()
                .try_iter()
                .map(AudioAssetManagementProgress::Pack)
                .collect(),
        }
    }

    /// Returns whether the background operation has exited without blocking.
    pub fn is_finished(&self) -> bool {
        match &self.worker {
            Worker::SourceScan(worker) => worker.is_finished(),
            Worker::Pack(worker) => worker.is_finished(),
        }
    }

    /// Joins the operation and checkpoints/closes the owning project store.
    pub fn join(self) -> Result<AudioAssetManagementResult, String> {
        let Self { store, worker } = self;
        let result = match worker {
            Worker::SourceScan(worker) => worker
                .join()
                .map_err(|error| error.to_string())
                .and_then(|results| {
                    results
                        .into_iter()
                        .map(|(media_ref, status)| {
                            let is_external_link = matches!(
                                store
                                    .resolve_audio_asset(&media_ref)
                                    .map_err(|error| error.to_string())?,
                                ResolvedAudioAsset::LinkedFile { .. }
                            );
                            Ok(AudioAssetSourceStatusEntry {
                                media_ref,
                                status,
                                is_external_link,
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()
                        .map(AudioAssetManagementResult::SourceScan)
                }),
            Worker::Pack(worker) => worker
                .join()
                .map(AudioAssetManagementResult::Packed)
                .map_err(|error| error.to_string()),
        };
        let close = store.close().map_err(|error| error.to_string());
        let result = result?;
        close?;
        Ok(result)
    }
}

impl std::fmt::Debug for AudioAssetManagementWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AudioAssetManagementWorker(..)")
    }
}

/// Opens a project and starts cancellable background source scanning or asset packing.
pub fn relink_external_audio_source(
    project_path: PathBuf,
    media_ref: String,
    source_path: PathBuf,
) -> Result<(), String> {
    let mut store = ProjectStore::open(project_path).map_err(|error| error.to_string())?;
    let relink = store
        .relink_external_audio_file(&media_ref, source_path)
        .map_err(|error| error.to_string());
    let close = store.close().map_err(|error| error.to_string());
    relink?;
    close?;
    Ok(())
}

/// Starts a cancellable scan or pack operation for a saved project file.
pub fn start_audio_asset_management(
    project_path: PathBuf,
    operation: AudioAssetManagementOperation,
) -> Result<AudioAssetManagementWorker, String> {
    let store = ProjectStore::open(project_path).map_err(|error| error.to_string())?;
    let worker = match operation {
        AudioAssetManagementOperation::ScanSources => Worker::SourceScan(
            store
                .start_audio_asset_source_scan()
                .map_err(|error| error.to_string())?,
        ),
        AudioAssetManagementOperation::PackExternalAssets => Worker::Pack(
            store
                .start_audio_asset_pack_all()
                .map_err(|error| error.to_string())?,
        ),
    };
    Ok(AudioAssetManagementWorker { store, worker })
}
