use aaadaw_core::{ActionError, DawAction, Project};

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

    project
        .apply(DawAction::SetTrackVolume {
            track_id,
            volume_db: -6.0,
        })
        .expect("setting an existing track's volume should succeed");

    assert_eq!(project.tracks()[0].volume_db(), -6.0);
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
