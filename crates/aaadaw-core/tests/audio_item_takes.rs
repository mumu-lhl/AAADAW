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
fn duplicate_appends_a_copy_and_activates_it() {
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
fn empty_take_insertion_switching_and_history_preserve_silent_source_state() {
    let (mut project, item_id) = project_with_audio_item();
    let original = project.snapshot();

    project
        .apply(DawAction::AddEmptyAudioItemTakeBefore { item_id })
        .expect("empty take should be inserted before the active take");
    let item = &project.audio_items()[0];
    assert_eq!(item.take_count(), 2);
    assert_eq!(item.active_take_index(), 1);
    assert!(item.other_takes()[0].is_empty());
    assert_eq!(item.other_takes()[0].media_ref(), "");
    assert!(!item.active_take_is_empty());

    project
        .apply(DawAction::AddEmptyAudioItemTakeAfter { item_id })
        .expect("empty take should be inserted after the active take");
    let item = &project.audio_items()[0];
    assert_eq!(item.take_count(), 3);
    assert_eq!(item.active_take_index(), 1);
    assert!(item.other_takes().iter().all(|take| take.is_empty()));

    project
        .apply(DawAction::SetAudioItemActiveTake {
            item_id,
            take_index: 0,
        })
        .expect("silent take should be activatable");
    assert!(project.audio_items()[0].active_take_is_empty());
    assert_eq!(project.audio_items()[0].media_ref(), "");
    assert!(Project::from_snapshot(project.snapshot().clone()).is_ok());

    assert!(project.undo().expect("switch should undo"));
    assert!(!project.audio_items()[0].active_take_is_empty());
    assert!(project.undo().expect("second insertion should undo"));
    assert_eq!(project.audio_items()[0].take_count(), 2);
    assert!(project.undo().expect("first insertion should undo"));
    assert_eq!(project.snapshot(), original);
    assert!(project.redo().expect("insertion should redo"));
    assert_eq!(project.audio_items()[0].take_count(), 2);
    assert!(project.redo().expect("second insertion should redo"));
    assert_eq!(project.audio_items()[0].take_count(), 3);
    assert!(project.redo().expect("switch should redo"));
    assert!(project.audio_items()[0].active_take_is_empty());
}

#[test]
fn active_empty_audio_take_keeps_timeline_placement_editable_without_a_source() {
    let (mut project, item_id) = project_with_audio_item();
    project
        .apply(DawAction::AddEmptyAudioItemTakeBefore { item_id })
        .expect("empty take should be inserted");
    project
        .apply(DawAction::SetAudioItemActiveTake {
            item_id,
            take_index: 0,
        })
        .expect("empty take should be activated");
    let before_edit = project.snapshot();

    project
        .apply(DawAction::EditAudioItem {
            item_id,
            media_ref: String::new(),
            start_sample: 240,
            source_offset_samples: 0,
            length_samples: 2_400,
        })
        .expect("source-less item placement should be editable");
    let item = &project.audio_items()[0];
    assert!(item.active_take_is_empty());
    assert_eq!(item.media_ref(), "");
    assert_eq!(item.start_sample(), 240);
    assert_eq!(item.length_samples(), 2_400);
    assert_eq!(
        Project::from_snapshot(project.snapshot().clone())
            .unwrap()
            .snapshot(),
        project.snapshot()
    );

    assert!(project.undo().expect("placement edit should undo"));
    assert_eq!(project.snapshot(), before_edit);
    let cursor = project.history_cursor();
    assert_eq!(
        project.apply(DawAction::EditAudioItem {
            item_id,
            media_ref: "asset://wrong-source.wav".to_owned(),
            start_sample: 240,
            source_offset_samples: 0,
            length_samples: 2_400,
        }),
        Err(ActionError::InvalidAudioMediaRef)
    );
    assert_eq!(project.snapshot(), before_edit);
    assert_eq!(project.history_cursor(), cursor);
}

#[test]
fn paste_as_take_preserves_an_empty_audio_take_and_history() {
    let (mut project, item_id) = project_with_audio_item();
    project
        .apply(DawAction::AddEmptyAudioItemTakeBefore { item_id })
        .expect("empty take should be inserted");
    project
        .apply(DawAction::SetAudioItemActiveTake {
            item_id,
            take_index: 0,
        })
        .expect("empty take should be activated");
    let empty_take = project.audio_items()[0].active_take_snapshot();
    assert!(empty_take.is_empty);
    assert_eq!(empty_take.media_ref, "");

    project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id,
            take: empty_take,
        })
        .expect("empty take should paste as a new take");
    let item = &project.audio_items()[0];
    assert_eq!(item.take_count(), 3);
    assert_eq!(item.active_take_index(), 2);
    assert!(item.active_take_is_empty());
    assert!(item.other_takes()[0].is_empty());
    assert!(!item.other_takes()[1].is_empty());
    let pasted = project.snapshot();
    assert_eq!(
        Project::from_snapshot(pasted.clone()).unwrap().snapshot(),
        pasted
    );

    assert!(project.undo().expect("pasting an empty take should undo"));
    assert_eq!(project.audio_items()[0].active_take_index(), 0);
    assert!(project.audio_items()[0].active_take_is_empty());
    assert!(project.redo().expect("pasting an empty take should redo"));
    assert_eq!(project.audio_items()[0].active_take_index(), 2);
    assert!(project.audio_items()[0].active_take_is_empty());
}

