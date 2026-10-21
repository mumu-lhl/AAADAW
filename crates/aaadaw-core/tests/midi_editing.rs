use aaadaw_core::{ActionError, DawAction, MidiNoteData, Project};

#[test]
fn midi_notes_can_be_added_queried_and_undone() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .expect("creating a track should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3840,
        })
        .expect("inserting a MIDI item should succeed");
    let item_id = project.midi_items()[0].id();

    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 960,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .expect("adding an in-range MIDI note should succeed");

    let note = &project.midi_items()[0].notes()[0];
    assert_eq!(note.pitch(), 60);
    assert_eq!(note.duration(), 960);
    assert!(project.undo().expect("undo should succeed"));
    assert!(project.midi_items()[0].notes().is_empty());
    assert!(project.redo().expect("redo should succeed"));
    assert_eq!(project.midi_items()[0].notes()[0].pitch(), 60);
}

#[test]
fn midi_item_mute_is_undoable_and_survives_snapshot_and_duplicate() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .expect("creating a track should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3_840,
        })
        .expect("inserting a MIDI item should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 960,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .expect("adding a MIDI note should succeed");
    project
        .apply(DawAction::SetMidiItemMute {
            item_id,
            muted: true,
        })
        .expect("MIDI item should mute");

    assert!(project.midi_items()[0].is_muted());
    let restored = Project::from_snapshot(project.snapshot()).expect("snapshot should restore");
    assert!(restored.midi_items()[0].is_muted());
    assert_eq!(restored.midi_items()[0].notes().len(), 1);

    assert!(project.undo().expect("mute should undo"));
    assert!(!project.midi_items()[0].is_muted());
    assert!(project.redo().expect("mute should redo"));
    assert!(project.midi_items()[0].is_muted());

    project
        .apply(DawAction::DuplicateMidiItem { item_id })
        .expect("muted MIDI item should duplicate");
    assert_eq!(project.midi_items().len(), 2);
    assert!(project.midi_items().iter().all(|item| item.is_muted()));
}

#[test]
fn midi_note_edits_change_timing_and_velocity_with_undo_redo() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .expect("creating a track should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3840,
        })
        .expect("inserting a MIDI item should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 960,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .expect("adding a MIDI note should succeed");
    let note_id = project.midi_items()[0].notes()[0].id();

    let edited = MidiNoteData {
        channel: 0,
        pitch: 67,
        tick: 120,
        duration: 480,
        velocity: 80,
        off_velocity: 37,
    };
    project
        .apply(DawAction::EditMidiNote {
            item_id,
            note_id,
            data: edited,
        })
        .expect("editing an in-range note should succeed");
    let note = &project.midi_items()[0].notes()[0];
    assert_eq!(note.id(), note_id);
    assert_eq!(note.pitch(), edited.pitch);
    assert_eq!(note.tick(), edited.tick);
    assert_eq!(note.duration(), edited.duration);
    assert_eq!(note.velocity(), edited.velocity);
    assert_eq!(note.off_velocity(), edited.off_velocity);

    assert!(project.undo().expect("edit undo should succeed"));
    let note = &project.midi_items()[0].notes()[0];
    assert_eq!(note.pitch(), 60);
    assert_eq!(note.tick(), 0);
    assert_eq!(note.duration(), 960);
    assert_eq!(note.velocity(), 100);
    assert_eq!(note.off_velocity(), 0);
    assert!(project.redo().expect("edit redo should succeed"));
    assert_eq!(project.midi_items()[0].notes()[0].pitch(), edited.pitch);
    assert_eq!(project.midi_items()[0].notes()[0].off_velocity(), 37);

    let invalid = project.apply(DawAction::EditMidiNote {
        item_id,
        note_id,
        data: MidiNoteData {
            tick: 3800,
            duration: 100,
            off_velocity: 128,
            ..edited
        },
    });
    assert_eq!(invalid, Err(ActionError::InvalidMidiNote));
    assert_eq!(project.midi_items()[0].notes()[0].tick(), edited.tick);
}
