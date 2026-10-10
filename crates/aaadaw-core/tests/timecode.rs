use aaadaw_core::{DawAction, FrameRate, Project, TimebaseError};

#[test]
fn frame_rates_and_timecodes_match_all_native_presets() {
    for row in include_str!("../../../docs/verification/reaper-parity/frame-displays-reference.tsv")
        .lines()
        .skip(2)
    {
        let columns: Vec<_> = row.split('\t').collect();
        let fps = columns[0].parse::<f64>().unwrap();
        let drop = columns[1] == "true";
        let rate = FrameRate::ALL
            .into_iter()
            .find(|rate| {
                (rate.frames_per_second() - fps).abs() < 1e-10 && rate.is_drop_frame() == drop
            })
            .unwrap();
        let seconds = columns[2].parse::<f64>().unwrap();
        assert_eq!(rate.format_timecode(seconds).unwrap(), columns[3], "{row}");
        assert_eq!(rate.format_timecode(seconds).unwrap(), columns[4], "{row}");
        for (text, expected) in [(columns[3], columns[5]), (columns[4], columns[6])] {
            assert!(
                (rate.parse_timecode(text).unwrap() - expected.parse::<f64>().unwrap()).abs()
                    < 1e-9,
                "{row}"
            );
        }
    }
}

#[test]
fn project_frame_rate_changes_are_undoable_and_preserve_media_clock() {
    let mut project = Project::new();
    let original = project.snapshot();
    for rate in FrameRate::ALL {
        project.apply(DawAction::SetFrameRate { rate }).unwrap();
        assert_eq!(project.settings().frame_rate(), rate);
        assert_eq!(project.sample_at_tick(960).unwrap(), 24000);
        assert_eq!(
            Project::from_snapshot(project.snapshot())
                .unwrap()
                .settings()
                .frame_rate(),
            rate
        );
        assert_eq!(
            FrameRate::from_storage_code(rate.storage_code()),
            Some(rate)
        );
    }
    for _ in FrameRate::ALL {
        assert!(project.undo().unwrap());
    }
    assert_eq!(project.snapshot(), original);
    for _ in FrameRate::ALL {
        assert!(project.redo().unwrap());
    }
    assert_eq!(project.settings().frame_rate(), FrameRate::Fps75);
    assert_eq!(FrameRate::from_storage_code(10), None);
}

#[test]
fn invalid_timecodes_fail_without_saturating_or_wrapping() {
    for seconds in [-1.0, f64::NAN, f64::INFINITY, f64::MAX] {
        assert_eq!(
            FrameRate::Fps30.format_timecode(seconds),
            Err(TimebaseError::PositionOutOfRange)
        );
    }
    for text in [
        "invalid",
        "00:60:00:00",
        "00:00:60:00",
        "00:00:00:30",
        "18446744073709551615:00:00:00",
    ] {
        assert!(FrameRate::Fps30.parse_timecode(text).is_err());
    }
    assert!(FrameRate::Fps2997Drop.parse_timecode("0:01:00:00").is_err());
    assert!(FrameRate::Fps2997Drop.parse_timecode("0:01:00:01").is_err());
    assert!(FrameRate::Fps2997Drop.parse_timecode("0:01:00:02").is_ok());
    assert!(FrameRate::Fps2997Drop.parse_timecode("0:10:00:00").is_ok());
}