#[test]
fn exploding_audio_item_takes_preserves_each_source_in_order_and_undoes_atomically() {
    let (project, item_id) = project_with_audio_item();
    let mut snapshot = project.snapshot();
    let original = &mut snapshot.audio_items[0];
    original.media_ref = "asset://take-b".to_owned();
    original.name = "Take B".to_owned();
    original.source_offset_samples = 200;
    original.active_take_index = 1;
    original.take_volume = 0.6;
    original.take_pan = 0.25;
    original.take_playback_rate = 1.1;
    original.volume = 0.75;
    original.other_takes = vec![
        aaadaw_core::AudioItemTakeSnapshot {
            index: 0,
            media_ref: "asset://take-a".to_owned(),
            is_empty: false,
            name: "Take A".to_owned(),
            source_offset_samples: 100,
            phase_inverted: true,
            loop_source: false,
            source_section_start_samples: None,
            source_section_length_samples: None,
            volume: 0.9,
            pan: -0.2,
            playback_rate: 0.8,
        },
        aaadaw_core::AudioItemTakeSnapshot {
            index: 2,
            media_ref: "asset://take-c".to_owned(),
            is_empty: false,
            name: "Take C".to_owned(),
            source_offset_samples: 300,
            phase_inverted: false,
            loop_source: true,
            source_section_start_samples: None,
            source_section_length_samples: None,
            volume: 0.4,
            pan: 0.35,
            playback_rate: 1.2,
        },
    ];
    let mut project = Project::from_snapshot(snapshot).expect("three-take item should restore");
    let before = project.snapshot();
    let cursor = project.history_cursor();

    project
        .apply(DawAction::ExplodeAudioItemTakesInPlace {
            tx_id: 45,
            item_ids: vec![item_id],
        })
        .expect("take explosion should apply");

    assert_eq!(project.history_cursor(), cursor + 1);
    assert_eq!(project.audio_items().len(), 3);
    let exploded = project.audio_items();
    assert_eq!(
        exploded.iter().map(|item| item.name()).collect::<Vec<_>>(),
        ["Take A", "Take B", "Take C"]
    );
    assert_eq!(
        exploded
            .iter()
            .map(|item| item.media_ref())
            .collect::<Vec<_>>(),
        ["asset://take-a", "asset://take-b", "asset://take-c"]
    );
    assert_eq!(
        exploded
            .iter()
            .map(|item| item.source_offset_samples())
            .collect::<Vec<_>>(),
        [100, 200, 300]
    );
    assert_eq!(
        exploded
            .iter()
            .map(|item| item.take_playback_rate())
            .collect::<Vec<_>>(),
        [0.8, 1.1, 1.2]
    );
    assert_eq!(
        exploded
            .iter()
            .map(|item| item.take_volume())
            .collect::<Vec<_>>(),
        [0.9, 0.6, 0.4]
    );
    assert!(exploded.iter().all(|item| {
        item.track_id() == exploded[0].track_id()
            && item.start_sample() == 480
            && item.length_samples() == 4_800
            && item.volume() == 0.75
            && item.take_count() == 1
            && item.active_take_index() == 0
    }));
    assert!(exploded[0].is_phase_inverted());
    assert!(!exploded[0].is_loop_source());
    assert!(exploded[2].is_loop_source());
    let exploded_snapshot = project.snapshot();
    let ids = exploded.iter().map(|item| item.id()).collect::<Vec<_>>();
    assert_ne!(ids[0], ids[1]);
    assert_ne!(ids[1], ids[2]);

    assert!(
        project
            .undo()
            .expect("the full explosion should undo at once")
    );
    assert_eq!(project.snapshot(), before);
    assert!(
        project
            .redo()
            .expect("the full explosion should redo at once")
    );
    assert_eq!(project.snapshot(), exploded_snapshot);
}

