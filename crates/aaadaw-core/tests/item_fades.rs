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
    project.undo().unwrap();
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
