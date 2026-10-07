use aaadaw_core::{DawAction, Project};

#[test]
fn midi_item_name_is_editable_and_undoable() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Instrument".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();

    assert_eq!(project.midi_items()[0].name(), "MIDI");
    project
        .apply(DawAction::SetMidiItemName {
            item_id,
            name: "Verse 1".to_owned(),
        })
        .unwrap();
    assert_eq!(project.midi_items()[0].name(), "Verse 1");

    project.undo().unwrap();
    assert_eq!(project.midi_items()[0].name(), "MIDI");
    project.redo().unwrap();
    assert_eq!(project.midi_items()[0].name(), "Verse 1");
}

#[test]
fn midi_item_names_reject_blank_and_overlong_values() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Instrument".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();

    for name in ["   ".to_owned(), "x".repeat(129)] {
        assert!(
            project
                .apply(DawAction::SetMidiItemName { item_id, name })
                .is_err()
        );
        assert_eq!(project.midi_items()[0].name(), "MIDI");
    }
}
