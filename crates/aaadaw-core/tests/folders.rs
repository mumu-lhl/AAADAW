use aaadaw_core::{DawAction, Project, TrackId};

fn project() -> (Project, Vec<TrackId>) {
    let mut project = Project::new();
    for (index, name) in ["Outer", "Inner", "Leaf", "Sibling", "Outside"]
        .into_iter()
        .enumerate()
    {
        project
            .apply(DawAction::CreateTrack {
                index,
                name: name.into(),
            })
            .unwrap();
    }
    let ids = project.tracks().iter().map(|track| track.id()).collect();
    (project, ids)
}
fn nest(project: &mut Project, ids: &[TrackId]) {
    for id in &ids[..2] {
        project
            .apply(DawAction::SetTrackFolder {
                track_id: *id,
                enabled: true,
            })
            .unwrap();
    }
    for (child, parent) in [(1, 0), (2, 1), (3, 0)] {
        project
            .apply(DawAction::SetTrackParent {
                track_id: ids[child],
                parent: Some(ids[parent]),
            })
            .unwrap();
    }
}
#[test]
fn nested_folders_preserve_identity_and_effective_outputs_through_undo_and_snapshot() {
    let (mut project, ids) = project();
    let before = project.snapshot();
    nest(&mut project, &ids);
    let after = project.snapshot();
    assert_eq!(project.folder_depth(ids[2]), 2);
    assert_eq!(project.tracks()[2].effective_output_track(), Some(ids[1]));
    assert_eq!(project.tracks()[1].effective_output_track(), Some(ids[0]));
    assert_eq!(
        Project::from_snapshot(after.clone()).unwrap().snapshot(),
        after
    );
    for _ in 0..5 {
        assert!(project.undo().unwrap());
    }
    assert_eq!(project.snapshot(), before);
    for _ in 0..5 {
        assert!(project.redo().unwrap());
    }
    assert_eq!(project.snapshot(), after);
}
#[test]
fn folder_moves_reparent_complete_subtrees_and_undo_restores_order() {
    let (mut project, ids) = project();
    nest(&mut project, &ids);
    let before = project.snapshot();
    project
        .apply(DawAction::MoveTrack {
            track_id: ids[0],
            index: 1,
        })
        .unwrap();
    assert_eq!(
        project
            .tracks()
            .iter()
            .map(|track| track.id())
            .collect::<Vec<_>>(),
        [ids[4], ids[0], ids[1], ids[2], ids[3]]
    );
    project.undo().unwrap();
    assert_eq!(project.snapshot(), before);
    project
        .apply(DawAction::MoveTrack {
            track_id: ids[1],
            index: 4,
        })
        .unwrap();
    assert_eq!(
        project
            .tracks()
            .iter()
            .map(|track| track.id())
            .collect::<Vec<_>>(),
        [ids[0], ids[3], ids[4], ids[1], ids[2]]
    );
    assert_eq!(project.tracks()[3].parent_track(), None);
    assert_eq!(project.tracks()[4].parent_track(), Some(ids[1]));
    project.undo().unwrap();
    assert_eq!(project.snapshot(), before);
    project
        .apply(DawAction::SetTrackParent {
            track_id: ids[1],
            parent: None,
        })
        .unwrap();
    assert_eq!(project.tracks().last().unwrap().id(), ids[2]);
    project.undo().unwrap();
    assert_eq!(project.snapshot(), before);
}
#[test]
fn invalid_hierarchy_routing_and_parent_deletion_fail_atomically() {
    let (mut project, ids) = project();
    nest(&mut project, &ids);
    let before = project.snapshot();
    for action in [
        DawAction::SetTrackParent {
            track_id: ids[0],
            parent: Some(ids[1]),
        },
        DawAction::SetTrackParent {
            track_id: ids[0],
            parent: Some(ids[0]),
        },
        DawAction::SetTrackParent {
            track_id: ids[3],
            parent: Some(ids[2]),
        },
        DawAction::SetTrackFolder {
            track_id: ids[0],
            enabled: false,
        },
        DawAction::SetTrackOutput {
            track_id: ids[0],
            output_track: Some(ids[2]),
        },
        DawAction::DeleteTrack { track_id: ids[0] },
    ] {
        assert!(project.apply(action).is_err());
        assert_eq!(project.snapshot(), before);
    }
    let mut invalid = before.clone();
    invalid.tracks[2].parent_track_id = Some(ids[4].value());
    assert!(Project::from_snapshot(invalid).is_err());
    let mut invalid = before.clone();
    invalid.tracks.swap(1, 4);
    assert!(Project::from_snapshot(invalid).is_err());
}
