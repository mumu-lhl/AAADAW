use aaadaw_core::{DawAction, MasterMix, Project};

#[test]
fn master_is_independent_undoable_snapshot_state_and_batch_failure_is_atomic() {
    let mut project = Project::new();
    let mix = MasterMix::new(-6.0206, 0.5).unwrap();
    assert_eq!(project.master_mix(), MasterMix::default());
    project.apply(DawAction::SetMasterMix { mix }).unwrap();
    assert!(project.can_undo_track_mix());
    let snapshot = project.snapshot();
    assert_eq!(
        Project::from_snapshot(snapshot.clone())
            .unwrap()
            .master_mix(),
        mix
    );
    project.undo().unwrap();
    assert_eq!(project.master_mix(), MasterMix::default());
    project.redo().unwrap();
    assert_eq!(project.snapshot(), snapshot);
    assert!(
        project
            .apply(DawAction::BatchTransaction {
                tx_id: 3,
                actions: vec![
                    DawAction::SetMasterMix {
                        mix: MasterMix::default()
                    },
                    DawAction::CreateTrack {
                        index: 4,
                        name: "Invalid".into()
                    }
                ]
            })
            .is_err()
    );
    assert_eq!(project.snapshot(), snapshot);
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "First".into(),
        })
        .unwrap();
    assert_eq!(project.tracks()[0].id().value(), 0);
    assert_eq!(project.master_mix(), mix);
}

#[test]
fn master_rejects_non_finite_overflow_gain_and_invalid_balance() {
    for gain in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
        assert!(MasterMix::new(gain, 0.0).is_err());
    }
    for pan in [f32::NAN, f32::INFINITY, -1.001, 1.001] {
        assert!(MasterMix::new(0.0, pan).is_err());
    }
    assert_eq!(
        MasterMix::new(0.0, 1.0).unwrap().channel_gains(),
        [0.0, 1.0]
    );
    assert_eq!(
        MasterMix::new(0.0, -1.0).unwrap().channel_gains(),
        [1.0, 0.0]
    );
}
