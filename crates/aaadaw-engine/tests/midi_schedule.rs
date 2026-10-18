use aaadaw_core::{
    DawAction, MidiChannelPressureData, MidiControllerData, MidiItemPosition, MidiNoteData,
    MidiPitchBendData, MidiPolyAftertouchData, MidiProgramChangeData, Project, TempoCurve,
};
use aaadaw_engine::{
    MidiEventKind, MidiEventPlan, MidiScheduleError, MidiSegmentCursor, ScheduledMidiEvent,
    TransportBlockSegment,
};

fn project_with_note(pitch: u8, tick: u64, duration: u64) -> (Project, aaadaw_core::TrackId) {
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
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
                channel: 0,
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
fn event_plan_preserves_note_controller_and_pitch_bend_channels_through_chase() {
    let (mut project, track_id) = project_with_note(60, 0, 480);
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                channel: 7,
                tick: 0,
                duration: 480,
                velocity: 90,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                MidiControllerData {
                    controller: 7,
                    channel: 2,
                    tick: 0,
                    value: 80,
                },
                MidiControllerData {
                    controller: 7,
                    channel: 9,
                    tick: 0,
                    value: 96,
                },
            ],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![
                MidiPitchBendData {
                    channel: 3,
                    tick: 0,
                    value: 8_000,
                },
                MidiPitchBendData {
                    channel: 12,
                    tick: 0,
                    value: 9_000,
                },
            ],
        })
        .unwrap();
    let plan = MidiEventPlan::compile(&project).unwrap();
    let mut events = [None; 6];
    assert_eq!(plan.events_for_block(0, 1, &mut events).unwrap(), 6);
    let events = events.map(Option::unwrap);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == MidiEventKind::NoteOn)
            .map(|event| (event.track_id, event.channel))
            .collect::<Vec<_>>(),
        [(track_id, 0), (track_id, 7)]
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == MidiEventKind::ControllerChange)
            .map(|event| event.channel)
            .collect::<Vec<_>>(),
        [2, 9]
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == MidiEventKind::PitchBend)
            .map(|event| event.channel)
            .collect::<Vec<_>>(),
        [3, 12]
    );

    let mut chased = [None; 6];
    assert_eq!(plan.active_controllers_at(1, &mut chased).unwrap(), 2);
    assert_eq!(
        chased[..2]
            .iter()
            .flatten()
            .map(|event| event.channel)
            .collect::<Vec<_>>(),
        [2, 9]
    );
    assert_eq!(plan.active_pitch_bends_at(1, &mut chased).unwrap(), 2);
    assert_eq!(
        chased[..2]
            .iter()
            .flatten()
            .map(|event| event.channel)
            .collect::<Vec<_>>(),
        [3, 12]
    );
    assert_eq!(plan.active_notes_at(1, &mut chased).unwrap(), 2);
    assert_eq!(
        chased[..2]
            .iter()
            .flatten()
            .map(|event| event.channel)
            .collect::<Vec<_>>(),
        [0, 7]
    );
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
fn event_plan_preserves_fractional_ppq_item_positions_in_playback_samples() {
    let (mut project, track_id) = project_with_note(69, 0, 120);
    let item_id = project.midi_items()[0].id();
    let position = MidiItemPosition::new(964, 160).unwrap();
    project
        .apply(DawAction::SetMidiItemPosition { item_id, position })
        .expect("fractional MIDI placement should succeed");

    let expected_start = project
        .sample_at_tick_position(position.as_ticks_f64())
        .unwrap()
        .round() as u64;
    assert_eq!(expected_start, 24_120);
    let plan = MidiEventPlan::compile(&project).expect("valid project should compile");
    let mut output = [None; 2];
    assert_eq!(
        plan.events_for_block(24_100, 1, &mut output)
            .expect("the preceding whole-tick sample should query"),
        0
    );
    assert_eq!(
        plan.events_for_block(expected_start, 1, &mut output)
            .expect("fractional event sample should query"),
        1
    );
    let event = output[0].expect("the note-on should be present");
    assert_eq!(event.kind, MidiEventKind::NoteOn);
    assert_eq!(event.track_id, track_id);
    assert_eq!(event.sample_offset, 0);
}

#[test]
fn segmented_event_plan_releases_and_chases_notes_at_a_loop_wrap() {
    let (project, track_id) = project_with_note(69, 0, 100);
    let plan = MidiEventPlan::compile(&project).expect("valid project should compile");
    let segments = [
        TransportBlockSegment {
            output_offset: 0,
            start_sample: 25,
            frame_count: 5,
            chase_generation: 4,
        },
        TransportBlockSegment {
            output_offset: 5,
            start_sample: 25,
            frame_count: 5,
            chase_generation: 5,
        },
    ];
    let mut output = [None; 8];
    let mut aggregate = [None; 8];
    let mut temporary = [None; 8];

    let schedule = plan
        .events_for_segments(
            &segments,
            MidiSegmentCursor::default(),
            &mut output,
            &mut aggregate,
            &mut temporary,
        )
        .expect("two short loop segments should fit the preallocated buffers");

    assert_eq!(schedule.event_count, 3);
    assert_eq!(schedule.cursor.sample_end, Some(30));
    assert_eq!(schedule.cursor.chase_generation, Some(5));
    let events: Vec<_> = output[..schedule.event_count]
        .iter()
        .flatten()
        .copied()
        .collect();
    assert_eq!(
        events
            .iter()
            .map(|event| (event.sample_offset, event.kind, event.track_id))
            .collect::<Vec<_>>(),
        vec![
            (0, MidiEventKind::NoteOn, track_id),
            (5, MidiEventKind::NoteOff, track_id),
            (5, MidiEventKind::NoteOn, track_id),
        ]
    );
}

