use aaadaw_core::{
    ActionError, DawAction, MidiItemTakeSnapshot, MidiNoteData, MidiNoteSnapshot, Project,
};

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
fn splitting_multi_take_midi_is_rejected_without_changing_any_take_data() {
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
    let mut snapshot = project.snapshot();
    let item = &mut snapshot.midi_items[0];
    item.active_take_index = 1;
    item.name = "Take B".into();
    item.other_takes = vec![MidiItemTakeSnapshot {
        index: 0,
        name: "Take A".into(),
        source_offset_ticks: 0,
        loop_source: false,
        source_loop_length_ticks: item.length_ticks,
        notes: Vec::new(),
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
    let history_cursor = project.history_cursor();

    assert_eq!(
        project.apply(DawAction::SplitMidiItem {
            item_id,
            split_ticks: vec![1_920],
        }),
        Err(ActionError::MidiItemHasMultipleTakes)
    );
    assert_eq!(project.snapshot(), before);
    assert_eq!(project.history_cursor(), history_cursor);
}
