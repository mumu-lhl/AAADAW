use aaadaw_core::{DawAction, Project, TempoCurve};

#[test]
fn sample_and_ppq_positions_follow_tempo_changes() {
    let mut project = Project::new();

    assert_eq!(
        project
            .sample_at_tick(960)
            .expect("a valid tick should convert"),
        24_000
    );

    project
        .apply(DawAction::SetTempo {
            start_tick: 960,
            bpm: 60.0,
        })
        .expect("a positive tempo should be accepted");

    assert_eq!(
        project
            .sample_at_tick(1920)
            .expect("a valid tick should convert"),
        72_000
    );
    assert_eq!(
        project
            .tick_at_sample(72_000)
            .expect("a valid sample should convert"),
        1920
    );
}

#[test]
fn linear_tempo_ramps_integrate_and_invert_positions() {
    let mut project = Project::new();
    project
        .apply(DawAction::SetTempo {
            start_tick: 960,
            bpm: 60.0,
        })
        .expect("the ramp end tempo should be accepted");
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: TempoCurve::Linear,
        })
        .expect("a linear transition from the initial tempo should be accepted");

    assert_eq!(
        project
            .sample_at_tick(480)
            .expect("the ramp midpoint should convert"),
        13_809
    );
    assert_eq!(
        project
            .tick_at_sample(13_809)
            .expect("the ramp midpoint should convert back"),
        480
    );
    assert_eq!(
        project
            .sample_at_tick(960)
            .expect("the ramp endpoint should convert"),
        33_271
    );
    assert_eq!(
        project
            .tick_at_sample(33_271)
            .expect("the ramp endpoint should convert back"),
        960
    );

    assert!(project.undo().expect("curve undo should succeed"));
    assert_eq!(
        project
            .sample_at_tick(960)
            .expect("step-tempo position should convert"),
        24_000
    );
    assert!(project.redo().expect("curve redo should succeed"));
    assert_eq!(
        project
            .sample_at_tick(960)
            .expect("ramp position should convert"),
        33_271
    );
}