#[test]
fn segmented_event_plan_chases_controller_and_pitch_bend_after_a_loop_wrap() {
    let (mut project, track_id) = project_with_note(69, 0, 100);
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                MidiControllerData {
                    channel: 0,
                    controller: 1,
                    tick: 0,
                    value: 64,
                },
                MidiControllerData {
                    channel: 0,
                    controller: 64,
                    tick: 0,
                    value: 127,
                },
            ],
        })
        .expect("controller should be accepted");
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                channel: 0,
                tick: 0,
                value: 10_000,
            }],
        })
        .expect("pitch bend should be accepted");
    let plan = MidiEventPlan::compile(&project).expect("valid project should compile");
    let segments = [
        TransportBlockSegment {
            output_offset: 0,
            start_sample: 25,
            frame_count: 5,
            chase_generation: 4,
        },
        TransportBlockSegment {
            output_offset: 5,
            start_sample: 25,
            frame_count: 5,
            chase_generation: 5,
        },
    ];
    let mut output = [None; 12];
    let mut aggregate = [None; 12];
    let mut temporary = [None; 12];

    let schedule = plan
        .events_for_segments(
            &segments,
            MidiSegmentCursor::default(),
            &mut output,
            &mut aggregate,
            &mut temporary,
        )
        .expect("controller, pitch-bend, and note chase should fit");
    let events = output[..schedule.event_count]
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>();

    assert_eq!(schedule.event_count, 10);
    let boundary_events = events
        .iter()
        .filter(|event| event.sample_offset == 5)
        .collect::<Vec<_>>();
    assert_eq!(boundary_events.len(), 6);
    assert_eq!(boundary_events[0].controller, Some(64));
    assert_eq!(boundary_events[0].velocity, 0);
    assert_eq!(boundary_events[1].kind, MidiEventKind::NoteOff);
    assert!(events.iter().any(|event| {
        event.sample_offset == 5
            && event.track_id == track_id
            && event.kind == MidiEventKind::ControllerChange
            && event.controller == Some(1)
            && event.velocity == 64
    }));
    assert!(events.iter().any(|event| {
        event.sample_offset == 5
            && event.track_id == track_id
            && event.kind == MidiEventKind::ControllerChange
            && event.controller == Some(64)
            && event.velocity == 127
    }));
    assert!(events.iter().any(|event| {
        event.sample_offset == 5
            && event.track_id == track_id
            && event.kind == MidiEventKind::PitchBend
            && event.pitch_bend == Some(10_000)
    }));
    assert!(events.iter().any(|event| {
        event.sample_offset == 5
            && event.track_id == track_id
            && event.kind == MidiEventKind::NoteOff
            && event.velocity == 0
    }));
    assert!(events.iter().any(|event| {
        event.sample_offset == 5
            && event.track_id == track_id
            && event.kind == MidiEventKind::NoteOn
            && event.velocity == 100
    }));
}

#[test]
fn segmented_event_plan_keeps_output_unchanged_when_callback_buffer_is_too_small() {
    let (project, _) = project_with_note(69, 0, 100);
    let plan = MidiEventPlan::compile(&project).expect("valid project should compile");
    let segments = [
        TransportBlockSegment {
            output_offset: 0,
            start_sample: 25,
            frame_count: 5,
            chase_generation: 4,
        },
        TransportBlockSegment {
            output_offset: 5,
            start_sample: 25,
            frame_count: 5,
            chase_generation: 5,
        },
    ];
    let sentinel = ScheduledMidiEvent {
        sample_offset: 99,
        track_id: project.tracks()[0].id(),
        channel: 0,
        note_id: None,
        pitch: 1,
        velocity: 2,
        controller: None,
        pitch_bend: None,
        program: None,
        kind: MidiEventKind::NoteOn,
    };
    let mut output = [Some(sentinel); 2];
    let mut aggregate = [None; 8];
    let mut temporary = [None; 8];

    assert_eq!(
        plan.events_for_segments(
            &segments,
            MidiSegmentCursor::default(),
            &mut output,
            &mut aggregate,
            &mut temporary,
        ),
        Err(MidiScheduleError::OutputBufferTooSmall {
            required: 3,
            available: 2,
        })
    );
    assert_eq!(output, [Some(sentinel); 2]);
}

