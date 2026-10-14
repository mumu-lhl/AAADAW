use aaadaw_core::{DawAction, Project, RazorEditArea};

#[test]
fn track_razor_edit_areas_round_trip_through_undo_redo_and_snapshot() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Razor track".into(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    let areas = vec![
        RazorEditArea::new(100, 200).unwrap(),
        RazorEditArea::new(300, 450).unwrap(),
    ];

    project
        .apply(DawAction::SetTrackRazorEditAreas {
            track_areas: vec![(track_id, areas.clone())],
        })
        .unwrap();
    assert_eq!(project.tracks()[0].razor_edit_areas(), areas);

    let restored = Project::from_snapshot(project.snapshot()).unwrap();
    assert_eq!(restored.tracks()[0].razor_edit_areas(), areas);

    assert!(project.undo().unwrap());
    assert!(project.tracks()[0].razor_edit_areas().is_empty());
    assert!(project.redo().unwrap());
    assert_eq!(project.tracks()[0].razor_edit_areas(), areas);
}

#[test]
fn track_razor_edit_areas_reject_overlaps_and_duplicate_tracks_atomically() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Razor track".into(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    let revision = project.snapshot();
    let overlap = vec![
        RazorEditArea::new(100, 250).unwrap(),
        RazorEditArea::new(200, 300).unwrap(),
    ];

    assert!(
        project
            .apply(DawAction::SetTrackRazorEditAreas {
                track_areas: vec![(track_id, overlap)],
            })
            .is_err()
    );
    assert_eq!(project.snapshot(), revision);
    assert!(project.undo().unwrap());
    assert!(
        project.tracks().is_empty(),
        "invalid edits must not add history"
    );
    assert!(project.redo().unwrap());

    assert!(
        project
            .apply(DawAction::SetTrackRazorEditAreas {
                track_areas: vec![(track_id, Vec::new()), (track_id, Vec::new())],
            })
            .is_err()
    );
    assert_eq!(project.snapshot(), revision);
    assert!(project.undo().unwrap());
    assert!(
        project.tracks().is_empty(),
        "invalid edits must not add history"
    );
}
