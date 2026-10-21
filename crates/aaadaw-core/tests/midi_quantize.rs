use aaadaw_core::{
    DawAction, GridFraction, MidiChannelPressureData, MidiControllerData, MidiNoteData,
    MidiNoteQuantizeOperation, MidiOpaqueEventData, MidiOpaqueEventKind, MidiPitchBendData,
    MidiPolyAftertouchData, MidiProgramChangeData, Project,
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
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 510,
                    duration: 120,
                    velocity: 90,
                    off_velocity: 0,
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
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 2,
                    pitch: 64,
                    tick: 510,
                    duration: 180,
                    velocity: 90,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 67,
                    tick: 730,
                    duration: 240,
                    velocity: 80,
                    off_velocity: 0,
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
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 2,
                    pitch: 62,
                    tick: 350,
                    duration: 80,
                    velocity: 80,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 550,
                    duration: 120,
                    velocity: 72,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 15,
                    pitch: 65,
                    tick: 900,
                    duration: 70,
                    velocity: 110,
                    off_velocity: 0,
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
                        off_velocity: 0,
                    },
                    MidiNoteData {
                        channel: 2,
                        pitch: 62,
                        tick: 350,
                        duration: 80,
                        velocity: 80,
                        off_velocity: 0,
                    },
                    MidiNoteData {
                        channel: 1,
                        pitch: 67,
                        tick: 550,
                        duration: 120,
                        velocity: 72,
                        off_velocity: 0,
                    },
                    MidiNoteData {
                        channel: 15,
                        pitch: 64,
                        tick: 900,
                        duration: 70,
                        velocity: 110,
                        off_velocity: 0,
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
                swing_amount_percent: 0,
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
fn midi_quantize_dialog_uses_swing_grid_for_note_positions_and_endpoints() {
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
                    pitch: 66,
                    tick: 75,
                    duration: 20,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 170,
                    duration: 90,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 62,
                    tick: 350,
                    duration: 80,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 550,
                    duration: 120,
                    velocity: 96,
                    off_velocity: 0,
                },
            ],
        })
        .expect("test notes should be inserted");
    let selected_note_ids = project.midi_items()[0]
        .notes()
        .iter()
        .take(3)
        .map(|note| note.id())
        .collect::<Vec<_>>();
    let endpoint_note_ids = selected_note_ids[1..].to_vec();
    let grid = GridFraction::new(1, 32).expect("1/32 is a valid grid");
    let positions = |project: &Project| {
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>()
    };

    project
        .apply(DawAction::QuantizeMidiNotesWithOperation {
            item_id,
            note_ids: selected_note_ids.clone(),
            grid,
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::Position,
            swing_amount_percent: 50,
            fix_overlaps: false,
        })
        .expect("swing position quantize should succeed");
    assert_eq!(
        positions(&project),
        [(150, 20), (150, 90), (390, 80), (550, 120)]
    );
    assert!(project.undo().expect("swing position quantize should undo"));

    project
        .apply(DawAction::QuantizeMidiNotesWithOperation {
            item_id,
            note_ids: endpoint_note_ids.clone(),
            grid,
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::PositionAndNoteEnd,
            swing_amount_percent: 50,
            fix_overlaps: false,
        })
        .expect("swing endpoint quantize should succeed");
    assert_eq!(
        positions(&project),
        [(75, 20), (150, 90), (390, 90), (550, 120)]
    );
    assert!(project.undo().expect("swing endpoint quantize should undo"));

    project
        .apply(DawAction::QuantizeMidiNotesWithOperation {
            item_id,
            note_ids: endpoint_note_ids.clone(),
            grid,
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::NoteEndOnly,
            swing_amount_percent: 50,
            fix_overlaps: false,
        })
        .expect("swing note-end-only quantize should succeed");
    assert_eq!(
        positions(&project),
        [(75, 20), (170, 70), (350, 40), (550, 120)]
    );
    assert!(
        project
            .undo()
            .expect("swing note-end-only quantize should undo")
    );

    project
        .apply(DawAction::QuantizeMidiNotesWithOperation {
            item_id,
            note_ids: endpoint_note_ids.clone(),
            grid,
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::NoteLengthOnly,
            swing_amount_percent: 50,
            fix_overlaps: false,
        })
        .expect("swing note-length-only quantize should succeed");
    assert_eq!(
        positions(&project),
        [(75, 20), (170, 120), (350, 120), (550, 120)]
    );
    assert!(
        project
            .undo()
            .expect("swing note-length-only quantize should undo")
    );

    project
        .apply(DawAction::QuantizeMidiNotesWithOperation {
            item_id,
            note_ids: endpoint_note_ids,
            grid,
            strength: 0.5,
            operation: MidiNoteQuantizeOperation::Position,
            swing_amount_percent: -50,
            fix_overlaps: false,
        })
        .expect("negative partial swing quantize should succeed");
    assert_eq!(
        positions(&project),
        [(75, 20), (205, 90), (340, 80), (550, 120)]
    );
    assert!(project.undo().expect("negative swing quantize should undo"));
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
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 180,
                    duration: 120,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 330,
                    duration: 20,
                    velocity: 96,
                    off_velocity: 0,
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
                swing_amount_percent: 0,
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
            swing_amount_percent: 0,
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
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 180,
                    duration: 120,
                    velocity: 96,
                    off_velocity: 0,
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
            swing_amount_percent: 0,
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

    project
        .apply(DawAction::QuantizeMidiNotesWithOperation {
            item_id,
            note_ids: vec![selected_note_id],
            grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::PositionAndNoteLength,
            swing_amount_percent: 0,
            fix_overlaps: false,
        })
        .expect("unchecked selected note should quantize without trimming its predecessor");
    assert_eq!(
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        [(100, 200), (240, 120)]
    );
    assert!(
        project
            .undo()
            .expect("unchecked quantize should be undoable")
    );
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
fn midi_quantize_keeps_selected_note_off_with_the_quantized_note() {
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
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 180,
                    duration: 120,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 330,
                    duration: 150,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 400,
                    duration: 80,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 530,
                    duration: 120,
                    velocity: 96,
                    off_velocity: 0,
                },
            ],
        })
        .expect("overlapping notes should be inserted");
    let selected_note_id = project.midi_items()[0].notes()[1].id();
    let original = [(100, 200), (180, 120), (330, 150), (400, 80), (530, 120)];
    let reaper_result = [(100, 200), (240, 120), (330, 150), (400, 80), (530, 120)];

    for operation in [
        MidiNoteQuantizeOperation::Position,
        MidiNoteQuantizeOperation::PositionAndNoteLength,
    ] {
        project
            .apply(DawAction::QuantizeMidiNotesWithOperation {
                item_id,
                note_ids: vec![selected_note_id],
                grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
                strength: 1.0,
                operation,
                swing_amount_percent: 0,
                fix_overlaps: false,
            })
            .expect("selected overlapping note should quantize");
        assert_eq!(
            project.midi_items()[0]
                .notes()
                .iter()
                .map(|note| (note.tick(), note.duration()))
                .collect::<Vec<_>>(),
            reaper_result
        );
        assert!(
            project
                .undo()
                .expect("selected quantize should be undoable")
        );
        assert_eq!(
            project.midi_items()[0]
                .notes()
                .iter()
                .map(|note| (note.tick(), note.duration()))
                .collect::<Vec<_>>(),
            original
        );
    }
}

