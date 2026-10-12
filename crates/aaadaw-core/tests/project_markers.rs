use aaadaw_core::{DawAction, Project};

#[test]
fn marker_edits_keep_stable_ids_order_by_position_and_undo_as_one_change() {
    let mut project = Project::new();
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 1_920,
            name: "Verse".to_owned(),
        })
        .expect("first marker should be inserted");
    let verse_id = project.markers()[0].id();

    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 960,
            name: "Intro".to_owned(),
        })
        .expect("earlier marker should be inserted");
    let intro_id = project.markers()[0].id();
    assert_eq!(project.markers()[0].position_ticks(), 960);
    assert_eq!(project.markers()[0].id(), intro_id);
    assert_eq!(project.markers()[1].id(), verse_id);

    assert!(project.undo().expect("insertion should undo"));
    assert_eq!(project.markers().len(), 1);
    assert_eq!(project.markers()[0].id(), verse_id);
    assert!(project.redo().expect("insertion should redo"));
    assert_eq!(project.markers()[0].id(), intro_id);

    project
        .apply(DawAction::UpdateProjectMarker {
            marker_id: verse_id,
            position_ticks: 2_880,
            name: "Chorus".to_owned(),
        })
        .expect("marker edit should succeed");
    assert_eq!(project.markers()[1].id(), verse_id);
    assert_eq!(project.markers()[1].name(), "Chorus");
    assert!(project.undo().expect("marker edit should undo"));
    assert_eq!(project.markers()[1].name(), "Verse");

    let snapshot = project.snapshot();
    let restored = Project::from_snapshot(snapshot).expect("markers should validate on restore");
    assert_eq!(restored.markers(), project.markers());
}

#[test]
fn invalid_marker_edits_leave_state_and_history_unchanged() {
    let mut project = Project::new();
    assert!(
        project
            .apply(DawAction::InsertProjectMarker {
                position_ticks: u64::MAX,
                name: "out of range".to_owned(),
            })
            .is_err()
    );
    assert!(
        project
            .apply(DawAction::InsertProjectMarker {
                position_ticks: 0,
                name: "invalid\0name".to_owned(),
            })
            .is_err()
    );
    assert!(project.markers().is_empty());
    assert!(!project.can_undo());

    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 0,
            name: String::new(),
        })
        .expect("empty names use automatic marker numbering");
    assert_eq!(project.markers()[0].id().value(), 0);
}

#[test]
fn visible_numbers_are_independent_from_stable_ids_and_may_be_sparse_or_duplicated() {
    let mut project = Project::new();
    project
        .apply(DawAction::InsertNumberedProjectMarker {
            number: 5,
            position_ticks: 1_920,
            name: "M5".to_owned(),
        })
        .unwrap();
    let stable_id = project.markers()[0].id();
    project
        .apply(DawAction::InsertNumberedProjectMarker {
            number: 10,
            position_ticks: 960,
            name: "M10".to_owned(),
        })
        .unwrap();
    let earlier_duplicate = project.markers()[0].id();

    project
        .apply(DawAction::InsertNumberedProjectMarker {
            number: 10,
            position_ticks: 2_880,
            name: "duplicate".to_owned(),
        })
        .unwrap();
    assert_eq!(project.markers()[0].number(), 10);
    assert_eq!(project.markers()[0].id(), earlier_duplicate);
    assert_eq!(project.markers()[2].number(), 10);

    project
        .apply(DawAction::UpdateProjectMarkerProperties {
            marker_id: stable_id,
            number: 1,
            position_ticks: 3_840,
            name: "Renumbered".to_owned(),
        })
        .unwrap();
    let changed = project
        .markers()
        .iter()
        .find(|marker| marker.id() == stable_id)
        .unwrap();
    assert_eq!(changed.number(), 1);
    assert_eq!(changed.name(), "Renumbered");
    assert_eq!(changed.position_ticks(), 3_840);
    assert!(project.undo().unwrap());
    let restored = project
        .markers()
        .iter()
        .find(|marker| marker.id() == stable_id)
        .unwrap();
    assert_eq!(restored.number(), 5);
    assert_eq!(restored.name(), "M5");
    assert_eq!(restored.position_ticks(), 1_920);
    assert!(project.redo().unwrap());
    let snapshot = project.snapshot();
    let restored_project = Project::from_snapshot(snapshot).unwrap();
    assert_eq!(restored_project.markers(), project.markers());
}

#[test]
fn plain_marker_insertion_uses_lowest_available_positive_number() {
    let mut project = Project::new();
    for number in [5, 10] {
        project
            .apply(DawAction::InsertNumberedProjectMarker {
                number,
                position_ticks: u64::from(number) * 960,
                name: String::new(),
            })
            .unwrap();
    }
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 0,
            name: String::new(),
        })
        .unwrap();
    assert_eq!(project.markers()[0].number(), 1);
}
