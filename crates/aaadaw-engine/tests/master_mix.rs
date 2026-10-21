use aaadaw_core::{DawAction, MasterMix, Project};
use aaadaw_engine::{AudioRenderGraph, pcm_stream};

fn graph(mix: MasterMix, frames: usize) -> AudioRenderGraph {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Source".into(),
        })
        .unwrap();
    project.apply(DawAction::SetMasterMix { mix }).unwrap();
    let (mut producer, consumer) = pcm_stream(frames).unwrap();
    assert_eq!(producer.push_samples(&vec![0.125; frames]), frames);
    let mut graph = AudioRenderGraph::new(&project, vec![consumer], 128).unwrap();
    graph.transport_mut().start();
    graph
}

#[test]
fn master_gain_balance_matches_reference_and_meter_is_post_master() {
    for (volume, pan, expected) in [
        (0.0, 0.0, [0.125, 0.125]),
        (-6.0206, 0.0, [0.0625, 0.0625]),
        (0.0, -0.5, [0.125, 0.0625]),
        (0.0, 0.5, [0.0625, 0.125]),
    ] {
        let mut graph = graph(MasterMix::new(volume, pan).unwrap(), 128);
        let mut output = [[0.0; 2]; 128];
        graph.render_into(&mut output).unwrap();
        let peak = graph.master_output_safety_controller().take_output_peak();
        for channel in 0..2 {
            assert!((output[64][channel] - expected[channel]).abs() < 1e-6);
            assert!((peak[channel] - expected[channel]).abs() < 1e-6);
        }
    }
}

#[test]
fn live_master_changes_ramp_across_blocks_and_restore_without_graph_rebuild() {
    let mut graph = graph(MasterMix::default(), 1024);
    let controller = graph.master_mix_controller();
    controller.set_mix(MasterMix::new(-6.0206, 0.5).unwrap());
    let mut output = [[0.0; 2]; 128];
    graph.render_into(&mut output).unwrap();
    assert!(output[0][0] > output[127][0]);
    graph.render_into(&mut output).unwrap();
    assert!((output[127][0] - 0.03125).abs() < 1e-6);
    assert!((output[127][1] - 0.0625).abs() < 1e-6);
    controller.set_mix(MasterMix::default());
    graph.render_into(&mut output).unwrap();
    graph.render_into(&mut output).unwrap();
    assert!((output[127][0] - 0.125).abs() < 1e-6);
    assert!((output[127][1] - 0.125).abs() < 1e-6);
}

#[test]
fn master_zero_gain_endpoint_silences_both_channels_and_meter() {
    let mut graph = graph(MasterMix::new(-1000.0, 0.5).unwrap(), 128);
    let mut output = [[1.0; 2]; 128];
    graph.render_into(&mut output).unwrap();
    assert_eq!(output, [[0.0; 2]; 128]);
    assert_eq!(
        graph.master_output_safety_controller().take_output_peak(),
        [0.0, 0.0]
    );
}
