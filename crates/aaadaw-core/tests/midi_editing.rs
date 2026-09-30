use aaadaw_core::{DawAction, MidiNoteData, Project};

#[test]
fn midi_notes_can_be_added_queried_and_undone() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
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
                pitch: 60,
                tick: 0,
                duration: 960,
                velocity: 100,
            }],
        })
        .expect("adding an in-range MIDI note should succeed");

    let note = &project.midi_items()[0].notes()[0];
    assert_eq!(note.pitch(), 60);
    assert_eq!(note.duration(), 960);
    assert!(project.undo().expect("undo should succeed"));
    assert!(project.midi_items()[0].notes().is_empty());
    assert!(project.redo().expect("redo should succeed"));
    assert_eq!(project.midi_items()[0].notes()[0].pitch(), 60);
}
