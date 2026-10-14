use aaadaw_core::{DawAction, Project};
use aaadaw_engine::{AudioItemStream, AudioRenderGraph, stereo_pcm_stream};

#[test]
fn live_item_volume_publication_and_history_change_pcm_without_graph_replacement() {
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
    let (mut producer, consumer) = stereo_pcm_stream(48).unwrap();
    producer.set_stereo_content(true);
    producer.push_frames(&[[0.125, 0.25]; 48]);
    let mut graph = AudioRenderGraph::new_for_audio_items(
        &project,
        vec![AudioItemStream::new_stereo(item_id, consumer)],
        8,
    )
    .unwrap();
    let controller = graph.item_volume_controller();
    graph.transport_mut().start();

    let mut output = [[0.0; 2]; 8];
    graph.render_into(&mut output).unwrap();
    assert_eq!(output, [[0.125, 0.25]; 8]);

    project
        .apply(DawAction::SetAudioItemVolume {
            item_id,
            volume: 0.5,
        })
        .unwrap();
    assert!(project.can_undo_track_mix());
    assert!(controller.set_volume(item_id, project.audio_items()[0].volume()));
    graph.render_into(&mut output).unwrap();
    assert_eq!(output, [[0.0625, 0.125]; 8]);

    assert!(project.undo().unwrap());
    assert!(controller.set_volume(item_id, project.audio_items()[0].volume()));
    graph.render_into(&mut output).unwrap();
    assert_eq!(output, [[0.125, 0.25]; 8]);

    assert!(project.redo().unwrap());
    assert!(controller.set_volume(item_id, project.audio_items()[0].volume()));
    graph.render_into(&mut output).unwrap();
    assert_eq!(output, [[0.0625, 0.125]; 8]);
}
