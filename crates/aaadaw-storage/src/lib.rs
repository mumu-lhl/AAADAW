//! SQLite-backed persistence for `.aaadaw` projects.
//!
//! The adapter saves and restores validated [`aaadaw_core::ProjectSnapshot`]s;
//! domain state remains owned by `aaadaw-core`.

mod project_lock;
mod store;

pub use project_lock::ProjectSessionLock;

pub use store::{
    AudioAssetImportProgress, AudioAssetImportWorker, AudioAssetMetadata, AudioAssetPackProgress,
    AudioAssetPackWorker, AudioAssetSourceScanProgress, AudioAssetSourceScanWorker,
    AudioAssetSourceStatus, CURRENT_SCHEMA_VERSION, ProjectStore, ResolvedAudioAsset,
    SqliteAudioAssetReader, StorageError,
};
