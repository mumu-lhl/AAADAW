use aaadaw_core::{ActionError, DawAction, MidiNoteData, Project};

fn project_with_midi_item() -> (Project, aaadaw_core::TrackId, aaadaw_core::ItemId) {
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
            start_tick: 960,
            length_ticks: 3_840,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 240,
                    duration: 1_200,
                    velocity: 100,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 1_440,
                    duration: 1_680,
                    velocity: 90,
                },
                MidiNoteData {
                    pitch: 67,
                    tick: 3_500,
                    duration: 100,
                    velocity: 80,
                },
            ],
        })
        .unwrap();
    (project, track_id, item_id)
}

#[test]
fn splitting_midi_item_preserves_notes_clip_positions_and_one_step_undo_redo() {
    let (mut project, track_id, item_id) = project_with_midi_item();
    let before = project.snapshot();
    let original_note_id = project.midi_items()[0].notes()[0].id();

    project
        .apply(DawAction::SplitMidiItem {
            item_id,
            split_ticks: vec![2_880, 1_920, 2_880],
        })
        .expect("two internal boundaries should split the MIDI item");

    let items = project.midi_items();
    assert_eq!(items.len(), 3);
    assert_eq!(
        items
            .iter()
            .map(|item| (item.start_tick(), item.length_ticks()))
            .collect::<Vec<_>>(),
        [(960, 960), (1_920, 960), (2_880, 1_920)]
    );
    assert_eq!(items[0].id(), item_id);
    assert_eq!(items[0].notes()[0].id(), original_note_id);
    assert_eq!(
        items[0].notes()[0].duration(),
        720,
        "the first note should stop at the first split"
    );
    assert_eq!(
        items[1]
            .notes()
            .iter()
            .map(|note| (note.pitch(), note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        [(60, 0, 480), (64, 480, 480)]
    );
    assert_eq!(
        items[2]
            .notes()
            .iter()
            .map(|note| (note.pitch(), note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        [(64, 0, 1_200), (67, 1_580, 100)]
    );
    assert!(items.iter().all(|item| item.track_id() == track_id));

    let after = project.snapshot();
    assert!(project.undo().expect("split should be undoable"));
    assert_eq!(project.snapshot(), before);
    assert!(project.redo().expect("split should be redoable"));
    assert_eq!(project.snapshot(), after);
}

#[test]
fn midi_split_without_an_internal_boundary_leaves_project_and_history_unchanged() {
    let (mut project, _, item_id) = project_with_midi_item();
    let before = project.snapshot();

    assert_eq!(
        project.apply(DawAction::SplitMidiItem {
            item_id,
            split_ticks: vec![0, 960, 4_800, 8_000],
        }),
        Err(ActionError::InvalidMidiItemPosition)
    );
    assert_eq!(project.snapshot(), before);
    assert!(
        project
            .undo()
            .expect("the note insertion remains the last edit")
    );
    assert_eq!(project.midi_items()[0].notes().len(), 0);
}
