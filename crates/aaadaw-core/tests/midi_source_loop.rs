use aaadaw_core::{DawAction, MidiNoteData, Project};

#[test]
fn midi_source_loop_length_and_offset_map_events_across_the_visible_clip() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Looped MIDI".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id: project.tracks()[0].id(),
            start_tick: 100,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 120,
                duration: 90,
                velocity: 100,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 100,
            length_ticks: 2_200,
        })
        .unwrap();
    project
        .apply(DawAction::EditMidiItemPlacement {
            item_id,
            start_tick: 100,
            length_ticks: 2_200,
            source_offset_ticks: 720,
        })
        .unwrap();

    let item = &project.midi_items()[0];
    assert!(item.is_loop_source());
    assert_eq!(item.source_loop_length_ticks(), 960);
    assert_eq!(item.source_tick_at_project_tick(100), Some(720));
    assert_eq!(item.source_tick_at_project_tick(460), Some(120));
    assert_eq!(
        item.project_ticks_at_content_tick(120).collect::<Vec<_>>(),
        [460, 1_420]
    );
}
