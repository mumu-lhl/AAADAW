use aaadaw_core::{ActionError, DawAction, GridFraction, MidiNoteData, Project};

fn project_with_midi_notes() -> (Project, aaadaw_core::ItemId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .expect("test track should be created");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3_840,
        })
        .expect("test MIDI item should be inserted");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 170,
                    duration: 90,
                    velocity: 70,
                },
                MidiNoteData {
                    channel: 2,
                    pitch: 64,
                    tick: 350,
                    duration: 110,
                    velocity: 80,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 67,
                    tick: 550,
                    duration: 150,
                    velocity: 90,
                },
            ],
        })
        .expect("test MIDI notes should be inserted");
    (project, item_id)
}

#[test]
fn humanize_moves_only_selected_notes_within_the_grid_and_is_replayable() {
    let (mut project, item_id) = project_with_midi_notes();
    let original = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| (note.tick(), note.duration(), note.velocity()))
        .collect::<Vec<_>>();
    let note_ids = project.midi_items()[0].notes()[..2]
        .iter()
        .map(|note| note.id())
        .collect::<Vec<_>>();
    let action = DawAction::HumanizeMidiNotes {
        item_id,
        note_ids,
        grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
        timing_amount_percent: 100,
        velocity_amount_percent: 100,
        timing_bias_percent: 0,
        seed: 0x43b0_1a52_74c9_203f,
    };

    project
        .apply(action.clone())
        .expect("humanizing valid notes should succeed");
    let after = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| (note.tick(), note.duration(), note.velocity()))
        .collect::<Vec<_>>();
    assert_ne!(after, original);
    for index in 0..2 {
        assert!(original[index].0.abs_diff(after[index].0) <= 60);
        assert_eq!(original[index].1, after[index].1);
        assert!(original[index].2.abs_diff(after[index].2) <= original[index].2 / 2);
    }
    assert_eq!(after[2], original[2]);

    assert!(project.undo().expect("humanize should undo"));
    assert_eq!(
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration(), note.velocity()))
            .collect::<Vec<_>>(),
        original
    );
    project
        .apply(action)
        .expect("the same seed should replay the same edit");
    assert_eq!(
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration(), note.velocity()))
            .collect::<Vec<_>>(),
        after
    );
    assert!(project.undo().expect("humanize should undo again"));
    assert!(project.redo().expect("humanize should redo"));
    assert_eq!(
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration(), note.velocity()))
            .collect::<Vec<_>>(),
        after
    );
}

#[test]
fn humanize_rejects_values_outside_native_dialog_ranges_without_editing() {
    let (mut project, item_id) = project_with_midi_notes();
    let note_id = project.midi_items()[0].notes()[0].id();
    let before = project.midi_items()[0].notes()[0].clone();
    let history_cursor = project.history_cursor();
    let result = project.apply(DawAction::HumanizeMidiNotes {
        item_id,
        note_ids: vec![note_id],
        grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
        timing_amount_percent: 101,
        velocity_amount_percent: 0,
        timing_bias_percent: 0,
        seed: 1,
    });

    assert_eq!(result, Err(ActionError::InvalidMidiHumanizeSettings));
    assert_eq!(project.midi_items()[0].notes()[0], before);
    assert_eq!(project.history_cursor(), history_cursor);
}

#[test]
fn humanize_timing_bias_uses_a_wider_range_than_the_grid_only_jitter() {
    let (mut project, item_id) = project_with_midi_notes();
    let note_ids = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.id())
        .collect::<Vec<_>>();
    let original_ticks = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.tick())
        .collect::<Vec<_>>();
    project
        .apply(DawAction::HumanizeMidiNotes {
            item_id,
            note_ids,
            grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
            timing_amount_percent: 100,
            velocity_amount_percent: 0,
            timing_bias_percent: 100,
            seed: 0x43b0_1a52_74c9_203f,
        })
        .expect("biased humanize should succeed");
    let notes = project.midi_items()[0].notes();
    assert!(
        notes
            .iter()
            .zip(&original_ticks)
            .any(|(note, tick)| note.tick().abs_diff(*tick) > 60)
    );
    assert!(
        notes
            .iter()
            .zip(&original_ticks)
            .all(|(note, tick)| { note.tick().abs_diff(*tick) <= 540 && note.velocity() > 0 })
    );
}
