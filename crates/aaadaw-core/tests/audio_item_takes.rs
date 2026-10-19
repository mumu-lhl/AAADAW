use aaadaw_core::{ActionError, DawAction, Project};

fn project_with_audio_item() -> (Project, aaadaw_core::ItemId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track should be created");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://take-a".to_owned(),
            start_sample: 480,
            source_offset_samples: 120,
            length_samples: 4_800,
        })
        .expect("audio item should be inserted");
    let item_id = project.audio_items()[0].id();
    (project, item_id)
}

#[test]
fn duplicate_inserts_take_at_front_and_keeps_the_original_active() {
    let (mut project, item_id) = project_with_audio_item();
    project
        .apply(DawAction::SetAudioItemTakeMix {
            item_id,
            volume: 0.5,
            pan: -0.25,
        })
        .expect("active take mix should be set");
    let placement = (
        project.audio_items()[0].start_sample(),
        project.audio_items()[0].source_offset_samples(),
        project.audio_items()[0].length_samples(),
    );

    project
        .apply(DawAction::DuplicateAudioItemTake { item_id })
        .expect("duplicate take should be inserted");
    let item = &project.audio_items()[0];
    assert_eq!(item.take_count(), 2);
    assert_eq!(item.active_take_index(), 1);
    assert_eq!(item.other_takes()[0].index(), 0);
    assert_eq!(item.other_takes()[0].media_ref(), "asset://take-a");
    assert_eq!(item.other_takes()[0].name(), item.name());
    assert_eq!(item.take_volume(), 0.5);
    assert_eq!(item.take_pan(), -0.25);
    assert_eq!(
        (
            item.start_sample(),
            item.source_offset_samples(),
            item.length_samples()
        ),
        placement
    );

    let snapshot = project.snapshot();
    assert_eq!(
        Project::from_snapshot(snapshot.clone()).unwrap().snapshot(),
        snapshot
    );
    assert!(project.undo().expect("duplicate should undo"));
    assert_eq!(project.audio_items()[0].take_count(), 1);
    assert_eq!(project.audio_items()[0].active_take_index(), 0);
    assert!(project.redo().expect("duplicate should redo"));
    assert_eq!(project.audio_items()[0].take_count(), 2);
    assert_eq!(project.audio_items()[0].active_take_index(), 1);
}

#[test]
fn switching_active_take_changes_playback_source_and_is_undoable() {
    let (mut project, item_id) = project_with_audio_item();
    project
        .apply(DawAction::DuplicateAudioItemTake { item_id })
        .expect("duplicate take should be inserted");
    let mut snapshot = project.snapshot();
    let alternate = &mut snapshot.audio_items[0].other_takes[0];
    alternate.media_ref = "asset://take-b".to_owned();
    alternate.name = "Take B".to_owned();
    let mut project = Project::from_snapshot(snapshot).expect("alternate take should restore");

    project
        .apply(DawAction::SetAudioItemActiveTake {
            item_id,
            take_index: 0,
        })
        .expect("take zero should activate");
    assert_eq!(project.audio_items()[0].active_take_index(), 0);
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-b");
    assert_eq!(project.audio_items()[0].name(), "Take B");
    assert_eq!(project.audio_items()[0].other_takes()[0].index(), 1);
    assert_eq!(
        project.audio_items()[0].other_takes()[0].media_ref(),
        "asset://take-a"
    );

    let switched = project.snapshot();
    assert_eq!(
        Project::from_snapshot(switched.clone()).unwrap().snapshot(),
        switched
    );
    assert!(project.undo().expect("take switch should undo"));
    assert_eq!(project.audio_items()[0].active_take_index(), 1);
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-a");
    assert!(project.redo().expect("take switch should redo"));
    assert_eq!(project.audio_items()[0].active_take_index(), 0);
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-b");
}

#[test]
fn invalid_or_missing_take_indices_do_not_change_state_or_history() {
    let (mut project, item_id) = project_with_audio_item();
    let before = project.snapshot();
    let history_cursor = project.history_cursor();
    assert!(matches!(
        project.apply(DawAction::SetAudioItemActiveTake {
            item_id,
            take_index: 1,
        }),
        Err(ActionError::AudioTakeNotFound { .. })
    ));
    assert_eq!(project.snapshot(), before);
    assert_eq!(project.history_cursor(), history_cursor);

    project
        .apply(DawAction::DuplicateAudioItemTake { item_id })
        .unwrap();
    let before = project.snapshot();
    let history_cursor = project.history_cursor();
    assert!(matches!(
        project.apply(DawAction::SetAudioItemActiveTake {
            item_id,
            take_index: 2,
        }),
        Err(ActionError::AudioTakeNotFound { .. })
    ));
    assert_eq!(project.snapshot(), before);
    assert_eq!(project.history_cursor(), history_cursor);
}
