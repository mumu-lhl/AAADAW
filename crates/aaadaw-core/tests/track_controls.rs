use aaadaw_core::{
    DawAction, Project, SnapshotError, TrackFxPlugin, TrackFxPluginSnapshot, TrackInstrument,
    TrackInstrumentSnapshot,
};

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
fn record_arm_is_undoable_and_survives_snapshot_roundtrip() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Vocal".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    assert!(!project.tracks()[0].is_record_armed());

    project
        .apply(DawAction::SetTrackRecordArm {
            track_id,
            armed: true,
        })
        .unwrap();
    assert!(project.tracks()[0].is_record_armed());
    assert!(project.undo().unwrap());
    assert!(!project.tracks()[0].is_record_armed());
    assert!(project.redo().unwrap());

    let snapshot = project.snapshot();
    assert!(snapshot.tracks[0].record_armed);
    let reopened = Project::from_snapshot(snapshot).unwrap();
    assert!(reopened.tracks()[0].is_record_armed());
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
        state: None,
    });

    assert!(matches!(
        Project::from_snapshot(snapshot),
        Err(SnapshotError::InvalidProjectData)
    ));
}

#[test]
fn track_fx_chain_order_and_bypass_are_undoable_and_redoable() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Guitar".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    let reverb = TrackFxPlugin::new("org.example.reverb", "/plugins/reverb.clap").unwrap();
    let chorus = TrackFxPlugin::new("org.example.chorus", "/plugins/chorus.clap")
        .unwrap()
        .with_enabled(false);

    project
        .apply(DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![reverb.clone(), chorus.clone()],
        })
        .unwrap();
    assert_eq!(
        project.tracks()[0].fx_chain(),
        &[reverb.clone(), chorus.clone()]
    );

    project
        .apply(DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![chorus.clone(), reverb.clone().with_enabled(false)],
        })
        .unwrap();
    assert_eq!(project.tracks()[0].fx_chain()[0], chorus);
    assert_eq!(
        project.tracks()[0].fx_chain()[1].plugin_id(),
        "org.example.reverb"
    );
    assert!(!project.tracks()[0].fx_chain()[1].is_enabled());

    assert!(project.undo().unwrap());
    assert_eq!(
        project.tracks()[0].fx_chain(),
        &[reverb.clone(), chorus.clone()]
    );
    assert!(project.undo().unwrap());
    assert!(project.tracks()[0].fx_chain().is_empty());
    assert!(project.redo().unwrap());
    assert_eq!(
        project.tracks()[0].fx_chain(),
        &[reverb.clone(), chorus.clone()]
    );
    assert!(project.redo().unwrap());
    assert_eq!(project.tracks()[0].fx_chain()[0], chorus);
    assert!(!project.tracks()[0].fx_chain()[1].is_enabled());
}

#[test]
fn track_fx_parameter_gesture_updates_plugin_state_and_undoes_as_one_action() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Guitar".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    let plugin = TrackFxPlugin::new("org.example.eq", "/plugins/eq.clap").unwrap();
    project
        .apply(DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![plugin],
        })
        .unwrap();

    project
        .apply(DawAction::SetTrackFxParameter {
            track_id,
            chain_index: 0,
            parameter_id: 7,
            before: 0.25,
            after: 0.75,
            before_state: None,
            after_state: Some(vec![7, 5]),
        })
        .unwrap();
    assert_eq!(project.last_fx_parameter_change().unwrap().value, 0.75);
    let effect = &project.tracks()[0].fx_chain()[0];
    assert_eq!(effect.parameter_value(7), Some(0.75));
    assert_eq!(effect.state(), Some(&[7, 5][..]));

    assert!(project.undo().unwrap());
    assert_eq!(project.last_fx_parameter_change().unwrap().value, 0.25);
    let effect = &project.tracks()[0].fx_chain()[0];
    assert_eq!(effect.parameter_value(7), Some(0.25));
    assert_eq!(effect.state(), None);
    assert!(project.redo().unwrap());
    assert_eq!(project.last_fx_parameter_change().unwrap().value, 0.75);
    let effect = &project.tracks()[0].fx_chain()[0];
    assert_eq!(effect.parameter_value(7), Some(0.75));
    assert_eq!(effect.state(), Some(&[7, 5][..]));

    let reopened = Project::from_snapshot(project.snapshot()).unwrap();
    let effect = &reopened.tracks()[0].fx_chain()[0];
    assert_eq!(effect.state(), Some(&[7, 5][..]));
    assert_eq!(effect.parameter_value(7), Some(0.75));
}

#[test]
fn track_fx_parameter_edit_adopts_the_value_reported_by_the_loaded_plugin() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Keys".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![TrackFxPlugin::new("org.example.fx", "/plugins/fx.clap").unwrap()],
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackFxParameter {
            track_id,
            chain_index: 0,
            parameter_id: 7,
            before: 0.0,
            after: 10.0,
            before_state: None,
            after_state: None,
        })
        .unwrap();

    project
        .apply(DawAction::SetTrackFxParameter {
            track_id,
            chain_index: 0,
            parameter_id: 7,
            before: 5.0,
            after: 7.0,
            before_state: None,
            after_state: None,
        })
        .unwrap();
    assert_eq!(
        project.tracks()[0].fx_chain()[0].parameter_value(7),
        Some(7.0)
    );
    assert!(project.undo().unwrap());
    assert_eq!(
        project.tracks()[0].fx_chain()[0].parameter_value(7),
        Some(5.0)
    );
    assert!(project.undo().unwrap());
    assert_eq!(
        project.tracks()[0].fx_chain()[0].parameter_value(7),
        Some(0.0)
    );
}

#[test]
fn snapshots_with_malformed_track_fx_references_are_rejected() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Guitar".to_owned(),
        })
        .unwrap();
    let mut snapshot = project.snapshot();
    snapshot.tracks[0].fx_chain.push(TrackFxPluginSnapshot {
        plugin_id: "org.example.effect".to_owned(),
        bundle_path: "  ".to_owned(),
        enabled: true,
        state: None,
        parameter_values: Vec::new(),
    });

    assert!(matches!(
        Project::from_snapshot(snapshot),
        Err(SnapshotError::InvalidProjectData)
    ));
    assert!(TrackFxPlugin::new(" ", "/plugins/effect.clap").is_none());
}