#[test]
fn event_plan_clips_notes_and_excludes_events_outside_the_midi_item() {
    let (mut project, track_id) = project_with_note(69, 720, 960);
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 72,
                tick: 1_200,
                duration: 120,
                velocity: 90,
            }],
        })
        .expect("the second note should fit before shortening the clip");
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                MidiControllerData {
                    channel: 0,
                    controller: 1,
                    tick: 840,
                    value: 64,
                },
                MidiControllerData {
                    channel: 0,
                    controller: 1,
                    tick: 1_200,
                    value: 100,
                },
            ],
        })
        .expect("controller events should be accepted");
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![
                MidiPitchBendData {
                    channel: 0,
                    tick: 840,
                    value: 9_000,
                },
                MidiPitchBendData {
                    channel: 0,
                    tick: 1_200,
                    value: 10_000,
                },
            ],
        })
        .expect("pitch bends should be accepted");
    project
        .apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .expect("the clip should retain hidden MIDI content");

    let plan = MidiEventPlan::compile(&project).expect("trimmed project should compile");
    assert_eq!(plan.len(), 4);
    let clipped_end = project.sample_at_tick(960).unwrap();
    let mut events = [None; 4];
    assert_eq!(
        plan.events_for_block(0, clipped_end as usize + 1, &mut events)
            .unwrap(),
        4
    );
    assert!(events.iter().any(|event| {
        event.is_some_and(|event| {
            event.track_id == track_id
                && event.pitch == 69
                && event.kind == MidiEventKind::NoteOff
                && event.sample_offset as u64 == clipped_end
        })
    }));
    assert!(events.iter().all(|event| {
        event.is_none_or(|event| {
            !(event.pitch == 72 || event.controller == Some(1) && event.velocity == 100)
        })
    }));
    assert_eq!(
        plan.active_notes_at(clipped_end, &mut events).unwrap(),
        0,
        "a note crossing the clip end must not remain active after the boundary"
    );
}

#[test]
fn event_plan_repeats_midi_notes_after_the_preserved_source_loop_length() {
    let (mut project, _) = project_with_note(60, 120, 90);
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::EditMidiItemPlacement {
            item_id,
            start_tick: 0,
            length_ticks: 8_000,
            source_offset_ticks: 3_600,
        })
        .expect("the visible MIDI clip should extend beyond its source loop");

    let plan = MidiEventPlan::compile(&project).expect("looped MIDI should compile");
    assert_eq!(plan.len(), 4);
    let block_end = project.sample_at_tick(8_000).unwrap();
    let mut events = [None; 4];
    assert_eq!(
        plan.events_for_block(0, block_end as usize + 1, &mut events)
            .unwrap(),
        4
    );
    let note_ons = events
        .iter()
        .flatten()
        .filter(|event| event.kind == MidiEventKind::NoteOn)
        .map(|event| event.sample_offset)
        .collect::<Vec<_>>();
    assert_eq!(
        note_ons,
        [
            project.sample_at_tick(360).unwrap() as usize,
            project.sample_at_tick(4_200).unwrap() as usize,
        ]
    );
}

#[test]
fn event_plan_stops_repeating_midi_source_when_loop_source_is_disabled() {
    let (mut project, _) = project_with_note(60, 120, 90);
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::EditMidiItemPlacement {
            item_id,
            start_tick: 0,
            length_ticks: 8_000,
            source_offset_ticks: 0,
        })
        .expect("the visible MIDI item should extend past its source loop");
    assert_eq!(project.midi_items()[0].source_loop_length_ticks(), 3_840);

    project
        .apply(DawAction::SetMidiItemLoopSource {
            item_id,
            enabled: false,
        })
        .expect("loop source should be disabled");
    assert_eq!(project.midi_items()[0].source_loop_length_ticks(), 3_840);

    let plan = MidiEventPlan::compile(&project).expect("non-looped MIDI should compile");
    assert_eq!(plan.len(), 2);
    let block_end = project.sample_at_tick(8_000).unwrap();
    let mut events = [None; 4];
    assert_eq!(
        plan.events_for_block(0, block_end as usize + 1, &mut events)
            .unwrap(),
        2
    );
    let note_ons = events
        .iter()
        .flatten()
        .filter(|event| event.kind == MidiEventKind::NoteOn)
        .map(|event| event.sample_offset)
        .collect::<Vec<_>>();
    assert_eq!(note_ons, [project.sample_at_tick(120).unwrap() as usize]);
}

#[test]
fn event_plan_applies_midi_source_offset_at_the_visible_start() {
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "MIDI".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 480,
            length_ticks: 1_920,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 120,
                    duration: 240,
                    velocity: 90,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 600,
                    duration: 720,
                    velocity: 90,
                },
            ],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                MidiControllerData {
                    channel: 0,
                    controller: 1,
                    tick: 120,
                    value: 10,
                },
                MidiControllerData {
                    channel: 0,
                    controller: 1,
                    tick: 600,
                    value: 80,
                },
            ],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![
                MidiPitchBendData {
                    channel: 0,
                    tick: 120,
                    value: 9_000,
                },
                MidiPitchBendData {
                    channel: 0,
                    tick: 600,
                    value: 11_000,
                },
            ],
        })
        .unwrap();
    project
        .apply(DawAction::TrimMidiItemStart {
            item_id,
            start_tick: 720,
            length_ticks: 720,
            source_offset_ticks: 240,
        })
        .unwrap();

    let plan = MidiEventPlan::compile(&project).unwrap();
    let start = project.sample_at_tick(720).unwrap();
    let end = project.sample_at_tick(1_440).unwrap();
    let mut events = [None; 8];
    let count = plan
        .events_for_block(start, end as usize - start as usize + 1, &mut events)
        .unwrap();
    let visible = &events[..count];
    assert_eq!(count, 4);
    assert!(
        visible
            .iter()
            .flatten()
            .all(|event| event.sample_offset as u64 <= end - start)
    );
    assert!(visible.iter().any(|event| {
        event.is_some_and(|event| event.pitch == 64 && event.kind == MidiEventKind::NoteOn)
    }));
    assert!(!visible.iter().any(|event| {
        event.is_some_and(|event| event.pitch == 60 && event.kind == MidiEventKind::NoteOn)
    }));
    assert!(visible.iter().any(|event| {
        event.is_some_and(|event| event.controller == Some(1) && event.velocity == 80)
    }));
    assert!(!visible.iter().any(|event| {
        event.is_some_and(|event| event.controller == Some(1) && event.velocity == 10)
    }));
}

