use aaadaw_core::{ActionError, DawAction, MidiNoteData, Project};

#[test]
fn midi_items_can_be_deleted_and_restored_with_their_notes() {
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
                pitch: 60,
                tick: 240,
                duration: 960,
                velocity: 100,
            }],
        })
        .expect("adding a note should succeed");
    let note_id = project.midi_items()[0].notes()[0].id();

    project
        .apply(DawAction::DeleteMidiItem { item_id })
        .expect("deleting a MIDI item should succeed");
    assert!(project.midi_items().is_empty());
    assert!(project.undo().expect("delete undo should succeed"));
    let restored = &project.midi_items()[0];
    assert_eq!(restored.id(), item_id);
    assert_eq!(restored.notes()[0].id(), note_id);
    assert_eq!(restored.notes()[0].tick(), 240);
    assert!(project.redo().expect("delete redo should succeed"));
    assert!(project.midi_items().is_empty());

    assert_eq!(
        project.apply(DawAction::DeleteMidiItem { item_id }),
        Err(ActionError::MidiItemNotFound { item_id })
    );
}

#[test]
fn midi_items_can_be_moved_and_resized_without_losing_notes() {
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
            start_tick: 960,
            length_ticks: 1920,
        })
        .expect("inserting a MIDI item should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 64,
                tick: 480,
                duration: 960,
                velocity: 96,
            }],
        })
        .expect("adding a note should succeed");

    project
        .apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 1920,
            length_ticks: 1800,
        })
        .expect("moving and resizing the item should succeed");
    let item = &project.midi_items()[0];
    assert_eq!(item.start_tick(), 1920);
    assert_eq!(item.length_ticks(), 1800);
    assert_eq!(item.notes()[0].tick(), 480);

    assert!(project.undo().expect("item edit undo should succeed"));
    assert_eq!(project.midi_items()[0].start_tick(), 960);
    assert_eq!(project.midi_items()[0].length_ticks(), 1920);
    assert!(project.redo().expect("item edit redo should succeed"));
    assert_eq!(project.midi_items()[0].start_tick(), 1920);
    assert_eq!(project.midi_items()[0].length_ticks(), 1800);

    project
        .apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 0,
            length_ticks: 1000,
        })
        .expect("clip bounds may be shorter than stored note content");
    assert_eq!(project.midi_items()[0].start_tick(), 0);
    assert_eq!(project.midi_items()[0].length_ticks(), 1000);
    assert!(project.undo().expect("short trim undo should succeed"));
    assert_eq!(project.midi_items()[0].start_tick(), 1920);
    assert_eq!(project.midi_items()[0].length_ticks(), 1800);

    let overflowing_position = project.apply(DawAction::EditMidiItem {
        item_id,
        start_tick: u64::MAX,
        length_ticks: 1,
    });
    assert_eq!(
        overflowing_position,
        Err(ActionError::InvalidMidiItemPosition)
    );
    assert_eq!(project.midi_items()[0].start_tick(), 1920);

    let overflowing_insert = project.apply(DawAction::InsertMidiItem {
        track_id,
        start_tick: u64::MAX,
        length_ticks: 1,
    });
    assert_eq!(
        overflowing_insert,
        Err(ActionError::InvalidMidiItemPosition)
    );
    assert_eq!(project.midi_items().len(), 1);
}

#[test]
fn shortening_a_midi_clip_keeps_hidden_events_for_later_expansion() {
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
            length_ticks: 1_920,
        })
        .expect("inserting a MIDI item should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 64,
                tick: 1_440,
                duration: 480,
                velocity: 96,
            }],
        })
        .expect("adding a note should succeed");
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![aaadaw_core::MidiControllerData {
                controller: 1,
                tick: 1_680,
                value: 80,
            }],
        })
        .expect("adding a controller should succeed");
    let notes = project.midi_items()[0].notes().to_vec();
    let controllers = project.midi_items()[0].controllers().to_vec();

    project
        .apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .expect("shortening a MIDI clip should preserve events beyond its end");
    let shortened = &project.midi_items()[0];
    assert_eq!(shortened.start_tick(), 0);
    assert_eq!(shortened.length_ticks(), 960);
    assert_eq!(shortened.notes(), notes);
    assert_eq!(shortened.controllers(), controllers);

    assert!(project.undo().expect("clip trim undo should succeed"));
    assert_eq!(project.midi_items()[0].length_ticks(), 1_920);
    assert!(project.redo().expect("clip trim redo should succeed"));
    project
        .apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 0,
            length_ticks: 1_680,
        })
        .expect("expanding the clip should reveal the retained events");
    assert_eq!(project.midi_items()[0].notes(), notes);
    assert_eq!(project.midi_items()[0].controllers(), controllers);

    let reopened = Project::from_snapshot(project.snapshot()).expect("snapshot should reopen");
    assert_eq!(reopened.midi_items()[0].notes(), notes);
    assert_eq!(reopened.midi_items()[0].controllers(), controllers);
}

#[test]
fn trimming_a_midi_clip_start_preserves_event_positions_and_source_bounds() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 1_920,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 64,
                tick: 960,
                duration: 480,
                velocity: 96,
            }],
        })
        .unwrap();

    project
        .apply(DawAction::TrimMidiItemStart {
            item_id,
            start_tick: 480,
            length_ticks: 1_440,
            source_offset_ticks: 480,
        })
        .unwrap();
    let trimmed = &project.midi_items()[0];
    assert_eq!(trimmed.start_tick(), 480);
    assert_eq!(trimmed.source_offset_ticks(), 480);
    assert_eq!(trimmed.project_tick_at_content_tick(960), Some(960));

    assert!(project.undo().unwrap());
    assert_eq!(project.midi_items()[0].source_offset_ticks(), 0);
    assert!(project.redo().unwrap());
    project
        .apply(DawAction::TrimMidiItemStart {
            item_id,
            start_tick: 0,
            length_ticks: 1_920,
            source_offset_ticks: 0,
        })
        .unwrap();
    let reopened = Project::from_snapshot(project.snapshot()).unwrap();
    assert_eq!(reopened.midi_items()[0].start_tick(), 0);
    assert_eq!(reopened.midi_items()[0].source_offset_ticks(), 0);
    assert_eq!(
        reopened.midi_items()[0].project_tick_at_content_tick(960),
        Some(960)
    );
}
