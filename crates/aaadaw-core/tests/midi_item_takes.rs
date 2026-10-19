use aaadaw_core::{
    DawAction, MidiEventOrderData, MidiEventOrderTarget, MidiItemTakeSnapshot, MidiNoteData,
    MidiNoteSnapshot, Project,
};

#[test]
fn adding_empty_midi_takes_preserves_the_active_source_and_round_trips() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Takes".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
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
                    channel: 0,
                    tick: 0,
                    duration: 480,
                    velocity: 100,
                    off_velocity: 0,
                },
                MidiNoteData {
                    pitch: 64,
                    channel: 0,
                    tick: 960,
                    duration: 480,
                    velocity: 92,
                    off_velocity: 12,
                },
            ],
        })
        .unwrap();
    let mut snapshot = project.snapshot();
    let item = &mut snapshot.midi_items[0];
    let take_a_note = item.notes[0].clone();
    item.notes = vec![item.notes[1].clone()];
    item.name = "Take B".to_owned();
    item.active_take_index = 1;
    item.source_offset_ticks = 120;
    item.other_takes = vec![MidiItemTakeSnapshot {
        index: 0,
        name: "Take A".to_owned(),
        source_offset_ticks: 0,
        loop_source: false,
        source_loop_length_ticks: 3_840,
        notes: vec![take_a_note],
        controllers: Vec::new(),
        pitch_bends: Vec::new(),
        program_changes: Vec::new(),
        poly_aftertouches: Vec::new(),
        channel_pressures: Vec::new(),
        opaque_events: Vec::new(),
        event_order: Vec::new(),
    }];
    let mut project = Project::from_snapshot(snapshot).unwrap();

    let before = project.snapshot();
    let cursor = project.history_cursor();
    project
        .apply(DawAction::AddEmptyMidiItemTakeBefore { item_id })
        .unwrap();
    let item = &project.midi_items()[0];
    assert_eq!(project.history_cursor(), cursor + 1);
    assert_eq!(item.active_take_index(), 2);
    assert_eq!(item.name(), "Take B");
    assert_eq!(item.source_offset_ticks(), 120);
    let takes = item.take_snapshots();
    assert_eq!(
        takes.iter().map(|take| take.index).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(takes[0].name, "Take A");
    assert!(takes[1].name.is_empty() && takes[1].notes.is_empty());
    assert_eq!(takes[1].source_loop_length_ticks, 3_840);
    assert_eq!(takes[2].name, "Take B");
    assert_eq!(takes[2].notes[0].data.pitch, 64);
    let after_before = project.snapshot();
    project.undo().unwrap();
    assert_eq!(project.snapshot(), before);
    project.redo().unwrap();
    assert_eq!(project.snapshot(), after_before);
    project.undo().unwrap();
    assert_eq!(project.snapshot(), before);

    project
        .apply(DawAction::SetMidiItemActiveTake {
            item_id,
            take_index: 0,
        })
        .unwrap();
    project
        .apply(DawAction::AddEmptyMidiItemTakeAfter { item_id })
        .unwrap();
    let item = &project.midi_items()[0];
    assert_eq!(item.active_take_index(), 0);
    let takes = item.take_snapshots();
    assert_eq!(
        takes.iter().map(|take| take.index).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(takes[0].name, "Take A");
    assert!(takes[1].name.is_empty() && takes[1].notes.is_empty());
    assert_eq!(takes[2].name, "Take B");
    assert_eq!(
        Project::from_snapshot(project.snapshot())
            .unwrap()
            .snapshot(),
        project.snapshot()
    );

    project
        .apply(DawAction::SetMidiItemActiveTake {
            item_id,
            take_index: 1,
        })
        .unwrap();
    assert!(project.midi_items()[0].name().is_empty());
    assert!(project.midi_items()[0].notes().is_empty());
    assert_eq!(
        Project::from_snapshot(project.snapshot())
            .unwrap()
            .snapshot(),
        project.snapshot()
    );
}

#[test]
fn switching_midi_item_active_take_preserves_both_take_states_and_undo_history() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "MIDI takes".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
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
                    channel: 0,
                    tick: 0,
                    duration: 480,
                    velocity: 100,
                    off_velocity: 0,
                },
                MidiNoteData {
                    pitch: 64,
                    channel: 0,
                    tick: 960,
                    duration: 480,
                    velocity: 100,
                    off_velocity: 0,
                },
            ],
        })
        .unwrap();

    let mut snapshot = project.snapshot();
    let item = &mut snapshot.midi_items[0];
    let take_a_note = item.notes[0].clone();
    item.notes = vec![item.notes[1].clone()];
    item.name = "Take B".to_owned();
    item.active_take_index = 1;
    item.source_offset_ticks = 120;
    item.other_takes = vec![MidiItemTakeSnapshot {
        index: 0,
        name: "Take A".to_owned(),
        source_offset_ticks: 0,
        loop_source: false,
        source_loop_length_ticks: 3_840,
        notes: vec![take_a_note],
        controllers: Vec::new(),
        pitch_bends: Vec::new(),
        program_changes: Vec::new(),
        poly_aftertouches: Vec::new(),
        channel_pressures: Vec::new(),
        opaque_events: Vec::new(),
        event_order: Vec::new(),
    }];
    let mut project = Project::from_snapshot(snapshot).unwrap();
    let midi_item = &project.midi_items()[0];
    let active_metadata = midi_item.take_properties(1).unwrap();
    assert_eq!(active_metadata.name, "Take B");
    assert_eq!(active_metadata.source_offset_ticks, 120);
    assert_eq!(active_metadata.event_end_ticks, 1_440);
    let inactive_metadata = midi_item.take_properties(0).unwrap();
    assert_eq!(inactive_metadata.name, "Take A");
    assert_eq!(inactive_metadata.event_end_ticks, 480);

    let before = project.snapshot();
    let cursor = project.history_cursor();

    project
        .apply(DawAction::SetMidiItemActiveTake {
            item_id,
            take_index: 0,
        })
        .unwrap();

    assert_eq!(project.history_cursor(), cursor + 1);
    let active = &project.midi_items()[0];
    assert_eq!(active.active_take_index(), 0);
    assert_eq!(active.name(), "Take A");
    assert_eq!(active.source_offset_ticks(), 0);
    assert_eq!(active.notes()[0].pitch(), 60);
    let takes = active.take_snapshots();
    assert_eq!(takes[1].index, 1);
    assert_eq!(takes[1].name, "Take B");
    assert_eq!(takes[1].source_offset_ticks, 120);
    assert_eq!(takes[1].notes[0].data.pitch, 64);
    let after = project.snapshot();

    project.undo().unwrap();
    assert_eq!(project.snapshot(), before);
    project.redo().unwrap();
    assert_eq!(project.snapshot(), after);
}

