use aaadaw_core::{DawAction, Project};

#[test]
fn phase_is_undoable_snapshot_state_and_failed_batch_is_atomic() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Phase".into(),
        })
        .unwrap();
    let id = project.tracks()[0].id();
    assert!(!project.tracks()[0].is_phase_inverted());
    project
        .apply(DawAction::SetTrackPhase {
            track_id: id,
            phase_inverted: true,
        })
        .unwrap();
    assert!(project.tracks()[0].is_phase_inverted());
    assert!(project.can_undo_track_mix());
    let inverted = project.snapshot();
    assert!(Project::from_snapshot(inverted.clone()).unwrap().tracks()[0].is_phase_inverted());
    project.undo().unwrap();
    assert!(!project.tracks()[0].is_phase_inverted());
    assert!(project.can_redo_track_mix());
    project.redo().unwrap();
    assert_eq!(project.snapshot(), inverted);
    assert!(
        project
            .apply(DawAction::BatchTransaction {
                tx_id: 1,
                actions: vec![
                    DawAction::SetTrackPhase {
                        track_id: id,
                        phase_inverted: false
                    },
                    DawAction::SetTrackPan {
                        track_id: id,
                        pan: 2.0
                    },
                ]
            })
            .is_err()
    );
    assert_eq!(project.snapshot(), inverted);
}
