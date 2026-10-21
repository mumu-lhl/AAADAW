use aaadaw_app::{AudioEditError, duplicate_audio_item, set_audio_item_start_sample};
use aaadaw_core::{DawAction, Project};

fn project_with_audio_item(
    start_sample: u64,
    length_samples: u64,
) -> (Project, aaadaw_core::ItemId) {
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
            media_ref: "asset://duplicate-test".to_owned(),
            start_sample,
            source_offset_samples: 120,
            length_samples,
        })
        .expect("audio item should be inserted");
    let item_id = project.audio_items()[0].id();
    (project, item_id)
}

#[test]
fn duplicate_audio_item_preserves_source_and_can_be_undone() {
    let (mut project, item_id) = project_with_audio_item(240, 960);
    let fades = aaadaw_core::AudioItemFades {
        fade_in: aaadaw_core::AudioFade::new(120.5, aaadaw_core::FadeShape::Smooth).unwrap(),
        fade_out: aaadaw_core::AudioFade::default(),
    };
    project
        .apply(DawAction::SetAudioItemFades { item_id, fades })
        .unwrap();
    let action = duplicate_audio_item(&project, item_id).expect("duplicate action should be built");
    project.apply(action).expect("duplicate should apply");

    let duplicate = &project.audio_items()[1];
    assert_eq!(duplicate.fades(), fades);
    assert_eq!(duplicate.start_sample(), 1_200);
    assert_eq!(duplicate.source_offset_samples(), 120);
    assert_eq!(duplicate.length_samples(), 960);
    assert_eq!(duplicate.media_ref(), "asset://duplicate-test");
    assert!(project.undo().expect("duplicate should be undoable"));
    assert_eq!(project.audio_items().len(), 1);
    assert_eq!(project.audio_items()[0].id(), item_id);
}

#[test]
fn exact_position_edit_preserves_source_range_and_rejects_overflow() {
    let (mut project, item_id) = project_with_audio_item(240, 960);
    let action = set_audio_item_start_sample(&project, item_id, 12_345)
        .expect("exact position action should be built");
    project.apply(action).expect("position should update");
    let item = &project.audio_items()[0];
    assert_eq!(item.start_sample(), 12_345);
    assert_eq!(item.source_offset_samples(), 120);
    assert_eq!(item.length_samples(), 960);
    assert!(project.undo().expect("position edit should be undoable"));
    assert_eq!(project.audio_items()[0].start_sample(), 240);

    let (project, item_id) = project_with_audio_item(u64::MAX - 10, 10);
    assert_eq!(
        set_audio_item_start_sample(&project, item_id, u64::MAX - 9),
        Err(AudioEditError::PositionOutOfRange)
    );
}

#[test]
fn duplicate_audio_item_rejects_missing_items_and_sample_overflow() {
    let (mut project, item_id) = project_with_audio_item(u64::MAX - 10, 10);
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .expect("MIDI item should be inserted");
    let other_item_id = project.midi_items()[0].id();
    assert_eq!(
        duplicate_audio_item(&project, other_item_id),
        Err(AudioEditError::ItemNotFound)
    );
    assert_eq!(
        duplicate_audio_item(&project, item_id),
        Err(AudioEditError::PositionOutOfRange)
    );
    assert_eq!(project.audio_items().len(), 1);
}
