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
mod routing;
mod snapshot;
mod timebase;
mod track;

pub use action::DawAction;
pub use audio::AudioItem;
pub use error::ActionError;
pub use midi::{
    ItemId, MidiControllerData, MidiItem, MidiNote, MidiNoteData, MidiPitchBendData, NoteId,
};
pub use project::{FxParameterChange, MAX_FX_PARAMETER_AUTOMATION_POINTS, Project};
pub use snapshot::{
    AudioItemSnapshot, MeterPointSnapshot, MidiItemSnapshot, MidiNoteSnapshot, ProjectSnapshot,
    SnapshotError, TempoPointSnapshot, TrackFxParameterAutomationLaneSnapshot,
    TrackFxParameterAutomationPointSnapshot, TrackFxParameterValueSnapshot, TrackFxPluginSnapshot,
    TrackInstrumentSnapshot, TrackSnapshot,
};
pub use timebase::{
    DEFAULT_PPQ, DEFAULT_SAMPLE_RATE, DEFAULT_TEMPO_BPM, GridFraction, MusicalPosition, PanMode,
    ProjectSettings, TempoCurve, TimeSignature, TimebaseError,
};
pub use track::{
    FxParameterAutomationLane, FxParameterAutomationPoint, MAX_TRACK_FX_PARAMETER_AUTOMATION_LANES,
    Track, TrackFxPlugin, TrackId, TrackInstrument, VolumeAutomationPoint,
};

pub use routing::{AudioSend, AudioSendParameters, AudioSendSnapshot, SendId};
