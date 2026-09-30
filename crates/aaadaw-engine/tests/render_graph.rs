use aaadaw_core::{DawAction, Project};
use aaadaw_engine::{
    AudioBlock, AudioGraphError, AudioRenderGraph, AudioRenderStats, PcmStreamError, pcm_stream,
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
        midi_output[0].expect("note-on should be written").kind,
        aaadaw_engine::MidiEventKind::NoteOn
    );
    assert_eq!(
        midi_output[1].expect("note-off should be written").kind,
        aaadaw_engine::MidiEventKind::NoteOff
    );
    assert_eq!(graph.transport_mut().position_samples(), 26);
}
