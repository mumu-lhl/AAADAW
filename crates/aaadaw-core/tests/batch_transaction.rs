use aaadaw_core::{ActionError, DawAction, Project};

#[test]
fn failed_batch_rolls_back_state_and_track_id_allocation() {
    let mut project = Project::new();

    let error = project
        .apply(DawAction::BatchTransaction {
            tx_id: 42,
            actions: vec![
                DawAction::CreateTrack {
                    index: 0,
                    name: "Valid".to_owned(),
                },
                DawAction::CreateTrack {
                    index: 2,
                    name: "Out of range".to_owned(),
                },
            ],
        })
        .expect_err("the invalid second action should fail the whole transaction");

    assert_eq!(
        error,
        ActionError::TrackIndexOutOfBounds {
            index: 2,
            track_count: 1,
        }
    );
    assert!(project.tracks().is_empty());

    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "First committed track".to_owned(),
        })
        .expect("a new action should succeed after rollback");
    assert_eq!(project.tracks()[0].id().value(), 0);
}
