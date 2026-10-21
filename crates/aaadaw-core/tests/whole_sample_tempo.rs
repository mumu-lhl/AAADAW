use aaadaw_core::{DawAction, Project, ProjectSettings};

#[test]
fn whole_sample_tempo_anchor_setting_round_trips_and_undoes_exactly() {
    let settings = ProjectSettings::new(44_100, 960, 120.0).unwrap();
    let mut project = Project::with_settings(settings);
    project
        .apply(DawAction::SetTempo {
            start_tick: 1,
            bpm: 90.0,
        })
        .unwrap();
    let unaligned_sample = project.sample_at_tick_position(1.0).unwrap();
    assert!((unaligned_sample - 22.96875).abs() < 1.0e-10);

    project
        .apply(DawAction::SetForceTempoChangesToWholeSamples { enabled: true })
        .unwrap();
    assert!(project.settings().force_tempo_changes_to_whole_samples());
    assert_eq!(project.sample_at_tick_position(1.0).unwrap(), 23.0);
    project
        .apply(DawAction::SetTempo {
            start_tick: 3,
            bpm: 100.0,
        })
        .unwrap();
    assert_eq!(project.sample_at_tick_position(3.0).unwrap().fract(), 0.0);
    assert_eq!(
        Project::from_snapshot(project.snapshot())
            .unwrap()
            .snapshot(),
        project.snapshot()
    );

    assert!(project.undo().unwrap());
    assert_eq!(project.tempo_at_tick(3), 90.0);
    assert!(project.undo().unwrap());
    assert!(!project.settings().force_tempo_changes_to_whole_samples());
    assert_eq!(
        project.sample_at_tick_position(1.0).unwrap(),
        unaligned_sample
    );
    assert!(project.redo().unwrap());
    assert!(project.settings().force_tempo_changes_to_whole_samples());
    assert_eq!(project.sample_at_tick_position(1.0).unwrap(), 23.0);
}