#[test]
fn midi_quantize_multiple_selected_notes_keep_note_offs_through_overlap_chain() {
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
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 180,
                    duration: 120,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 330,
                    duration: 150,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 400,
                    duration: 300,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 530,
                    duration: 120,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 620,
                    duration: 140,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 800,
                    duration: 80,
                    velocity: 96,
                    off_velocity: 0,
                },
            ],
        })
        .expect("overlapping notes should be inserted");
    let notes = project.midi_items()[0].notes();
    let selected_note_ids = vec![notes[1].id(), notes[3].id()];
    let original = [
        (100, 200),
        (180, 120),
        (330, 150),
        (400, 300),
        (530, 120),
        (620, 140),
        (800, 80),
    ];
    let quantized = [
        (100, 200),
        (240, 120),
        (330, 150),
        (360, 360),
        (530, 120),
        (620, 140),
        (800, 80),
    ];

    project
        .apply(DawAction::QuantizeMidiNotesWithOperation {
            item_id,
            note_ids: selected_note_ids,
            grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::PositionAndNoteLength,
            swing_amount_percent: 0,
            fix_overlaps: false,
        })
        .expect("selected overlap chain should quantize");
    let note_positions = |project: &Project| {
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>()
    };
    assert_eq!(note_positions(&project), quantized);
    assert!(
        project
            .undo()
            .expect("multi-note quantize should be undoable")
    );
    assert_eq!(note_positions(&project), original);
    assert!(
        project
            .redo()
            .expect("multi-note quantize should be redoable")
    );
    assert_eq!(note_positions(&project), quantized);
}

