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
    assert!(
        initial.is_loop_source(),
        "new items follow REAPER's default"
    );

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
fn audio_take_playback_rate_is_validated_snapshot_safe_and_undoable() {
    let (mut project, track_id) = project_with_track();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://take-rate".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .expect("audio item insertion should succeed");
    let item_id = project.audio_items()[0].id();
    assert_eq!(project.audio_items()[0].take_playback_rate(), 1.0);

    project
        .apply(DawAction::SetAudioItemTakePlaybackRate {
            item_id,
            rate: 1.25,
        })
        .expect("positive finite take rate should apply");
    assert_eq!(project.audio_items()[0].take_playback_rate(), 1.25);
    assert_eq!(
        Project::from_snapshot(project.snapshot())
            .expect("take rate should be part of project snapshots")
            .audio_items()[0]
            .take_playback_rate(),
        1.25
    );

    assert!(project.undo().expect("take rate edit should undo"));
    assert_eq!(project.audio_items()[0].take_playback_rate(), 1.0);
    assert!(project.redo().expect("take rate edit should redo"));
    assert_eq!(project.audio_items()[0].take_playback_rate(), 1.25);

    let before = project.snapshot();
    assert!(matches!(
        project.apply(DawAction::SetAudioItemTakePlaybackRate { item_id, rate: 0.0 }),
        Err(aaadaw_core::ActionError::InvalidAudioItemTakePlaybackRate)
    ));
    assert_eq!(project.snapshot(), before);
}

#[test]
fn bounded_source_loop_can_be_disabled_and_undo_restores_its_flag() {
    let (mut project, track_id) = project_with_track();
    project
        .apply(DawAction::InsertLoopedAudioItem {
            track_id,
            media_ref: "asset://bounded-loop".to_owned(),
            start_sample: 0,
            source_offset_samples: 120,
            length_samples: 4_800,
            muted: false,
            phase_inverted: false,
            source_section_start_samples: 100,
            source_section_length_samples: 240,
        })
        .expect("bounded source loop should insert");
    let item_id = project.audio_items()[0].id();

    project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id,
            enabled: false,
        })
        .expect("source loop should be disabled");
    let item = &project.audio_items()[0];
    assert!(!item.is_loop_source());
    assert_eq!(item.source_section_start_samples(), Some(100));
    assert_eq!(item.source_section_length_samples(), Some(240));

    assert!(project.undo().expect("loop toggle should undo"));
    assert!(project.audio_items()[0].is_loop_source());
    assert!(project.redo().expect("loop toggle should redo"));
    assert!(!project.audio_items()[0].is_loop_source());
    assert_eq!(
        project.audio_items()[0].source_section_start_samples(),
        Some(100)
    );
}

#[test]
fn disabled_bounded_source_section_accepts_and_restores_negative_offset() {
    let (mut project, track_id) = project_with_track();
    project
        .apply(DawAction::InsertLoopedAudioItem {
            track_id,
            media_ref: "asset://bounded-preroll".to_owned(),
            start_sample: 48_000,
            source_offset_samples: 240,
            length_samples: 4_800,
            muted: false,
            phase_inverted: false,
            source_section_start_samples: 0,
            source_section_length_samples: 144_000,
        })
        .expect("bounded source loop should insert");
    let item = project.audio_items()[0].clone();
    project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id: item.id(),
            enabled: false,
        })
        .expect("looping should be disabled before signed nudge");
    project
        .apply(DawAction::EditAudioItem {
            item_id: item.id(),
            media_ref: item.media_ref().to_owned(),
            start_sample: item.start_sample(),
            source_offset_samples: -7_200,
            length_samples: item.length_samples(),
        })
        .expect("disabled bounded sections should allow pre-roll before their start");

    let edited = project.audio_items()[0].clone();
    assert_eq!(edited.source_offset_samples(), -7_200);
    assert_eq!(edited.source_section_start_samples(), Some(0));
    assert_eq!(edited.source_section_length_samples(), Some(144_000));
    assert!(!edited.is_loop_source());
    assert_eq!(
        Project::from_snapshot(project.snapshot())
            .expect("signed bounded section should restore")
            .audio_items()[0],
        edited
    );
    project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id: item.id(),
            enabled: true,
        })
        .expect("negative offsets should wrap within a bounded loop section");
    assert!(project.audio_items()[0].is_loop_source());
    assert_eq!(project.audio_items()[0].source_offset_samples(), -7_200);
}

