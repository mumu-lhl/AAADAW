use aaadaw_core::{ActionError, AudioSendParameters, DawAction, Project};

fn project() -> Project {
    let mut project = Project::new();
    for (index, name) in ["Source", "Receiver", "Other"].into_iter().enumerate() {
        project
            .apply(DawAction::CreateTrack {
                index,
                name: name.into(),
            })
            .unwrap();
    }
    project
}

fn create(project: &mut Project, source: usize, destination: usize) {
    project
        .apply(DawAction::CreateAudioSend {
            track_id: project.tracks()[source].id(),
            destination: project.tracks()[destination].id(),
            parameters: AudioSendParameters::default(),
        })
        .unwrap();
}

#[test]
fn parallel_sends_keep_ids_parameters_order_and_main_output_through_history_and_snapshot() {
    let mut project = project();
    create(&mut project, 0, 1);
    create(&mut project, 0, 1);
    let source = project.tracks()[0].id();
    let receiver = project.tracks()[1].id();
    let first = project.tracks()[0].sends()[0].id();
    let second = project.tracks()[0].sends()[1].id();
    assert_ne!(first, second);
    assert!(project.tracks()[0].main_send_enabled());
    let parameters = AudioSendParameters {
        volume_db: -6.0,
        pan: -0.5,
        muted: true,
        phase_inverted: true,
        tap: aaadaw_core::AudioSendTap::PreFx,
    };
    project
        .apply(DawAction::UpdateAudioSend {
            track_id: source,
            send_id: first,
            destination: receiver,
            parameters,
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackMainSend {
            track_id: source,
            enabled: false,
        })
        .unwrap();
    let snapshot = project.snapshot();
    let restored = Project::from_snapshot(snapshot.clone()).unwrap();
    assert_eq!(restored.snapshot(), snapshot);
    assert_eq!(restored.tracks()[0].sends()[0].parameters(), parameters);
    project
        .apply(DawAction::DeleteAudioSend {
            track_id: source,
            send_id: first,
        })
        .unwrap();
    assert_eq!(project.tracks()[0].sends()[0].id(), second);
    project.undo().unwrap();
    assert_eq!(project.snapshot(), snapshot);
    project.redo().unwrap();
    assert_eq!(project.tracks()[0].sends()[0].id(), second);
}

#[test]
fn all_edges_participate_in_cycle_and_deletion_validation_and_failed_batch_preserves_ids() {
    let mut project = project();
    let a = project.tracks()[0].id();
    let b = project.tracks()[1].id();
    let c = project.tracks()[2].id();
    let create_action = |source, destination| DawAction::CreateAudioSend {
        track_id: source,
        destination,
        parameters: AudioSendParameters::default(),
    };
    let before = project.snapshot();
    assert_eq!(
        project.apply(DawAction::BatchTransaction {
            tx_id: 1,
            actions: vec![create_action(a, b), create_action(b, a)]
        }),
        Err(ActionError::TrackRoutingCycle)
    );
    assert_eq!(project.snapshot(), before);
    project.undo().unwrap();
    assert_eq!(project.tracks().len(), 2);
    project.redo().unwrap();
    assert_eq!(project.snapshot(), before);
    project.apply(create_action(a, b)).unwrap();
    assert_eq!(project.tracks()[0].sends()[0].id().value(), 0);
    project.apply(create_action(b, c)).unwrap();
    assert_eq!(
        project.apply(DawAction::SetTrackOutput {
            track_id: c,
            output_track: Some(a)
        }),
        Err(ActionError::TrackRoutingCycle)
    );
    assert_eq!(
        project.apply(DawAction::DeleteTrack { track_id: b }),
        Err(ActionError::TrackHasRoutingDependents { track_id: b })
    );
    assert_eq!(
        project.apply(create_action(a, a)),
        Err(ActionError::InvalidTrackOutput)
    );
    let mut invalid = project.snapshot();
    invalid.tracks[0].sends[0].parameters.volume_db = f32::NAN;
    assert!(Project::from_snapshot(invalid).is_err());
    let mut duplicate = project.snapshot();
    duplicate.tracks[1].sends[0].id = duplicate.tracks[0].sends[0].id;
    assert!(Project::from_snapshot(duplicate).is_err());
    let mut cycle = project.snapshot();
    cycle.tracks[2].output_track_id = Some(a.value());
    assert!(Project::from_snapshot(cycle).is_err());
}
