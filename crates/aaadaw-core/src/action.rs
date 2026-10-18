use crate::{
    GridFraction, ItemId, MarkerId, MeterPointSnapshot, MidiChannelPressureData,
    MidiControllerData, MidiImportedTrack, MidiItemPosition, MidiNoteData, MidiPitchBendData,
    MidiPolyAftertouchData, MidiProgramChangeData, NoteId, RegionId, TempoCurve, TimeSignature,
    TrackFxPlugin, TrackId, TrackInstrument, VolumeAutomationPoint,
};

/// Which note timing values a MIDI quantize action moves to the current grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidiNoteQuantizeOperation {
    Position,
    PositionAndNoteEnd,
    PositionAndNoteLength,
    NoteEndOnly,
    NoteLengthOnly,
}

/// A command that changes project state.
#[derive(Clone, Debug, PartialEq)]
pub enum DawAction {
    /// Replace project notes without adding to or clearing media undo/redo history.
    SetProjectNotes {
        notes: crate::ProjectNotes,
    },
    /// Change video/timecode frame rate without altering audio placement.
    SetFrameRate {
        rate: crate::FrameRate,
    },
    /// Change the project's sample clock while preserving content in seconds.
    SetProjectSampleRate {
        sample_rate: u32,
    },
    /// Align tempo-map change anchors to whole project samples.
    SetForceTempoChangesToWholeSamples {
        enabled: bool,
    },
    /// Change the note value counted as one BPM beat.
    SetProjectBpmBasis {
        basis: crate::BpmBasis,
    },
    /// Choose whether ruler label spacing is anchored at the project's start measure.
    SetBaseRulerLabelSpacingOnMeasure {
        enabled: bool,
    },
    /// Change the signed display-time offset at the start of the project.
    SetProjectStartTimeOffsetSeconds {
        seconds: f64,
    },
    /// Allow the project start time to use a fractional video-frame offset.
    SetProjectStartTimeFractionalFrameOffsetAllowed {
        allowed: bool,
    },
    /// Change the signed measure number displayed at project tick zero.
    SetProjectStartMeasure {
        measure: i32,
    },
    /// Change the Master output gain and stereo balance as one undoable edit.
    SetMasterMix {
        mix: crate::MasterMix,
    },
    /// Insert a named marker at an absolute project tick.
    InsertProjectMarker {
        position_ticks: u64,
        name: String,
    },
    /// Insert a project marker with an explicit REAPER marker number.
    InsertNumberedProjectMarker {
        number: u32,
        position_ticks: u64,
        name: String,
    },
    /// Rename or reposition a project marker while preserving its identity.
    UpdateProjectMarker {
        marker_id: MarkerId,
        position_ticks: u64,
        name: String,
    },
    /// Update marker properties while preserving its stable internal identity.
    UpdateProjectMarkerProperties {
        marker_id: MarkerId,
        number: u32,
        position_ticks: u64,
        name: String,
    },
    /// Change whether one project marker is shown in the ruler.
    SetProjectMarkerVisibility {
        marker_id: MarkerId,
        visible: bool,
    },
    /// Change visibility of the project marker lane in the ruler.
    SetProjectMarkersVisibleOnRuler {
        visible: bool,
    },
    /// Change whether one project region is shown in the ruler.
    SetProjectRegionVisibility {
        region_id: RegionId,
        visible: bool,
    },
    /// Replace the Master mix and track assignments for one region render matrix row.
    SetProjectRegionRenderMatrix {
        region_id: RegionId,
        render_master_mix: bool,
        render_track_ids: Vec<TrackId>,
    },
    /// Change visibility of the project region lane in the ruler.
    SetProjectRegionsVisibleOnRuler {
        visible: bool,
    },
    /// Remove a project marker by stable identity.
    DeleteProjectMarker {
        marker_id: MarkerId,
    },
    /// Insert a region, assigning the lowest available positive region number.
    InsertProjectRegion {
        start_ticks: u64,
        end_ticks: u64,
        name: String,
    },
    /// Update the name, visible number, or boundaries of a project region.
    UpdateProjectRegion {
        region_id: RegionId,
        number: u32,
        start_ticks: u64,
        end_ticks: u64,
        name: String,
    },
    /// Remove a project region by stable identity.
    DeleteProjectRegion {
        region_id: RegionId,
    },
    /// Create a track at `index` in the project's ordered track list.
    CreateTrack {
        index: usize,
        name: String,
    },
    /// Create a subgroup bus at `index` in the project's ordered track list.
    CreateBusTrack {
        index: usize,
        name: String,
    },
    /// Set or insert a tempo point at the given project tick.
    SetTempo {
        start_tick: u64,
        bpm: f64,
    },
    /// Remove a tempo point, except for the required initial point at tick zero.
    DeleteTempoPoint {
        start_tick: u64,
    },
    /// Set the interpolation curve from one tempo point to the next.
    SetTempoCurve {
        start_tick: u64,
        curve: TempoCurve,
    },
    /// Set or insert a time signature at the given project tick.
    SetTimeSignature {
        start_tick: u64,
        signature: TimeSignature,
    },
    /// Replace the complete meter map as one validated, undoable operation.
    SetTimeSignatureMap {
        points: Vec<MeterPointSnapshot>,
    },
    /// Set a track's volume in decibels.
    SetTrackVolume {
        track_id: TrackId,
        volume_db: f32,
    },
    /// Replace a track's read-mode sample-clock volume automation lane.
    SetTrackVolumeAutomation {
        track_id: TrackId,
        points: Vec<VolumeAutomationPoint>,
    },
    /// Replace the media razor edit areas on multiple tracks as one undoable edit.
    SetTrackRazorEditAreas {
        track_areas: Vec<(TrackId, Vec<crate::RazorEditArea>)>,
    },
    /// Enable or disable the main output without deleting any sends.
    SetTrackMainSend {
        track_id: TrackId,
        enabled: bool,
    },
    /// Add an independent post-fader audio connection.
    CreateAudioSend {
        track_id: TrackId,
        destination: TrackId,
        parameters: crate::AudioSendParameters,
    },
    /// Change one connection while retaining its stable identity and position.
    UpdateAudioSend {
        track_id: TrackId,
        send_id: crate::SendId,
        destination: TrackId,
        parameters: crate::AudioSendParameters,
    },
    /// Remove one sender-owned connection.
    DeleteAudioSend {
        track_id: TrackId,
        send_id: crate::SendId,
    },
    /// Route a track to another track, or directly to Master when `output_track` is `None`.
    SetTrackOutput {
        track_id: TrackId,
        output_track: Option<TrackId>,
    },
    /// Set a track's pan position in the inclusive range `-1.0..=1.0`.
    SetTrackFolder {
        track_id: TrackId,
        enabled: bool,
    },
    SetTrackParent {
        track_id: TrackId,
        parent: Option<TrackId>,
    },
    SetTrackPan {
        track_id: TrackId,
        pan: f32,
    },
    /// Set post-fader track polarity.
    SetTrackPhase {
        track_id: TrackId,
        phase_inverted: bool,
    },
    /// Mute or unmute a track.
    SetTrackMute {
        track_id: TrackId,
        muted: bool,
    },
    /// Solo or unsolo a track.
    SetTrackSolo {
        track_id: TrackId,
        solo: bool,
    },
    /// Arm or disarm a track to receive the next live audio take.
    SetTrackRecordArm {
        track_id: TrackId,
        armed: bool,
    },
    /// Assign or clear a track's CLAP instrument reference.
    SetTrackInstrument {
        track_id: TrackId,
        instrument: Option<TrackInstrument>,
    },
    /// Replace a track's live source chain with a project-managed audio render.
    FreezeTrack {
        track_id: TrackId,
        media_ref: String,
        start_sample: u64,
        length_samples: u64,
    },
    /// Resume a track's retained MIDI, instrument, and effect source chain.
    UnfreezeTrack {
        track_id: TrackId,
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
    /// Replace the sample-clock automation points for one CLAP FX parameter.
    SetTrackFxParameterAutomation {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        points: Vec<crate::FxParameterAutomationPoint>,
    },
    /// Rename a track.
    SetTrackName {
        track_id: TrackId,
        name: String,
    },
    /// Move a track to a final position in the ordered track list.
    MoveTrack {
        track_id: TrackId,
        index: usize,
    },
    /// Insert an audio item referencing an opaque media source.
    InsertAudioItem {
        track_id: TrackId,
        media_ref: String,
        start_sample: u64,
        source_offset_samples: i64,
        length_samples: u64,
    },
    /// Insert an audio item that repeats a bounded source section during playback.
    InsertLoopedAudioItem {
        track_id: TrackId,
        media_ref: String,
        start_sample: u64,
        source_offset_samples: u64,
        length_samples: u64,
        muted: bool,
        phase_inverted: bool,
        source_section_start_samples: u64,
        source_section_length_samples: u64,
    },
    /// Insert a looped source section while preserving item volume and active Take mix.
    InsertLoopedAudioItemWithMix {
        track_id: TrackId,
        media_ref: String,
        name: String,
        start_sample: u64,
        source_offset_samples: u64,
        length_samples: u64,
        muted: bool,
        phase_inverted: bool,
        source_section_start_samples: u64,
        source_section_length_samples: u64,
        volume: f32,
        take_volume: f32,
        take_pan: f32,
        take_playback_rate: f64,
    },
    /// Enable or disable looping for an audio item's current source section.
    SetAudioItemLoopSource {
        item_id: ItemId,
        enabled: bool,
    },
    /// Mute or unmute an audio item without changing its stored gain.
    SetAudioItemMute {
        item_id: ItemId,
        muted: bool,
    },
    /// Invert or restore the active audio take's polarity.
    SetAudioItemPhase {
        item_id: ItemId,
        inverted: bool,
    },
    /// Set both manual Item fades as one undoable edit.
    SetAudioItemFades {
        item_id: ItemId,
        fades: crate::AudioItemFades,
    },
    /// Set an audio item's linear playback volume (0 is silent, 1 is unity).
    SetAudioItemVolume {
        item_id: ItemId,
        volume: f32,
    },
    /// Set the active audio take's linear volume and stereo balance together.
    SetAudioItemTakeMix {
        item_id: ItemId,
        volume: f32,
        pan: f32,
    },
    /// Set the active audio take's playback rate; 1.0 is normal speed.
    SetAudioItemTakePlaybackRate {
        item_id: ItemId,
        rate: f64,
    },
    /// Duplicate the complete audio Item at a new track and position.
    DuplicateAudioItemAt {
        item_id: ItemId,
        track_id: TrackId,
        start_sample: u64,
    },
    /// Move, trim, or extend an audio item.
    EditAudioItem {
        item_id: ItemId,
        media_ref: String,
        start_sample: u64,
        source_offset_samples: i64,
        length_samples: u64,
    },
    /// Move an audio or MIDI item to another track without changing its content or position.
    MoveItemToTrack {
        item_id: ItemId,
        track_id: TrackId,
    },
    /// Delete an audio item from the project.
    DeleteAudioItem {
        item_id: ItemId,
    },
    /// Set the audio item's active take name. An empty name restores the source label.
    SetAudioItemName {
        item_id: ItemId,
        name: String,
    },
    /// Insert an empty MIDI item on a track.
    InsertMidiItem {
        track_id: TrackId,
        start_tick: u64,
        length_ticks: u64,
    },
    /// Import one decoded SMF track as a new project track and editable MIDI item.
    ImportMidiTrack {
        start_tick: u64,
        track: MidiImportedTrack,
    },
    /// Set a MIDI item's user-visible name.
    SetMidiItemName {
        item_id: ItemId,
        name: String,
    },
    /// Move a MIDI item and/or change its length without discarding notes.
    EditMidiItem {
        item_id: ItemId,
        start_tick: u64,
        length_ticks: u64,
    },
    /// Set a MIDI item's exact PPQ start position, including fractional PPQ.
    SetMidiItemPosition {
        item_id: ItemId,
        position: MidiItemPosition,
    },
    /// Edit a MIDI item's timeline and source offsets together without changing its events.
    EditMidiItemPlacement {
        item_id: ItemId,
        start_tick: u64,
        length_ticks: u64,
        source_offset_ticks: u64,
    },
    /// Align a non-loop MIDI source to an absolute time while preserving its events.
    /// When the requested source start falls before the item, store it as a source offset;
    /// when it falls after the item, rebase the events and source length forward.
    /// The event shift is signed so source-boundary nudge operations can rebase either way.
    SetMidiItemContents {
        item_id: ItemId,
        source_offset_ticks: u64,
        source_loop_length_ticks: u64,
        content_shift_ticks: i64,
    },
    /// Mute or unmute a MIDI item without changing its notes or timing.
    SetMidiItemMute {
        item_id: ItemId,
        muted: bool,
    },
    /// Enable or disable looping for a MIDI item's source without changing its stored length.
    SetMidiItemLoopSource {
        item_id: ItemId,
        enabled: bool,
    },
    /// Trim the visible start of a MIDI item without moving its stored events.
    TrimMidiItemStart {
        item_id: ItemId,
        start_tick: u64,
        length_ticks: u64,
        source_offset_ticks: u64,
    },
    /// Duplicate a MIDI item immediately after its source, assigning fresh item and note IDs.
    DuplicateMidiItem {
        item_id: ItemId,
    },
    /// Duplicate a MIDI item at a specific project tick, assigning fresh item and note IDs.
    DuplicateMidiItemAt {
        item_id: ItemId,
        start_tick: u64,
    },
    /// Duplicate a MIDI item onto a track at a project tick, assigning fresh item and note IDs.
    DuplicateMidiItemToTrack {
        item_id: ItemId,
        track_id: TrackId,
        start_tick: u64,
    },
    /// Split a MIDI item at one or more absolute project ticks while preserving its notes.
    /// Notes crossing a split are continued at the start of the following segment.
    SplitMidiItem {
        item_id: ItemId,
        split_ticks: Vec<u64>,
    },
    /// Delete a MIDI item and retain it for undo.
    DeleteMidiItem {
        item_id: ItemId,
    },
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
    /// Replace the Program Change events belonging to a MIDI item.
    SetMidiProgramChanges {
        item_id: ItemId,
        program_changes: Vec<MidiProgramChangeData>,
    },
    /// Replace Polyphonic Aftertouch events belonging to a MIDI item.
    SetMidiPolyAftertouches {
        item_id: ItemId,
        poly_aftertouches: Vec<MidiPolyAftertouchData>,
    },
    /// Replace Channel Pressure events belonging to a MIDI item.
    SetMidiChannelPressures {
        item_id: ItemId,
        channel_pressures: Vec<MidiChannelPressureData>,
    },
    /// Move note starts toward the nearest musical grid position.
    QuantizeItem {
        item_id: ItemId,
        grid: GridFraction,
        strength: f32,
    },
    /// Move the starts of selected notes toward the nearest musical grid position.
    QuantizeMidiNotes {
        item_id: ItemId,
        note_ids: Vec<crate::NoteId>,
        grid: GridFraction,
        strength: f32,
    },
    /// Move selected note starts and ends to the nearest musical grid position.
    QuantizeMidiNotesAndEnds {
        item_id: ItemId,
        note_ids: Vec<crate::NoteId>,
        grid: GridFraction,
        strength: f32,
    },
    /// Quantize selected MIDI note positions, ends, or lengths using the dialog operation.
    QuantizeMidiNotesWithOperation {
        item_id: ItemId,
        note_ids: Vec<crate::NoteId>,
        grid: GridFraction,
        strength: f32,
        operation: MidiNoteQuantizeOperation,
        /// Swing amount of the MIDI editor grid, clamped to -100..=100 percent.
        swing_amount_percent: i16,
        /// Shorten notes that overlap the next note with the same channel and pitch.
        fix_overlaps: bool,
    },
    /// Quantize every MIDI note, controller, and pitch-bend event in an item as one edit.
    QuantizeMidiEventsWithOperation {
        item_id: ItemId,
        grid: GridFraction,
        strength: f32,
        operation: MidiNoteQuantizeOperation,
        /// Swing amount of the MIDI editor grid, clamped to -100..=100 percent.
        swing_amount_percent: i16,
        /// Shorten notes that overlap the next note with the same channel and pitch.
        fix_overlaps: bool,
    },
    /// Randomize MIDI note positions and velocities as one undoable edit.
    HumanizeMidiNotes {
        item_id: ItemId,
        note_ids: Vec<crate::NoteId>,
        grid: GridFraction,
        timing_amount_percent: u8,
        velocity_amount_percent: u8,
        timing_bias_percent: i16,
        seed: u64,
    },
    /// Quantize selected MIDI notes, controller events, and pitch bends as one edit.
    /// Controller and pitch-bend selections identify the source events by their current data.
    QuantizeSelectedMidiEventsWithOperation {
        item_id: ItemId,
        note_ids: Vec<crate::NoteId>,
        controllers: Vec<MidiControllerData>,
        pitch_bends: Vec<crate::MidiPitchBendData>,
        grid: GridFraction,
        strength: f32,
        operation: MidiNoteQuantizeOperation,
        /// Swing amount of the MIDI editor grid, clamped to -100..=100 percent.
        swing_amount_percent: i16,
        /// Shorten notes that overlap the next note with the same channel and pitch.
        fix_overlaps: bool,
    },
    /// Delete a track from the project.
    DeleteTrack {
        track_id: TrackId,
    },
    /// Apply several actions atomically, grouping media edits in one history entry.
    /// Project documentation remains independent of media undo/redo.
    BatchTransaction {
        tx_id: u64,
        actions: Vec<DawAction>,
    },
}
