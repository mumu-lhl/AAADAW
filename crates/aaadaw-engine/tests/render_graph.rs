use aaadaw_core::{DawAction, Project};
use aaadaw_engine::{
    AudioBlock, AudioGraphError, AudioItemStream, AudioRenderGraph, AudioRenderStats,
    PcmStreamError, pcm_stream,
};

#[test]
fn render_graph_mixes_streamed_pcm_and_reports_underruns() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::SetTrackPan {
            track_id,
            pan: -1.0,
        })
        .expect("hard-left pan should succeed");

    let (mut producer, consumer) = pcm_stream(4).expect("positive queue size is valid");
    assert_eq!(producer.push_samples(&[0.25, 0.5]), 2);
    let mut graph = AudioRenderGraph::new(&project, vec![consumer], 8)
        .expect("one input stream should match the track count");
    graph.transport_mut().start();
    let mut output = [[9.0_f32, 9.0_f32]; 3];

    let stats = graph
        .render_into(&mut output)
        .expect("a valid callback block should render");

    assert_eq!(
        stats,
        AudioRenderStats {
            block: AudioBlock {
                start_sample: 0,
                frame_count: 3,
                is_playing: true,
            },
            underrun_samples: 1,
            midi_event_count: 0,
            master_guarded_samples: 0,
            master_non_finite_samples: 0,
        }
    );
    assert_eq!(output, [[0.25, 0.0], [0.5, 0.0], [0.0, 0.0]]);
    assert_eq!(graph.transport_mut().position_samples(), 3);

    graph.transport_mut().stop();
    output.fill([9.0, 9.0]);
    let stopped = graph
        .render_into(&mut output[..2])
        .expect("stopped block should render silence");
    assert_eq!(stopped.underrun_samples, 0);
    assert_eq!(output[..2], [[0.0, 0.0]; 2]);
    assert_eq!(graph.transport_mut().position_samples(), 3);
}

#[test]
fn live_mix_controller_updates_a_running_graph_on_the_next_block() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    let (mut producer, consumer) = pcm_stream(8).expect("positive queue size is valid");
    assert_eq!(producer.push_samples(&[1.0; 8]), 8);
    let mut graph = AudioRenderGraph::new(&project, vec![consumer], 8)
        .expect("one input stream should match the track count");
    let mix = graph.track_mix_controller();
    graph.transport_mut().start();
    let mut output = [[0.0_f32, 0.0_f32]; 2];

    graph
        .render_into(&mut output)
        .expect("initial block should render");
    let center = std::f32::consts::FRAC_1_SQRT_2;
    for frame in output {
        assert!((frame[0] - center).abs() < 1.0e-6);
        assert!((frame[1] - center).abs() < 1.0e-6);
    }

    assert!(mix.set_track_mix(track_id, -6.0, 1.0));
    graph
        .render_into(&mut output)
        .expect("updated block should render");
    let right_gain = 10.0_f32.powf(-6.0 / 20.0);
    for frame in output {
        assert_eq!(frame[0], 0.0);
        assert!((frame[1] - right_gain).abs() < 1.0e-6);
    }
    assert_eq!(graph.transport_mut().position_samples(), 4);
}

#[test]
fn master_sample_peak_ceiling_applies_after_mixing_and_updates_live() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Hot audio".to_owned(),
        })
        .expect("track creation should succeed");
    let (mut producer, consumer) = pcm_stream(4).expect("positive queue capacity is valid");
    assert_eq!(producer.push_samples(&[0.5, 1.5, -2.0]), 3);
    let mut graph = AudioRenderGraph::new(&project, vec![consumer], 4)
        .expect("one stream should match the track count");
    let safety = graph.master_output_safety_controller();
    safety
        .set_ceiling_dbfs(-6.0)
        .expect("a supported ceiling should be accepted");
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 3];

    let stats = graph
        .render_into(&mut output)
        .expect("the guarded graph should render");

    let center = std::f32::consts::FRAC_1_SQRT_2;
    let ceiling = 10.0_f32.powf(-6.0 / 20.0);
    assert!((output[0][0] - 0.5 * center).abs() < 1.0e-6);
    assert!((output[0][1] - 0.5 * center).abs() < 1.0e-6);
    assert_eq!(output[1], [ceiling; 2]);
    assert_eq!(output[2], [-ceiling; 2]);
    assert_eq!(stats.master_guarded_samples, 4);
    assert_eq!(stats.master_non_finite_samples, 0);
}