#[test]
fn pitch_bend_schedule_preserves_14_bit_values_and_chases_the_latest_state() {
    let (mut project, track_id) = project_with_note(69, 480, 240);
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![
                MidiPitchBendData {
                    channel: 0,
                    tick: 0,
                    value: 8192,
                },
                MidiPitchBendData {
                    channel: 0,
                    tick: 960,
                    value: 16_383,
                },
            ],
        })
        .expect("pitch-bend points should be accepted");
    let bend_sample = project.sample_at_tick(960).expect("bend tick should map");
    let plan = MidiEventPlan::compile(&project).expect("valid project should compile");
    let mut output = [None; 4];

    assert_eq!(
        plan.events_for_block(bend_sample, 1, &mut output)
            .expect("bend point should be scheduled at its sample"),
        1
    );
    let event = output[0].unwrap();
    assert_eq!(event.kind, MidiEventKind::PitchBend);
    assert_eq!(event.track_id, track_id);
    assert_eq!(event.pitch_bend, Some(16_383));
    assert_eq!(event.controller, None);
    assert_eq!(event.sample_offset, 0);

    assert_eq!(
        plan.active_pitch_bends_at(bend_sample + 1, &mut output)
            .expect("seek chase should return the latest bend"),
        1
    );
    let chased = output[0].unwrap();
    assert_eq!(chased.kind, MidiEventKind::PitchBend);
    assert_eq!(chased.pitch_bend, Some(16_383));
    assert_eq!(chased.sample_offset, 0);
}

#[test]
fn program_change_schedule_uses_channel_status_and_chases_the_latest_program() {
    let (mut project, track_id) = project_with_note(69, 480, 240);
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::SetMidiProgramChanges {
            item_id,
            program_changes: vec![
                MidiProgramChangeData {
                    channel: 6,
                    tick: 0,
                    program: 7,
                },
                MidiProgramChangeData {
                    channel: 6,
                    tick: 960,
                    program: 41,
                },
            ],
        })
        .expect("program changes should be accepted");
    let change_sample = project.sample_at_tick(960).unwrap();
    let plan = MidiEventPlan::compile(&project).unwrap();
    let mut output = [None; 8];

    assert_eq!(plan.events_for_block(0, 1, &mut output).unwrap(), 1);
    let first = output[0].unwrap();
    assert_eq!(first.kind, MidiEventKind::ProgramChange);
    assert_eq!(first.track_id, track_id);
    assert_eq!(first.channel, 6);
    assert_eq!(first.program, Some(7));

    assert_eq!(
        plan.active_program_changes_at(change_sample + 1, &mut output)
            .unwrap(),
        1
    );
    let chased = output[0].unwrap();
    assert_eq!(chased.kind, MidiEventKind::ProgramChange);
    assert_eq!(chased.program, Some(41));
    assert_eq!(chased.sample_offset, 0);
}

#[test]
fn aftertouch_schedule_preserves_message_data_and_chases_latest_pressure() {
    let (mut project, track_id) = project_with_note(60, 960, 240);
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::SetMidiPolyAftertouches {
            item_id,
            poly_aftertouches: vec![
                MidiPolyAftertouchData {
                    channel: 6,
                    tick: 0,
                    note: 60,
                    value: 7,
                },
                MidiPolyAftertouchData {
                    channel: 6,
                    tick: 960,
                    note: 60,
                    value: 41,
                },
            ],
        })
        .expect("Poly Aftertouch events should be accepted");
    project
        .apply(DawAction::SetMidiChannelPressures {
            item_id,
            channel_pressures: vec![
                MidiChannelPressureData {
                    channel: 6,
                    tick: 0,
                    value: 13,
                },
                MidiChannelPressureData {
                    channel: 6,
                    tick: 960,
                    value: 74,
                },
            ],
        })
        .expect("Channel Pressure events should be accepted");

    let sample = project.sample_at_tick(960).unwrap();
    let plan = MidiEventPlan::compile(&project).unwrap();
    let mut events = [None; 8];
    assert_eq!(plan.events_for_block(sample, 1, &mut events).unwrap(), 3);
    let scheduled = events[..3]
        .iter()
        .map(|event| event.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(scheduled[0].kind, MidiEventKind::ChannelPressure);
    assert_eq!(scheduled[0].velocity, 74);
    assert_eq!(scheduled[1].kind, MidiEventKind::NoteOn);
    assert_eq!(scheduled[2].kind, MidiEventKind::PolyAftertouch);
    assert_eq!(scheduled[2].pitch, 60);
    assert_eq!(scheduled[2].velocity, 41);

    let mut chased = [None; 8];
    assert_eq!(
        plan.active_aftertouches_at(sample + 1, &mut chased)
            .unwrap(),
        2
    );
    assert!(chased[..2].iter().flatten().any(|event| {
        event.kind == MidiEventKind::PolyAftertouch && event.pitch == 60 && event.velocity == 41
    }));
    assert!(
        chased[..2]
            .iter()
            .flatten()
            .any(|event| { event.kind == MidiEventKind::ChannelPressure && event.velocity == 74 })
    );
    assert!(
        chased[..2]
            .iter()
            .flatten()
            .all(|event| event.track_id == track_id && event.sample_offset == 0)
    );
}

