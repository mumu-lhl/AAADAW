use aaadaw_core::{
    ActionError, DawAction, MidiControllerData, MidiNoteData, MidiPitchBendData, Project,
};

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
fn splitting_a_trimmed_clip_keeps_hidden_events_in_the_final_segment() {
    let (mut project, _, item_id) = project_with_midi_item();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 72,
                tick: 3_600,
                duration: 240,
                velocity: 90,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                controller: 1,
                tick: 3_700,
                value: 80,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                tick: 3_750,
                value: 12_000,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 960,
            length_ticks: 960,
        })
        .expect("clip should shorten without deleting its stored tail");

    project
        .apply(DawAction::SplitMidiItem {
            item_id,
            split_ticks: vec![1_440],
        })
        .expect("the shortened clip should split at its visible midpoint");

    let final_segment = &project.midi_items()[1];
    assert_eq!(final_segment.start_tick(), 1_440);
    assert_eq!(final_segment.length_ticks(), 480);
    let hidden_note = final_segment
        .notes()
        .iter()
        .find(|note| note.pitch() == 72)
        .expect("the note beyond the clip end should stay stored");
    assert_eq!((hidden_note.tick(), hidden_note.duration()), (3_120, 240));
    assert_eq!(final_segment.controllers()[0].tick, 3_220);
    assert_eq!(final_segment.pitch_bends()[0].tick, 3_270);
}

#[test]
fn splitting_a_left_trimmed_clip_keeps_source_offsets_and_hidden_leading_events() {
    let (mut project, _, item_id) = project_with_midi_item();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 120,
                    duration: 240,
                    velocity: 90,
                },
                MidiNoteData {
                    pitch: 72,
                    tick: 2_100,
                    duration: 480,
                    velocity: 100,
                },
            ],
        })
        .unwrap();
    project
        .apply(DawAction::TrimMidiItemStart {
            item_id,
            start_tick: 1_920,
            length_ticks: 2_880,
            source_offset_ticks: 960,
        })
        .unwrap();
    project
        .apply(DawAction::SplitMidiItem {
            item_id,
            split_ticks: vec![2_880],
        })
        .unwrap();

    let first = &project.midi_items()[0];
    assert_eq!(first.start_tick(), 1_920);
    assert_eq!(first.length_ticks(), 960);
    assert_eq!(first.source_offset_ticks(), 960);
    assert!(first.notes().iter().any(|note| note.tick() == 120));

    let second = &project.midi_items()[1];
    assert_eq!(second.start_tick(), 2_880);
    assert_eq!(second.source_offset_ticks(), 0);
    let continued = second
        .notes()
        .iter()
        .find(|note| note.pitch() == 72)
        .unwrap();
    assert_eq!((continued.tick(), continued.duration()), (180, 480));
    assert_eq!(
        second.project_tick_at_content_tick(continued.tick()),
        Some(3_060)
    );
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