#[test]
fn midi_quantize_all_modeled_events_and_undoes_as_one_edit() {
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
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 62,
                    tick: 350,
                    duration: 80,
                    velocity: 96,
                    off_velocity: 0,
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
    let program_changes = [MidiProgramChangeData {
        channel: 1,
        tick: 475,
        program: 42,
    }];
    project
        .apply(DawAction::SetMidiProgramChanges {
            item_id,
            program_changes: program_changes.to_vec(),
        })
        .expect("test Program Change should be inserted");
    let poly_aftertouches = [MidiPolyAftertouchData {
        channel: 2,
        tick: 835,
        note: 60,
        value: 72,
    }];
    project
        .apply(DawAction::SetMidiPolyAftertouches {
            item_id,
            poly_aftertouches: poly_aftertouches.to_vec(),
        })
        .expect("test Poly Aftertouch should be inserted");
    let channel_pressures = [MidiChannelPressureData {
        channel: 3,
        tick: 730,
        value: 88,
    }];
    project
        .apply(DawAction::SetMidiChannelPressures {
            item_id,
            channel_pressures: channel_pressures.to_vec(),
        })
        .expect("test Channel Pressure should be inserted");
    let opaque_events = [
        MidiOpaqueEventData {
            tick: 730,
            order: 900,
            kind: MidiOpaqueEventKind::Meta {
                kind: 0x01,
                data: vec![0xff, 0x00, b'x'],
            },
        },
        MidiOpaqueEventData {
            tick: 835,
            order: 901,
            kind: MidiOpaqueEventKind::SystemExclusive {
                status: 0xF0,
                data: vec![0x43, 0x12, 0x34],
            },
        },
    ];
    project
        .apply(DawAction::SetMidiOpaqueEvents {
            item_id,
            events: opaque_events.to_vec(),
        })
        .expect("test Meta event should be inserted");
    let before_notes = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| (note.tick(), note.duration()))
        .collect::<Vec<_>>();
    let before_controllers = project.midi_items()[0].controllers().to_vec();
    let before_pitch_bends = project.midi_items()[0].pitch_bends().to_vec();
    let before_program_changes = project.midi_items()[0].program_changes().to_vec();
    let before_poly_aftertouches = project.midi_items()[0].poly_aftertouches().to_vec();
    let before_channel_pressures = project.midi_items()[0].channel_pressures().to_vec();
    let before_opaque_events = project.midi_items()[0].opaque_events().to_vec();

    project
        .apply(DawAction::QuantizeMidiEventsWithOperation {
            item_id,
            grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::Position,
            swing_amount_percent: 50,
            fix_overlaps: false,
        })
        .expect("all MIDI event positions should be quantized");

    let item = &project.midi_items()[0];
    assert_eq!(
        item.notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        [(150, 180), (390, 80)]
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
        [(0, 870, 8_192)]
    );
    assert_eq!(
        item.program_changes(),
        [MidiProgramChangeData {
            tick: 480,
            ..program_changes[0]
        }]
    );
    assert_eq!(
        item.poly_aftertouches(),
        [MidiPolyAftertouchData {
            tick: 870,
            ..poly_aftertouches[0]
        }]
    );
    assert_eq!(
        item.channel_pressures(),
        [MidiChannelPressureData {
            tick: 720,
            ..channel_pressures[0]
        }]
    );
    assert_eq!(
        item.opaque_events(),
        [
            MidiOpaqueEventData {
                tick: 720,
                ..opaque_events[0].clone()
            },
            MidiOpaqueEventData {
                tick: 870,
                ..opaque_events[1].clone()
            },
        ]
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
    assert_eq!(item.program_changes(), before_program_changes);
    assert_eq!(item.poly_aftertouches(), before_poly_aftertouches);
    assert_eq!(item.channel_pressures(), before_channel_pressures);
    assert_eq!(item.opaque_events(), before_opaque_events);
    assert!(
        project
            .redo()
            .expect("all-event quantize should be redoable")
    );
    assert_eq!(project.midi_items()[0].controllers().len(), 2);
    assert_eq!(project.midi_items()[0].controllers()[0].value, 65);
    assert_eq!(project.midi_items()[0].pitch_bends().len(), 1);
    assert_eq!(project.midi_items()[0].pitch_bends()[0].value, 8_192);
    assert_eq!(project.midi_items()[0].program_changes()[0].tick, 480);
    assert_eq!(project.midi_items()[0].poly_aftertouches()[0].tick, 870);
    assert_eq!(project.midi_items()[0].channel_pressures()[0].tick, 720);
    assert_eq!(
        project.midi_items()[0].opaque_events(),
        [
            MidiOpaqueEventData {
                tick: 720,
                ..opaque_events[0].clone()
            },
            MidiOpaqueEventData {
                tick: 870,
                ..opaque_events[1].clone()
            },
        ]
    );
}