#[test]
fn program_change_follows_bank_select_and_precedes_note_on_at_the_same_tick() {
    let (mut project, track_id) = project_with_note(69, 960, 240);
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                MidiControllerData {
                    controller: 0,
                    channel: 6,
                    tick: 960,
                    value: 4,
                },
                MidiControllerData {
                    controller: 32,
                    channel: 6,
                    tick: 960,
                    value: 2,
                },
            ],
        })
        .expect("bank-select controller values should be accepted");
    project
        .apply(DawAction::SetMidiProgramChanges {
            item_id,
            program_changes: vec![MidiProgramChangeData {
                channel: 6,
                tick: 960,
                program: 41,
            }],
        })
        .expect("program change should be accepted");

    let sample = project.sample_at_tick(960).unwrap();
    let plan = MidiEventPlan::compile(&project).unwrap();
    let mut output = [None; 8];
    assert_eq!(plan.events_for_block(sample, 1, &mut output).unwrap(), 4);
    let events = output[..4]
        .iter()
        .map(|event| event.unwrap())
        .collect::<Vec<_>>();
    assert!(events.iter().all(|event| event.track_id == track_id));
    assert!(events[..3].iter().all(|event| event.channel == 6));
    assert_eq!(events[3].channel, 0);
    assert_eq!(events[0].kind, MidiEventKind::ControllerChange);
    assert_eq!(events[0].controller, Some(0));
    assert_eq!(events[1].kind, MidiEventKind::ControllerChange);
    assert_eq!(events[1].controller, Some(32));
    assert_eq!(events[2].kind, MidiEventKind::ProgramChange);
    assert_eq!(events[2].program, Some(41));
    assert_eq!(events[3].kind, MidiEventKind::NoteOn);
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
    assert_eq!(chased.note_id, Some(note));
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
fn midi_controller_schedule_is_sample_accurate_and_chases_latest_state() {
    let (mut project, track_id) = project_with_note(60, 480, 480);
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                MidiControllerData {
                    channel: 0,
                    controller: 64,
                    tick: 0,
                    value: 127,
                },
                MidiControllerData {
                    channel: 0,
                    controller: 64,
                    tick: 960,
                    value: 0,
                },
                MidiControllerData {
                    channel: 0,
                    controller: 1,
                    tick: 480,
                    value: 64,
                },
                MidiControllerData {
                    channel: 0,
                    controller: 11,
                    tick: 240,
                    value: 96,
                },
                MidiControllerData {
                    channel: 0,
                    controller: 7,
                    tick: 120,
                    value: 88,
                },
                MidiControllerData {
                    channel: 0,
                    controller: 10,
                    tick: 360,
                    value: 32,
                },
            ],
        })
        .expect("MIDI controller events should be accepted");
    let attack_sample = project.sample_at_tick(480).expect("tick should map");
    let expression_sample = project.sample_at_tick(240).expect("tick should map");
    let volume_sample = project.sample_at_tick(120).expect("tick should map");
    let pan_sample = project.sample_at_tick(360).expect("tick should map");
    let release_sample = project.sample_at_tick(960).expect("tick should map");
    let plan = MidiEventPlan::compile(&project).expect("valid project should compile");
    let mut output = [None; 8];

    assert_eq!(
        plan.events_for_block(expression_sample, 1, &mut output)
            .expect("CC11 event should query at its scheduled sample"),
        1
    );
    let expression = output[0].expect("CC11 should be scheduled");
    assert_eq!(expression.controller, Some(11));
    assert_eq!(expression.sample_offset, 0);
    assert_eq!(expression.velocity, 96);

    assert_eq!(
        plan.events_for_block(volume_sample, 1, &mut output)
            .expect("CC7 event should query at its scheduled sample"),
        1
    );
    let volume = output[0].expect("CC7 should be scheduled");
    assert_eq!(volume.controller, Some(7));
    assert_eq!(volume.sample_offset, 0);
    assert_eq!(volume.velocity, 88);

    assert_eq!(
        plan.events_for_block(pan_sample, 1, &mut output)
            .expect("CC10 event should query at its scheduled sample"),
        1
    );
    let pan = output[0].expect("CC10 should be scheduled");
    assert_eq!(pan.controller, Some(10));
    assert_eq!(pan.sample_offset, 0);
    assert_eq!(pan.velocity, 32);

    assert_eq!(
        plan.events_for_block(attack_sample, 1, &mut output)
            .expect("events at the note attack should query"),
        2
    );
    assert_eq!(output[0].unwrap().kind, MidiEventKind::ControllerChange);
    assert_eq!(output[0].unwrap().controller, Some(1));
    assert_eq!(output[1].unwrap().kind, MidiEventKind::NoteOn);
    assert_eq!(
        plan.active_controllers_at(attack_sample, &mut output)
            .expect("controller state before the attack should query"),
        4
    );
    let chased = output[0].unwrap();
    assert_eq!(chased.kind, MidiEventKind::ControllerChange);
    assert_eq!(chased.track_id, track_id);
    assert_eq!(chased.controller, Some(64));
    assert_eq!(chased.velocity, 127);
    assert!(
        output[..4]
            .iter()
            .flatten()
            .any(|event| { event.controller == Some(11) && event.velocity == 96 })
    );
    assert!(
        output[..4]
            .iter()
            .flatten()
            .any(|event| { event.controller == Some(10) && event.velocity == 32 })
    );
    assert!(
        output[..4]
            .iter()
            .flatten()
            .any(|event| { event.controller == Some(7) && event.velocity == 88 })
    );

    assert_eq!(
        plan.events_for_block(release_sample, 1, &mut output)
            .expect("controller-off point should query at its sample"),
        2
    );
    assert_eq!(output[0].unwrap().kind, MidiEventKind::NoteOff);
    assert_eq!(output[1].unwrap().kind, MidiEventKind::ControllerChange);
    assert_eq!(output[1].unwrap().velocity, 0);
    assert_eq!(
        plan.active_controllers_at(release_sample + 1, &mut output)
            .expect("controller state after release should query"),
        5
    );
    assert!(
        output[..5]
            .iter()
            .flatten()
            .any(|event| { event.controller == Some(64) && event.velocity == 0 })
    );
    assert!(
        output[..5]
            .iter()
            .flatten()
            .any(|event| { event.controller == Some(1) && event.velocity == 64 })
    );
    assert!(
        output[..5]
            .iter()
            .flatten()
            .any(|event| { event.controller == Some(11) && event.velocity == 96 })
    );
    assert!(
        output[..5]
            .iter()
            .flatten()
            .any(|event| { event.controller == Some(7) && event.velocity == 88 })
    );
    assert!(
        output[..5]
            .iter()
            .flatten()
            .any(|event| { event.controller == Some(10) && event.velocity == 32 })
    );
}

