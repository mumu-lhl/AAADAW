use aaadaw_core::{
    ActionError, DawAction, MidiChannelPressureData, MidiPolyAftertouchData, Project,
};

fn project_with_midi_item() -> (Project, aaadaw_core::ItemId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Aftertouch".to_owned(),
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
    (project, item_id)
}

#[test]
fn aftertouch_events_are_sorted_undoable_and_snapshot_safe() {
    let (mut project, item_id) = project_with_midi_item();
    project
        .apply(DawAction::SetMidiPolyAftertouches {
            item_id,
            poly_aftertouches: vec![
                MidiPolyAftertouchData {
                    channel: 2,
                    tick: 960,
                    note: 60,
                    value: 37,
                },
                MidiPolyAftertouchData {
                    channel: 2,
                    tick: 960,
                    note: 60,
                    value: 41,
                },
                MidiPolyAftertouchData {
                    channel: 0,
                    tick: 0,
                    note: 64,
                    value: 0,
                },
                MidiPolyAftertouchData {
                    channel: 2,
                    tick: 0,
                    note: 62,
                    value: 12,
                },
            ],
        })
        .expect("valid Poly Aftertouch events should be accepted");
    project
        .apply(DawAction::SetMidiChannelPressures {
            item_id,
            channel_pressures: vec![
                MidiChannelPressureData {
                    channel: 2,
                    tick: 960,
                    value: 74,
                },
                MidiChannelPressureData {
                    channel: 2,
                    tick: 0,
                    value: 5,
                },
            ],
        })
        .expect("valid Channel Pressure events should be accepted");

    assert_eq!(
        project.midi_items()[0].poly_aftertouches(),
        [
            MidiPolyAftertouchData {
                channel: 0,
                tick: 0,
                note: 64,
                value: 0,
            },
            MidiPolyAftertouchData {
                channel: 2,
                tick: 0,
                note: 62,
                value: 12,
            },
            MidiPolyAftertouchData {
                channel: 2,
                tick: 960,
                note: 60,
                value: 37,
            },
            MidiPolyAftertouchData {
                channel: 2,
                tick: 960,
                note: 60,
                value: 41,
            },
        ]
    );
    assert_eq!(
        project.midi_items()[0].channel_pressures(),
        [
            MidiChannelPressureData {
                channel: 2,
                tick: 0,
                value: 5,
            },
            MidiChannelPressureData {
                channel: 2,
                tick: 960,
                value: 74,
            },
        ]
    );

    let snapshot = project.snapshot();
    assert_eq!(
        Project::from_snapshot(snapshot.clone())
            .expect("aftertouch snapshot should restore")
            .snapshot(),
        snapshot
    );
    project.undo().expect("Channel Pressure should undo");
    assert!(project.midi_items()[0].channel_pressures().is_empty());
    project.redo().expect("Channel Pressure should redo");
    assert_eq!(project.snapshot(), snapshot);
}

#[test]
fn aftertouch_events_allow_same_position_poly_values_but_reject_out_of_range_data() {
    let (mut project, item_id) = project_with_midi_item();
    let invalid_poly = [
        vec![MidiPolyAftertouchData {
            channel: 16,
            tick: 10,
            note: 60,
            value: 12,
        }],
        vec![MidiPolyAftertouchData {
            channel: 1,
            tick: 10,
            note: 60,
            value: 128,
        }],
    ];
    for poly_aftertouches in invalid_poly {
        assert_eq!(
            project.apply(DawAction::SetMidiPolyAftertouches {
                item_id,
                poly_aftertouches,
            }),
            Err(ActionError::InvalidMidiPolyAftertouch)
        );
    }

    let invalid_channel = [
        vec![
            MidiChannelPressureData {
                channel: 1,
                tick: 10,
                value: 12,
            },
            MidiChannelPressureData {
                channel: 1,
                tick: 10,
                value: 14,
            },
        ],
        vec![MidiChannelPressureData {
            channel: 16,
            tick: 10,
            value: 12,
        }],
        vec![MidiChannelPressureData {
            channel: 1,
            tick: 10,
            value: 128,
        }],
    ];
    for channel_pressures in invalid_channel {
        assert_eq!(
            project.apply(DawAction::SetMidiChannelPressures {
                item_id,
                channel_pressures,
            }),
            Err(ActionError::InvalidMidiChannelPressure)
        );
    }
}

#[test]
fn splitting_carries_active_polyphonic_and_channel_pressure_into_the_new_item() {
    let (mut project, item_id) = project_with_midi_item();
    project
        .apply(DawAction::SetMidiPolyAftertouches {
            item_id,
            poly_aftertouches: vec![
                MidiPolyAftertouchData {
                    channel: 2,
                    tick: 0,
                    note: 60,
                    value: 12,
                },
                MidiPolyAftertouchData {
                    channel: 2,
                    tick: 960,
                    note: 60,
                    value: 37,
                },
            ],
        })
        .expect("Poly Aftertouch events should be accepted");
    project
        .apply(DawAction::SetMidiChannelPressures {
            item_id,
            channel_pressures: vec![
                MidiChannelPressureData {
                    channel: 2,
                    tick: 0,
                    value: 5,
                },
                MidiChannelPressureData {
                    channel: 2,
                    tick: 960,
                    value: 74,
                },
            ],
        })
        .expect("Channel Pressure events should be accepted");
    project
        .apply(DawAction::SplitMidiItem {
            item_id,
            split_ticks: vec![480],
        })
        .expect("MIDI item should split");
    let second = &project.midi_items()[1];
    assert_eq!(
        second.poly_aftertouches(),
        [
            MidiPolyAftertouchData {
                channel: 2,
                tick: 0,
                note: 60,
                value: 12,
            },
            MidiPolyAftertouchData {
                channel: 2,
                tick: 480,
                note: 60,
                value: 37,
            },
        ]
    );
    assert_eq!(
        second.channel_pressures(),
        [
            MidiChannelPressureData {
                channel: 2,
                tick: 0,
                value: 5,
            },
            MidiChannelPressureData {
                channel: 2,
                tick: 480,
                value: 74,
            },
        ]
    );
}
