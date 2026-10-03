use aaadaw_core::{ActionError, DawAction, MidiControllerData, Project};

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
            length_ticks: 3840,
        })
        .expect("MIDI item should be inserted");
    let item_id = project.midi_items()[0].id();
    (project, item_id)
}

#[test]
fn controller_changes_are_sorted_undoable_and_snapshot_safe() {
    let (mut project, item_id) = project_with_midi_item();
    let controllers = vec![
        MidiControllerData {
            controller: 64,
            tick: 960,
            value: 0,
        },
        MidiControllerData {
            controller: 1,
            tick: 480,
            value: 64,
        },
        MidiControllerData {
            controller: 64,
            tick: 0,
            value: 127,
        },
    ];
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: controllers.clone(),
        })
        .expect("valid controller changes should be accepted");

    assert_eq!(
        project.midi_items()[0].controllers(),
        &[controllers[2], controllers[1], controllers[0],]
    );
    let snapshot = project.snapshot();
    assert_eq!(
        Project::from_snapshot(snapshot.clone())
            .expect("controller snapshot should restore")
            .snapshot(),
        snapshot
    );

    project.undo().expect("controller edit should undo");
    assert!(project.midi_items()[0].controllers().is_empty());
    project.redo().expect("controller edit should redo");
    assert_eq!(project.snapshot(), snapshot);
}

#[test]
fn controller_changes_reject_duplicate_positions_and_out_of_range_data_atomically() {
    let (mut project, item_id) = project_with_midi_item();
    for controllers in [
        vec![
            MidiControllerData {
                controller: 64,
                tick: 10,
                value: 0,
            },
            MidiControllerData {
                controller: 64,
                tick: 10,
                value: 127,
            },
        ],
        vec![MidiControllerData {
            controller: 128,
            tick: 10,
            value: 0,
        }],
        vec![MidiControllerData {
            controller: 64,
            tick: 3840,
            value: 127,
        }],
    ] {
        let before = project.snapshot();
        assert_eq!(
            project.apply(DawAction::SetMidiControllers {
                item_id,
                controllers,
            }),
            Err(ActionError::InvalidMidiController)
        );
        assert_eq!(project.snapshot(), before);
    }
}

#[test]
fn splitting_a_midi_item_carries_the_active_controller_state_forward() {
    let (mut project, item_id) = project_with_midi_item();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                MidiControllerData {
                    controller: 64,
                    tick: 0,
                    value: 127,
                },
                MidiControllerData {
                    controller: 64,
                    tick: 960,
                    value: 0,
                },
            ],
        })
        .expect("controller changes should be accepted");
    project
        .apply(DawAction::SplitMidiItem {
            item_id,
            split_ticks: vec![480],
        })
        .expect("item should split");
    assert_eq!(
        project.midi_items()[1].controllers(),
        &[
            MidiControllerData {
                controller: 64,
                tick: 0,
                value: 127,
            },
            MidiControllerData {
                controller: 64,
                tick: 480,
                value: 0,
            },
        ]
    );
    let split_snapshot = project.snapshot();
    project.undo().expect("split should undo");
    assert_eq!(project.midi_items()[0].controllers().len(), 2);
    project.redo().expect("split should redo");
    assert_eq!(project.snapshot(), split_snapshot);
}
