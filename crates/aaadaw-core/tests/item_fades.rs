use aaadaw_core::{AudioFade, AudioItemFades, DawAction, FadeShape, Project};

#[test]
fn fades_survive_move_trim_duplicate_snapshot_and_atomic_history() {
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
            media_ref: "asset://constant".into(),
            start_sample: 100,
            source_offset_samples: 200,
            length_samples: 48_000,
        })
        .unwrap();
    let item_id = project.audio_items()[0].id();
    assert_eq!(project.audio_items()[0].fades(), AudioItemFades::default());
    let fades = AudioItemFades {
        fade_in: AudioFade::new(12_000.5, FadeShape::Smooth).unwrap(),
        fade_out: AudioFade::new(60_000.25, FadeShape::SlowStart).unwrap(),
    };
    project
        .apply(DawAction::SetAudioItemFades { item_id, fades })
        .unwrap();
    assert!(project.can_undo_track_mix());
    project.undo().unwrap();
    assert!(project.can_redo_track_mix());
    assert_eq!(project.audio_items()[0].fades(), AudioItemFades::default());
    project.redo().unwrap();
    assert_eq!(project.audio_items()[0].fades(), fades);
    project
        .apply(DawAction::EditAudioItem {
            item_id,
            media_ref: "asset://constant".into(),
            start_sample: 200,
            source_offset_samples: 300,
            length_samples: 24_000,
        })
        .unwrap();
    project
        .apply(DawAction::DuplicateAudioItemAt {
            item_id,
            track_id,
            start_sample: 24_200,
        })
        .unwrap();
    assert_eq!(project.audio_items()[1].fades(), fades);
    assert_eq!(project.audio_items()[1].source_offset_samples(), 300);
    assert_eq!(
        Project::from_snapshot(project.snapshot())
            .unwrap()
            .snapshot(),
        project.snapshot()
    );
    let before = project.snapshot();
    assert!(
        project
            .apply(DawAction::BatchTransaction {
                tx_id: 1,
                actions: vec![
                    DawAction::SetAudioItemFades {
                        item_id,
                        fades: AudioItemFades::default()
                    },
                    DawAction::DuplicateAudioItemAt {
                        item_id,
                        track_id,
                        start_sample: u64::MAX
                    }
                ]
            })
            .is_err()
    );
    assert_eq!(project.snapshot(), before);
    project.undo().unwrap();
    assert_eq!(project.audio_items().len(), 1);
    assert_eq!(project.audio_items()[0].fades(), fades);
}

#[test]
fn current_curvature_and_s_controls_match_independent_pcm_points() {
    use aaadaw_core::{FadeCurve, FadeCurveParameters};
    let cases: [(f64, f64, [f64; 4]); 11] = [
        (-0.25, 0.0, [0.0703125, 0.15625, 0.375, 0.65625]),
        (-0.75, 0.0, [0.00793457, 0.03320312, 0.15625, 0.43945312]),
        (0.25, 0.0, [0.1796875, 0.34375, 0.625, 0.84375]),
        (0.75, 0.0, [0.32409668, 0.56054688, 0.84375, 0.96679688]),
        (0.0, -0.25, [0.171875, 0.3125, 0.5, 0.6875]),
        (0.0, -0.5, [0.21875, 0.375, 0.5, 0.625]),
        (0.0, -0.75, [0.28027344, 0.421875, 0.5, 0.578125]),
        (0.0, 0.25, [0.078125, 0.1875, 0.5, 0.8125]),
        (0.0, 0.5, [0.03125, 0.125, 0.5, 0.875]),
        (0.0, 0.75, [0.01660156, 0.078125, 0.5, 0.921875]),
        (0.25, 0.5, [0.0645752, 0.23632812, 0.71875, 0.95117188]),
    ];
    for (curvature, s, expected) in cases {
        let fade = AudioFade::with_curve(
            12_000.0,
            FadeCurve::Native(FadeCurveParameters::new(curvature, s).unwrap()),
        )
        .unwrap();
        let fades = AudioItemFades {
            fade_in: fade,
            fade_out: fade,
        };
        for (offset, gain) in [1500, 3000, 6000, 9000].into_iter().zip(expected) {
            assert!((f64::from(fades.gain_at(offset, 48_000)) - gain).abs() < 0.000001);
            assert!((f64::from(fades.gain_at(48_000 - offset, 48_000)) - gain).abs() < 0.000001);
        }
    }
    let native = AudioItemFades {
        fade_in: AudioFade::with_curve(
            12_000.0,
            FadeCurve::Native(FadeCurveParameters::new(0.0, 0.5).unwrap()),
        )
        .unwrap(),
        fade_out: AudioFade::default(),
    };
    let legacy = AudioItemFades {
        fade_in: AudioFade::new(12_000.0, FadeShape::Smooth).unwrap(),
        fade_out: AudioFade::default(),
    };
    assert_eq!(native.gain_at(3000, 48_000), 0.125);
    assert_eq!(legacy.gain_at(3000, 48_000), 0.15625);
    assert_ne!(native, legacy);
}

#[test]
fn continuous_curves_validate_range_and_are_monotonic_at_extremes() {
    use aaadaw_core::{FadeCurve, FadeCurveParameters};
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.001, 1.001] {
        assert!(FadeCurveParameters::new(invalid, 0.0).is_err());
        assert!(FadeCurveParameters::new(0.0, invalid).is_err());
    }
    for c in [-1.0, -0.75, -0.5, -0.25, 0.0, 0.25, 0.5, 0.75, 1.0] {
        for s in [-1.0, -0.75, -0.5, -0.25, 0.0, 0.25, 0.5, 0.75, 1.0] {
            let fade = AudioFade::with_curve(
                1000.0,
                FadeCurve::Native(FadeCurveParameters::new(c, s).unwrap()),
            )
            .unwrap();
            let fades = AudioItemFades {
                fade_in: fade,
                fade_out: AudioFade::default(),
            };
            let mut previous = 0.0;
            for offset in 0..=1000 {
                let value = fades.gain_at(offset, 2000);
                assert!(value.is_finite() && (0.0..=1.0).contains(&value));
                assert!(value >= previous);
                previous = value;
            }
            assert_eq!(fades.gain_at(0, 2000), 0.0);
            assert_eq!(previous, 1.0);
        }
    }
}
