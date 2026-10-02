use crate::{ItemId, NoteId, TrackId};
use std::fmt;

/// An action could not be applied to the project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionError {
    /// The requested track insertion index is beyond the end of the track list.
    TrackIndexOutOfBounds {
        index: usize,
        track_count: usize,
    },
    /// The requested track does not exist in the project.
    TrackNotFound {
        track_id: TrackId,
    },
    /// A track volume must be a finite decibel value.
    InvalidVolumeDb,
    InvalidTrackInstrument,
    InvalidTrackFxPlugin,
    /// A tempo must be finite and greater than zero.
    InvalidTempoBpm,
    /// The resulting tempo map cannot be represented in sample positions.
    TempoMapOutOfRange,
    /// The requested tempo point does not exist.
    TempoPointNotFound {
        start_tick: u64,
    },
    /// The time signature is invalid or cannot be represented at this PPQ.
    InvalidTimeSignature,
    /// A time-signature change must occur on a bar line.
    MeterChangeNotOnBarBoundary,
    /// The resulting measure numbering exceeds its supported range.
    MeterMapOutOfRange,
    /// An audio item's media reference must not be blank.
    InvalidAudioMediaRef,
    /// An audio item must have a positive length.
    InvalidAudioItemLength,
    /// An audio item's project start and duration exceed the sample range.
    InvalidAudioItemPosition,
    /// The requested audio item does not exist in the project.
    AudioItemNotFound {
        item_id: ItemId,
    },
    /// A MIDI item must have a positive length.
    InvalidMidiItemLength,
    /// A MIDI item's start and length exceed the supported tick range.
    InvalidMidiItemPosition,
    /// The requested MIDI item does not exist in the project.
    MidiItemNotFound {
        item_id: ItemId,
    },
    /// The requested audio or MIDI item does not exist in the project.
    ItemNotFound {
        item_id: ItemId,
    },
    /// A MIDI note must have a valid pitch, velocity, and in-item duration.
    InvalidMidiNote,
    /// The requested MIDI note does not exist in the given item.
    MidiNoteNotFound {
        item_id: ItemId,
        note_id: NoteId,
    },
    /// Quantize strength must be finite and in `0.0..=1.0`.
    InvalidQuantizeStrength,
    /// The quantization grid cannot be represented at the project's PPQ.
    InvalidQuantizeGrid,
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
            Self::InvalidTrackInstrument => formatter
                .write_str("track instrument reference must have a plugin ID and bundle path"),
            Self::InvalidTrackFxPlugin => {
                formatter.write_str("track FX entries must have a plugin ID and bundle path")
            }
            Self::InvalidTempoBpm => {
                formatter.write_str("tempo must be finite and greater than zero")
            }
            Self::TempoMapOutOfRange => {
                formatter.write_str("tempo map exceeds the sample position range")
            }
            Self::TempoPointNotFound { start_tick } => {
                write!(formatter, "tempo point at tick {start_tick} does not exist")
            }
            Self::InvalidTimeSignature => formatter.write_str("time signature is invalid"),
            Self::MeterChangeNotOnBarBoundary => {
                formatter.write_str("meter changes must occur on a bar line")
            }
            Self::MeterMapOutOfRange => {
                formatter.write_str("meter map exceeds the measure number range")
            }
            Self::InvalidAudioMediaRef => {
                formatter.write_str("audio media reference must not be blank")
            }
            Self::InvalidAudioItemLength => {
                formatter.write_str("audio item length must be positive")
            }
            Self::InvalidAudioItemPosition => {
                formatter.write_str("audio item position exceeds the supported sample range")
            }
            Self::AudioItemNotFound { item_id } => {
                write!(formatter, "audio item {} does not exist", item_id.value())
            }
            Self::InvalidMidiItemLength => formatter.write_str("MIDI item length must be positive"),
            Self::InvalidMidiItemPosition => {
                formatter.write_str("MIDI item position exceeds the supported tick range")
            }
            Self::MidiItemNotFound { item_id } => {
                write!(formatter, "MIDI item {} does not exist", item_id.value())
            }
            Self::ItemNotFound { item_id } => {
                write!(formatter, "item {} does not exist", item_id.value())
            }
            Self::InvalidMidiNote => formatter.write_str("MIDI note data is invalid"),
            Self::MidiNoteNotFound { item_id, note_id } => write!(
                formatter,
                "MIDI note {} does not exist in item {}",
                note_id.value(),
                item_id.value()
            ),
            Self::InvalidQuantizeStrength => {
                formatter.write_str("quantize strength must be finite and in 0..=1")
            }
            Self::InvalidQuantizeGrid => {
                formatter.write_str("quantization grid is invalid at the project PPQ")
            }
            Self::ItemIdExhausted => formatter.write_str("item identifiers are exhausted"),
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