#[test]
fn audio_item_streams_respect_sample_clock_start_and_end_positions() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://clip".to_owned(),
            start_sample: 2,
            source_offset_samples: 0,
            length_samples: 2,
        })
        .expect("audio item insertion should succeed");
    let item_id = project.audio_items()[0].id();
    let (mut producer, consumer) = pcm_stream(4).expect("positive queue capacity is valid");
    assert_eq!(producer.push_samples(&[0.25, 0.5, 0.75]), 3);
    let mut graph = AudioRenderGraph::new_for_audio_items(
        &project,
        vec![AudioItemStream::new(item_id, consumer)],
        5,
    )
    .expect("item stream should match the project item");
    graph.transport_mut().start();
    let mut output = [[9.0_f32, 9.0_f32]; 5];

    let stats = graph
        .render_into(&mut output)
        .expect("timeline item should render");

    assert_eq!(stats.underrun_samples, 0);
    let center_gain = std::f32::consts::FRAC_1_SQRT_2;
    assert_eq!(output[..2], [[0.0, 0.0]; 2]);
    assert!((output[2][0] - 0.25 * center_gain).abs() < 1.0e-6);
    assert!((output[3][0] - 0.5 * center_gain).abs() < 1.0e-6);
    assert_eq!(output[4], [0.0, 0.0]);
    assert_eq!(graph.transport_mut().position_samples(), 5);
}

#[test]
fn seeking_inside_an_audio_item_requires_a_refilled_source_stream() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://clip".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 4,
        })
        .expect("audio item insertion should succeed");
    let item_id = project.audio_items()[0].id();
    let (mut producer, consumer) = pcm_stream(4).expect("positive queue capacity is valid");
    assert_eq!(producer.push_samples(&[0.25, 0.5]), 2);
    let mut graph = AudioRenderGraph::new_for_audio_items(
        &project,
        vec![AudioItemStream::new(item_id, consumer)],
        4,
    )
    .expect("item stream should match the project item");
    graph.transport_mut().seek_sample(1);
    graph.transport_mut().start();
    let mut output = [[3.0_f32, 3.0_f32]; 2];

    assert!(matches!(
        graph.render_into(&mut output),
        Err(AudioGraphError::AudioItemSeekRequiresRefill { item_id: found }) if found == item_id
    ));
    assert_eq!(graph.transport_mut().position_samples(), 1);
    assert_eq!(output, [[3.0, 3.0]; 2]);
    assert_eq!(producer.available_capacity(), 2);
}

#[test]
fn refilled_audio_item_stream_can_start_at_a_seek_position() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://clip".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 4,
        })
        .expect("audio item insertion should succeed");
    let item_id = project.audio_items()[0].id();
    let (mut producer, consumer) = pcm_stream(4).expect("positive queue capacity is valid");
    assert_eq!(producer.push_samples(&[0.25, 0.5]), 2);
    let stream = AudioItemStream::new_at_sample(item_id, 1, consumer);
    let mut graph = AudioRenderGraph::new_for_audio_items(&project, vec![stream], 2)
        .expect("refilled stream should match its audio item");
    graph.transport_mut().seek_sample(1);
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 2];

    graph
        .render_into(&mut output)
        .expect("render should start at the refilled sample");

    let center_gain = std::f32::consts::FRAC_1_SQRT_2;
    assert!((output[0][0] - 0.25 * center_gain).abs() < 1.0e-6);
    assert!((output[1][0] - 0.5 * center_gain).abs() < 1.0e-6);
    assert_eq!(producer.available_capacity(), 4);
}

