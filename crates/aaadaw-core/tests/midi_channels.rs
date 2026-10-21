use aaadaw_core::{
    ActionError, DawAction, MidiControllerData, MidiNoteData, MidiPitchBendData, Project,
};

fn midi_project() -> (Project, aaadaw_core::ItemId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Channels".to_owned(),
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
    (project, item_id)
}

#[test]
fn events_can_share_positions_on_distinct_channels_and_reject_invalid_channels() {
    let (mut project, item_id) = midi_project();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    channel: 1,
                    tick: 0,
                    duration: 240,
                    velocity: 100,
                    off_velocity: 0,
                },
                MidiNoteData {
                    pitch: 60,
                    channel: 15,
                    tick: 0,
                    duration: 240,
                    velocity: 100,
                    off_velocity: 0,
                },
            ],
        })
        .unwrap();
    assert_eq!(
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| note.channel())
            .collect::<Vec<_>>(),
        [1, 15]
    );

    let invalid_note = project.apply(DawAction::AddMidiNotes {
        item_id,
        notes: vec![MidiNoteData {
            pitch: 60,
            channel: 16,
            tick: 0,
            duration: 240,
            velocity: 100,
            off_velocity: 0,
        }],
    });
    assert!(matches!(invalid_note, Err(ActionError::InvalidMidiNote)));

    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                MidiControllerData {
                    controller: 7,
                    channel: 2,
                    tick: 120,
                    value: 80,
                },
                MidiControllerData {
                    controller: 7,
                    channel: 9,
                    tick: 120,
                    value: 100,
                },
            ],
        })
        .unwrap();
    assert_eq!(project.midi_items()[0].controllers().len(), 2);

    let duplicate_controller = project.apply(DawAction::SetMidiControllers {
        item_id,
        controllers: vec![
            MidiControllerData {
                controller: 7,
                channel: 2,
                tick: 120,
                value: 80,
            },
            MidiControllerData {
                controller: 7,
                channel: 2,
                tick: 120,
                value: 100,
            },
        ],
    });
    assert!(matches!(
        duplicate_controller,
        Err(ActionError::InvalidMidiController)
    ));

    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![
                MidiPitchBendData {
                    channel: 3,
                    tick: 120,
                    value: 8_000,
                },
                MidiPitchBendData {
                    channel: 12,
                    tick: 120,
                    value: 9_000,
                },
            ],
        })
        .unwrap();
    assert_eq!(project.midi_items()[0].pitch_bends().len(), 2);

    let invalid_bend = project.apply(DawAction::SetMidiPitchBends {
        item_id,
        pitch_bends: vec![MidiPitchBendData {
            channel: 16,
            tick: 240,
            value: 8_192,
        }],
    });
    assert!(matches!(
        invalid_bend,
        Err(ActionError::InvalidMidiPitchBend)
    ));

    let restored = Project::from_snapshot(project.snapshot()).unwrap();
    assert_eq!(restored.snapshot(), project.snapshot());
}
