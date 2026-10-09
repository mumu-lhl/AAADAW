use aaadaw_core::{AudioSendParameters, DawAction, Project};
use aaadaw_engine::{AudioRenderGraph, pcm_stream};

fn project() -> Project {
    let mut project = Project::new();
    // Display order intentionally differs from processing order.
    for (index, name) in ["Receiver", "Source", "Other"].into_iter().enumerate() {
        project
            .apply(DawAction::CreateTrack {
                index,
                name: name.into(),
            })
            .unwrap();
    }
    project
}

fn send(project: &mut Project, destination: usize, parameters: AudioSendParameters) {
    project
        .apply(DawAction::CreateAudioSend {
            track_id: project.tracks()[1].id(),
            destination: project.tracks()[destination].id(),
            parameters,
        })
        .unwrap();
}

fn graph(project: &Project, values: [f32; 3], frames: usize) -> AudioRenderGraph {
    let streams = values
        .into_iter()
        .map(|value| {
            let (mut producer, consumer) = pcm_stream(frames).unwrap();
            assert_eq!(producer.push_samples(&vec![value; frames]), frames);
            consumer
        })
        .collect();
    let mut graph = AudioRenderGraph::new(project, streams, frames).unwrap();
    graph.transport_mut().start();
    graph
}

#[test]
fn main_and_parallel_sends_apply_independent_gain_pan_mute_and_phase() {
    for main in [false, true] {
        let mut project = project();
        let source = project.tracks()[1].id();
        project
            .apply(DawAction::SetTrackMainSend {
                track_id: source,
                enabled: main,
            })
            .unwrap();
        send(
            &mut project,
            0,
            AudioSendParameters {
                volume_db: -6.0,
                pan: -1.0,
                ..Default::default()
            },
        );
        send(
            &mut project,
            2,
            AudioSendParameters {
                pan: 1.0,
                phase_inverted: true,
                ..Default::default()
            },
        );
        send(
            &mut project,
            0,
            AudioSendParameters {
                muted: true,
                ..Default::default()
            },
        );
        let mut graph = graph(&project, [0.2, 0.1, 0.0], 1);
        let mut output = [[0.0; 2]; 1];
        graph.render_into(&mut output).unwrap();
        let direct = if main { 0.1 } else { 0.0 };
        let expected = [
            0.2 + direct + 0.1 * 10.0_f32.powf(-6.0 / 20.0),
            0.2 + direct - 0.1,
        ];
        for channel in 0..2 {
            assert!((output[0][channel] - expected[channel]).abs() < 1.0e-6);
        }
        assert_eq!(
            graph.track_mix_controller().take_track_peak(source),
            Some([0.1, 0.1])
        );
    }
}

#[test]
fn receiver_solo_closes_unrelated_source_outputs_while_source_solo_opens_all_paths() {
    for solo_source in [false, true] {
        let mut project = project();
        send(&mut project, 0, AudioSendParameters::default());
        send(&mut project, 2, AudioSendParameters::default());
        let id = project.tracks()[usize::from(solo_source)].id();
        project
            .apply(DawAction::SetTrackSolo {
                track_id: id,
                solo: true,
            })
            .unwrap();
        let mut graph = graph(&project, [0.25, 0.125, 0.0], 1);
        let mut output = [[0.0; 2]; 1];
        graph.render_into(&mut output).unwrap();
        // Reference REAPER: receiver solo includes own .25 + received .125;
        // source solo suppresses receiver-owned .25 and opens its three .125 paths.
        assert_eq!(output, [[0.375, 0.375]]);
    }
}

