use aaadaw_core::{ActionError, DawAction, MidiNoteData, Project};

fn project_with_two_tracks() -> (Project, aaadaw_core::TrackId, aaadaw_core::TrackId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Source".to_owned(),
        })
        .expect("source track creation should succeed");
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Target".to_owned(),
        })
        .expect("target track creation should succeed");
    let source = project.tracks()[0].id();
    let target = project.tracks()[1].id();
    (project, source, target)
}

#[test]
fn moving_audio_and_midi_items_preserves_content_and_is_undoable() {
    let (mut project, source_track, target_track) = project_with_two_tracks();
    project
        .apply(DawAction::InsertAudioItem {
            track_id: source_track,
            media_ref: "asset://room-tone".to_owned(),
            start_sample: 24_000,
            source_offset_samples: 480,
            length_samples: 96_000,
        })
        .expect("audio item insertion should succeed");
    project
        .apply(DawAction::InsertMidiItem {
            track_id: source_track,
            start_tick: 960,
            length_ticks: 3_840,
        })
        .expect("MIDI item insertion should succeed");
    let audio_before = project.audio_items()[0].clone();
    let midi_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id: midi_id,
            notes: vec![MidiNoteData {
                pitch: 67,
                tick: 480,
                duration: 720,
                velocity: 94,
            }],
        })
        .expect("MIDI note insertion should succeed");
    let midi_before = project.midi_items()[0].clone();

    for item_id in [audio_before.id(), midi_id] {
        project
            .apply(DawAction::MoveItemToTrack {
                item_id,
                track_id: target_track,
            })
            .expect("moving an item should succeed");
    }

    assert_eq!(project.audio_items()[0].track_id(), target_track);
    assert_eq!(project.audio_items()[0].id(), audio_before.id());
    assert_eq!(
        project.audio_items()[0].media_ref(),
        audio_before.media_ref()
    );
    assert_eq!(
        project.audio_items()[0].start_sample(),
        audio_before.start_sample()
    );
    assert_eq!(
        project.audio_items()[0].source_offset_samples(),
        audio_before.source_offset_samples()
    );
    assert_eq!(
        project.audio_items()[0].length_samples(),
        audio_before.length_samples()
    );
    assert_eq!(project.midi_items()[0].track_id(), target_track);
    assert_eq!(project.midi_items()[0].id(), midi_before.id());
    assert_eq!(project.midi_items()[0].notes(), midi_before.notes());

    assert!(project.undo().expect("MIDI move should undo"));
    assert_eq!(project.midi_items()[0], midi_before);
    assert!(project.undo().expect("audio move should undo"));
    assert_eq!(project.audio_items()[0], audio_before);
    assert!(project.redo().expect("audio move should redo"));
    assert!(project.redo().expect("MIDI move should redo"));
    assert_eq!(project.audio_items()[0].track_id(), target_track);
    assert_eq!(project.midi_items()[0].track_id(), target_track);
}

#[test]
fn invalid_item_moves_do_not_change_project_or_history() {
    let (mut project, source_track, target_track) = project_with_two_tracks();
    project
        .apply(DawAction::CreateTrack {
            index: 2,
            name: "Temporary".to_owned(),
        })
        .expect("temporary track creation should succeed");
    let stale_track = project.tracks()[2].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id: stale_track,
            media_ref: "asset://temporary".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .expect("temporary audio item insertion should succeed");
    let stale_item = project.audio_items()[0].id();
    project
        .apply(DawAction::DeleteAudioItem {
            item_id: stale_item,
        })
        .expect("temporary audio item deletion should succeed");
    project
        .apply(DawAction::DeleteTrack {
            track_id: stale_track,
        })
        .expect("temporary track deletion should succeed");
    project
        .apply(DawAction::InsertMidiItem {
            track_id: source_track,
            start_tick: 0,
            length_ticks: 960,
        })
        .expect("MIDI item insertion should succeed");
    let item_id = project.midi_items()[0].id();
    let before = project.snapshot();

    assert_eq!(
        project.apply(DawAction::MoveItemToTrack {
            item_id,
            track_id: stale_track,
        }),
        Err(ActionError::TrackNotFound {
            track_id: stale_track
        })
    );
    assert_eq!(
        project.apply(DawAction::MoveItemToTrack {
            item_id: stale_item,
            track_id: target_track,
        }),
        Err(ActionError::ItemNotFound {
            item_id: stale_item
        })
    );
    assert_eq!(project.snapshot(), before);
    assert!(
        project
            .undo()
            .expect("MIDI insertion should remain undoable")
    );
    assert!(project.midi_items().is_empty());
}
