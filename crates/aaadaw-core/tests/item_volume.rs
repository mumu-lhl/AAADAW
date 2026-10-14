use aaadaw_core::{ActionError, DawAction, Project};

#[test]
fn item_volume_is_bounded_undoable_and_snapshot_safe() {
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
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let item_id = project.audio_items()[0].id();

    project
        .apply(DawAction::SetAudioItemVolume {
            item_id,
            volume: 0.5,
        })
        .unwrap();
    assert_eq!(project.audio_items()[0].volume(), 0.5);
    assert!(project.can_undo_track_mix());
    assert!(project.undo().unwrap());
    assert_eq!(project.audio_items()[0].volume(), 1.0);
    assert!(project.redo().unwrap());
    assert_eq!(project.audio_items()[0].volume(), 0.5);

    project
        .apply(DawAction::SetAudioItemTakeMix {
            item_id,
            volume: 0.5,
            pan: -0.25,
        })
        .unwrap();
    assert_eq!(project.audio_items()[0].take_volume(), 0.5);
    assert_eq!(project.audio_items()[0].take_pan(), -0.25);
    assert!(project.can_undo_track_mix());
    assert!(project.undo().unwrap());
    assert_eq!(project.audio_items()[0].take_volume(), 1.0);
    assert_eq!(project.audio_items()[0].take_pan(), 0.0);
    assert!(project.redo().unwrap());

    let snapshot = project.snapshot();
    assert_eq!(
        Project::from_snapshot(snapshot.clone()).unwrap().snapshot(),
        snapshot
    );
    let mut invalid_snapshot = snapshot;
    invalid_snapshot.audio_items[0].volume = f32::NAN;
    assert!(Project::from_snapshot(invalid_snapshot).is_err());

    let mut invalid_snapshot = project.snapshot();
    invalid_snapshot.audio_items[0].take_pan = 1.1;
    assert!(Project::from_snapshot(invalid_snapshot).is_err());

    for (volume, pan, error) in [
        (-0.01, 0.0, ActionError::InvalidAudioItemTakeVolume),
        (1.0, f32::NAN, ActionError::InvalidAudioItemTakePan),
        (1.0, 1.01, ActionError::InvalidAudioItemTakePan),
    ] {
        assert_eq!(
            project.apply(DawAction::SetAudioItemTakeMix {
                item_id,
                volume,
                pan,
            }),
            Err(error)
        );
        assert_eq!(project.audio_items()[0].take_volume(), 0.5);
        assert_eq!(project.audio_items()[0].take_pan(), -0.25);
    }

    for volume in [-0.01, f32::NAN, f32::INFINITY, 16.01] {
        assert!(matches!(
            project.apply(DawAction::SetAudioItemVolume { item_id, volume }),
            Err(ActionError::InvalidAudioItemVolume)
        ));
        assert_eq!(project.audio_items()[0].volume(), 0.5);
    }
}
