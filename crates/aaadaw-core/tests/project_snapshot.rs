use aaadaw_core::{DawAction, MidiNoteData, Project, TimeSignature};

#[test]
fn project_snapshot_roundtrip_preserves_ids_and_timebase_state() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Keys".to_owned(),
        })
        .expect("creating a track should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3840,
        })
        .expect("inserting a MIDI item should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 64,
                tick: 0,
                duration: 960,
                velocity: 90,
            }],
        })
        .expect("adding a note should succeed");
    project
        .apply(DawAction::SetTempo {
            start_tick: 960,
            bpm: 90.0,
        })
        .expect("setting tempo should succeed");
    project
        .apply(DawAction::SetTimeSignature {
            start_tick: 3840,
            signature: TimeSignature::new(7, 8).expect("7/8 is valid"),
        })
        .expect("setting meter should succeed");

    let snapshot = project.snapshot();
    let restored = Project::from_snapshot(snapshot.clone()).expect("valid state should restore");

    assert_eq!(restored.snapshot(), snapshot);
}
