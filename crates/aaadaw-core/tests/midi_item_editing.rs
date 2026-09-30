use aaadaw_core::{ActionError, DawAction, MidiNoteData, Project};

#[test]
fn midi_items_can_be_moved_and_resized_without_losing_notes() {
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
            start_tick: 960,
            length_ticks: 1920,
        })
        .expect("inserting a MIDI item should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 64,
                tick: 480,
                duration: 960,
                velocity: 96,
            }],
        })
        .expect("adding a note should succeed");

    project
        .apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 1920,
            length_ticks: 1800,
        })
        .expect("moving and resizing the item should succeed");
    let item = &project.midi_items()[0];
    assert_eq!(item.start_tick(), 1920);
    assert_eq!(item.length_ticks(), 1800);
    assert_eq!(item.notes()[0].tick(), 480);

    assert!(project.undo().expect("item edit undo should succeed"));
    assert_eq!(project.midi_items()[0].start_tick(), 960);
    assert_eq!(project.midi_items()[0].length_ticks(), 1920);
    assert!(project.redo().expect("item edit redo should succeed"));
    assert_eq!(project.midi_items()[0].start_tick(), 1920);
    assert_eq!(project.midi_items()[0].length_ticks(), 1800);

    let too_short = project.apply(DawAction::EditMidiItem {
        item_id,
        start_tick: 0,
        length_ticks: 1000,
    });
    assert_eq!(too_short, Err(ActionError::InvalidMidiNote));
    assert_eq!(project.midi_items()[0].start_tick(), 1920);
    assert_eq!(project.midi_items()[0].length_ticks(), 1800);

    let overflowing_position = project.apply(DawAction::EditMidiItem {
        item_id,
        start_tick: u64::MAX,
        length_ticks: 1,
    });
    assert_eq!(
        overflowing_position,
        Err(ActionError::InvalidMidiItemPosition)
    );
    assert_eq!(project.midi_items()[0].start_tick(), 1920);

    let overflowing_insert = project.apply(DawAction::InsertMidiItem {
        track_id,
        start_tick: u64::MAX,
        length_ticks: 1,
    });
    assert_eq!(
        overflowing_insert,
        Err(ActionError::InvalidMidiItemPosition)
    );
    assert_eq!(project.midi_items().len(), 1);
}
