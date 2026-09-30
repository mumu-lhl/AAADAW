use aaadaw_core::{DawAction, Project};

#[test]
fn setting_track_pan_changes_the_track_position() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Lead".to_owned(),
        })
        .expect("creating a track should succeed");
    let track_id = project.tracks()[0].id();

    project
        .apply(DawAction::SetTrackPan {
            track_id,
            pan: -0.25,
        })
        .expect("setting an existing track's pan should succeed");

    assert_eq!(project.tracks()[0].pan(), -0.25);
}
