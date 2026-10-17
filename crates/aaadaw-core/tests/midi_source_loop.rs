use aaadaw_core::{
    ActionError, DawAction, GridFraction, MidiControllerData, MidiItemPosition, MidiNoteData,
    MidiPitchBendData, Project,
};

#[test]
fn midi_source_loop_length_and_offset_map_events_across_the_visible_clip() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Looped MIDI".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id: project.tracks()[0].id(),
            start_tick: 100,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 120,
                duration: 90,
                velocity: 100,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 100,
            length_ticks: 2_200,
        })
        .unwrap();
    project
        .apply(DawAction::EditMidiItemPlacement {
            item_id,
            start_tick: 100,
            length_ticks: 2_200,
            source_offset_ticks: 720,
        })
        .unwrap();

    let item = &project.midi_items()[0];
    assert!(item.is_loop_source());
    assert_eq!(item.source_loop_length_ticks(), 960);
    assert_eq!(item.source_tick_at_project_tick(100), Some(720));
    assert_eq!(item.source_tick_at_project_tick(460), Some(120));
    assert_eq!(
        item.project_ticks_at_content_tick(120).collect::<Vec<_>>(),
        [460, 1_420]
    );
}

#[test]
fn fractional_midi_item_positions_survive_moves_and_event_mapping() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Fractional MIDI".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id: project.tracks()[0].id(),
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 120,
                velocity: 100,
            }],
        })
        .unwrap();
    let position = MidiItemPosition::new(964, 160).unwrap();
    project
        .apply(DawAction::SetMidiItemPosition { item_id, position })
        .unwrap();

    let item = &project.midi_items()[0];
    assert_eq!(item.start_tick(), 964);
    assert_eq!(item.start_position(), position);
    assert_eq!(
        item.project_position_at_content_tick(0),
        Some(MidiItemPosition::new(964, 160).unwrap())
    );
    assert_eq!(
        item.project_position_at_content_tick(1),
        Some(MidiItemPosition::new(965, 160).unwrap())
    );

    project.undo().unwrap();
    assert_eq!(
        project.midi_items()[0].start_position(),
        MidiItemPosition::default()
    );
    project.redo().unwrap();
    assert_eq!(project.midi_items()[0].start_position(), position);
    project
        .apply(DawAction::DuplicateMidiItem { item_id })
        .unwrap();
    assert_eq!(
        project.midi_items()[1].start_position(),
        position.checked_add_ticks(960).unwrap()
    );
}

#[test]
fn loop_source_edits_use_source_bounds_after_the_item_offset_wraps() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Looped MIDI".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id: project.tracks()[0].id(),
            start_tick: 100,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 120,
                duration: 90,
                velocity: 100,
            }],
        })
        .unwrap();
    let note_id = project.midi_items()[0].notes()[0].id();
    project
        .apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 100,
            length_ticks: 2_200,
        })
        .unwrap();
    project
        .apply(DawAction::EditMidiItemPlacement {
            item_id,
            start_tick: 100,
            length_ticks: 2_200,
            source_offset_ticks: 720,
        })
        .unwrap();

    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 64,
                tick: 900,
                duration: 60,
                velocity: 96,
            }],
        })
        .expect("loop source note insertion should use loop-relative bounds");
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                channel: 0,
                controller: 1,
                tick: 120,
                value: 64,
            }],
        })
        .expect("loop source controllers should use loop-relative ticks");
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                channel: 0,
                tick: 360,
                value: 12_000,
            }],
        })
        .expect("loop source pitch bends should use loop-relative ticks");

    project
        .apply(DawAction::QuantizeItem {
            item_id,
            grid: GridFraction::new(1, 16).unwrap(),
            strength: 1.0,
        })
        .expect("quantization should convert back into the loop source clock");
    assert_eq!(
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| note.tick())
            .collect::<Vec<_>>(),
        [140, 860]
    );
    assert!(project.undo().unwrap());
    assert_eq!(project.midi_items()[0].notes()[0].tick(), 120);
    assert!(project.redo().unwrap());
    assert_eq!(project.midi_items()[0].notes()[0].tick(), 140);

    project
        .apply(DawAction::EditMidiNote {
            item_id,
            note_id,
            data: MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 480,
                duration: 120,
                velocity: 100,
            },
        })
        .expect("loop source note movement should use loop-relative bounds");
    let before_invalid_resize = project.snapshot();
    assert_eq!(
        project.apply(DawAction::EditMidiNote {
            item_id,
            note_id,
            data: MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 930,
                duration: 60,
                velocity: 100,
            },
        }),
        Err(ActionError::InvalidMidiNote)
    );
    assert_eq!(project.snapshot(), before_invalid_resize);
}