#[test]
fn source_fader_ramp_and_meter_advance_once_regardless_of_send_count() {
    let run = |count: usize| {
        let mut project = project();
        let source = project.tracks()[1].id();
        project
            .apply(DawAction::SetTrackMainSend {
                track_id: source,
                enabled: false,
            })
            .unwrap();
        for _ in 0..count {
            send(&mut project, 0, AudioSendParameters::default());
        }
        let mut graph = graph(&project, [0.0, 0.1, 0.0], 241);
        let mix = graph.track_mix_controller();
        graph.render_into(&mut [[0.0; 2]; 1]).unwrap();
        mix.take_track_peak(source);
        assert!(mix.set_track_mix(source, -6.0, 0.0));
        let mut output = vec![[0.0; 2]; 240];
        graph.render_into(&mut output).unwrap();
        (output, mix.take_track_peak(source).unwrap())
    };
    let (one, one_peak) = run(1);
    let (five, five_peak) = run(5);
    assert_eq!(one_peak, five_peak);
    for (one, five) in one.iter().zip(&five) {
        for channel in 0..2 {
            assert!((one[channel] * 5.0 - five[channel]).abs() < 1.0e-6);
        }
    }
    assert!((one[239][0] - 0.1 * 10.0_f32.powf(-6.0 / 20.0)).abs() < 1.0e-6);
}

#[test]
fn stereo_send_channels_and_receiver_owned_stereo_are_summed_without_collapse() {
    use aaadaw_engine::{AudioItemStream, stereo_pcm_stream};
    let mut project = project();
    let source = project.tracks()[1].id();
    project
        .apply(DawAction::SetTrackMainSend {
            track_id: source,
            enabled: false,
        })
        .unwrap();
    send(&mut project, 0, Default::default());
    send(&mut project, 2, Default::default());
    let mut streams = Vec::new();
    for (index, frame) in [(1, [0.1, -0.2]), (0, [0.05, 0.06])] {
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[index].id(),
                media_ref: format!("asset://stereo-{index}"),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 1,
            })
            .unwrap();
        let (mut producer, consumer) = stereo_pcm_stream(1).unwrap();
        producer.set_stereo_content(true);
        assert_eq!(producer.push_frames(&[frame]), 1);
        streams.push(AudioItemStream::new_stereo(
            project.audio_items().last().unwrap().id(),
            consumer,
        ));
    }
    let mut graph = AudioRenderGraph::new_for_audio_items(&project, streams, 1).unwrap();
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 1];
    graph.render_into(&mut output).unwrap();
    assert!((output[0][0] - 0.25).abs() < 1.0e-6);
    assert!((output[0][1] + 0.34).abs() < 1.0e-6);
}

#[test]
fn incomplete_mixer_track_sets_reject_missing_main_or_send_targets() {
    use aaadaw_engine::{MixerPlan, MixerPlanError};
    for explicit_send in [false, true] {
        let mut project = project();
        let source = project.tracks()[1].id();
        if explicit_send {
            send(&mut project, 0, Default::default());
        } else {
            project
                .apply(DawAction::SetTrackOutput {
                    track_id: source,
                    output_track: Some(project.tracks()[0].id()),
                })
                .unwrap();
        }
        assert!(matches!(
            MixerPlan::compile(&project.tracks()[1..2], 16),
            Err(MixerPlanError::InvalidRouting)
        ));
    }
}

#[test]
fn send_taps_match_reference_fader_pan_and_track_mute() {
    use aaadaw_core::AudioSendTap;
    for tap in [
        AudioSendTap::PostFader,
        AudioSendTap::PreFx,
        AudioSendTap::PreFader,
    ] {
        for muted in [false, true] {
            let mut project = project();
            let source = project.tracks()[1].id();
            project
                .apply(DawAction::SetTrackMainSend {
                    track_id: source,
                    enabled: false,
                })
                .unwrap();
            project
                .apply(DawAction::SetTrackVolume {
                    track_id: source,
                    volume_db: 20.0 * 0.5_f32.log10(),
                })
                .unwrap();
            project
                .apply(DawAction::SetTrackPan {
                    track_id: source,
                    pan: 1.0,
                })
                .unwrap();
            project
                .apply(DawAction::SetTrackMute {
                    track_id: source,
                    muted,
                })
                .unwrap();
            send(
                &mut project,
                0,
                AudioSendParameters {
                    tap,
                    ..Default::default()
                },
            );
            let mut graph = graph(&project, [0.0, 0.125, 0.0], 1);
            let mut output = [[0.0; 2]; 1];
            graph.render_into(&mut output).unwrap();
            let expected = if muted {
                [0.0, 0.0]
            } else if tap == AudioSendTap::PostFader {
                [0.0, 0.0625]
            } else {
                [0.125, 0.125]
            };
            for channel in 0..2 {
                assert!(
                    (output[0][channel] - expected[channel]).abs() < 1e-6,
                    "{tap:?} mute={muted} {output:?}"
                );
            }
        }
    }
}

