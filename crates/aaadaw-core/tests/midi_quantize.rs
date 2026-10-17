use aaadaw_core::{
    DawAction, GridFraction, MidiControllerData, MidiNoteData, MidiNoteQuantizeOperation,
    MidiPitchBendData, Project,
};

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
                    channel: 0,
                    pitch: 60,
                    tick: 110,
                    duration: 120,
                    velocity: 100,
                },
                MidiNoteData {
                    channel: 0,
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

    project
        .apply(DawAction::QuantizeItem {
            item_id,
            grid: GridFraction::new(1, 16).expect("1/16 is a valid grid"),
            strength: 0.0,
        })
        .expect("zero-strength quantize should be a no-op");
    assert!(project.redo().expect("no-op should preserve redo history"));
    let notes = project.midi_items()[0].notes();
    assert_eq!(notes[0].tick(), 0);
    assert_eq!(notes[1].tick(), 480);
}

#[test]
fn midi_quantize_can_limit_changes_to_the_selected_note_ids() {
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
            length_ticks: 3_840,
        })
        .expect("inserting a MIDI item should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 110,
                    duration: 120,
                    velocity: 100,
                },
                MidiNoteData {
                    channel: 2,
                    pitch: 64,
                    tick: 510,
                    duration: 180,
                    velocity: 90,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 67,
                    tick: 730,
                    duration: 240,
                    velocity: 80,
                },
            ],
        })
        .expect("adding notes should succeed");
    let selected_note_id = project.midi_items()[0].notes()[1].id();

    project
        .apply(DawAction::QuantizeMidiNotes {
            item_id,
            note_ids: vec![selected_note_id],
            grid: GridFraction::new(1, 16).expect("1/16 is a valid grid"),
            strength: 1.0,
        })
        .expect("quantizing selected notes should succeed");

    let notes = project.midi_items()[0].notes();
    assert_eq!(notes[0].tick(), 110);
    assert_eq!(notes[1].tick(), 480);
    assert_eq!(notes[1].duration(), 180);
    assert_eq!(notes[1].channel(), 2);
    assert_eq!(notes[2].tick(), 730);
    assert!(
        project
            .undo()
            .expect("selected quantize should be undoable")
    );
    assert_eq!(project.midi_items()[0].notes()[1].tick(), 510);
    assert!(
        project
            .redo()
            .expect("selected quantize should be redoable")
    );
    assert_eq!(project.midi_items()[0].notes()[1].tick(), 480);
}

#[test]
fn midi_quantize_selected_positions_and_ends_match_reaper_and_are_undoable() {
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
            length_ticks: 4_000,
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
                    tick: 100,
                    duration: 120,
                    velocity: 96,
                },
                MidiNoteData {
                    channel: 2,
                    pitch: 62,
                    tick: 350,
                    duration: 80,
                    velocity: 80,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 550,
                    duration: 120,
                    velocity: 72,
                },
                MidiNoteData {
                    channel: 15,
                    pitch: 65,
                    tick: 900,
                    duration: 70,
                    velocity: 110,
                },
            ],
        })
        .expect("test notes should be inserted");
    let note_ids = project.midi_items()[0]
        .notes()
        .iter()
        .enumerate()
        .filter_map(|(index, note)| (index != 2).then_some(note.id()))
        .collect::<Vec<_>>();

    project
        .apply(DawAction::QuantizeMidiNotesAndEnds {
            item_id,
            note_ids,
            grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
            strength: 1.0,
        })
        .expect("selected note positions and ends should quantize");

    let notes = project.midi_items()[0].notes();
    assert_eq!((notes[0].tick(), notes[0].duration()), (120, 120));
    assert_eq!(
        (
            notes[1].tick(),
            notes[1].duration(),
            notes[1].channel(),
            notes[1].velocity()
        ),
        (360, 120, 2, 80)
    );
    assert_eq!((notes[2].tick(), notes[2].duration()), (550, 120));
    // REAPER keeps a positive note when its start/end would otherwise collapse to one grid line.
    assert_eq!(
        (
            notes[3].tick(),
            notes[3].duration(),
            notes[3].channel(),
            notes[3].velocity()
        ),
        (960, 120, 15, 110)
    );

    assert!(project.undo().expect("quantize should be undoable"));
    let notes = project.midi_items()[0].notes();
    assert_eq!((notes[1].tick(), notes[1].duration()), (350, 80));
    assert_eq!((notes[3].tick(), notes[3].duration()), (900, 70));
    assert!(project.redo().expect("quantize should be redoable"));
    let notes = project.midi_items()[0].notes();
    assert_eq!((notes[1].tick(), notes[1].duration()), (360, 120));
    assert_eq!((notes[3].tick(), notes[3].duration()), (960, 120));
}

