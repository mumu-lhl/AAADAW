//! Core domain and application logic for AAADAW.
//!
//! This crate is independent of GUI, platform audio drivers, and storage
//! adapters. [`Project`] is the public seam for applying validated actions and
//! observing project state.

mod action;
mod audio;
mod error;
mod fades;
mod markers;
mod master;
mod midi;
mod midi_file;
mod project;
mod project_notes;
mod regions;
mod routing;
mod snapshot;
mod timebase;
mod timecode;
mod track;

pub use action::{DawAction, MidiNoteQuantizeOperation};
pub use audio::{AudioItem, MAX_AUDIO_ITEM_TAKE_VOLUME, MAX_AUDIO_ITEM_VOLUME};
pub use error::ActionError;
pub use fades::{AudioFade, AudioItemFades, FadeCurve, FadeCurveParameters, FadeShape};
pub use markers::{MAX_PROJECT_MARKER_NUMBER, MarkerId, ProjectMarker};
pub use master::MasterMix;
pub use midi::{
    ItemId, MIDI_CHANNEL_COUNT, MIDI_ITEM_SUBTICKS_PER_TICK, MidiChannelPressureData,
    MidiControllerData, MidiItem, MidiItemPosition, MidiNote, MidiNoteData, MidiPitchBendData,
    MidiPolyAftertouchData, MidiProgramChangeData, NoteId,
};
pub use midi_file::{
    MidiExportItemScope, MidiExportOptions, MidiExportSelection, MidiExportTimeScope, MidiFileType,
    MidiMarkerType, encode_standard_midi, encode_standard_midi_with_conductor_track_name,
    encode_standard_midi_with_selection, midi_export_has_items_in_scope,
};
pub use project::{FxParameterChange, MAX_FX_PARAMETER_AUTOMATION_POINTS, Project};
pub use project_notes::ProjectNotes;
pub use regions::{MAX_PROJECT_REGION_NUMBER, MAX_PROJECT_REGION_TICKS, ProjectRegion, RegionId};
pub use snapshot::{
    AudioItemSnapshot, MeterPointSnapshot, MidiItemSnapshot, MidiNoteSnapshot,
    ProjectMarkerSnapshot, ProjectRegionSnapshot, ProjectSnapshot, SnapshotError,
    TempoPointSnapshot, TrackFxParameterAutomationLaneSnapshot,
    TrackFxParameterAutomationPointSnapshot, TrackFxParameterValueSnapshot, TrackFxPluginSnapshot,
    TrackInstrumentSnapshot, TrackSnapshot,
};
pub use timebase::{
    BpmBasis, DEFAULT_PPQ, DEFAULT_SAMPLE_RATE, DEFAULT_TEMPO_BPM, GridFraction, MusicalPosition,
    PanMode, ProjectSettings, TempoCurve, TimeSignature, TimebaseError,
};
pub use timecode::FrameRate;
pub use track::{
    FxParameterAutomationLane, FxParameterAutomationPoint, MAX_TRACK_FX_PARAMETER_AUTOMATION_LANES,
    RazorEditArea, Track, TrackFxPlugin, TrackId, TrackInstrument, VolumeAutomationPoint,
};

pub use routing::{AudioSend, AudioSendParameters, AudioSendSnapshot, AudioSendTap, SendId};
