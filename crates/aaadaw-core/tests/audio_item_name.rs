use aaadaw_core::{DawAction, Project};

#[test]
fn audio_item_name_is_undoable_and_snapshot_round_trips() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertAudioItem {
            track_id: project.tracks()[0].id(),
            media_ref: "asset://kick".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let item_id = project.audio_items()[0].id();

    assert_eq!(project.audio_items()[0].name(), "");
    project
        .apply(DawAction::SetAudioItemName {
            item_id,
            name: "Kick close".to_owned(),
        })
        .unwrap();
    assert_eq!(project.audio_items()[0].name(), "Kick close");
    let restored = Project::from_snapshot(project.snapshot()).unwrap();
    assert_eq!(restored.audio_items()[0].name(), "Kick close");

    project.undo().unwrap();
    assert_eq!(project.audio_items()[0].name(), "");
    project.redo().unwrap();
    assert_eq!(project.audio_items()[0].name(), "Kick close");
}

#[test]
fn audio_item_name_accepts_empty_and_rejects_overlong_values() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertAudioItem {
            track_id: project.tracks()[0].id(),
            media_ref: "asset://kick".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let item_id = project.audio_items()[0].id();
    project
        .apply(DawAction::SetAudioItemName {
            item_id,
            name: "Kick".to_owned(),
        })
        .unwrap();

    assert!(
        project
            .apply(DawAction::SetAudioItemName {
                item_id,
                name: "x".repeat(129),
            })
            .is_err()
    );
    project
        .apply(DawAction::SetAudioItemName {
            item_id,
            name: String::new(),
        })
        .unwrap();
    assert_eq!(project.audio_items()[0].name(), "");
}
