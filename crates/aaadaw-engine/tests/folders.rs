use aaadaw_core::{DawAction, Project};
use aaadaw_engine::{AudioRenderGraph, pcm_stream};

#[test]
fn nested_parent_sends_match_reference_sum_switch_and_solo_paths() {
    for (solo, disable_inner, mute_outer, expected) in [
        (None, false, false, 0.5),
        (None, true, false, 0.375),
        (Some(1), false, false, 0.125),
        (Some(2), false, false, 0.125),
        (Some(0), false, false, 0.5),
        (None, false, true, 0.0),
    ] {
        let mut project = Project::new();
        for (index, name) in ["Outer", "Inner", "Leaf", "Sibling", "Outside"]
            .into_iter()
            .enumerate()
        {
            project
                .apply(DawAction::CreateTrack {
                    index,
                    name: name.into(),
                })
                .unwrap();
        }
        let ids: Vec<_> = project.tracks().iter().map(|track| track.id()).collect();
        for id in &ids[..2] {
            project
                .apply(DawAction::SetTrackFolder {
                    track_id: *id,
                    enabled: true,
                })
                .unwrap();
        }
        for (child, parent) in [(1, 0), (2, 1), (3, 0)] {
            project
                .apply(DawAction::SetTrackParent {
                    track_id: ids[child],
                    parent: Some(ids[parent]),
                })
                .unwrap();
        }
        project
            .apply(DawAction::SetTrackMainSend {
                track_id: ids[1],
                enabled: !disable_inner,
            })
            .unwrap();
        project
            .apply(DawAction::SetTrackMute {
                track_id: ids[0],
                muted: mute_outer,
            })
            .unwrap();
        if let Some(index) = solo {
            project
                .apply(DawAction::SetTrackSolo {
                    track_id: ids[index],
                    solo: true,
                })
                .unwrap();
        }
        let consumers = [0.25, 0.0, 0.125, 0.125, 0.0]
            .into_iter()
            .map(|value| {
                let (mut producer, consumer) = pcm_stream(4).unwrap();
                producer.push_samples(&[value; 4]);
                consumer
            })
            .collect();
        let mut graph = AudioRenderGraph::new(&project, consumers, 4).unwrap();
        graph.transport_mut().start();
        let mut output = [[0.0; 2]; 4];
        graph.render_into(&mut output).unwrap();
        for frame in output {
            for value in frame {
                assert!(
                    (value - expected).abs() < 1e-6,
                    "solo={solo:?} disabled={disable_inner} mute={mute_outer} {frame:?}"
                );
            }
        }
    }
}
