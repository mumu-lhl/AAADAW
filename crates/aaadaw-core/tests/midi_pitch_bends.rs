use aaadaw_core::{ActionError, DawAction, MidiPitchBendData, Project};

fn project_with_midi_item() -> (Project, aaadaw_core::ItemId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Keys".to_owned(),
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
fn pitch_bends_are_14_bit_sorted_undoable_and_snapshot_safe() {
    let (mut project, item_id) = project_with_midi_item();
    let pitch_bends = vec![
        MidiPitchBendData {
            tick: 960,
            value: 16_383,
        },
        MidiPitchBendData {
            tick: 0,
            value: 8192,
        },
        MidiPitchBendData {
            tick: 480,
            value: 0,
        },
    ];
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends,
        })
        .expect("valid pitch bends should be accepted");
    let expected = [
        MidiPitchBendData {
            tick: 0,
            value: 8192,
        },
        MidiPitchBendData {
            tick: 480,
            value: 0,
        },
        MidiPitchBendData {
            tick: 960,
            value: 16_383,
        },
    ];
    assert_eq!(project.midi_items()[0].pitch_bends(), expected);
    let snapshot = project.snapshot();
    assert_eq!(
        Project::from_snapshot(snapshot.clone())
            .expect("pitch-bend snapshot should restore")
            .snapshot(),
        snapshot
    );
    project.undo().expect("pitch-bend edit should undo");
    assert!(project.midi_items()[0].pitch_bends().is_empty());
    project.redo().expect("pitch-bend edit should redo");
    assert_eq!(project.snapshot(), snapshot);
}

#[test]
fn pitch_bends_reject_invalid_values_positions_and_duplicates_atomically() {
    let (mut project, item_id) = project_with_midi_item();
    for pitch_bends in [
        vec![
            MidiPitchBendData { tick: 10, value: 0 },
            MidiPitchBendData {
                tick: 10,
                value: 16_383,
            },
        ],
        vec![MidiPitchBendData {
            tick: 10,
            value: 16_384,
        }],
        vec![MidiPitchBendData {
            tick: 3_840,
            value: 8192,
        }],
    ] {
        let before = project.snapshot();
        assert_eq!(
            project.apply(DawAction::SetMidiPitchBends {
                item_id,
                pitch_bends,
            }),
            Err(ActionError::InvalidMidiPitchBend)
        );
        assert_eq!(project.snapshot(), before);
    }
}

#[test]
fn splitting_a_midi_item_carries_the_active_pitch_bend_forward() {
    let (mut project, item_id) = project_with_midi_item();
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![
                MidiPitchBendData {
                    tick: 0,
                    value: 8192,
                },
                MidiPitchBendData {
                    tick: 960,
                    value: 12_000,
                },
            ],
        })
        .expect("pitch bends should be accepted");
    project
        .apply(DawAction::SplitMidiItem {
            item_id,
            split_ticks: vec![480],
        })
        .expect("item should split");
    assert_eq!(
        project.midi_items()[1].pitch_bends(),
        &[
            MidiPitchBendData {
                tick: 0,
                value: 8192
            },
            MidiPitchBendData {
                tick: 480,
                value: 12_000
            },
        ]
    );
    let split_snapshot = project.snapshot();
    project.undo().expect("split should undo");
    assert_eq!(project.midi_items()[0].pitch_bends().len(), 2);
    project.redo().expect("split should redo");
    assert_eq!(project.snapshot(), split_snapshot);
}

#[test]
fn shrinking_a_midi_item_cannot_discard_pitch_bend_points() {
    let (mut project, item_id) = project_with_midi_item();
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                tick: 2_400,
                value: 10_000,
            }],
        })
        .expect("pitch bend should be accepted");
    let before = project.snapshot();
    assert_eq!(
        project.apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 0,
            length_ticks: 1_920,
        }),
        Err(ActionError::InvalidMidiPitchBend)
    );
    assert_eq!(project.snapshot(), before);
}
