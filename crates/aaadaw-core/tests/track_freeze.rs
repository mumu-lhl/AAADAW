use aaadaw_core::{ActionError, DawAction, MidiNoteData, Project, TrackInstrument};

fn project_with_instrument_midi() -> (Project, aaadaw_core::TrackId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Lead".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::SetTrackInstrument {
            track_id,
            instrument: Some(TrackInstrument::new("org.example.synth", "/synth.clap").unwrap()),
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 0,
                duration: 480,
                velocity: 100,
            }],
        })
        .unwrap();
    (project, track_id)
}

#[test]
fn freezing_is_one_undoable_source_preserving_action() {
    let (mut project, track_id) = project_with_instrument_midi();
    let source_before = project.snapshot();
    project
        .apply(DawAction::FreezeTrack {
            track_id,
            media_ref: "asset://freeze-render".to_owned(),
            start_sample: 0,
            length_samples: 48_000,
        })
        .unwrap();

    let track = &project.tracks()[0];
    assert!(track.is_frozen());
    assert_eq!(project.audio_items().len(), 1);
    assert_eq!(project.audio_items()[0].track_id(), track_id);
    assert!(project.tracks()[0].instrument().is_some());
    assert_eq!(project.midi_items().len(), 1);

    let frozen = project.snapshot();
    assert_eq!(
        Project::from_snapshot(frozen.clone()).unwrap().snapshot(),
        frozen
    );
    assert!(project.undo().unwrap());
    assert!(!project.tracks()[0].is_frozen());
    assert!(project.audio_items().is_empty());
    assert_eq!(project.snapshot(), source_before);
    assert!(project.redo().unwrap());
    assert!(project.tracks()[0].is_frozen());
    assert_eq!(project.snapshot(), frozen);
}

#[test]
fn unfreezing_restores_source_and_removes_only_its_render() {
    let (mut project, track_id) = project_with_instrument_midi();
    project
        .apply(DawAction::FreezeTrack {
            track_id,
            media_ref: "asset://freeze-render".to_owned(),
            start_sample: 0,
            length_samples: 48_000,
        })
        .unwrap();

    project
        .apply(DawAction::UnfreezeTrack { track_id })
        .unwrap();
    assert!(!project.tracks()[0].is_frozen());
    assert!(project.audio_items().is_empty());
    assert!(project.tracks()[0].instrument().is_some());
    assert_eq!(project.midi_items()[0].notes().len(), 1);
    assert!(project.undo().unwrap());
    assert!(project.tracks()[0].is_frozen());
    assert_eq!(project.audio_items().len(), 1);
    assert!(project.redo().unwrap());
    assert!(!project.tracks()[0].is_frozen());
    assert!(project.audio_items().is_empty());
}

#[test]
fn failed_freeze_and_frozen_source_edits_leave_the_project_unchanged() {
    let (mut project, track_id) = project_with_instrument_midi();
    let before = project.snapshot();
    assert_eq!(
        project.apply(DawAction::FreezeTrack {
            track_id,
            media_ref: " ".to_owned(),
            start_sample: 0,
            length_samples: 48_000,
        }),
        Err(ActionError::InvalidAudioMediaRef)
    );
    assert_eq!(project.snapshot(), before);

    project
        .apply(DawAction::FreezeTrack {
            track_id,
            media_ref: "asset://freeze-render".to_owned(),
            start_sample: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let frozen = project.snapshot();
    let midi_item_id = project.midi_items()[0].id();
    assert!(
        project
            .apply(DawAction::AddMidiNotes {
                item_id: midi_item_id,
                notes: vec![MidiNoteData {
                    pitch: 64,
                    tick: 0,
                    duration: 120,
                    velocity: 100,
                }],
            })
            .is_err()
    );
    assert!(
        project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: "asset://other-audio".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .is_err()
    );
    assert_eq!(project.snapshot(), frozen);
}

#[test]
fn track_with_existing_audio_cannot_be_frozen_over_its_source() {
    let (mut project, track_id) = project_with_instrument_midi();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://existing-audio".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let before = project.snapshot();
    assert_eq!(
        project.apply(DawAction::FreezeTrack {
            track_id,
            media_ref: "asset://freeze-render".to_owned(),
            start_sample: 0,
            length_samples: 48_000,
        }),
        Err(ActionError::TrackCannotBeFrozen)
    );
    assert_eq!(project.snapshot(), before);
}