#[test]
fn midi_quantize_all_events_preserves_selected_priority_and_poly_aftertouch_collisions() {
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
    let selected_program = MidiProgramChangeData {
        channel: 0,
        tick: 475,
        program: 12,
    };
    let unselected_program = MidiProgramChangeData {
        tick: 480,
        program: 13,
        ..selected_program
    };
    project
        .apply(DawAction::SetMidiProgramChanges {
            item_id,
            program_changes: vec![selected_program, unselected_program],
        })
        .expect("test Program Changes should be inserted");
    let selected_pressure = MidiChannelPressureData {
        channel: 0,
        tick: 475,
        value: 60,
    };
    let unselected_pressure = MidiChannelPressureData {
        tick: 480,
        value: 70,
        ..selected_pressure
    };
    project
        .apply(DawAction::SetMidiChannelPressures {
            item_id,
            channel_pressures: vec![selected_pressure, unselected_pressure],
        })
        .expect("test Channel Pressure events should be inserted");
    let selected_aftertouch = MidiPolyAftertouchData {
        channel: 0,
        tick: 355,
        note: 60,
        value: 80,
    };
    let unselected_aftertouch = MidiPolyAftertouchData {
        tick: 360,
        value: 90,
        ..selected_aftertouch
    };
    project
        .apply(DawAction::SetMidiPolyAftertouches {
            item_id,
            poly_aftertouches: vec![selected_aftertouch, unselected_aftertouch],
        })
        .expect("test Poly Aftertouch events should be inserted");

    project
        .apply(DawAction::QuantizeMidiEventsWithSelection {
            item_id,
            selected_controllers: Vec::new(),
            selected_pitch_bends: Vec::new(),
            selected_program_changes: vec![selected_program],
            selected_channel_pressures: vec![selected_pressure],
            grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::Position,
            swing_amount_percent: 0,
            fix_overlaps: false,
        })
        .expect("All events should retain selected collision priority");

    let item = &project.midi_items()[0];
    assert_eq!(
        item.program_changes(),
        [MidiProgramChangeData {
            tick: 480,
            ..selected_program
        }]
    );
    assert_eq!(
        item.channel_pressures(),
        [MidiChannelPressureData {
            tick: 480,
            ..selected_pressure
        }]
    );
    assert_eq!(
        item.poly_aftertouches(),
        [
            MidiPolyAftertouchData {
                tick: 360,
                ..selected_aftertouch
            },
            MidiPolyAftertouchData {
                tick: 360,
                ..unselected_aftertouch
            },
        ]
    );
}

#[test]
fn midi_quantize_all_opaque_events_preserves_reaper_same_tick_collisions() {
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
    let events = [
        MidiOpaqueEventData {
            tick: 475,
            order: 1,
            kind: MidiOpaqueEventKind::Meta {
                kind: 0x01,
                data: b"selected text at 475".to_vec(),
            },
        },
        MidiOpaqueEventData {
            tick: 484,
            order: 2,
            kind: MidiOpaqueEventKind::Meta {
                kind: 0x01,
                data: b"unselected text at 484".to_vec(),
            },
        },
        MidiOpaqueEventData {
            tick: 595,
            order: 3,
            kind: MidiOpaqueEventKind::SystemExclusive {
                status: 0xF0,
                data: vec![0x43, 0x12],
            },
        },
        MidiOpaqueEventData {
            tick: 600,
            order: 4,
            kind: MidiOpaqueEventKind::SystemExclusive {
                status: 0xF0,
                data: vec![0x55, 0x66],
            },
        },
        MidiOpaqueEventData {
            tick: 715,
            order: 5,
            kind: MidiOpaqueEventKind::Meta {
                kind: 0x06,
                data: b"unselected marker at 715".to_vec(),
            },
        },
    ];
    project
        .apply(DawAction::SetMidiOpaqueEvents {
            item_id,
            events: events.to_vec(),
        })
        .expect("test opaque MIDI events should be inserted");

    project
        .apply(DawAction::QuantizeMidiEventsWithOperation {
            item_id,
            grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::Position,
            swing_amount_percent: 0,
            fix_overlaps: false,
        })
        .expect("All events quantize should preserve opaque collisions");

    assert_eq!(
        project.midi_items()[0].opaque_events(),
        [
            MidiOpaqueEventData {
                tick: 480,
                ..events[0].clone()
            },
            MidiOpaqueEventData {
                tick: 480,
                ..events[1].clone()
            },
            MidiOpaqueEventData {
                tick: 600,
                ..events[2].clone()
            },
            MidiOpaqueEventData {
                tick: 600,
                ..events[3].clone()
            },
            MidiOpaqueEventData {
                tick: 720,
                ..events[4].clone()
            },
        ]
    );
}

