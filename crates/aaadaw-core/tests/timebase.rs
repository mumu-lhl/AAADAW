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

#[test]
fn curved_tempo_ramps_keep_bpm_and_tick_sample_conversion_consistent() {
    for curve in [TempoCurve::Logarithmic, TempoCurve::Bézier] {
        for end_bpm in [60.0, 120.0, 180.0] {
            let mut project = Project::new();
            project
                .apply(DawAction::SetTempo {
                    start_tick: 960,
                    bpm: end_bpm,
                })
                .expect("ramp endpoint should be accepted");
            project
                .apply(DawAction::SetTempoCurve {
                    start_tick: 0,
                    curve,
                })
                .expect("curved tempo ramp should be accepted");

            let midpoint_bpm = project.tempo_at_tick(480);
            assert!(midpoint_bpm.is_finite());
            assert!(midpoint_bpm > 0.0);
            assert!(midpoint_bpm >= end_bpm.min(120.0) - 1.0e-10);
            assert!(midpoint_bpm <= end_bpm.max(120.0) + 1.0e-10);

            let mut previous_sample = 0;
            for tick in (0..=960).step_by(37).chain([960]) {
                let sample = project
                    .sample_at_tick(tick)
                    .expect("tempo ramp tick should map to a sample");
                assert!(sample >= previous_sample, "curve {curve:?} at {tick}");
                previous_sample = sample;
                let restored_tick = project
                    .tick_at_sample(sample)
                    .expect("tempo ramp sample should map back to a tick");
                assert!(
                    restored_tick.abs_diff(tick) <= 1,
                    "curve {curve:?}, tick {tick} mapped through sample {sample} to {restored_tick}"
                );
            }
            assert_eq!(project.tempo_at_tick(960), end_bpm);
        }
    }
}

#[test]
fn tempo_map_conversions_stay_monotonic_and_sample_accurate_over_long_ranges() {
    let mut project = Project::new();
    let mut specs = vec![(0_u64, 120.0_f64, TempoCurve::Step)];
    let mut seed = 0x8d26_4f3a_91c7_5b0d_u64;
    let mut last_tick = 0;
    for index in 0..24 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        last_tick += 100_000 + seed % 1_900_000;
        let bpm = 55.0 + f64::from((seed >> 16) as u32 % 146);
        project
            .apply(DawAction::SetTempo {
                start_tick: last_tick,
                bpm,
            })
            .expect("generated tempos should be valid");
        let curve = if index % 2 == 0 {
            TempoCurve::Linear
        } else {
            TempoCurve::Step
        };
        project
            .apply(DawAction::SetTempoCurve {
                start_tick: specs.last().unwrap().0,
                curve,
            })
            .expect("the previous tempo point should accept a curve");
        specs.last_mut().unwrap().2 = curve;
        specs.push((last_tick, bpm, TempoCurve::Step));
    }

    let mut ticks = Vec::new();
    for (index, (start_tick, _, _)) in specs.iter().enumerate() {
        ticks.extend([*start_tick, start_tick.saturating_add(1)]);
        if let Some((end_tick, _, _)) = specs.get(index + 1) {
            ticks.push(start_tick + (end_tick - start_tick) / 2);
            ticks.push(end_tick - 1);
        }
    }
    ticks.extend([
        6_912_000,  // one hour at the default tempo
        41_472_000, // six hours at the default tempo
        82_944_000, // twelve hours at the default tempo
    ]);
    ticks.sort_unstable();
    ticks.dedup();

    let mut previous_sample = 0;
    for tick in ticks {
        let sample = project
            .sample_at_tick(tick)
            .expect("long but representable positions should convert");
        assert!(
            sample >= previous_sample,
            "tempo map moved backward at tick {tick}"
        );
        previous_sample = sample;

        let reference_sample = reference_sample_at_tick(&specs, tick);
        assert!(
            sample.abs_diff(reference_sample) <= 1,
            "tick {tick}: got sample {sample}, expected {reference_sample}"
        );
        let round_trip_tick = project
            .tick_at_sample(sample)
            .expect("a mapped sample should convert back to ticks");
        let round_trip_sample = project
            .sample_at_tick(round_trip_tick)
            .expect("the round-trip tick should remain representable");
        let tempo = project.tempo_at_tick(tick);
        let samples_per_tick = 48_000.0 * 60.0 / (960.0 * tempo);
        assert!(
            sample.abs_diff(round_trip_sample) as f64 <= samples_per_tick + 1.0,
            "tick {tick} round-tripped outside one tick of sample time"
        );
    }
}

#[test]
fn tempo_map_reports_out_of_range_positions_and_keeps_large_sample_roundtrips_bounded() {
    let project = Project::new();
    let near_f64_integer_limit = (1_u64 << 53) - 2_048;
    let tick = project
        .tick_at_sample(near_f64_integer_limit)
        .expect("long sample positions should remain representable");
    let sample = project
        .sample_at_tick(tick)
        .expect("the converted tick should remain representable");
    assert!(sample.abs_diff(near_f64_integer_limit) <= 26);
    let beyond_f64_integer_limit = (1_u64 << 53) + 1;
    assert!(project.sample_at_tick(beyond_f64_integer_limit).is_err());
    assert!(project.tick_at_sample(beyond_f64_integer_limit).is_err());
    assert!(project.sample_at_tick(u64::MAX).is_err());
}

#[test]
fn tempo_points_beyond_the_exact_tick_range_are_rejected_atomically() {
    let mut project = Project::new();
    let before = project.sample_at_tick(960).unwrap();
    assert!(
        project
            .apply(DawAction::SetTempo {
                start_tick: (1_u64 << 53) + 1,
                bpm: 90.0,
            })
            .is_err()
    );
    assert_eq!(project.sample_at_tick(960).unwrap(), before);
    assert_eq!(project.tempo_at_tick(1_u64 << 53), 120.0);
    assert!(!project.undo().unwrap());
}

fn reference_sample_at_tick(specs: &[(u64, f64, TempoCurve)], tick: u64) -> u64 {
    let scale = 48_000.0 * 60.0 / 960.0;
    let mut samples = 0.0;
    for (index, (start_tick, start_bpm, curve)) in specs.iter().enumerate() {
        if *start_tick >= tick {
            break;
        }
        let offset = tick - start_tick;
        let Some((end_tick, end_bpm, _)) = specs.get(index + 1) else {
            samples += scale * offset as f64 / start_bpm;
            break;
        };
        let segment_length = end_tick - start_tick;
        let elapsed_ticks = offset.min(segment_length);
        if *curve == TempoCurve::Linear {
            let progress = elapsed_ticks as f64 / segment_length as f64;
            let bpm_at_offset = start_bpm + (end_bpm - start_bpm) * progress;
            samples += scale * segment_length as f64 / (end_bpm - start_bpm)
                * (bpm_at_offset / start_bpm).ln();
        } else {
            samples += scale * elapsed_ticks as f64 / start_bpm;
        }
        if offset <= segment_length {
            break;
        }
    }
    samples.round() as u64
}
