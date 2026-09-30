use aaadaw_core::{DawAction, GridFraction, MidiNoteData, Project};

#[test]
fn midi_quantize_moves_note_starts_to_the_grid_and_is_undoable() {
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
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 110,
                    duration: 120,
                    velocity: 100,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 510,
                    duration: 120,
                    velocity: 90,
                },
            ],
        })
        .expect("adding notes should succeed");

    project
        .apply(DawAction::QuantizeItem {
            item_id,
            grid: GridFraction::new(1, 16).expect("1/16 is a valid grid"),
            strength: 1.0,
        })
        .expect("quantizing valid notes should succeed");

    let notes = project.midi_items()[0].notes();
    assert_eq!(notes[0].tick(), 0);
    assert_eq!(notes[1].tick(), 480);
    assert!(project.undo().expect("quantize should be undoable"));
    let notes = project.midi_items()[0].notes();
    assert_eq!(notes[0].tick(), 110);
    assert_eq!(notes[1].tick(), 510);
}
