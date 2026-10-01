//! SQLite-backed persistence for `.aaadaw` projects.
//!
//! The adapter saves and restores validated [`aaadaw_core::ProjectSnapshot`]s;
//! domain state remains owned by `aaadaw-core`.

mod store;

pub use store::{CURRENT_SCHEMA_VERSION, ProjectStore, SqliteAudioAssetReader, StorageError};