#[test]
fn midi_quantize_dialog_operations_match_reaper_ticks_and_round_trip_history() {
    let grid = GridFraction::new(1, 32).expect("1/32 is a valid grid");
    let cases = [
        (
            MidiNoteQuantizeOperation::Position,
            [(120, 180), (360, 80), (550, 120), (960, 70)],
        ),
        (
            MidiNoteQuantizeOperation::PositionAndNoteEnd,
            [(120, 120), (360, 120), (550, 120), (960, 120)],
        ),
        (
            MidiNoteQuantizeOperation::PositionAndNoteLength,
            [(120, 240), (360, 120), (550, 120), (960, 120)],
        ),
        (
            MidiNoteQuantizeOperation::NoteEndOnly,
            [(100, 140), (350, 130), (550, 120), (900, 60)],
        ),
        (
            MidiNoteQuantizeOperation::NoteLengthOnly,
            [(100, 240), (350, 120), (550, 120), (900, 120)],
        ),
    ];

    for (operation, expected) in cases {
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
                length_ticks: 4_000,
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
                        tick: 100,
                        duration: 180,
                        velocity: 96,
                    },
                    MidiNoteData {
                        channel: 2,
                        pitch: 62,
                        tick: 350,
                        duration: 80,
                        velocity: 80,
                    },
                    MidiNoteData {
                        channel: 1,
                        pitch: 67,
                        tick: 550,
                        duration: 120,
                        velocity: 72,
                    },
                    MidiNoteData {
                        channel: 15,
                        pitch: 64,
                        tick: 900,
                        duration: 70,
                        velocity: 110,
                    },
                ],
            })
            .expect("test notes should be inserted");
        let notes = project.midi_items()[0].notes();
        let note_ids = [notes[0].id(), notes[1].id(), notes[3].id()];
        let original = notes
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>();

        project
            .apply(DawAction::QuantizeMidiNotesWithOperation {
                item_id,
                note_ids: note_ids.to_vec(),
                grid,
                strength: 1.0,
                operation,
                fix_overlaps: false,
            })
            .expect("dialog operation should quantize selected notes");
        let actual = project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "unexpected result for {operation:?}");

        assert!(project.undo().expect("dialog quantize should be undoable"));
        let restored = project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>();
        assert_eq!(restored, original, "undo failed for {operation:?}");
        assert!(project.redo().expect("dialog quantize should be redoable"));
        let redone = project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>();
        assert_eq!(redone, expected, "redo failed for {operation:?}");
    }
}

#[test]
fn midi_quantize_fix_overlaps_matches_reaper_for_same_channel_and_pitch() {
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
            length_ticks: 4_000,
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
                    tick: 100,
                    duration: 200,
                    velocity: 96,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 180,
                    duration: 120,
                    velocity: 96,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 330,
                    duration: 20,
                    velocity: 96,
                },
            ],
        })
        .expect("overlapping notes should be inserted");
    let note_ids = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.id())
        .collect::<Vec<_>>();
    let grid = GridFraction::new(1, 32).expect("1/32 is a valid grid");
    let original = [(100, 200), (180, 120), (330, 20)];
    let apply_quantize = |project: &mut Project, fix_overlaps| {
        project
            .apply(DawAction::QuantizeMidiNotesWithOperation {
                item_id,
                note_ids: note_ids.clone(),
                grid,
                strength: 1.0,
                operation: MidiNoteQuantizeOperation::PositionAndNoteLength,
                fix_overlaps,
            })
            .expect("quantize should succeed");
    };
    let note_positions = |project: &Project| {
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>()
    };

    project
        .apply(DawAction::QuantizeMidiNotesWithOperation {
            item_id,
            note_ids: note_ids.clone(),
            grid,
            strength: 0.0,
            operation: MidiNoteQuantizeOperation::PositionAndNoteLength,
            fix_overlaps: false,
        })
        .expect("zero-strength note-length quantize should be valid");
    assert_eq!(note_positions(&project), original);

    apply_quantize(&mut project, true);
    let fixed = [(120, 120), (240, 120), (360, 120)];
    assert_eq!(note_positions(&project), fixed);
    assert!(project.undo().expect("Fix overlaps should be undoable"));
    assert_eq!(note_positions(&project), original);
    assert!(project.redo().expect("Fix overlaps should be redoable"));
    assert_eq!(note_positions(&project), fixed);
    assert!(project.undo().expect("restore input before unchecked case"));

    apply_quantize(&mut project, false);
    let overlapping = [(120, 240), (240, 120), (360, 120)];
    assert_eq!(note_positions(&project), overlapping);
    assert!(
        project
            .undo()
            .expect("unchecked quantize should be undoable")
    );
    assert_eq!(note_positions(&project), original);
    assert!(
        project
            .redo()
            .expect("unchecked quantize should be redoable")
    );
    assert_eq!(note_positions(&project), overlapping);
}

