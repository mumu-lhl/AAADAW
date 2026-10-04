use crate::{
    GridFraction, ItemId, MeterPointSnapshot, MidiControllerData, MidiNoteData, MidiPitchBendData,
    NoteId, TempoCurve, TimeSignature, TrackFxPlugin, TrackId, TrackInstrument,
    VolumeAutomationPoint,
};

/// A command that changes project state.
#[derive(Clone, Debug, PartialEq)]
pub enum DawAction {
    /// Create a track at `index` in the project's ordered track list.
    CreateTrack { index: usize, name: String },
    /// Create a subgroup bus at `index` in the project's ordered track list.
    CreateBusTrack { index: usize, name: String },
    /// Set or insert a tempo point at the given project tick.
    SetTempo { start_tick: u64, bpm: f64 },
    /// Remove a tempo point, except for the required initial point at tick zero.
    DeleteTempoPoint { start_tick: u64 },
    /// Set the interpolation curve from one tempo point to the next.
    SetTempoCurve { start_tick: u64, curve: TempoCurve },
    /// Set or insert a time signature at the given project tick.
    SetTimeSignature {
        start_tick: u64,
        signature: TimeSignature,
    },
    /// Replace the complete meter map as one validated, undoable operation.
    SetTimeSignatureMap { points: Vec<MeterPointSnapshot> },
    /// Set a track's volume in decibels.
    SetTrackVolume { track_id: TrackId, volume_db: f32 },
    /// Replace a track's read-mode sample-clock volume automation lane.
    SetTrackVolumeAutomation {
        track_id: TrackId,
        points: Vec<VolumeAutomationPoint>,
    },
    /// Route a track to a bus, or directly to Master when `output_track` is `None`.
    SetTrackOutput {
        track_id: TrackId,
        output_track: Option<TrackId>,
    },
    /// Set a track's pan position in the inclusive range `-1.0..=1.0`.
    SetTrackPan { track_id: TrackId, pan: f32 },
    /// Mute or unmute a track.
    SetTrackMute { track_id: TrackId, muted: bool },
    /// Solo or unsolo a track.
    SetTrackSolo { track_id: TrackId, solo: bool },
    /// Arm or disarm a track to receive the next live audio take.
    SetTrackRecordArm { track_id: TrackId, armed: bool },
    /// Assign or clear a track's CLAP instrument reference.
    SetTrackInstrument {
        track_id: TrackId,
        instrument: Option<TrackInstrument>,
    },
    /// Replace a track's ordered CLAP FX chain as one undoable operation.
    SetTrackFxChain {
        track_id: TrackId,
        plugins: Vec<TrackFxPlugin>,
    },
    /// Record a CLAP FX parameter gesture for undo/redo without duplicating plugin-owned state.
    SetTrackFxParameter {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        before: f64,
        after: f64,
        before_state: Option<Vec<u8>>,
        after_state: Option<Vec<u8>>,
    },
    /// Rename a track.
    SetTrackName { track_id: TrackId, name: String },
    /// Move a track to a final position in the ordered track list.
    MoveTrack { track_id: TrackId, index: usize },
    /// Insert an audio item referencing an opaque media source.
    InsertAudioItem {
        track_id: TrackId,
        media_ref: String,
        start_sample: u64,
        source_offset_samples: u64,
        length_samples: u64,
    },
    /// Move, trim, or extend an audio item.
    EditAudioItem {
        item_id: ItemId,
        media_ref: String,
        start_sample: u64,
        source_offset_samples: u64,
        length_samples: u64,
    },
    /// Move an audio or MIDI item to another track without changing its content or position.
    MoveItemToTrack { item_id: ItemId, track_id: TrackId },
    /// Delete an audio item from the project.
    DeleteAudioItem { item_id: ItemId },
    /// Insert an empty MIDI item on a track.
    InsertMidiItem {
        track_id: TrackId,
        start_tick: u64,
        length_ticks: u64,
    },
    /// Move a MIDI item and/or change its length without discarding notes.
    EditMidiItem {
        item_id: ItemId,
        start_tick: u64,
        length_ticks: u64,
    },
    /// Duplicate a MIDI item immediately after its source, assigning fresh item and note IDs.
    DuplicateMidiItem { item_id: ItemId },
    /// Split a MIDI item at one or more absolute project ticks while preserving its notes.
    /// Notes crossing a split are continued at the start of the following segment.
    SplitMidiItem {
        item_id: ItemId,
        split_ticks: Vec<u64>,
    },
    /// Delete a MIDI item and retain it for undo.
    DeleteMidiItem { item_id: ItemId },
    /// Add notes to a MIDI item. Note ticks are relative to the item start.
    AddMidiNotes {
        item_id: ItemId,
        notes: Vec<MidiNoteData>,
    },
    /// Edit a MIDI note's pitch, relative tick, duration, or velocity.
    EditMidiNote {
        item_id: ItemId,
        note_id: NoteId,
        data: MidiNoteData,
    },
    /// Delete notes from a MIDI item by identifier.
    DeleteMidiNotes {
        item_id: ItemId,
        note_ids: Vec<NoteId>,
    },
    /// Replace the control-change events belonging to a MIDI item.
    SetMidiControllers {
        item_id: ItemId,
        controllers: Vec<MidiControllerData>,
    },
    /// Replace the 14-bit pitch-bend events belonging to a MIDI item.
    SetMidiPitchBends {
        item_id: ItemId,
        pitch_bends: Vec<MidiPitchBendData>,
    },
    /// Move note starts toward the nearest musical grid position.
    QuantizeItem {
        item_id: ItemId,
        grid: GridFraction,
        strength: f32,
    },
    /// Delete a track from the project.
    DeleteTrack { track_id: TrackId },
    /// Apply several actions as one atomic, undoable transaction.
    BatchTransaction { tx_id: u64, actions: Vec<DawAction> },
}
