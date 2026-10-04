//! Core domain and application logic for AAADAW.
//!
//! This crate is independent of GUI, platform audio drivers, and storage
//! adapters. [`Project`] is the public seam for applying validated actions and
//! observing project state.

mod action;
mod audio;
mod error;
mod midi;
mod project;
mod snapshot;
mod timebase;
mod track;

pub use action::DawAction;
pub use audio::AudioItem;
pub use error::ActionError;
pub use midi::{
    ItemId, MidiControllerData, MidiItem, MidiNote, MidiNoteData, MidiPitchBendData, NoteId,
};
pub use project::{FxParameterChange, Project};
pub use snapshot::{
    AudioItemSnapshot, MeterPointSnapshot, MidiItemSnapshot, MidiNoteSnapshot, ProjectSnapshot,
    SnapshotError, TempoPointSnapshot, TrackFxParameterValueSnapshot, TrackFxPluginSnapshot,
    TrackInstrumentSnapshot, TrackSnapshot,
};
pub use timebase::{
    DEFAULT_PPQ, DEFAULT_SAMPLE_RATE, DEFAULT_TEMPO_BPM, GridFraction, MusicalPosition,
    ProjectSettings, TempoCurve, TimeSignature, TimebaseError,
};
pub use track::{Track, TrackFxPlugin, TrackId, TrackInstrument, VolumeAutomationPoint};
