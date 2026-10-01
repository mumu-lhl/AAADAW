use aaadaw_app::{AudioEditError, duplicate_audio_item};
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
    let action = duplicate_audio_item(&project, item_id).expect("duplicate action should be built");
    project.apply(action).expect("duplicate should apply");

    let duplicate = &project.audio_items()[1];
    assert_eq!(duplicate.start_sample(), 1_200);
    assert_eq!(duplicate.source_offset_samples(), 120);
    assert_eq!(duplicate.length_samples(), 960);
    assert_eq!(duplicate.media_ref(), "asset://duplicate-test");
    assert!(project.undo().expect("duplicate should be undoable"));
    assert_eq!(project.audio_items().len(), 1);
    assert_eq!(project.audio_items()[0].id(), item_id);
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