#[test]
fn track_phase_matches_reference_tap_boundaries_and_live_switches() {
    use aaadaw_core::AudioSendTap;
    for tap in [
        AudioSendTap::PostFader,
        AudioSendTap::PreFx,
        AudioSendTap::PreFader,
    ] {
        let mut project = project();
        let source = project.tracks()[1].id();
        project
            .apply(DawAction::SetTrackMainSend {
                track_id: source,
                enabled: false,
            })
            .unwrap();
        project
            .apply(DawAction::SetTrackVolume {
                track_id: source,
                volume_db: 20.0 * 0.5_f32.log10(),
            })
            .unwrap();
        project
            .apply(DawAction::SetTrackPhase {
                track_id: source,
                phase_inverted: true,
            })
            .unwrap();
        send(
            &mut project,
            0,
            AudioSendParameters {
                tap,
                ..Default::default()
            },
        );
        let mut graph = graph(&project, [0.0, 0.125, 0.0], 24);
        let control = graph.track_mix_controller();
        let mut output = [[0.0; 2]; 8];
        graph.render_into(&mut output).unwrap();
        let inverted = if tap == AudioSendTap::PostFader {
            -0.0625
        } else {
            0.125
        };
        assert!(
            output
                .iter()
                .all(|frame| frame.iter().all(|value| (*value - inverted).abs() < 1e-6)),
            "{tap:?}: {output:?}"
        );
        assert!(control.set_track_phase(source, false));
        graph.render_into(&mut output).unwrap();
        let normal = if tap == AudioSendTap::PostFader {
            0.0625
        } else {
            0.125
        };
        assert!(
            output
                .iter()
                .all(|frame| frame.iter().all(|value| (*value - normal).abs() < 1e-6))
        );
        assert!(control.set_track_phase(source, true));
        graph.render_into(&mut output).unwrap();
        assert!(
            output
                .iter()
                .all(|frame| frame.iter().all(|value| (*value - inverted).abs() < 1e-6))
        );
        assert!(
            control
                .take_track_peak(source)
                .unwrap()
                .iter()
                .all(|peak| *peak >= 0.0)
        );
    }
}

#[test]
fn track_phase_inverts_main_mix_without_negative_meter_levels() {
    use aaadaw_engine::MixerPlan;
    let mut project = project();
    let source = project.tracks()[1].id();
    project
        .apply(DawAction::SetTrackPhase {
            track_id: source,
            phase_inverted: true,
        })
        .unwrap();
    let mut render = graph(&project, [0.0, 0.125, 0.0], 8);
    let mut output = [[0.0; 2]; 8];
    render.render_into(&mut output).unwrap();
    assert!(output.iter().all(|frame| *frame == [-0.125, -0.125]));
    let mixer = MixerPlan::compile(project.tracks(), 8).unwrap();
    output.fill([0.0, 0.0]);
    mixer
        .mix_mono_into(&[&[0.0; 8], &[0.125; 8], &[0.0; 8]], &mut output)
        .unwrap();
    assert!(output.iter().all(|frame| *frame == [-0.125, -0.125]));
    assert_eq!(
        mixer.track_mix_controller().take_track_peak(source),
        Some([0.125, 0.125])
    );
}
