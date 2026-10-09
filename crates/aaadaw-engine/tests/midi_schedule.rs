use aaadaw_core::{
    DawAction, MidiControllerData, MidiNoteData, MidiPitchBendData, Project, TempoCurve,
};
use aaadaw_engine::{MidiEventKind, MidiEventPlan, MidiScheduleError, ScheduledMidiEvent};

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
fn event_plan_clips_notes_and_excludes_events_outside_the_midi_item() {
    let (mut project, track_id) = project_with_note(69, 720, 960);
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
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
                    controller: 1,
                    tick: 840,
                    value: 64,
                },
                MidiControllerData {
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
                    tick: 840,
                    value: 9_000,
                },
                MidiPitchBendData {
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
                    pitch: 60,
                    tick: 120,
                    duration: 240,
                    velocity: 90,
                },
                MidiNoteData {
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
                    controller: 1,
                    tick: 120,
                    value: 10,
                },
                MidiControllerData {
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
                    tick: 120,
                    value: 9_000,
                },
                MidiPitchBendData {
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
                    tick: 0,
                    value: 8192,
                },
                MidiPitchBendData {
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
                    controller: 64,
                    tick: 0,
                    value: 127,
                },
                MidiControllerData {
                    controller: 64,
                    tick: 960,
                    value: 0,
                },
                MidiControllerData {
                    controller: 1,
                    tick: 480,
                    value: 64,
                },
                MidiControllerData {
                    controller: 11,
                    tick: 240,
                    value: 96,
                },
                MidiControllerData {
                    controller: 7,
                    tick: 120,
                    value: 88,
                },
                MidiControllerData {
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
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
            }],
        })
        .unwrap();

    let plan = MidiEventPlan::compile(&project).unwrap();
    assert_eq!(plan.len(), 2);
    let mut events = [None; 2];
    assert_eq!(plan.events_for_block(0, 1, &mut events).unwrap(), 1);
    assert_eq!(events[0].unwrap().track_id, source);
}

#[test]
fn soloed_audio_send_receiver_keeps_source_instrument_events_without_copying_midi() {
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
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
            }],
        })
        .unwrap();

    let plan = MidiEventPlan::compile(&project).unwrap();
    assert_eq!(plan.len(), 2);
    let mut events = [None; 2];
    assert_eq!(plan.events_for_block(0, 1, &mut events).unwrap(), 1);
    assert_eq!(events[0].unwrap().track_id, source);
}

#[test]
fn midi_source_audibility_follows_nested_unmuted_audio_sends_without_copying_events() {
    for muted in [false, true] {
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
                    tick: 0,
                    value: 9000,
                }],
            })
            .unwrap();
        let plan = MidiEventPlan::compile(&project).unwrap();
        let mut events = [None; 4];
        let count = plan.events_for_block(0, 1, &mut events).unwrap();
        assert_eq!(count, if muted { 0 } else { 3 });
        assert!(
            events[..count]
                .iter()
                .all(|event| event.unwrap().track_id == source)
        );
    }
}