#[test]
fn midi_quantize_selected_events_only_changes_the_selected_note_cc_and_pitch_bend() {
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
                    tick: 170,
                    duration: 80,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 62,
                    tick: 350,
                    duration: 80,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 550,
                    duration: 80,
                    velocity: 96,
                    off_velocity: 0,
                },
            ],
        })
        .expect("test notes should be inserted");
    let selected_note_id = project.midi_items()[0].notes()[0].id();
    let selected_controller = MidiControllerData {
        controller: 1,
        channel: 0,
        tick: 475,
        value: 64,
    };
    let colliding_unselected_controller = MidiControllerData {
        tick: 480,
        value: 65,
        ..selected_controller
    };
    let untouched_controller = MidiControllerData {
        controller: 7,
        channel: 1,
        tick: 730,
        value: 96,
    };
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                selected_controller,
                colliding_unselected_controller,
                untouched_controller,
            ],
        })
        .expect("test controllers should be inserted");
    let selected_pitch_bend = MidiPitchBendData {
        channel: 0,
        tick: 835,
        value: 8_000,
    };
    let colliding_unselected_pitch_bend = MidiPitchBendData {
        tick: 870,
        value: 8_192,
        ..selected_pitch_bend
    };
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![selected_pitch_bend, colliding_unselected_pitch_bend],
        })
        .expect("test pitch bends should be inserted");
    let before_notes = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.tick())
        .collect::<Vec<_>>();

    project
        .apply(DawAction::QuantizeSelectedMidiEventsWithOperation {
            item_id,
            note_ids: vec![selected_note_id],
            controllers: vec![selected_controller],
            pitch_bends: vec![selected_pitch_bend],
            program_changes: Vec::new(),
            poly_aftertouches: Vec::new(),
            channel_pressures: Vec::new(),
            opaque_events: Vec::new(),
            grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
            strength: 1.0,
            operation: MidiNoteQuantizeOperation::Position,
            swing_amount_percent: 50,
            fix_overlaps: false,
        })
        .expect("selected MIDI events should quantize");

    let item = &project.midi_items()[0];
    assert_eq!(
        item.notes()
            .iter()
            .map(|note| note.tick())
            .collect::<Vec<_>>(),
        [150, before_notes[1], before_notes[2]]
    );
    assert_eq!(
        item.controllers(),
        [
            MidiControllerData {
                tick: 480,
                ..selected_controller
            },
            untouched_controller,
        ]
    );
    assert_eq!(
        item.pitch_bends(),
        [MidiPitchBendData {
            tick: 870,
            ..selected_pitch_bend
        }]
    );

    assert!(project.undo().expect("selected quantize should undo"));
    let item = &project.midi_items()[0];
    assert_eq!(
        item.notes()
            .iter()
            .map(|note| note.tick())
            .collect::<Vec<_>>(),
        before_notes
    );
    assert_eq!(
        item.controllers(),
        [
            selected_controller,
            colliding_unselected_controller,
            untouched_controller,
        ]
    );
    assert_eq!(
        item.pitch_bends(),
        [selected_pitch_bend, colliding_unselected_pitch_bend]
    );
    assert!(project.redo().expect("selected quantize should redo"));
    assert_eq!(project.midi_items()[0].controllers()[0].tick, 480);
    assert_eq!(project.midi_items()[0].pitch_bends()[0].tick, 870);
}
