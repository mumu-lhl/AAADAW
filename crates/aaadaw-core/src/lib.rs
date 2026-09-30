//! Core domain and application logic for AAADAW.
//!
//! This crate is independent of GUI, platform audio drivers, and storage
//! adapters. [`Project`] is the public seam for applying validated actions and
//! observing project state.

mod action;
mod error;
mod midi;
mod project;
mod timebase;
mod track;

pub use action::DawAction;
pub use error::ActionError;
pub use midi::{ItemId, MidiItem, MidiNote, MidiNoteData, NoteId};
pub use project::Project;
pub use timebase::{
    DEFAULT_PPQ, DEFAULT_SAMPLE_RATE, DEFAULT_TEMPO_BPM, ProjectSettings, TimebaseError,
};
pub use track::{Track, TrackId};