#[test]
fn exploding_only_single_take_audio_items_does_not_add_project_history() {
    let (mut project, item_id) = project_with_audio_item();
    let before = project.snapshot();
    let cursor = project.history_cursor();

    project
        .apply(DawAction::ExplodeAudioItemTakesInPlace {
            tx_id: 46,
            item_ids: vec![item_id],
        })
        .expect("a single-take selection should remain unchanged");

    assert_eq!(project.snapshot(), before);
    assert_eq!(project.history_cursor(), cursor);
}

#[test]
fn duplicate_active_take_appends_after_existing_take_order_from_any_active_index() {
    let (mut project, item_id) = project_with_audio_item();
    project
        .apply(DawAction::DuplicateAudioItemTake { item_id })
        .expect("second take should be appended and activated");
    let mut snapshot = project.snapshot();
    let inactive = &mut snapshot.audio_items[0].other_takes[0];
    inactive.media_ref = "asset://take-b".to_owned();
    inactive.name = "Take B".to_owned();
    let mut project = Project::from_snapshot(snapshot).expect("alternate take should restore");
    project
        .apply(DawAction::SetAudioItemActiveTake {
            item_id,
            take_index: 0,
        })
        .expect("the first take should become active");

    let before_first_active_duplicate = project.snapshot();
    project
        .apply(DawAction::DuplicateAudioItemTake { item_id })
        .expect("copy of the first active take should be appended");
    let item = &project.audio_items()[0];
    assert_eq!(item.take_count(), 3);
    assert_eq!(item.active_take_index(), 2);
    assert_eq!(item.media_ref(), "asset://take-b");
    assert_eq!(
        item.other_takes()
            .iter()
            .map(|take| (take.index(), take.media_ref()))
            .collect::<Vec<_>>(),
        [(0, "asset://take-b"), (1, "asset://take-a")]
    );

    project
        .apply(DawAction::SetAudioItemActiveTake {
            item_id,
            take_index: 1,
        })
        .expect("the middle take should become active");
    let before_middle_active_duplicate = project.snapshot();
    project
        .apply(DawAction::DuplicateAudioItemTake { item_id })
        .expect("copy of the middle active take should be appended");
    let item = &project.audio_items()[0];
    assert_eq!(item.take_count(), 4);
    assert_eq!(item.active_take_index(), 3);
    assert_eq!(item.media_ref(), "asset://take-a");
    assert_eq!(
        item.other_takes()
            .iter()
            .map(|take| (take.index(), take.media_ref()))
            .collect::<Vec<_>>(),
        [
            (0, "asset://take-b"),
            (1, "asset://take-a"),
            (2, "asset://take-b"),
        ]
    );

    let after = project.snapshot();
    assert!(project.undo().expect("the second duplicate should undo"));
    assert_eq!(project.snapshot(), before_middle_active_duplicate);
    assert!(project.undo().expect("the active-take switch should undo"));
    assert!(project.undo().expect("the first duplicate should undo"));
    assert_eq!(project.snapshot(), before_first_active_duplicate);
    assert!(project.redo().expect("the first duplicate should redo"));
    assert!(project.redo().expect("the active-take switch should redo"));
    assert!(project.redo().expect("the second duplicate should redo"));
    assert_eq!(project.snapshot(), after);
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
fn paste_as_take_appends_and_activates_the_copied_source_with_undo_redo() {
    let (mut project, item_id) = project_with_audio_item();
    let placement = (
        project.audio_items()[0].start_sample(),
        project.audio_items()[0].source_offset_samples(),
        project.audio_items()[0].length_samples(),
    );
    let mut copied_take = project.audio_items()[0].active_take_snapshot();
    copied_take.index = 53;
    copied_take.media_ref = "asset://take-b".to_owned();
    copied_take.name = "Take B".to_owned();
    copied_take.source_offset_samples = 960;
    copied_take.phase_inverted = true;
    copied_take.volume = 0.75;
    copied_take.pan = 0.25;
    copied_take.playback_rate = 1.25;

    project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id,
            take: copied_take,
        })
        .expect("copied take should append and activate");
    let item = &project.audio_items()[0];
    assert_eq!(item.take_count(), 2);
    assert_eq!(item.active_take_index(), 1);
    assert_eq!(item.media_ref(), "asset://take-b");
    assert_eq!(item.name(), "Take B");
    assert_eq!(item.source_offset_samples(), 960);
    assert!(item.is_phase_inverted());
    assert_eq!(item.take_volume(), 0.75);
    assert_eq!(item.take_pan(), 0.25);
    assert_eq!(item.take_playback_rate(), 1.25);
    assert_eq!(item.other_takes()[0].index(), 0);
    assert_eq!(item.other_takes()[0].media_ref(), "asset://take-a");
    assert_eq!(
        (
            item.start_sample(),
            item.source_offset_samples(),
            item.length_samples()
        ),
        (placement.0, 960, placement.2)
    );
    let snapshot = project.snapshot();
    assert_eq!(
        Project::from_snapshot(snapshot.clone()).unwrap().snapshot(),
        snapshot
    );

    assert!(project.undo().expect("paste as takes should undo"));
    assert_eq!(project.audio_items()[0].take_count(), 1);
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-a");
    assert!(project.redo().expect("paste as takes should redo"));
    assert_eq!(project.audio_items()[0].take_count(), 2);
    assert_eq!(project.audio_items()[0].active_take_index(), 1);
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-b");
}

