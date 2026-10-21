use aaadaw_core::{ActionError, DawAction, MidiProgramChangeData, Project};

fn project_with_midi_item() -> (Project, aaadaw_core::ItemId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Program changes".to_owned(),
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
fn program_changes_are_sorted_undoable_and_snapshot_safe() {
    let (mut project, item_id) = project_with_midi_item();
    let events = vec![
        MidiProgramChangeData {
            channel: 3,
            tick: 960,
            program: 41,
        },
        MidiProgramChangeData {
            channel: 0,
            tick: 0,
            program: 12,
        },
        MidiProgramChangeData {
            channel: 3,
            tick: 0,
            program: 8,
        },
    ];
    project
        .apply(DawAction::SetMidiProgramChanges {
            item_id,
            program_changes: events,
        })
        .expect("valid program changes should be accepted");
    let expected = [
        MidiProgramChangeData {
            channel: 0,
            tick: 0,
            program: 12,
        },
        MidiProgramChangeData {
            channel: 3,
            tick: 0,
            program: 8,
        },
        MidiProgramChangeData {
            channel: 3,
            tick: 960,
            program: 41,
        },
    ];
    assert_eq!(project.midi_items()[0].program_changes(), expected);
    let snapshot = project.snapshot();
    assert_eq!(
        Project::from_snapshot(snapshot.clone())
            .expect("program-change snapshot should restore")
            .snapshot(),
        snapshot
    );
    project.undo().expect("program-change edit should undo");
    assert!(project.midi_items()[0].program_changes().is_empty());
    project.redo().expect("program-change edit should redo");
    assert_eq!(project.snapshot(), snapshot);
}

#[test]
fn program_changes_reject_out_of_range_values_and_duplicate_positions() {
    let (mut project, item_id) = project_with_midi_item();
    for program_changes in [
        vec![
            MidiProgramChangeData {
                channel: 0,
                tick: 10,
                program: 1,
            },
            MidiProgramChangeData {
                channel: 0,
                tick: 10,
                program: 2,
            },
        ],
        vec![MidiProgramChangeData {
            channel: 16,
            tick: 10,
            program: 1,
        }],
        vec![MidiProgramChangeData {
            channel: 0,
            tick: 10,
            program: 128,
        }],
    ] {
        let before = project.snapshot();
        assert_eq!(
            project.apply(DawAction::SetMidiProgramChanges {
                item_id,
                program_changes,
            }),
            Err(ActionError::InvalidMidiProgramChange)
        );
        assert_eq!(project.snapshot(), before);
    }
}

#[test]
fn splitting_a_midi_item_carries_the_active_program_into_the_new_piece() {
    let (mut project, item_id) = project_with_midi_item();
    project
        .apply(DawAction::SetMidiProgramChanges {
            item_id,
            program_changes: vec![
                MidiProgramChangeData {
                    channel: 2,
                    tick: 0,
                    program: 5,
                },
                MidiProgramChangeData {
                    channel: 2,
                    tick: 960,
                    program: 63,
                },
            ],
        })
        .expect("program changes should be accepted");
    project
        .apply(DawAction::SplitMidiItem {
            item_id,
            split_ticks: vec![480],
        })
        .expect("item should split");
    assert_eq!(
        project.midi_items()[1].program_changes(),
        &[
            MidiProgramChangeData {
                channel: 2,
                tick: 0,
                program: 5,
            },
            MidiProgramChangeData {
                channel: 2,
                tick: 480,
                program: 63,
            },
        ]
    );
    let split_snapshot = project.snapshot();
    project.undo().expect("split should undo");
    project.redo().expect("split should redo");
    assert_eq!(project.snapshot(), split_snapshot);
}
