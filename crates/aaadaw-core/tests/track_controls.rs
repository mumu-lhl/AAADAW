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

#[test]
fn mute_and_solo_states_are_undoable_and_redoable() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Drums".to_owned(),
        })
        .expect("creating a track should succeed");
    let track_id = project.tracks()[0].id();

    project
        .apply(DawAction::SetTrackMute {
            track_id,
            muted: true,
        })
        .expect("muting a track should succeed");
    project
        .apply(DawAction::SetTrackSolo {
            track_id,
            solo: true,
        })
        .expect("soloing a track should succeed");
    assert!(project.tracks()[0].is_muted());
    assert!(project.tracks()[0].is_solo());

    assert!(project.undo().expect("solo undo should succeed"));
    assert!(!project.tracks()[0].is_solo());
    assert!(project.tracks()[0].is_muted());
    assert!(project.undo().expect("mute undo should succeed"));
    assert!(!project.tracks()[0].is_muted());

    assert!(project.redo().expect("mute redo should succeed"));
    assert!(project.tracks()[0].is_muted());
    assert!(project.redo().expect("solo redo should succeed"));
    assert!(project.tracks()[0].is_solo());
}
