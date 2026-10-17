use aaadaw_core::{DawAction, Project};

#[test]
fn undo_and_redo_restore_the_track_state() {
    let mut project = Project::new();
    assert!(!project.can_undo());
    assert!(!project.can_redo());
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .expect("creating a track should succeed");
    let track_id = project.tracks()[0].id();
    assert!(project.can_undo());
    assert!(!project.can_redo());

    assert!(project.undo().expect("undo should succeed"));
    assert!(project.tracks().is_empty());
    assert!(!project.can_undo());
    assert!(project.can_redo());

    assert!(project.redo().expect("redo should succeed"));
    assert_eq!(project.tracks()[0].id(), track_id);
    assert_eq!(project.tracks()[0].name(), "Piano");
    assert!(project.can_undo());
    assert!(!project.can_redo());
    assert!(!project.redo().expect("redo at the history end is a no-op"));
}