#[test]
fn graph_rejects_mismatched_topology_and_oversized_blocks() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track creation should succeed");
    let (_, consumer) = pcm_stream(4).expect("positive queue size is valid");
    let mut graph = AudioRenderGraph::new(&project, vec![consumer], 2)
        .expect("stream count should match project tracks");
    graph.transport_mut().start();
    let mut output = [[7.0_f32, -7.0_f32]; 3];

    assert_eq!(
        graph.render_into(&mut output),
        Err(AudioGraphError::BlockTooLarge {
            requested: 3,
            maximum: 2,
        })
    );
    assert_eq!(output, [[7.0, -7.0]; 3]);
    assert_eq!(graph.transport_mut().position_samples(), 0);

    let (_, consumer) = pcm_stream(4).expect("positive queue size is valid");
    let (_, extra_consumer) = pcm_stream(1).expect("positive queue size is valid");
    let mismatch = AudioRenderGraph::new(&project, vec![consumer, extra_consumer], 2);
    assert!(mismatch.is_err());
    assert!(matches!(pcm_stream(0), Err(PcmStreamError::ZeroCapacity)));
}

#[test]
fn render_graph_schedules_midi_atomically_with_the_audio_block() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Instrument".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .expect("MIDI item insertion should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![aaadaw_core::MidiNoteData {
                pitch: 64,
                tick: 0,
                duration: 1,
                velocity: 100,
            }],
        })
        .expect("MIDI note insertion should succeed");
    let (_, consumer) = pcm_stream(26).expect("positive queue capacity is valid");
    let mut graph = AudioRenderGraph::new(&project, vec![consumer], 26)
        .expect("stream count should match track count");
    graph.transport_mut().start();
    let mut output = [[8.0_f32, 8.0_f32]; 26];
    let mut midi_output = [None; 1];

    let short_buffer = graph.render_with_midi(&mut midi_output, &mut output);
    assert!(matches!(
        short_buffer,
        Err(AudioGraphError::MidiSchedule(
            aaadaw_engine::MidiScheduleError::OutputBufferTooSmall {
                required: 2,
                available: 1
            }
        ))
    ));
    assert_eq!(graph.transport_mut().position_samples(), 0);
    assert_eq!(output, [[8.0, 8.0]; 26]);
    assert_eq!(midi_output, [None]);

    let mut midi_output = [None; 2];
    let stats = graph
        .render_with_midi(&mut midi_output, &mut output)
        .expect("sufficient event capacity should render");
    assert_eq!(stats.midi_event_count, 2);
    assert_eq!(
        midi_output
            .iter()
            .flatten()
            .filter(|event| event.kind == aaadaw_engine::MidiEventKind::NoteOn)
            .count(),
        1,
        "a note beginning at the playhead is scheduled exactly once"
    );
    assert_eq!(
        midi_output[0].expect("note-on should be written").kind,
        aaadaw_engine::MidiEventKind::NoteOn
    );
    assert_eq!(
        midi_output[1].expect("note-off should be written").kind,
        aaadaw_engine::MidiEventKind::NoteOff
    );
    assert_eq!(graph.transport_mut().position_samples(), 26);
}

