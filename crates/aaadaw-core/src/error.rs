use crate::{ItemId, TrackId};
use std::fmt;

/// An action could not be applied to the project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionError {
    /// The requested track insertion index is beyond the end of the track list.
    TrackIndexOutOfBounds { index: usize, track_count: usize },
    /// The requested track does not exist in the project.
    TrackNotFound { track_id: TrackId },
    /// A track volume must be a finite decibel value.
    InvalidVolumeDb,
    /// A MIDI item must have a positive length.
    InvalidMidiItemLength,
    /// The requested MIDI item does not exist in the project.
    MidiItemNotFound { item_id: ItemId },
    /// A MIDI note must have a valid pitch, velocity, and in-item duration.
    InvalidMidiNote,
    /// The project has exhausted its available MIDI item identifiers.
    ItemIdExhausted,
    /// The project has exhausted its available MIDI note identifiers.
    NoteIdExhausted,
    /// A track pan must be finite and within `-1.0..=1.0`.
    InvalidPan,
    /// The project has exhausted its available track identifiers.
    TrackIdExhausted,
    /// The internal event history could not be applied consistently.
    HistoryInvariantViolation,
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TrackIndexOutOfBounds { index, track_count } => write!(
                formatter,
                "track insertion index {index} exceeds track count {track_count}"
            ),
            Self::TrackNotFound { track_id } => {
                write!(formatter, "track {} does not exist", track_id.value())
            }
            Self::InvalidVolumeDb => formatter.write_str("track volume must be finite"),
            Self::InvalidMidiItemLength => formatter.write_str("MIDI item length must be positive"),
            Self::MidiItemNotFound { item_id } => {
                write!(formatter, "MIDI item {} does not exist", item_id.value())
            }
            Self::InvalidMidiNote => formatter.write_str("MIDI note data is invalid"),
            Self::ItemIdExhausted => formatter.write_str("MIDI item identifiers are exhausted"),
            Self::NoteIdExhausted => formatter.write_str("MIDI note identifiers are exhausted"),
            Self::InvalidPan => formatter.write_str("track pan must be finite and in -1..=1"),
            Self::TrackIdExhausted => formatter.write_str("track identifiers are exhausted"),
            Self::HistoryInvariantViolation => {
                formatter.write_str("project event history is inconsistent with its state")
            }
        }
    }
}

impl std::error::Error for ActionError {}
