use aaadaw_core::{DawAction, Project};

fn project_with_track() -> (Project, aaadaw_core::TrackId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    (project, track_id)
}

#[test]
fn audio_items_can_be_inserted_edited_deleted_and_restored() {
    let (mut project, track_id) = project_with_track();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://field-recording".to_owned(),
            start_sample: 48_000,
            source_offset_samples: 240,
            length_samples: 96_000,
        })
        .expect("audio item insertion should succeed");
    let initial = project.audio_items()[0].clone();
    assert_eq!(initial.end_sample(), 144_000);

    project
        .apply(DawAction::EditAudioItem {
            item_id: initial.id(),
            media_ref: "asset://field-recording".to_owned(),
            start_sample: 96_000,
            source_offset_samples: 480,
            length_samples: 24_000,
        })
        .expect("audio item edit should succeed");
    let edited = project.audio_items()[0].clone();
    assert_eq!(edited.start_sample(), 96_000);
    assert!(project.undo().expect("edit should undo"));
    assert_eq!(project.audio_items()[0], initial);
    assert!(project.redo().expect("edit should redo"));
    assert_eq!(project.audio_items()[0], edited);

    project
        .apply(DawAction::DeleteAudioItem {
            item_id: edited.id(),
        })
        .expect("audio item deletion should succeed");
    assert!(project.audio_items().is_empty());
    assert!(project.undo().expect("deletion should undo"));
    assert_eq!(project.audio_items()[0], edited);
}

#[test]
fn audio_and_midi_items_share_unique_ids_and_survive_snapshot_restore() {
    let (mut project, track_id) = project_with_track();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://kick".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .expect("audio item insertion should succeed");
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .expect("MIDI item insertion should succeed");

    assert_ne!(project.audio_items()[0].id(), project.midi_items()[0].id());
    let restored = Project::from_snapshot(project.snapshot())
        .expect("valid audio and MIDI items should restore");
    assert_eq!(restored.snapshot(), project.snapshot());
}

#[test]
fn deleting_a_track_removes_and_undo_restores_its_audio_items() {
    let (mut project, track_id) = project_with_track();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://ambience".to_owned(),
            start_sample: 12,
            source_offset_samples: 34,
            length_samples: 56,
        })
        .expect("audio item insertion should succeed");
    let item = project.audio_items()[0].clone();

    project
        .apply(DawAction::DeleteTrack { track_id })
        .expect("track deletion should cascade to items");
    assert!(project.audio_items().is_empty());
    assert!(project.undo().expect("track deletion should undo"));
    assert_eq!(project.audio_items(), &[item]);
}

#[test]
fn invalid_audio_item_actions_leave_state_and_history_unchanged() {
    let (mut project, track_id) = project_with_track();
    let before = project.snapshot();
    let invalid = project.apply(DawAction::InsertAudioItem {
        track_id,
        media_ref: "   ".to_owned(),
        start_sample: 0,
        source_offset_samples: 0,
        length_samples: 1,
    });
    assert!(matches!(
        invalid,
        Err(aaadaw_core::ActionError::InvalidAudioMediaRef)
    ));
    assert_eq!(project.snapshot(), before);

    let invalid = project.apply(DawAction::InsertAudioItem {
        track_id,
        media_ref: "asset://valid".to_owned(),
        start_sample: u64::MAX,
        source_offset_samples: 0,
        length_samples: 1,
    });
    assert!(matches!(
        invalid,
        Err(aaadaw_core::ActionError::InvalidAudioItemPosition)
    ));
    assert_eq!(project.snapshot(), before);
    assert!(
        project
            .undo()
            .expect("track creation should remain undoable")
    );
    assert!(
        !project
            .undo()
            .expect("failed actions must not enter history")
    );
}
