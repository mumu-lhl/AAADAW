use aaadaw_core::{DawAction, Project};

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