#[test]
fn exploding_selected_midi_takes_preserves_take_order_notes_and_undo_history() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "MIDI takes".to_owned(),
        })
        .expect("track should be created");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3_840,
        })
        .expect("MIDI item should be inserted");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    channel: 0,
                    tick: 0,
                    duration: 480,
                    velocity: 100,
                    off_velocity: 0,
                },
                MidiNoteData {
                    pitch: 64,
                    channel: 0,
                    tick: 960,
                    duration: 480,
                    velocity: 100,
                    off_velocity: 0,
                },
            ],
        })
        .expect("notes should be inserted");

    let mut snapshot = project.snapshot();
    let item = &mut snapshot.midi_items[0];
    let take_a_note = item.notes[0].clone();
    item.notes = vec![item.notes[1].clone()];
    item.name = "MIDI take B".to_owned();
    item.active_take_index = 1;
    item.other_takes = vec![MidiItemTakeSnapshot {
        index: 0,
        name: "MIDI take A".to_owned(),
        source_offset_ticks: 0,
        loop_source: false,
        source_loop_length_ticks: 3_840,
        notes: vec![MidiNoteSnapshot {
            id: take_a_note.id,
            data: take_a_note.data,
        }],
        controllers: Vec::new(),
        pitch_bends: Vec::new(),
        program_changes: Vec::new(),
        poly_aftertouches: Vec::new(),
        channel_pressures: Vec::new(),
        opaque_events: Vec::new(),
        event_order: Vec::new(),
    }];
    let mut project = Project::from_snapshot(snapshot).expect("two MIDI takes should restore");
    let before = project.snapshot();
    let cursor = project.history_cursor();

    project
        .apply(DawAction::ExplodeMidiItemTakesInPlace {
            tx_id: 77,
            item_ids: vec![item_id],
        })
        .expect("MIDI take explosion should apply");

    assert_eq!(project.history_cursor(), cursor + 1);
    assert_eq!(project.midi_items().len(), 2);
    let exploded = project.midi_items();
    assert_eq!(
        exploded.iter().map(|item| item.name()).collect::<Vec<_>>(),
        ["MIDI take A", "MIDI take B"]
    );
    assert_eq!(
        exploded
            .iter()
            .map(|item| item.notes()[0].pitch())
            .collect::<Vec<_>>(),
        [60, 64]
    );
    assert!(exploded.iter().all(|item| {
        item.track_id() == track_id
            && item.start_tick() == 0
            && item.length_ticks() == 3_840
            && item.take_count() == 1
            && item.active_take_index() == 0
    }));
    let after = project.snapshot();

    project
        .undo()
        .expect("one Undo should restore the source take list");
    assert_eq!(project.snapshot(), before);
    project
        .redo()
        .expect("one Redo should split both MIDI takes again");
    assert_eq!(project.snapshot(), after);
}

