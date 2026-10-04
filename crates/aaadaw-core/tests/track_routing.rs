use aaadaw_core::{ActionError, DawAction, Project};

#[test]
fn bus_track_routes_are_undoable_and_survive_snapshot_round_trip() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Bass".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::CreateBusTrack {
            index: 1,
            name: "Low End".to_owned(),
        })
        .unwrap();
    let source = project.tracks()[0].id();
    let bus = project.tracks()[1].id();
    assert!(project.tracks()[1].is_bus());
    assert_eq!(project.tracks()[0].output_track(), None);

    project
        .apply(DawAction::SetTrackOutput {
            track_id: source,
            output_track: Some(bus),
        })
        .unwrap();
    assert_eq!(project.tracks()[0].output_track(), Some(bus));
    project.undo().unwrap();
    assert_eq!(project.tracks()[0].output_track(), None);
    project.redo().unwrap();
    assert_eq!(project.tracks()[0].output_track(), Some(bus));

    let restored = Project::from_snapshot(project.snapshot()).unwrap();
    assert!(restored.tracks()[1].is_bus());
    assert_eq!(restored.tracks()[0].output_track(), Some(bus));
}

#[test]
fn routing_rejects_non_bus_targets_cycles_and_bus_deletion_with_dependents() {
    let mut project = Project::new();
    for (index, name) in ["Source", "Bus A", "Bus B"].into_iter().enumerate() {
        let action = if index == 0 {
            DawAction::CreateTrack {
                index,
                name: name.to_owned(),
            }
        } else {
            DawAction::CreateBusTrack {
                index,
                name: name.to_owned(),
            }
        };
        project.apply(action).unwrap();
    }
    let source = project.tracks()[0].id();
    let bus_a = project.tracks()[1].id();
    let bus_b = project.tracks()[2].id();

    assert_eq!(
        project.apply(DawAction::SetTrackOutput {
            track_id: bus_a,
            output_track: Some(source)
        }),
        Err(ActionError::InvalidTrackOutput)
    );
    project
        .apply(DawAction::SetTrackOutput {
            track_id: bus_a,
            output_track: Some(bus_b),
        })
        .unwrap();
    assert_eq!(
        project.apply(DawAction::SetTrackOutput {
            track_id: bus_b,
            output_track: Some(bus_a)
        }),
        Err(ActionError::TrackRoutingCycle)
    );

    project
        .apply(DawAction::SetTrackOutput {
            track_id: source,
            output_track: Some(bus_a),
        })
        .unwrap();
    assert_eq!(
        project.apply(DawAction::DeleteTrack { track_id: bus_a }),
        Err(ActionError::TrackHasRoutingDependents { track_id: bus_a })
    );
    assert_eq!(project.tracks()[0].output_track(), Some(bus_a));
}
