use aaadaw_core::{AudioFade, AudioItemFades, DawAction, FadeShape, Project};
use aaadaw_engine::{AudioItemStream, AudioRenderGraph, stereo_pcm_stream};

#[test]
fn real_stereo_pcm_fades_follow_item_clock_across_blocks_and_seek() {
    for code in 0..=6 {
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
                start_sample: 7,
                source_offset_samples: 200,
                length_samples: 48,
            })
            .unwrap();
        let item_id = project.audio_items()[0].id();
        let shape = FadeShape::from_code(code).unwrap();
        let fades = AudioItemFades {
            fade_in: AudioFade::new(12.0, shape).unwrap(),
            fade_out: AudioFade::new(12.0, shape).unwrap(),
        };
        project
            .apply(DawAction::SetAudioItemFades { item_id, fades })
            .unwrap();
        let (mut producer, consumer) = stereo_pcm_stream(48).unwrap();
        producer.set_stereo_content(true);
        assert_eq!(producer.push_frames(&[[0.125, 0.25]; 48]), 48);
        let mut graph = AudioRenderGraph::new_for_audio_items(
            &project,
            vec![AudioItemStream::new_stereo(item_id, consumer)],
            13,
        )
        .unwrap();
        graph.transport_mut().start();
        let mut rendered = Vec::new();
        for _ in 0..5 {
            let mut output = [[0.0; 2]; 13];
            assert_eq!(graph.render_into(&mut output).unwrap().underrun_samples, 0);
            rendered.extend(output);
        }
        for (index, frame) in rendered.iter().enumerate() {
            let gain = if (7..55).contains(&index) {
                fades.gain_at((index - 7) as u64, 48)
            } else {
                0.0
            };
            assert!(
                (frame[0] - 0.125 * gain).abs() < 1e-6,
                "shape {code}, frame {index}"
            );
            assert!((frame[1] - 0.25 * gain).abs() < 1e-6);
        }
        let (mut producer, consumer) = stereo_pcm_stream(12).unwrap();
        producer.set_stereo_content(true);
        producer.push_frames(&[[0.125, 0.25]; 12]);
        let stream = AudioItemStream::new_stereo_at_sample(item_id, 43, consumer);
        let mut graph = AudioRenderGraph::new_for_audio_items(&project, vec![stream], 12).unwrap();
        graph.transport_mut().seek_sample(43);
        graph.transport_mut().start();
        let mut output = [[0.0; 2]; 12];
        assert_eq!(graph.render_into(&mut output).unwrap().underrun_samples, 0);
        assert_eq!(output, rendered[43..55]);
    }
}

#[test]
fn live_fade_publication_and_history_change_real_pcm_without_graph_replacement() {
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
    let controller = graph.item_fade_controller();
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 8];
    graph.render_into(&mut output).unwrap();
    assert_eq!(output, [[0.125, 0.25]; 8]);
    let fades = AudioItemFades {
        fade_in: AudioFade::new(48.0, FadeShape::Linear).unwrap(),
        fade_out: AudioFade::default(),
    };
    project
        .apply(DawAction::SetAudioItemFades { item_id, fades })
        .unwrap();
    assert!(project.can_undo_track_mix());
    assert!(controller.set_fades(item_id, fades));
    graph.render_into(&mut output).unwrap();
    for (index, frame) in output.iter().enumerate() {
        let gain = (8 + index) as f32 / 48.0;
        assert_eq!(*frame, [0.125 * gain, 0.25 * gain]);
    }
    project.undo().unwrap();
    controller.set_fades(item_id, project.audio_items()[0].fades());
    graph.render_into(&mut output).unwrap();
    assert_eq!(output, [[0.125, 0.25]; 8]);
    project.redo().unwrap();
    controller.set_fades(item_id, project.audio_items()[0].fades());
    graph.render_into(&mut output).unwrap();
    assert_eq!(output[0], [0.0625, 0.125]);
    let native = AudioItemFades {
        fade_in: AudioFade::with_curve(
            48.0,
            aaadaw_core::FadeCurve::Native(
                aaadaw_core::FadeCurveParameters::new(0.0, 0.5).unwrap(),
            ),
        )
        .unwrap(),
        fade_out: AudioFade::default(),
    };
    project
        .apply(DawAction::SetAudioItemFades {
            item_id,
            fades: native,
        })
        .unwrap();
    assert!(controller.set_fades(item_id, native));
    graph.render_into(&mut output).unwrap();
    for (index, frame) in output.iter().enumerate() {
        let x = (32 + index) as f32 / 48.0;
        let gain = 1.0 - 2.0 * (1.0 - x).powi(2);
        assert!((frame[0] - 0.125 * gain).abs() < 1e-6);
        assert!((frame[1] - 0.25 * gain).abs() < 1e-6);
    }

    project
        .apply(DawAction::DuplicateAudioItemAt {
            item_id,
            track_id,
            start_sample: 48,
        })
        .unwrap();
    assert!(!controller.set_fades(project.audio_items()[1].id(), fades));
}