#[test]
fn non_looping_audio_items_support_signed_source_offsets() {
    let (mut project, track_id) = project_with_track();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://preroll".to_owned(),
            start_sample: 24,
            source_offset_samples: -12,
            length_samples: 48,
        })
        .expect("negative source offsets are valid for one-pass playback");
    let item = project.audio_items()[0].clone();
    assert_eq!(item.start_sample(), 24);
    assert_eq!(item.length_samples(), 48);
    assert_eq!(item.source_offset_samples(), -12);
    assert!(!item.is_loop_source());
    assert_eq!(
        Project::from_snapshot(project.snapshot())
            .expect("signed item snapshot should restore")
            .audio_items()[0]
            .source_offset_samples(),
        -12
    );

    project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id: item.id(),
            enabled: true,
        })
        .expect("negative full-source offsets should wrap by source length");
    assert!(project.audio_items()[0].is_loop_source());
    assert_eq!(project.audio_items()[0].source_offset_samples(), -12);

    project
        .apply(DawAction::EditAudioItem {
            item_id: item.id(),
            media_ref: item.media_ref().to_owned(),
            start_sample: item.start_sample(),
            source_offset_samples: -24,
            length_samples: item.length_samples(),
        })
        .expect("non-looping items should allow further signed nudges");
    assert_eq!(project.audio_items()[0].source_offset_samples(), -24);
    assert!(project.undo().expect("signed offset edit should undo"));
    assert_eq!(project.audio_items()[0].source_offset_samples(), -12);
}

#[test]
fn audio_item_mute_is_undoable_and_keeps_item_gain_unchanged() {
    let (mut project, track_id) = project_with_track();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://mute".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 4_800,
        })
        .expect("audio item should insert");
    let item_id = project.audio_items()[0].id();
    project
        .apply(DawAction::SetAudioItemVolume {
            item_id,
            volume: 0.5,
        })
        .expect("item volume should be set");

    project
        .apply(DawAction::SetAudioItemMute {
            item_id,
            muted: true,
        })
        .expect("item mute should be set");
    assert!(project.audio_items()[0].is_muted());
    assert_eq!(project.audio_items()[0].volume(), 0.5);
    let restored = Project::from_snapshot(project.snapshot()).expect("muted snapshot should load");
    assert!(restored.audio_items()[0].is_muted());
    assert_eq!(restored.audio_items()[0].volume(), 0.5);

    assert!(project.undo().expect("mute should undo"));
    assert!(!project.audio_items()[0].is_muted());
    assert_eq!(project.audio_items()[0].volume(), 0.5);
    assert!(project.redo().expect("mute should redo"));
    assert!(project.audio_items()[0].is_muted());
    assert_eq!(project.audio_items()[0].volume(), 0.5);
}

#[test]
fn audio_item_phase_is_undoable_and_persisted_in_snapshots() {
    let (mut project, track_id) = project_with_track();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://phase".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 4_800,
        })
        .expect("audio item should insert");
    let item_id = project.audio_items()[0].id();
    project
        .apply(DawAction::SetAudioItemPhase {
            item_id,
            inverted: true,
        })
        .expect("item polarity should invert");
    assert!(project.audio_items()[0].is_phase_inverted());
    assert!(
        Project::from_snapshot(project.snapshot())
            .unwrap()
            .audio_items()[0]
            .is_phase_inverted()
    );
    assert!(project.undo().unwrap());
    assert!(!project.audio_items()[0].is_phase_inverted());
    assert!(project.redo().unwrap());
    assert!(project.audio_items()[0].is_phase_inverted());
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
