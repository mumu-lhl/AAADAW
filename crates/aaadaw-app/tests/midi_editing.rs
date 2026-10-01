use aaadaw_app::{
    MidiEditError, add_quarter_note, adjust_midi_note_pitch, adjust_midi_note_velocity,
    create_four_beat_midi_item, move_midi_item_by_beat, move_midi_note_by_sixteenth,
    quantize_midi_item_to_sixteenth,
};
use aaadaw_core::{DawAction, Project};

fn project_with_track() -> (Project, aaadaw_core::TrackId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "MIDI".to_owned(),
        })
        .expect("track should be created");
    let track_id = project.tracks()[0].id();
    (project, track_id)
}

#[test]
fn four_beat_items_append_after_existing_items() {
    let (mut project, _) = project_with_track();
    let action = create_four_beat_midi_item(&project).expect("item should be created");
    project
        .apply(action)
        .expect("first item action should apply");
    let action = create_four_beat_midi_item(&project).expect("second item should be created");
    project
        .apply(action)
        .expect("second item action should apply");

    assert_eq!(project.midi_items()[0].start_tick(), 0);
    assert_eq!(project.midi_items()[1].start_tick(), 3_840);
    assert_eq!(project.midi_items()[1].length_ticks(), 3_840);

    let item_id = project.midi_items()[1].id();
    let action = move_midi_item_by_beat(&project, item_id, -1).expect("item should move left");
    project.apply(action).expect("move action should apply");
    assert_eq!(project.midi_items()[1].start_tick(), 2_880);
}

#[test]
fn quarter_notes_fill_clip_and_note_edits_are_bounded_and_quantizable() {
    let (mut project, _) = project_with_track();
    let action = create_four_beat_midi_item(&project).expect("item should be created");
    project.apply(action).expect("item action should apply");
    let item_id = project.midi_items()[0].id();

    for tick in [0, 960, 1_920, 2_880] {
        let action = add_quarter_note(&project, item_id).expect("quarter note should fit");
        project.apply(action).expect("note action should apply");
        assert_eq!(
            project.midi_items()[0]
                .notes()
                .last()
                .expect("the inserted note should exist")
                .tick(),
            tick
        );
    }
    assert_eq!(
        add_quarter_note(&project, item_id),
        Err(MidiEditError::ItemHasNoRoomForNote)
    );

    let note_id = project.midi_items()[0].notes()[0].id();
    assert_eq!(
        move_midi_note_by_sixteenth(&project, item_id, note_id, -1),
        Err(MidiEditError::CannotMoveBeforeTimelineStart)
    );
    let action = move_midi_note_by_sixteenth(&project, item_id, note_id, 1)
        .expect("note should move by a sixteenth");
    project.apply(action).expect("note move should apply");
    assert_eq!(project.midi_items()[0].notes()[0].tick(), 240);

    let action = adjust_midi_note_pitch(&project, item_id, note_id, 100)
        .expect("pitch action should be created");
    project.apply(action).expect("pitch edit should apply");
    let action = adjust_midi_note_velocity(&project, item_id, note_id, -100)
        .expect("velocity action should be created");
    project.apply(action).expect("velocity edit should apply");
    assert_eq!(project.midi_items()[0].notes()[0].pitch(), 127);
    assert_eq!(project.midi_items()[0].notes()[0].velocity(), 0);

    let action = quantize_midi_item_to_sixteenth(item_id).expect("grid action should be created");
    project.apply(action).expect("quantize should apply");
    assert_eq!(project.midi_items()[0].notes()[0].tick(), 240);
}

#[test]
fn midi_item_creation_and_note_edits_report_missing_domain_objects() {
    let project = Project::new();
    assert_eq!(
        create_four_beat_midi_item(&project),
        Err(MidiEditError::NoTrack)
    );
}