#[test]
fn playback_start_and_restart_chase_sustained_midi_notes() {
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
            length_ticks: 960,
        })
        .expect("MIDI item insertion should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![aaadaw_core::MidiNoteData {
                pitch: 64,
                tick: 0,
                duration: 480,
                velocity: 100,
            }],
        })
        .expect("MIDI note insertion should succeed");
    let (_, consumer) = pcm_stream(64).expect("positive queue capacity is valid");
    let mut graph = AudioRenderGraph::new(&project, vec![consumer], 64)
        .expect("stream count should match track count");
    graph.transport_mut().seek_sample(1);
    graph.transport_mut().start();
    let mut output = [[0.0_f32, 0.0_f32]; 64];
    let mut midi_output = [None; 2];

    let empty = graph
        .render_with_midi(&mut midi_output, &mut [])
        .expect("empty blocks should not consume a pending note chase");
    assert_eq!(empty.midi_event_count, 0);

    let first = graph
        .render_with_midi(&mut midi_output, &mut output)
        .expect("seeked playback should render");
    assert_eq!(first.midi_event_count, 1);
    let chased = midi_output[0].expect("sustained note should be chased");
    assert_eq!(chased.kind, aaadaw_engine::MidiEventKind::NoteOn);
    assert_eq!(chased.sample_offset, 0);

    midi_output.fill(None);
    let empty_during_playback = graph
        .render_with_midi(&mut midi_output, &mut [])
        .expect("empty blocks should not chase already-active notes again");
    assert_eq!(empty_during_playback.midi_event_count, 0);
    graph
        .render_into(&mut output)
        .expect("audio-only rendering should continue playback");
    let resumed_from_audio_only = graph
        .render_with_midi(&mut midi_output, &mut output)
        .expect("returning to MIDI rendering should chase notes missed by audio-only blocks");
    assert_eq!(resumed_from_audio_only.midi_event_count, 1);
    assert_eq!(
        midi_output[0]
            .expect("active note should be chased after audio-only rendering")
            .kind,
        aaadaw_engine::MidiEventKind::NoteOn
    );

    midi_output.fill(None);
    let empty_during_playback = graph
        .render_with_midi(&mut midi_output, &mut [])
        .expect("empty blocks should not chase already-active notes again");
    assert_eq!(empty_during_playback.midi_event_count, 0);
    let continuous = graph
        .render_with_midi(&mut midi_output, &mut output)
        .expect("continuous playback should render without another chase");
    assert_eq!(continuous.midi_event_count, 0);

    // Both commands can arrive before one callback, so the transport generation must retain the
    // stop/start transition even though the playhead does not move.
    graph.transport_mut().stop();
    graph.transport_mut().start();
    let resumed = graph
        .render_with_midi(&mut midi_output, &mut output)
        .expect("resumed playback should render");
    assert_eq!(resumed.midi_event_count, 1);
    assert_eq!(
        midi_output[0]
            .expect("resumed sustained note should be chased")
            .kind,
        aaadaw_engine::MidiEventKind::NoteOn
    );
}

#[test]
fn seek_chases_sustain_state_before_resuming_sustained_notes() {
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
            length_ticks: 1920,
        })
        .expect("MIDI item insertion should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![aaadaw_core::MidiNoteData {
                pitch: 64,
                tick: 0,
                duration: 960,
                velocity: 100,
            }],
        })
        .expect("MIDI note should be added");
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                aaadaw_core::MidiControllerData {
                    controller: 64,
                    tick: 0,
                    value: 127,
                },
                aaadaw_core::MidiControllerData {
                    controller: 64,
                    tick: 1200,
                    value: 0,
                },
                aaadaw_core::MidiControllerData {
                    controller: 1,
                    tick: 240,
                    value: 80,
                },
                aaadaw_core::MidiControllerData {
                    controller: 11,
                    tick: 360,
                    value: 96,
                },
            ],
        })
        .expect("sustain pedal events should be added");
    let seek_sample = project.sample_at_tick(480).expect("tick should map");
    let (_, consumer) = pcm_stream(16).expect("positive queue capacity is valid");
    let mut graph = AudioRenderGraph::new(&project, vec![consumer], 16)
        .expect("stream count should match track count");
    graph.transport_mut().seek_sample(seek_sample);
    graph.transport_mut().start();
    let mut output = [[0.0_f32, 0.0_f32]; 16];
    let mut midi_output = [None; 6];

    let stats = graph
        .render_with_midi(&mut midi_output, &mut output)
        .expect("seeked MIDI block should render");

    assert_eq!(stats.midi_event_count, 4);
    let pedal = midi_output[0].expect("sustain state should be chased");
    let modulation = midi_output[1].expect("modulation state should be chased");
    assert_eq!(pedal.kind, aaadaw_engine::MidiEventKind::ControllerChange);
    assert_eq!(
        modulation.kind,
        aaadaw_engine::MidiEventKind::ControllerChange
    );
    assert!(
        midi_output[..3]
            .iter()
            .flatten()
            .any(|event| { event.controller == Some(64) && event.velocity == 127 })
    );
    assert!(
        midi_output[..3]
            .iter()
            .flatten()
            .any(|event| { event.controller == Some(1) && event.velocity == 80 })
    );
    assert!(
        midi_output[..3]
            .iter()
            .flatten()
            .any(|event| { event.controller == Some(11) && event.velocity == 96 })
    );
    let note = midi_output[3].expect("sustained note should be chased");
    assert_eq!(note.kind, aaadaw_engine::MidiEventKind::NoteOn);
    assert_eq!(note.sample_offset, 0);
}