#[test]
fn midi_quantize_fix_overlaps_trims_unselected_predecessor_in_selected_scope() {
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
            length_ticks: 4_000,
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
                    tick: 100,
                    duration: 200,
                    velocity: 96,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 180,
                    duration: 120,
                    velocity: 96,
                },
            ],
        })
        .expect("overlapping notes should be inserted");
    let selected_note_id = project.midi_items()[0].notes()[1].id();

    project
        .apply(DawAction::QuantizeMidiNotesWithOperation {
            item_id,
            note_ids: vec![selected_note_id],
            grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::PositionAndNoteLength,
            fix_overlaps: true,
        })
        .expect("selected note should quantize and fix its overlap");

    let item = &project.midi_items()[0];
    assert_eq!(
        item.notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        [(100, 140), (240, 120)]
    );
    assert!(project.undo().expect("overlap fix should be undoable"));
    assert_eq!(
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        [(100, 200), (180, 120)]
    );
}

#[test]
fn midi_quantize_all_events_matches_reaper_and_undoes_as_one_edit() {
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
            length_ticks: 4_000,
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
                    tick: 100,
                    duration: 180,
                    velocity: 96,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 62,
                    tick: 350,
                    duration: 80,
                    velocity: 96,
                },
            ],
        })
        .expect("test notes should be inserted");
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                MidiControllerData {
                    controller: 1,
                    channel: 0,
                    tick: 475,
                    value: 64,
                },
                MidiControllerData {
                    controller: 1,
                    channel: 0,
                    tick: 485,
                    value: 65,
                },
                MidiControllerData {
                    controller: 7,
                    channel: 1,
                    tick: 730,
                    value: 96,
                },
            ],
        })
        .expect("test controllers should be inserted");
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![
                MidiPitchBendData {
                    channel: 0,
                    tick: 835,
                    value: 8_000,
                },
                MidiPitchBendData {
                    channel: 0,
                    tick: 850,
                    value: 8_192,
                },
            ],
        })
        .expect("test pitch bend should be inserted");
    let before_notes = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| (note.tick(), note.duration()))
        .collect::<Vec<_>>();
    let before_controllers = project.midi_items()[0].controllers().to_vec();
    let before_pitch_bends = project.midi_items()[0].pitch_bends().to_vec();

    project
        .apply(DawAction::QuantizeMidiEventsWithOperation {
            item_id,
            grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::Position,
            fix_overlaps: false,
        })
        .expect("all MIDI event positions should be quantized");

    let item = &project.midi_items()[0];
    assert_eq!(
        item.notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        [(120, 180), (360, 80)]
    );
    assert_eq!(
        item.controllers()
            .iter()
            .map(|controller| (
                controller.channel,
                controller.controller,
                controller.tick,
                controller.value
            ))
            .collect::<Vec<_>>(),
        [(0, 1, 480, 65), (1, 7, 720, 96)]
    );
    assert_eq!(
        item.pitch_bends()
            .iter()
            .map(|bend| (bend.channel, bend.tick, bend.value))
            .collect::<Vec<_>>(),
        [(0, 840, 8_192)]
    );

    assert!(
        project
            .undo()
            .expect("all-event quantize should be undoable")
    );
    let item = &project.midi_items()[0];
    assert_eq!(
        item.notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        before_notes
    );
    assert_eq!(item.controllers(), before_controllers);
    assert_eq!(item.pitch_bends(), before_pitch_bends);
    assert!(
        project
            .redo()
            .expect("all-event quantize should be redoable")
    );
    assert_eq!(project.midi_items()[0].controllers().len(), 2);
    assert_eq!(project.midi_items()[0].controllers()[0].value, 65);
    assert_eq!(project.midi_items()[0].pitch_bends().len(), 1);
    assert_eq!(project.midi_items()[0].pitch_bends()[0].value, 8_192);
}
