use aaadaw_core::{DawAction, MidiNoteData, Project, TempoCurve};
use aaadaw_engine::{MidiEventKind, MidiEventPlan, MidiScheduleError, ScheduledMidiEvent};

fn project_with_note(pitch: u8, tick: u64, duration: u64) -> (Project, aaadaw_core::TrackId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "MIDI".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3840,
        })
        .expect("MIDI item insertion should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch,
                tick,
                duration,
                velocity: 100,
            }],
        })
        .expect("MIDI note insertion should succeed");
    (project, track_id)
}

#[test]
fn event_plan_uses_tempo_map_and_queries_half_open_sample_blocks() {
    let (mut project, track_id) = project_with_note(69, 480, 240);
    project
        .apply(DawAction::SetTempo {
            start_tick: 960,
            bpm: 60.0,
        })
        .expect("tempo point should be accepted");
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: TempoCurve::Linear,
        })
        .expect("linear ramp should be accepted");
    let on_sample = project.sample_at_tick(480).expect("note start should map");
    let off_sample = project.sample_at_tick(720).expect("note end should map");
    let plan = MidiEventPlan::compile(&project).expect("valid project should compile");
    assert_eq!(plan.len(), 2);
    let mut output = [None; 2];

    assert_eq!(
        plan.events_for_block(on_sample, 1, &mut output)
            .expect("one-frame block query should succeed"),
        1
    );
    let event = output[0].expect("the event should be initialized");
    assert_eq!(event.kind, MidiEventKind::NoteOn);
    assert_eq!(event.track_id, track_id);
    assert_eq!(event.pitch, 69);
    assert_eq!(event.sample_offset, 0);

    assert_eq!(
        plan.events_for_block(off_sample, 1, &mut output)
            .expect("note-off block query should succeed"),
        1
    );
    let event = output[0].expect("the event should be initialized");
    assert_eq!(event.kind, MidiEventKind::NoteOff);
    assert_eq!(event.velocity, 0);
}

#[test]
fn event_blocks_exclude_the_end_sample_and_short_buffers_are_atomic() {
    let (project, _) = project_with_note(60, 0, 960);
    let plan = MidiEventPlan::compile(&project).expect("valid project should compile");
    let mut output = [None; 2];

    assert_eq!(
        plan.events_for_block(0, 24_000, &mut output)
            .expect("first half-open block query should succeed"),
        1
    );
    assert_eq!(
        output[0].expect("event should exist").kind,
        MidiEventKind::NoteOn
    );
    assert_eq!(
        plan.events_for_block(24_000, 1, &mut output)
            .expect("second block should contain the endpoint event"),
        1
    );
    assert_eq!(
        output[0].expect("event should exist").kind,
        MidiEventKind::NoteOff
    );

    let mut too_small = [None; 1];
    let before = too_small;
    assert_eq!(
        plan.events_for_block(0, 24_001, &mut too_small),
        Err(MidiScheduleError::OutputBufferTooSmall {
            required: 2,
            available: 1,
        })
    );
    assert_eq!(too_small, before);
}

#[test]
fn active_note_query_chases_only_notes_strictly_inside_their_sample_range() {
    let (project, track_id) = project_with_note(60, 0, 480);
    let note = project.midi_items()[0].notes()[0].id();
    let end_sample = project.sample_at_tick(480).expect("note end should map");
    let plan = MidiEventPlan::compile(&project).expect("valid project should compile");
    let mut output = [None; 2];

    assert_eq!(
        plan.active_notes_at(0, &mut output)
            .expect("note attack belongs to the regular schedule"),
        0
    );
    assert_eq!(
        plan.active_notes_at(1, &mut output)
            .expect("sustained note should be chased"),
        1
    );
    let chased = output[0].expect("chased note should be initialized");
    assert_eq!(chased.kind, MidiEventKind::NoteOn);
    assert_eq!(chased.sample_offset, 0);
    assert_eq!(chased.track_id, track_id);
    assert_eq!(chased.note_id, note);
    assert_eq!(
        plan.active_notes_at(end_sample, &mut output)
            .expect("note end belongs to the regular schedule"),
        0
    );

    let mut too_small = [];
    assert_eq!(
        plan.active_notes_at(1, &mut too_small),
        Err(MidiScheduleError::OutputBufferTooSmall {
            required: 1,
            available: 0,
        })
    );
}

#[test]
fn event_plan_filters_muted_and_non_solo_tracks() {
    let mut project = Project::new();
    for (index, name) in ["Muted", "Solo", "Other"].into_iter().enumerate() {
        project
            .apply(DawAction::CreateTrack {
                index,
                name: name.to_owned(),
            })
            .expect("track creation should succeed");
    }
    let track_ids: Vec<_> = project.tracks().iter().map(|track| track.id()).collect();
    project
        .apply(DawAction::SetTrackMute {
            track_id: track_ids[0],
            muted: true,
        })
        .expect("mute should succeed");
    project
        .apply(DawAction::SetTrackSolo {
            track_id: track_ids[1],
            solo: true,
        })
        .expect("solo should succeed");

    for track_id in track_ids.iter().copied() {
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 960,
            })
            .expect("MIDI item insertion should succeed");
        let item_id = project.midi_items().last().expect("item exists").id();
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
            .expect("MIDI note insertion should succeed");
    }

    let plan = MidiEventPlan::compile(&project).expect("valid project should compile");
    assert_eq!(plan.len(), 2);
    let mut output: [Option<ScheduledMidiEvent>; 4] = [None; 4];
    assert_eq!(
        plan.events_for_block(0, 12_001, &mut output)
            .expect("block query should succeed"),
        2
    );
    assert!(
        output[..2]
            .iter()
            .flatten()
            .all(|event| event.track_id == track_ids[1])
    );
    let mut chased = [None; 2];
    assert_eq!(
        plan.active_notes_at(1, &mut chased)
            .expect("active notes should be queryable at seek positions"),
        1
    );
    assert_eq!(
        chased[0].expect("solo note should be chased").track_id,
        track_ids[1]
    );
}