#[test]
fn active_note_interval_index_matches_brute_force_for_overlapping_ranges() {
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
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
            length_ticks: 5_000,
        })
        .expect("MIDI item insertion should succeed");
    let item_id = project.midi_items()[0].id();
    let notes: Vec<_> = (0..64)
        .map(|index| MidiNoteData {
            channel: 0,
            pitch: index,
            tick: u64::from(index) * 37,
            duration: 83 + u64::from(index % 11) * 41,
            velocity: 90,
        })
        .collect();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: notes.clone(),
        })
        .expect("MIDI notes should be accepted");
    let plan = MidiEventPlan::compile(&project).expect("valid project should compile");
    let mut output = [None; 64];

    for tick in (0..3_000).step_by(19) {
        let sample = project
            .sample_at_tick(tick)
            .expect("query tick should map to a sample");
        let mut expected: Vec<_> = notes
            .iter()
            .filter(|note| note.tick < tick && tick < note.tick + note.duration)
            .map(|note| note.pitch)
            .collect();
        let count = plan
            .active_notes_at(sample, &mut output)
            .expect("interval query should fit its preallocated note buffer");
        let mut actual: Vec<_> = output[..count]
            .iter()
            .flatten()
            .map(|event| event.pitch)
            .collect();
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected, "active notes at tick {tick}");
    }
}

#[test]
fn event_plan_filters_muted_and_non_solo_tracks() {
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
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
                    channel: 0,
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

#[test]
fn event_plan_omits_muted_midi_items_without_changing_other_items() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    let mut item_ids = Vec::new();
    for pitch in [48, 60] {
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 960,
            })
            .expect("MIDI item should insert");
        let item_id = project.midi_items().last().unwrap().id();
        item_ids.push(item_id);
        project
            .apply(DawAction::AddMidiNotes {
                item_id,
                notes: vec![MidiNoteData {
                    channel: 0,
                    pitch,
                    tick: 0,
                    duration: 480,
                    velocity: 100,
                }],
            })
            .expect("MIDI note should insert");
    }
    project
        .apply(DawAction::SetMidiItemMute {
            item_id: item_ids[0],
            muted: true,
        })
        .expect("first MIDI item should mute");

    let plan = MidiEventPlan::compile(&project).expect("muted project should compile");
    let mut events = [None; 4];
    assert_eq!(plan.events_for_block(0, 24_000, &mut events).unwrap(), 2);
    assert!(events[..2].iter().flatten().all(|event| event.pitch == 60));
    let mut active = [None; 2];
    assert_eq!(plan.active_notes_at(1, &mut active).unwrap(), 1);
    assert_eq!(active[0].unwrap().pitch, 60);
}

#[test]
fn soloed_bus_keeps_midi_events_on_tracks_routed_into_it() {
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Synth".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::CreateBusTrack {
            index: 1,
            name: "Synth Bus".to_owned(),
        })
        .unwrap();
    let source = project.tracks()[0].id();
    let bus = project.tracks()[1].id();
    project
        .apply(DawAction::SetTrackOutput {
            track_id: source,
            output_track: Some(bus),
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackSolo {
            track_id: bus,
            solo: true,
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id: source,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
            }],
        })
        .unwrap();

    let plan = MidiEventPlan::compile(&project).unwrap();
    assert_eq!(plan.len(), 4);
    let mut events = [None; 4];
    assert_eq!(plan.events_for_block(0, 1, &mut events).unwrap(), 2);
    let event_tracks: std::collections::HashSet<_> = events[..2]
        .iter()
        .map(|event| event.unwrap().track_id)
        .collect();
    assert_eq!(event_tracks, [source, bus].into_iter().collect());

    let mut active = [None; 2];
    assert_eq!(plan.active_notes_at(1, &mut active).unwrap(), 2);
    assert_eq!(
        active
            .iter()
            .map(|event| event.unwrap().track_id)
            .collect::<std::collections::HashSet<_>>(),
        [source, bus].into_iter().collect()
    );
}

