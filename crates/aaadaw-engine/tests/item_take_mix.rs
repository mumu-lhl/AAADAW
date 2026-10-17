use aaadaw_core::{DawAction, Project};
use aaadaw_engine::{AudioItemStream, AudioRenderGraph, stereo_pcm_stream};

#[test]
fn take_volume_and_pan_render_and_update_live_without_graph_replacement() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".into(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://constant".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48,
        })
        .unwrap();
    let item_id = project.audio_items()[0].id();
    project
        .apply(DawAction::SetAudioItemTakeMix {
            item_id,
            volume: 0.5,
            pan: 0.5,
        })
        .unwrap();

    let (mut producer, consumer) = stereo_pcm_stream(48).unwrap();
    producer.set_stereo_content(true);
    producer.push_frames(&[[0.125, 0.25]; 48]);
    let mut graph = AudioRenderGraph::new_for_audio_items(
        &project,
        vec![AudioItemStream::new_stereo(item_id, consumer)],
        8,
    )
    .unwrap();
    let controller = graph.item_take_mix_controller();
    graph.transport_mut().start();

    let mut output = [[0.0; 2]; 8];
    graph.render_into(&mut output).unwrap();
    assert_eq!(output, [[0.03125, 0.125]; 8]);

    assert!(controller.set_mix(item_id, 1.0, -0.5));
    graph.render_into(&mut output).unwrap();
    assert_eq!(output, [[0.125, 0.125]; 8]);

    project
        .apply(DawAction::SetAudioItemTakeMix {
            item_id,
            volume: 0.25,
            pan: 0.0,
        })
        .unwrap();
    assert!(controller.set_mix(
        item_id,
        project.audio_items()[0].take_volume(),
        project.audio_items()[0].take_pan(),
    ));
    graph.render_into(&mut output).unwrap();
    assert_eq!(output, [[0.03125, 0.0625]; 8]);
}
