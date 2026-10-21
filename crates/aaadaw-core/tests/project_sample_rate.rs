use aaadaw_core::{
    AudioFade, AudioItemFades, DawAction, FadeShape, FxParameterAutomationPoint, Project,
    TrackFxPlugin, VolumeAutomationPoint,
};

#[test]
fn sample_rate_changes_preserve_seconds_and_are_exactly_undoable() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".into(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://tone".into(),
            start_sample: 24_000,
            source_offset_samples: 44_100,
            length_samples: 48_000,
        })
        .unwrap();
    let item_id = project.audio_items()[0].id();
    project
        .apply(DawAction::SetAudioItemFades {
            item_id,
            fades: AudioItemFades {
                fade_in: AudioFade::new(12_000.5, FadeShape::Smooth).unwrap(),
                fade_out: AudioFade::new(6_000.25, FadeShape::SlowStart).unwrap(),
            },
        })
        .unwrap();
    let automation = vec![
        VolumeAutomationPoint::new(24_000, -6.0).unwrap(),
        VolumeAutomationPoint::new(48_000, 0.0).unwrap(),
    ];
    project
        .apply(DawAction::SetTrackVolumeAutomation {
            track_id,
            points: automation.clone(),
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![TrackFxPlugin::new("org.example.eq", "/plugins/eq.clap").unwrap()],
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackFxParameterAutomation {
            track_id,
            chain_index: 0,
            parameter_id: 3,
            points: vec![
                FxParameterAutomationPoint::new(24_000, 0.25).unwrap(),
                FxParameterAutomationPoint::new(48_000, 0.75).unwrap(),
            ],
        })
        .unwrap();

    let original = project.snapshot();
    project
        .apply(DawAction::SetProjectSampleRate {
            sample_rate: 24_000,
        })
        .unwrap();

    assert_eq!(project.settings().sample_rate(), 24_000);
    assert_eq!(project.sample_at_tick(960).unwrap(), 12_000);
    let item = &project.audio_items()[0];
    assert_eq!(item.start_sample(), 12_000);
    assert_eq!(item.length_samples(), 24_000);
    assert_eq!(item.source_offset_samples(), 44_100);
    assert_eq!(item.fades().fade_in.length_samples(), 6_000.25);
    assert_eq!(item.fades().fade_out.length_samples(), 3_000.125);
    assert_eq!(
        project.tracks()[0].volume_automation(),
        &[
            VolumeAutomationPoint::new(12_000, -6.0).unwrap(),
            VolumeAutomationPoint::new(24_000, 0.0).unwrap(),
        ]
    );
    assert_eq!(
        project.tracks()[0].fx_chain()[0]
            .parameter_automation_for(3)
            .unwrap()
            .points(),
        &[
            FxParameterAutomationPoint::new(12_000, 0.25).unwrap(),
            FxParameterAutomationPoint::new(24_000, 0.75).unwrap(),
        ]
    );

    let reopened = Project::from_snapshot(project.snapshot()).unwrap();
    assert_eq!(reopened.snapshot(), project.snapshot());
    assert!(project.undo().unwrap());
    assert_eq!(project.snapshot(), original);
    assert!(project.redo().unwrap());
    assert_eq!(project.settings().sample_rate(), 24_000);
    assert_eq!(project.audio_items()[0].start_sample(), 12_000);
}

#[test]
fn invalid_or_unrepresentable_sample_rate_changes_are_atomic() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Automation".into(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::SetTrackVolumeAutomation {
            track_id,
            points: vec![
                VolumeAutomationPoint::new(1, -3.0).unwrap(),
                VolumeAutomationPoint::new(2, 0.0).unwrap(),
            ],
        })
        .unwrap();
    let before = project.snapshot();

    assert!(
        project
            .apply(DawAction::SetProjectSampleRate { sample_rate: 0 })
            .is_err()
    );
    assert!(
        project
            .apply(DawAction::SetProjectSampleRate {
                sample_rate: 24_000,
            })
            .is_err()
    );
    assert_eq!(project.snapshot(), before);
    assert_eq!(project.settings().sample_rate(), 48_000);
}
