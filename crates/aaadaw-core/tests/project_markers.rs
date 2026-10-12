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