#[test]
fn folder_parent_midi_forwarding_obeys_each_main_send_switch() {
    for (disable_child_send, disable_inner_send, expected_tracks) in
        [(false, false, 3), (true, false, 1), (false, true, 2)]
    {
        let mut project = Project::default();
        for (index, name) in [(0, "Synth"), (1, "Inner"), (2, "Outer")] {
            project
                .apply(DawAction::CreateTrack {
                    index,
                    name: name.into(),
                })
                .unwrap();
        }
        let source = project.tracks()[0].id();
        let inner = project.tracks()[1].id();
        let outer = project.tracks()[2].id();
        for folder in [inner, outer] {
            project
                .apply(DawAction::SetTrackFolder {
                    track_id: folder,
                    enabled: true,
                })
                .unwrap();
        }
        project
            .apply(DawAction::SetTrackParent {
                track_id: source,
                parent: Some(inner),
            })
            .unwrap();
        project
            .apply(DawAction::SetTrackParent {
                track_id: inner,
                parent: Some(outer),
            })
            .unwrap();
        if disable_child_send {
            project
                .apply(DawAction::SetTrackMainSend {
                    track_id: source,
                    enabled: false,
                })
                .unwrap();
        }
        if disable_inner_send {
            project
                .apply(DawAction::SetTrackMainSend {
                    track_id: inner,
                    enabled: false,
                })
                .unwrap();
        }
        project
            .apply(DawAction::InsertMidiItem {
                track_id: source,
                start_tick: 0,
                length_ticks: 960,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(DawAction::AddMidiNotes {
                item_id,
                notes: vec![MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 0,
                    duration: 240,
                    velocity: 100,
                }],
            })
            .unwrap();

        let plan = MidiEventPlan::compile(&project).unwrap();
        assert_eq!(plan.len(), expected_tracks * 2);
        let mut attacks = vec![None; expected_tracks];
        assert_eq!(
            plan.events_for_block(0, 1, &mut attacks).unwrap(),
            expected_tracks
        );
        let actual_tracks: std::collections::HashSet<_> = attacks
            .iter()
            .map(|event| event.unwrap().track_id)
            .collect();
        let expected: std::collections::HashSet<_> = match (disable_child_send, disable_inner_send)
        {
            (false, false) => [source, inner, outer].into_iter().collect(),
            (true, false) => [source].into_iter().collect(),
            (false, true) => [source, inner].into_iter().collect(),
            (true, true) => unreachable!(),
        };
        assert_eq!(actual_tracks, expected);

        let mut chased = vec![None; expected_tracks];
        assert_eq!(
            plan.active_notes_at(1, &mut chased).unwrap(),
            expected_tracks
        );
        assert_eq!(
            chased
                .iter()
                .map(|event| event.unwrap().track_id)
                .collect::<std::collections::HashSet<_>>(),
            expected
        );

        let mut releases = vec![None; expected_tracks];
        assert_eq!(
            plan.events_for_block(6_000, 1, &mut releases).unwrap(),
            expected_tracks
        );
        assert!(
            releases
                .iter()
                .all(|event| { event.is_some_and(|event| event.kind == MidiEventKind::NoteOff) })
        );
        assert_eq!(
            releases
                .iter()
                .map(|event| event.unwrap().track_id)
                .collect::<std::collections::HashSet<_>>(),
            expected
        );
    }
}

#[test]
fn soloed_audio_send_receiver_routes_midi_to_source_and_destination_tracks() {
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Synth".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Synth Bus".to_owned(),
        })
        .unwrap();
    let source = project.tracks()[0].id();
    let bus = project.tracks()[1].id();
    project
        .apply(DawAction::CreateAudioSend {
            track_id: source,
            destination: bus,
            parameters: aaadaw_core::AudioSendParameters::default(),
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackSolo {
            track_id: bus,
            solo: true,
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id: source,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
            }],
        })
        .unwrap();

    let plan = MidiEventPlan::compile(&project).unwrap();
    assert_eq!(plan.len(), 4);
    let mut events = [None; 4];
    assert_eq!(plan.events_for_block(0, 1, &mut events).unwrap(), 2);
    let tracks: std::collections::HashSet<_> = events[..2]
        .iter()
        .map(|event| event.unwrap().track_id)
        .collect();
    assert_eq!(tracks, [source, bus].into_iter().collect());
    let mut active = [None; 2];
    assert_eq!(plan.active_notes_at(1, &mut active).unwrap(), 2);
    let active_tracks: std::collections::HashSet<_> =
        active.iter().map(|event| event.unwrap().track_id).collect();
    assert_eq!(active_tracks, [source, bus].into_iter().collect());
}

#[test]
fn parallel_midi_sends_keep_distinct_note_ids_through_attack_release_and_chase() {
    let (mut project, source) = project_with_note(60, 0, 240);
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Instrument".to_owned(),
        })
        .unwrap();
    let destination = project.tracks()[1].id();
    for _ in 0..2 {
        project
            .apply(DawAction::CreateAudioSend {
                track_id: source,
                destination,
                parameters: aaadaw_core::AudioSendParameters::default(),
            })
            .unwrap();
    }
    let source_note_id = project.midi_items()[0].notes()[0].id();
    let plan = MidiEventPlan::compile(&project).unwrap();
    let mut attacks = [None; 3];
    assert_eq!(plan.events_for_block(0, 1, &mut attacks).unwrap(), 3);
    let routed_attack_ids: std::collections::HashSet<_> = attacks
        .iter()
        .map(|event| event.unwrap())
        .filter(|event| event.track_id == destination)
        .map(|event| event.note_id.unwrap())
        .collect();
    assert_eq!(routed_attack_ids.len(), 2);
    assert!(!routed_attack_ids.contains(&source_note_id));
    assert!(routed_attack_ids.iter().all(|id| id.value() < 1 << 30));

    let mut chased = [None; 3];
    assert_eq!(plan.active_notes_at(1, &mut chased).unwrap(), 3);
    let chased_route_ids: std::collections::HashSet<_> = chased
        .iter()
        .map(|event| event.unwrap())
        .filter(|event| event.track_id == destination)
        .map(|event| event.note_id.unwrap())
        .collect();
    assert_eq!(chased_route_ids, routed_attack_ids);

    let mut releases = [None; 3];
    assert_eq!(plan.events_for_block(6_000, 1, &mut releases).unwrap(), 3);
    let released_route_ids: std::collections::HashSet<_> = releases
        .iter()
        .map(|event| event.unwrap())
        .filter(|event| event.track_id == destination)
        .map(|event| event.note_id.unwrap())
        .collect();
    assert_eq!(released_route_ids, routed_attack_ids);
}