#[test]
fn splitting_multi_take_midi_partitions_each_take_and_undoes_as_one_action() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "MIDI takes".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
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
                    channel: 0,
                    tick: 960,
                    duration: 1_920,
                    velocity: 100,
                    off_velocity: 37,
                },
                MidiNoteData {
                    pitch: 67,
                    channel: 0,
                    tick: 3_000,
                    duration: 480,
                    velocity: 90,
                    off_velocity: 0,
                },
                MidiNoteData {
                    pitch: 64,
                    channel: 0,
                    tick: 480,
                    duration: 960,
                    velocity: 92,
                    off_velocity: 41,
                },
                MidiNoteData {
                    pitch: 72,
                    channel: 0,
                    tick: 3_360,
                    duration: 360,
                    velocity: 88,
                    off_velocity: 0,
                },
            ],
        })
        .unwrap();
    let mut snapshot = project.snapshot();
    let item = &mut snapshot.midi_items[0];
    let notes = item.notes.clone();
    item.notes = vec![notes[2].clone(), notes[3].clone()];
    item.event_order = vec![
        MidiEventOrderData {
            target: MidiEventOrderTarget::Note(notes[2].data),
            order: 4,
            note_off_order: Some(5),
        },
        MidiEventOrderData {
            target: MidiEventOrderTarget::Note(notes[3].data),
            order: 6,
            note_off_order: Some(7),
        },
    ];
    item.active_take_index = 1;
    item.name = "Take B".into();
    item.source_offset_ticks = 960;
    item.loop_source = false;
    item.other_takes = vec![MidiItemTakeSnapshot {
        index: 0,
        name: "Take A".into(),
        source_offset_ticks: 0,
        loop_source: false,
        source_loop_length_ticks: item.length_ticks,
        notes: vec![notes[0].clone(), notes[1].clone()],
        controllers: Vec::new(),
        pitch_bends: Vec::new(),
        program_changes: Vec::new(),
        poly_aftertouches: Vec::new(),
        channel_pressures: Vec::new(),
        opaque_events: Vec::new(),
        event_order: vec![
            MidiEventOrderData {
                target: MidiEventOrderTarget::Note(notes[0].data),
                order: 0,
                note_off_order: Some(1),
            },
            MidiEventOrderData {
                target: MidiEventOrderTarget::Note(notes[1].data),
                order: 2,
                note_off_order: Some(3),
            },
        ],
    }];
    let mut project = Project::from_snapshot(snapshot).unwrap();
    let before = project.snapshot();
    let history_cursor = project.history_cursor();

    project
        .apply(DawAction::SplitMidiItem {
            item_id,
            split_ticks: vec![1_920],
        })
        .expect("every MIDI take should split at the project boundary");

    assert_eq!(project.history_cursor(), history_cursor + 1);
    let items = project.midi_items();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id(), item_id);
    assert_eq!(
        items
            .iter()
            .map(|item| (item.start_tick(), item.length_ticks()))
            .collect::<Vec<_>>(),
        [(0, 1_920), (1_920, 1_920)]
    );
    assert!(items.iter().all(|item| {
        item.take_count() == 2 && item.active_take_index() == 1 && item.name() == "Take B"
    }));
    let left_takes = items[0].take_snapshots();
    assert_eq!(left_takes[0].index, 0);
    assert_eq!(left_takes[0].name, "Take A");
    assert_eq!(left_takes[0].notes.len(), 1);
    assert_eq!(left_takes[0].notes[0].data.pitch, 60);
    assert_eq!(
        (
            left_takes[0].notes[0].data.tick,
            left_takes[0].notes[0].data.duration
        ),
        (960, 960)
    );
    assert_eq!(
        left_takes[0]
            .event_order
            .iter()
            .filter_map(|event| match event.target {
                MidiEventOrderTarget::Note(note) => Some(note),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [left_takes[0].notes[0].data]
    );
    assert_eq!(left_takes[1].index, 1);
    assert_eq!(left_takes[1].source_offset_ticks, 960);
    assert_eq!(left_takes[1].notes.len(), 1);
    assert_eq!(left_takes[1].notes[0].data.pitch, 64);

    let right_takes = items[1].take_snapshots();
    assert_eq!(right_takes[0].notes.len(), 2);
    assert_eq!(
        right_takes[0]
            .notes
            .iter()
            .map(|note| (note.data.pitch, note.data.tick, note.data.duration))
            .collect::<Vec<_>>(),
        [(60, 0, 960), (67, 1_080, 480)]
    );
    assert_eq!(
        right_takes[0]
            .event_order
            .iter()
            .filter_map(|event| match event.target {
                MidiEventOrderTarget::Note(note) => Some(note),
                _ => None,
            })
            .collect::<Vec<_>>(),
        right_takes[0]
            .notes
            .iter()
            .map(|note| note.data)
            .collect::<Vec<_>>()
    );
    assert_eq!(right_takes[1].source_offset_ticks, 0);
    assert_eq!(right_takes[1].notes.len(), 1);
    assert_eq!(
        (
            right_takes[1].notes[0].data.pitch,
            right_takes[1].notes[0].data.tick,
            right_takes[1].notes[0].data.duration
        ),
        (72, 480, 360)
    );
    assert_eq!(
        right_takes[1].event_order[0].target,
        MidiEventOrderTarget::Note(right_takes[1].notes[0].data)
    );

    let after = project.snapshot();
    project.undo().unwrap();
    assert_eq!(project.snapshot(), before);
    project.redo().unwrap();
    assert_eq!(project.snapshot(), after);
}

#[test]
fn splitting_looped_midi_take_wraps_its_source_phase_per_take() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Looped MIDI takes".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
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
                    channel: 0,
                    tick: 960,
                    duration: 1_920,
                    velocity: 100,
                    off_velocity: 37,
                },
                MidiNoteData {
                    pitch: 67,
                    channel: 0,
                    tick: 3_000,
                    duration: 480,
                    velocity: 90,
                    off_velocity: 0,
                },
                MidiNoteData {
                    pitch: 62,
                    channel: 0,
                    tick: 480,
                    duration: 240,
                    velocity: 84,
                    off_velocity: 0,
                },
                MidiNoteData {
                    pitch: 64,
                    channel: 0,
                    tick: 1_200,
                    duration: 240,
                    velocity: 92,
                    off_velocity: 41,
                },
                MidiNoteData {
                    pitch: 72,
                    channel: 0,
                    tick: 3_360,
                    duration: 360,
                    velocity: 88,
                    off_velocity: 0,
                },
            ],
        })
        .unwrap();

    let mut snapshot = project.snapshot();
    let item = &mut snapshot.midi_items[0];
    let notes = item.notes.clone();
    item.notes = notes[2..].to_vec();
    item.active_take_index = 1;
    item.name = "Take B".into();
    item.source_offset_ticks = 2_880;
    item.loop_source = true;
    item.other_takes = vec![MidiItemTakeSnapshot {
        index: 0,
        name: "Take A".into(),
        source_offset_ticks: 0,
        loop_source: true,
        source_loop_length_ticks: item.length_ticks,
        notes: notes[..2].to_vec(),
        controllers: Vec::new(),
        pitch_bends: Vec::new(),
        program_changes: Vec::new(),
        poly_aftertouches: Vec::new(),
        channel_pressures: Vec::new(),
        opaque_events: Vec::new(),
        event_order: Vec::new(),
    }];
    let mut project = Project::from_snapshot(snapshot).unwrap();
    let before = project.snapshot();

    project
        .apply(DawAction::SplitMidiItem {
            item_id,
            split_ticks: vec![1_920],
        })
        .expect("looped MIDI takes should split at their wrapped source phase");

    let items = project.midi_items();
    assert_eq!(items.len(), 2);
    let left_takes = items[0].take_snapshots();
    assert_eq!(left_takes[1].source_offset_ticks, 2_880);
    assert_eq!(
        left_takes[1]
            .notes
            .iter()
            .map(|note| note.data.pitch)
            .collect::<Vec<_>>(),
        [62, 64, 72]
    );
    let right_takes = items[1].take_snapshots();
    assert_eq!(right_takes[0].notes.len(), 2);
    assert_eq!(
        right_takes[0]
            .notes
            .iter()
            .map(|note| (note.data.pitch, note.data.tick, note.data.duration))
            .collect::<Vec<_>>(),
        [(60, 0, 960), (67, 1_080, 480)]
    );
    assert_eq!(right_takes[1].source_offset_ticks, 0);
    assert_eq!(
        right_takes[1]
            .notes
            .iter()
            .map(|note| (note.data.pitch, note.data.tick, note.data.duration))
            .collect::<Vec<_>>(),
        [(64, 240, 240), (72, 2_400, 360)]
    );
    let after = project.snapshot();
    project.undo().unwrap();
    assert_eq!(project.snapshot(), before);
    project.redo().unwrap();
    assert_eq!(project.snapshot(), after);
}
