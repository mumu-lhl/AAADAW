use aaadaw_core::{DawAction, Project, SnapshotError, TrackInstrument, TrackInstrumentSnapshot};

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

#[test]
fn track_instrument_assignment_and_clear_are_undoable() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Keys".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    let instrument = TrackInstrument::new("org.example.keys", "/plugins/keys.clap")
        .expect("a plugin ID and bundle path make a usable reference");

    project
        .apply(DawAction::SetTrackInstrument {
            track_id,
            instrument: Some(instrument.clone()),
        })
        .unwrap();
    assert_eq!(project.tracks()[0].instrument(), Some(&instrument));
    assert_eq!(
        project.snapshot().tracks[0]
            .instrument
            .as_ref()
            .map(|value| (value.plugin_id.as_str(), value.bundle_path.as_str())),
        Some(("org.example.keys", "/plugins/keys.clap"))
    );

    project
        .apply(DawAction::SetTrackInstrument {
            track_id,
            instrument: None,
        })
        .unwrap();
    assert_eq!(project.tracks()[0].instrument(), None);
    assert!(project.undo().unwrap());
    assert_eq!(project.tracks()[0].instrument(), Some(&instrument));
    assert!(project.undo().unwrap());
    assert_eq!(project.tracks()[0].instrument(), None);
    assert!(project.redo().unwrap());
    assert_eq!(project.tracks()[0].instrument(), Some(&instrument));
    assert!(project.redo().unwrap());
    assert_eq!(project.tracks()[0].instrument(), None);
}

#[test]
fn empty_track_instrument_fields_are_rejected() {
    assert!(TrackInstrument::new("   ", "/plugins/keys.clap").is_none());
    assert!(TrackInstrument::new("org.example.keys", "  ").is_none());
}

#[test]
fn snapshots_with_malformed_track_instruments_are_rejected() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Keys".to_owned(),
        })
        .unwrap();
    let mut snapshot = project.snapshot();
    snapshot.tracks[0].instrument = Some(TrackInstrumentSnapshot {
        plugin_id: " ".to_owned(),
        bundle_path: "/plugins/keys.clap".to_owned(),
    });

    assert!(matches!(
        Project::from_snapshot(snapshot),
        Err(SnapshotError::InvalidProjectData)
    ));
}
