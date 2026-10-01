use aaadaw_core::{DawAction, Project, TimeSignature};

#[test]
fn time_signature_changes_start_a_new_measure_at_the_bar_line() {
    let mut project = Project::new();
    project
        .apply(DawAction::SetTimeSignature {
            start_tick: 3840,
            signature: TimeSignature::new(7, 8).expect("7/8 is a valid meter"),
        })
        .expect("a meter change on a bar line should succeed");

    let position = project
        .musical_position_at_tick(7200)
        .expect("a valid tick should have a musical position");
    assert_eq!(position.measure(), 3);
    assert_eq!(position.beat(), 1);
    assert_eq!(position.tick_in_beat(), 0);
    assert_eq!(
        project.time_signature_at_tick(0),
        TimeSignature::new(4, 4).unwrap()
    );
    assert_eq!(
        project.time_signature_at_tick(3840),
        TimeSignature::new(7, 8).unwrap()
    );
    assert_eq!(
        project.time_signature_points().collect::<Vec<_>>(),
        vec![
            (0, TimeSignature::new(4, 4).unwrap()),
            (3840, TimeSignature::new(7, 8).unwrap()),
        ]
    );
}