#[test]
fn midi_send_routes_notes_controls_and_bends_over_nested_paths() {
    for (muted, midi_enabled) in [(false, true), (true, true), (false, false)] {
        let (mut project, source) = project_with_note(60, 0, 240);
        for (index, name) in [(1, "Intermediate"), (2, "Solo receiver")] {
            project
                .apply(DawAction::CreateTrack {
                    index,
                    name: name.into(),
                })
                .unwrap();
        }
        let mid = project.tracks()[1].id();
        let receiver = project.tracks()[2].id();
        project
            .apply(DawAction::CreateAudioSend {
                track_id: source,
                destination: mid,
                parameters: aaadaw_core::AudioSendParameters {
                    muted,
                    midi_enabled,
                    ..Default::default()
                },
            })
            .unwrap();
        project
            .apply(DawAction::CreateAudioSend {
                track_id: mid,
                destination: receiver,
                parameters: Default::default(),
            })
            .unwrap();
        project
            .apply(DawAction::SetTrackSolo {
                track_id: receiver,
                solo: true,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(DawAction::SetMidiControllers {
                item_id,
                controllers: vec![MidiControllerData {
                    channel: 0,
                    controller: 64,
                    tick: 0,
                    value: 127,
                }],
            })
            .unwrap();
        project
            .apply(DawAction::SetMidiPitchBends {
                item_id,
                pitch_bends: vec![MidiPitchBendData {
                    channel: 0,
                    tick: 0,
                    value: 9000,
                }],
            })
            .unwrap();
        let plan = MidiEventPlan::compile(&project).unwrap();
        let mut events = [None; 9];
        let count = plan.events_for_block(0, 1, &mut events).unwrap();
        assert_eq!(count, if muted || !midi_enabled { 0 } else { 9 });
        if count > 0 {
            let tracks: std::collections::HashSet<_> = events[..count]
                .iter()
                .map(|event| event.unwrap().track_id)
                .collect();
            assert_eq!(tracks, [source, mid, receiver].into_iter().collect());
            for track_id in tracks {
                let forwarded: Vec<_> = events[..count]
                    .iter()
                    .filter_map(|event| event.filter(|event| event.track_id == track_id))
                    .collect();
                assert_eq!(forwarded.len(), 3);
                assert!(
                    forwarded
                        .iter()
                        .any(|event| event.kind == MidiEventKind::NoteOn)
                );
                assert!(forwarded.iter().any(|event| {
                    event.kind == MidiEventKind::ControllerChange && event.controller == Some(64)
                }));
                assert!(
                    forwarded
                        .iter()
                        .any(|event| event.kind == MidiEventKind::PitchBend)
                );
            }
            let mut chased_controllers = [None; 3];
            assert_eq!(
                plan.active_controllers_at(1, &mut chased_controllers)
                    .unwrap(),
                3
            );
            let controller_tracks: std::collections::HashSet<_> = chased_controllers
                .iter()
                .map(|event| event.unwrap().track_id)
                .collect();
            assert_eq!(
                controller_tracks,
                [source, mid, receiver].into_iter().collect()
            );
            let mut chased_bends = [None; 3];
            assert_eq!(plan.active_pitch_bends_at(1, &mut chased_bends).unwrap(), 3);
            let bend_tracks: std::collections::HashSet<_> = chased_bends
                .iter()
                .map(|event| event.unwrap().track_id)
                .collect();
            assert_eq!(bend_tracks, [source, mid, receiver].into_iter().collect());
        }
    }
}

#[test]
fn soloed_folder_keeps_events_on_child_instruments_through_parent_send() {
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Synth".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Synth Bus".to_owned(),
        })
        .unwrap();
    let source = project.tracks()[0].id();
    let bus = project.tracks()[1].id();
    project
        .apply(DawAction::SetTrackFolder {
            track_id: bus,
            enabled: true,
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackParent {
            track_id: source,
            parent: Some(bus),
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackSolo {
            track_id: bus,
            solo: true,
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id: source,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
            }],
        })
        .unwrap();

    let plan = MidiEventPlan::compile(&project).unwrap();
    assert_eq!(plan.len(), 4);
    let mut events = [None; 4];
    assert_eq!(plan.events_for_block(0, 1, &mut events).unwrap(), 2);
    assert_eq!(
        events[..2]
            .iter()
            .map(|event| event.unwrap().track_id)
            .collect::<std::collections::HashSet<_>>(),
        [source, bus].into_iter().collect()
    );
}