#[test]
fn deleting_active_take_selects_the_next_available_take_and_is_undoable() {
    let (mut project, item_id) = project_with_audio_item();
    let mut copied_take = project.audio_items()[0].active_take_snapshot();
    copied_take.media_ref = "asset://take-b".to_owned();
    copied_take.name = "Take B".to_owned();
    project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id,
            take: copied_take,
        })
        .expect("second take should be appended");
    project
        .apply(DawAction::SetAudioItemActiveTake {
            item_id,
            take_index: 0,
        })
        .expect("first take should be active");

    project
        .apply(DawAction::DeleteActiveAudioItemTake { item_id })
        .expect("deleting the first take should keep its successor");
    let item = &project.audio_items()[0];
    assert_eq!(item.take_count(), 1);
    assert_eq!(item.active_take_index(), 0);
    assert_eq!(item.media_ref(), "asset://take-b");
    assert_eq!(item.name(), "Take B");
    assert!(item.other_takes().is_empty());

    let after_first_delete = project.snapshot();
    assert_eq!(
        Project::from_snapshot(after_first_delete.clone())
            .unwrap()
            .snapshot(),
        after_first_delete
    );
    assert!(project.undo().expect("first-take deletion should undo"));
    assert_eq!(project.audio_items()[0].take_count(), 2);
    assert_eq!(project.audio_items()[0].active_take_index(), 0);
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-a");
    assert!(project.redo().expect("first-take deletion should redo"));
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-b");

    let mut second_take = project.audio_items()[0].active_take_snapshot();
    second_take.media_ref = "asset://take-c".to_owned();
    second_take.name = "Take C".to_owned();
    project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id,
            take: second_take,
        })
        .expect("a take should append after the first deletion");
    assert_eq!(project.audio_items()[0].active_take_index(), 1);
    project
        .apply(DawAction::DeleteActiveAudioItemTake { item_id })
        .expect("deleting the last take should keep its predecessor");
    assert_eq!(project.audio_items()[0].take_count(), 1);
    assert_eq!(project.audio_items()[0].active_take_index(), 0);
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-b");
    assert!(project.undo().expect("last-take deletion should undo"));
    assert_eq!(project.audio_items()[0].take_count(), 2);
    assert_eq!(project.audio_items()[0].active_take_index(), 1);
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-c");
    assert!(project.redo().expect("last-take deletion should redo"));
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-b");
}

#[test]
fn deleting_the_only_active_take_removes_the_item_and_undo_restores_it() {
    let (mut project, item_id) = project_with_audio_item();
    let before = project.snapshot();

    project
        .apply(DawAction::DeleteActiveAudioItemTake { item_id })
        .expect("deleting the only take should remove its item");
    assert!(project.audio_items().is_empty());
    assert!(project.undo().expect("single-take removal should undo"));
    assert_eq!(project.snapshot(), before);
    assert!(project.redo().expect("single-take removal should redo"));
    assert!(project.audio_items().is_empty());
}

