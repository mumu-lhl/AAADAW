use aaadaw_core::{ActionError, DawAction, MeterPointSnapshot, Project, TimeSignature};

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

#[test]
fn replacing_the_meter_map_is_atomic_and_undoable() {
    let mut project = Project::new();
    let points = vec![
        MeterPointSnapshot {
            start_tick: 0,
            numerator: 3,
            denominator: 4,
        },
        MeterPointSnapshot {
            start_tick: 2880,
            numerator: 7,
            denominator: 8,
        },
    ];
    project
        .apply(DawAction::SetTimeSignatureMap {
            points: points.clone(),
        })
        .expect("the replacement should validate using the new initial meter");
    assert_eq!(project.time_signature_map(), points);
    let position = project.musical_position_at_tick(2880).unwrap();
    assert_eq!(position.measure(), 2);
    assert_eq!(position.beat(), 1);
    let reopened = Project::from_snapshot(project.snapshot()).unwrap();
    assert_eq!(reopened.time_signature_map(), points);

    let revision = project.snapshot();
    assert_eq!(
        project.apply(DawAction::SetTimeSignatureMap {
            points: vec![
                MeterPointSnapshot {
                    start_tick: 0,
                    numerator: 4,
                    denominator: 4,
                },
                MeterPointSnapshot {
                    start_tick: 100,
                    numerator: 7,
                    denominator: 8,
                },
            ],
        }),
        Err(ActionError::MeterChangeNotOnBarBoundary)
    );
    assert_eq!(project.snapshot(), revision);

    assert!(project.undo().unwrap());
    assert_eq!(
        project.time_signature_map(),
        vec![MeterPointSnapshot {
            start_tick: 0,
            numerator: 4,
            denominator: 4,
        }]
    );
    assert!(project.redo().unwrap());
    assert_eq!(project.time_signature_map(), points);
}
