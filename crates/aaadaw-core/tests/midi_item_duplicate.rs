use aaadaw_core::{ActionError, DawAction, MidiNoteData, Project};

#[test]
fn duplicate_midi_item_copies_content_with_fresh_ids_and_is_undoable() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 480,
            length_ticks: 1_920,
        })
        .unwrap();
    let source_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id: source_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 0,
                    duration: 480,
                    velocity: 100,
                },
                MidiNoteData {
                    pitch: 67,
                    tick: 1_200,
                    duration: 720,
                    velocity: 82,
                },
            ],
        })
        .unwrap();
    let source = project.midi_items()[0].clone();
    let source_note_ids = source
        .notes()
        .iter()
        .map(|note| note.id())
        .collect::<Vec<_>>();
    let before_duplicate = project.snapshot();

    project
        .apply(DawAction::DuplicateMidiItem { item_id: source_id })
        .expect("duplicate should be appended after its source");
    let items = project.midi_items();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0], source);
    let duplicate = &items[1];
    assert_ne!(duplicate.id(), source.id());
    assert_eq!(duplicate.track_id(), track_id);
    assert_eq!(duplicate.start_tick(), 2_400);
    assert_eq!(duplicate.length_ticks(), source.length_ticks());
    assert_eq!(
        duplicate
            .notes()
            .iter()
            .map(|note| (note.pitch(), note.tick(), note.duration(), note.velocity()))
            .collect::<Vec<_>>(),
        source
            .notes()
            .iter()
            .map(|note| (note.pitch(), note.tick(), note.duration(), note.velocity()))
            .collect::<Vec<_>>()
    );
    assert!(
        duplicate
            .notes()
            .iter()
            .all(|note| !source_note_ids.contains(&note.id()))
    );
    let after_duplicate = project.snapshot();

    assert!(project.undo().unwrap());
    assert_eq!(project.snapshot(), before_duplicate);
    assert!(project.redo().unwrap());
    assert_eq!(project.snapshot(), after_duplicate);
}

#[test]
fn duplicate_midi_item_errors_do_not_change_project_or_history() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: u64::MAX - 10,
            length_ticks: 10,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    let before = project.snapshot();

    assert_eq!(
        project.apply(DawAction::DuplicateMidiItem { item_id }),
        Err(ActionError::InvalidMidiItemPosition)
    );
    assert_eq!(project.snapshot(), before);

    assert!(project.undo().unwrap());
    assert!(project.midi_items().is_empty());
    assert_eq!(
        project.apply(DawAction::DuplicateMidiItem { item_id }),
        Err(ActionError::MidiItemNotFound { item_id })
    );
}