#[test]
fn moving_active_take_to_top_preserves_source_and_undo_redo_restore_order() {
    let (mut project, item_id) = project_with_audio_item();
    let mut copied_take = project.audio_items()[0].active_take_snapshot();
    copied_take.media_ref = "asset://take-b".to_owned();
    copied_take.name = "Take B".to_owned();
    project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id,
            take: copied_take,
        })
        .expect("second take should be appended");
    assert_eq!(project.audio_items()[0].active_take_index(), 1);

    project
        .apply(DawAction::MoveAudioItemActiveTakeToTop { item_id })
        .expect("active take should move to the top");
    let item = &project.audio_items()[0];
    assert_eq!(item.take_count(), 2);
    assert_eq!(item.active_take_index(), 0);
    assert_eq!(item.media_ref(), "asset://take-b");
    assert_eq!(item.name(), "Take B");
    assert_eq!(item.other_takes()[0].index(), 1);
    assert_eq!(item.other_takes()[0].media_ref(), "asset://take-a");

    let moved = project.snapshot();
    assert_eq!(
        Project::from_snapshot(moved.clone()).unwrap().snapshot(),
        moved
    );
    assert!(project.undo().expect("take reorder should undo"));
    assert_eq!(project.audio_items()[0].active_take_index(), 1);
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-b");
    assert_eq!(project.audio_items()[0].other_takes()[0].index(), 0);
    assert!(project.redo().expect("take reorder should redo"));
    assert_eq!(project.audio_items()[0].active_take_index(), 0);
    assert_eq!(project.audio_items()[0].media_ref(), "asset://take-b");

    let history_cursor = project.history_cursor();
    let unchanged = project.snapshot();
    project
        .apply(DawAction::MoveAudioItemActiveTakeToTop { item_id })
        .expect("moving an already top take should be a no-op");
    assert_eq!(project.snapshot(), unchanged);
    assert_eq!(project.history_cursor(), history_cursor);
}

#[test]
fn crop_to_active_take_preserves_the_active_source_and_undo_redo_restore_all_takes() {
    for active_index in [0, 1] {
        let (mut project, item_id) = project_with_audio_item();
        let mut copied_take = project.audio_items()[0].active_take_snapshot();
        copied_take.media_ref = "asset://take-b".to_owned();
        copied_take.name = "Take B".to_owned();
        copied_take.source_offset_samples = 12_345;
        copied_take.volume = 0.73;
        copied_take.pan = -0.4;
        copied_take.playback_rate = 0.8;
        project
            .apply(DawAction::PasteAudioItemAsTake {
                item_id,
                take: copied_take,
            })
            .expect("second take should be appended");
        if active_index == 0 {
            project
                .apply(DawAction::SetAudioItemActiveTake {
                    item_id,
                    take_index: active_index,
                })
                .expect("Take A should become active");
        }
        let before = project.snapshot();
        let mut active = project.audio_items()[0].active_take_snapshot();
        active.index = 0;
        let position = project.audio_items()[0].start_sample();
        let length = project.audio_items()[0].length_samples();

        project
            .apply(DawAction::CropAudioItemToActiveTake { item_id })
            .expect("inactive takes should be removed");
        let cropped = project.snapshot();
        let item = &project.audio_items()[0];
        assert_eq!(item.take_count(), 1);
        assert_eq!(item.active_take_index(), 0);
        assert_eq!(item.active_take_snapshot(), active);
        assert_eq!(item.start_sample(), position);
        assert_eq!(item.length_samples(), length);
        assert_eq!(
            Project::from_snapshot(cropped.clone()).unwrap().snapshot(),
            cropped
        );

        assert!(project.undo().expect("crop should undo"));
        assert_eq!(project.snapshot(), before);
        assert!(project.redo().expect("crop should redo"));
        assert_eq!(project.snapshot(), cropped);

        let history_cursor = project.history_cursor();
        project
            .apply(DawAction::CropAudioItemToActiveTake { item_id })
            .expect("cropping a single take should be a no-op");
        assert_eq!(project.snapshot(), cropped);
        assert_eq!(project.history_cursor(), history_cursor);
    }
}

#[test]
fn paste_as_take_rejects_invalid_source_data_without_changing_project_or_history() {
    let (mut project, item_id) = project_with_audio_item();
    let mut copied_take = project.audio_items()[0].active_take_snapshot();
    copied_take.playback_rate = f64::NAN;
    let before = project.snapshot();
    let history_cursor = project.history_cursor();

    assert!(matches!(
        project.apply(DawAction::PasteAudioItemAsTake {
            item_id,
            take: copied_take,
        }),
        Err(ActionError::InvalidAudioItemTakePlaybackRate)
    ));
    assert_eq!(project.snapshot(), before);
    assert_eq!(project.history_cursor(), history_cursor);
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
