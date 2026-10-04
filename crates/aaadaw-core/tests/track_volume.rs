use aaadaw_core::{ActionError, DawAction, Project, VolumeAutomationPoint};

#[test]
fn setting_track_volume_changes_the_track_level() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Vocals".to_owned(),
        })
        .expect("creating a track should succeed");
    let track_id = project.tracks()[0].id();
    assert!(!project.can_undo_track_mix());

    project
        .apply(DawAction::SetTrackVolume {
            track_id,
            volume_db: -6.0,
        })
        .expect("setting an existing track's volume should succeed");

    assert_eq!(project.tracks()[0].volume_db(), -6.0);
    assert!(project.can_undo_track_mix());
    assert!(!project.can_redo_track_mix());
    project.undo().expect("volume edit should undo");
    assert!(project.can_redo_track_mix());
    project.redo().expect("volume edit should redo");
    assert!(project.can_undo_track_mix());
}

#[test]
fn non_finite_track_volume_is_rejected_without_changing_the_track() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Vocals".to_owned(),
        })
        .expect("creating a track should succeed");
    let track_id = project.tracks()[0].id();

    let result = project.apply(DawAction::SetTrackVolume {
        track_id,
        volume_db: f32::NAN,
    });

    assert_eq!(result, Err(ActionError::InvalidVolumeDb));
    assert_eq!(project.tracks()[0].volume_db(), 0.0);
}

#[test]
fn track_volume_automation_is_undoable_and_survives_snapshot_round_trip() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Vocals".to_owned(),
        })
        .expect("creating a track should succeed");
    let track_id = project.tracks()[0].id();
    let points = vec![
        VolumeAutomationPoint::new(0, -12.0).unwrap(),
        VolumeAutomationPoint::new(48_000, 0.0).unwrap(),
        VolumeAutomationPoint::new(96_000, -6.0).unwrap(),
    ];

    project
        .apply(DawAction::SetTrackVolumeAutomation {
            track_id,
            points: points.clone(),
        })
        .expect("ordered automation points should be accepted");
    assert_eq!(project.tracks()[0].volume_automation(), points);
    assert!(project.can_undo_track_mix());

    project.undo().expect("automation edit should undo");
    assert!(project.tracks()[0].volume_automation().is_empty());
    project.redo().expect("automation edit should redo");
    assert_eq!(project.tracks()[0].volume_automation(), points);

    let restored = Project::from_snapshot(project.snapshot())
        .expect("automation should be valid in a project snapshot");
    assert_eq!(restored.tracks()[0].volume_automation(), points);
}

#[test]
fn volume_automation_rejects_duplicate_or_out_of_order_samples_atomically() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Vocals".to_owned(),
        })
        .expect("creating a track should succeed");
    let track_id = project.tracks()[0].id();
    let original = vec![VolumeAutomationPoint::new(0, 0.0).unwrap()];
    project
        .apply(DawAction::SetTrackVolumeAutomation {
            track_id,
            points: original.clone(),
        })
        .expect("first point should be accepted");
    let before = project.snapshot();

    let result = project.apply(DawAction::SetTrackVolumeAutomation {
        track_id,
        points: vec![
            VolumeAutomationPoint::new(48_000, -3.0).unwrap(),
            VolumeAutomationPoint::new(48_000, -6.0).unwrap(),
        ],
    });

    assert_eq!(result, Err(ActionError::InvalidVolumeAutomation));
    assert_eq!(project.snapshot(), before);

    let result = project.apply(DawAction::SetTrackVolumeAutomation {
        track_id,
        points: vec![
            VolumeAutomationPoint::new(48_000, -3.0).unwrap(),
            VolumeAutomationPoint::new(24_000, -6.0).unwrap(),
        ],
    });
    assert_eq!(result, Err(ActionError::InvalidVolumeAutomation));
    assert_eq!(project.snapshot(), before);
}

#[test]
fn volume_automation_points_reject_values_outside_the_lane_range() {
    assert!(VolumeAutomationPoint::new(0, f32::NAN).is_none());
    assert!(VolumeAutomationPoint::new(0, -60.1).is_none());
    assert!(VolumeAutomationPoint::new(0, 6.1).is_none());
}
