use aaadaw_core::{DawAction, MidiNoteData, Project};
use aaadaw_engine::{
    AudioRenderGraph, CLAP_IPC_MAX_BLOCK_FRAMES, CLAP_IPC_MAX_EVENTS, ClapInstrumentHelperProcess,
    ClapIpcConfig, TrackIsolatedInstrument, pcm_stream,
};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

fn helper_path() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_aaadaw-engine-test-helper"))
}

fn add_track_with_note(project: &mut Project, name: &str) -> aaadaw_core::TrackId {
    project
        .apply(DawAction::CreateTrack {
            index: project.tracks().len(),
            name: name.to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks().last().unwrap().id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3840,
        })
        .expect("MIDI item should be created");
    let item_id = project
        .midi_items()
        .iter()
        .find(|item| item.track_id() == track_id)
        .unwrap()
        .id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 0,
                duration: 960,
                velocity: 100,
            }],
        })
        .expect("MIDI note should be created");
    track_id
}

#[test]
fn crashed_helper_silences_only_its_track_while_isolated_midi_keeps_rendering() {
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    let failed_track = add_track_with_note(&mut project, "Failed instrument");
    let working_track = add_track_with_note(&mut project, "Working instrument");
    let config = ClapIpcConfig::new(
        project.settings().sample_rate(),
        CLAP_IPC_MAX_BLOCK_FRAMES,
        CLAP_IPC_MAX_EVENTS,
    )
    .unwrap();
    let mut failed = ClapInstrumentHelperProcess::spawn(
        helper_path(),
        Path::new("unused-test-plugin.clap"),
        "test.crash-only",
        config,
        None,
    )
    .expect("crashing helper should complete its handshake");
    let mut working = ClapInstrumentHelperProcess::spawn(
        helper_path(),
        Path::new("unused-test-plugin.clap"),
        "test.synth",
        config,
        None,
    )
    .expect("synthetic instrument helper should complete its handshake");

    let (_, first_stream) = pcm_stream(16_384).unwrap();
    let (_, second_stream) = pcm_stream(16_384).unwrap();
    let mut graph = AudioRenderGraph::new(&project, vec![first_stream, second_stream], 8192)
        .expect("render graph should compile");
    let mut instruments = vec![
        TrackIsolatedInstrument::new(
            failed_track,
            failed.instance_id(),
            failed.audio_port(),
            config,
        ),
        TrackIsolatedInstrument::new(
            working_track,
            working.instance_id(),
            working.audio_port(),
            config,
        ),
    ];
    graph
        .install_isolated_instrument_ports(&project, &mut instruments)
        .expect("isolated instrument routes should install");

    let deadline = Instant::now() + Duration::from_secs(2);
    while failed.try_wait().unwrap().is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        failed.region().is_faulted(),
        "failed child should fault its mapping"
    );

    graph.transport_mut().start();
    let mut output = [[0.0_f32; 2]; 128];
    graph
        .render_into(&mut output)
        .expect("a crashed instrument must not fail the whole render graph");
    assert!(
        output
            .iter()
            .all(|sample| sample[0] > 0.45 && sample[1] > 0.45)
    );

    graph.transport_mut().seek_sample(48_000);
    for _ in 0..8 {
        output.fill([1.0, 1.0]);
        graph
            .render_into(&mut output)
            .expect("seeking must keep the isolated render graph usable");
        assert!(
            output.iter().all(|sample| *sample == [0.0, 0.0]),
            "seek beyond the MIDI note must silence old-generation audio and voices"
        );
    }

    drop(graph);
    failed.shutdown().unwrap();
    working.shutdown().unwrap();
}

#[test]
fn stalled_helper_silences_only_its_track_while_isolated_midi_keeps_rendering() {
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    let stalled_track = add_track_with_note(&mut project, "Stalled instrument");
    let working_track = add_track_with_note(&mut project, "Working instrument");
    let config = ClapIpcConfig::new(
        project.settings().sample_rate(),
        CLAP_IPC_MAX_BLOCK_FRAMES,
        CLAP_IPC_MAX_EVENTS,
    )
    .unwrap();
    let mut stalled = ClapInstrumentHelperProcess::spawn(
        helper_path(),
        Path::new("unused-test-plugin.clap"),
        "test.stall",
        config,
        None,
    )
    .expect("stalled helper should complete its handshake");
    let mut working = ClapInstrumentHelperProcess::spawn(
        helper_path(),
        Path::new("unused-test-plugin.clap"),
        "test.synth",
        config,
        None,
    )
    .expect("synthetic instrument helper should complete its handshake");

    let (_, first_stream) = pcm_stream(16_384).unwrap();
    let (_, second_stream) = pcm_stream(16_384).unwrap();
    let mut graph = AudioRenderGraph::new(&project, vec![first_stream, second_stream], 8192)
        .expect("render graph should compile");
    let mut instruments = vec![
        TrackIsolatedInstrument::new(
            stalled_track,
            stalled.instance_id(),
            stalled.audio_port(),
            config,
        ),
        TrackIsolatedInstrument::new(
            working_track,
            working.instance_id(),
            working.audio_port(),
            config,
        ),
    ];
    graph
        .install_isolated_instrument_ports(&project, &mut instruments)
        .expect("isolated instrument routes should install");

    let deadline = Instant::now() + Duration::from_secs(2);
    while !stalled.is_stalled(Duration::from_millis(30)) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        stalled.region().is_faulted(),
        "stalled helper should fault its mapping"
    );

    graph.transport_mut().start();
    let mut output = [[0.0_f32; 2]; 128];
    graph
        .render_into(&mut output)
        .expect("a stalled instrument must not fail the whole render graph");
    assert!(
        output
            .iter()
            .all(|sample| sample[0] > 0.45 && sample[1] > 0.45)
    );

    drop(graph);
    stalled.shutdown().unwrap();
    working.shutdown().unwrap();
}

#[test]
fn protocol_mismatch_does_not_prevent_other_isolated_tracks_from_rendering() {
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    let _mismatched_track = add_track_with_note(&mut project, "Mismatched instrument");
    let working_track = add_track_with_note(&mut project, "Working instrument");
    let config = ClapIpcConfig::new(
        project.settings().sample_rate(),
        CLAP_IPC_MAX_BLOCK_FRAMES,
        CLAP_IPC_MAX_EVENTS,
    )
    .unwrap();
    let mismatch = match ClapInstrumentHelperProcess::spawn(
        helper_path(),
        Path::new("unused-test-plugin.clap"),
        "test.mismatch",
        config,
        None,
    ) {
        Ok(_) => panic!("protocol mismatch must prevent the helper from becoming a route"),
        Err(error) => error,
    };
    assert_eq!(mismatch.kind(), std::io::ErrorKind::InvalidData);

    let mut working = ClapInstrumentHelperProcess::spawn(
        helper_path(),
        Path::new("unused-test-plugin.clap"),
        "test.synth",
        config,
        None,
    )
    .expect("synthetic instrument helper should complete its handshake");
    let (_, first_stream) = pcm_stream(16_384).unwrap();
    let (_, second_stream) = pcm_stream(16_384).unwrap();
    let mut graph = AudioRenderGraph::new(&project, vec![first_stream, second_stream], 8192)
        .expect("render graph should compile");
    let mut instruments = vec![TrackIsolatedInstrument::new(
        working_track,
        working.instance_id(),
        working.audio_port(),
        config,
    )];
    graph
        .install_isolated_instrument_ports(&project, &mut instruments)
        .expect("the healthy instrument route should install despite the rejected helper");

    graph.transport_mut().start();
    let mut output = [[0.0_f32; 2]; 128];
    graph
        .render_into(&mut output)
        .expect("a rejected instrument must not fail the whole render graph");
    assert!(
        output
            .iter()
            .all(|sample| sample[0] > 0.45 && sample[1] > 0.45)
    );

    drop(graph);
    working.shutdown().unwrap();
}

#[test]
fn audio_send_receiver_solo_keeps_source_instrument_midi_and_audio() {
    let mut project = Project::new();
    let source = add_track_with_note(&mut project, "Synth source");
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Solo receiver".into(),
        })
        .unwrap();
    let receiver = project.tracks()[1].id();
    project
        .apply(DawAction::SetTrackMainSend {
            track_id: source,
            enabled: false,
        })
        .unwrap();
    project
        .apply(DawAction::CreateAudioSend {
            track_id: source,
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
    let config = ClapIpcConfig::new(
        project.settings().sample_rate(),
        CLAP_IPC_MAX_BLOCK_FRAMES,
        CLAP_IPC_MAX_EVENTS,
    )
    .unwrap();
    let mut helper = ClapInstrumentHelperProcess::spawn(
        helper_path(),
        Path::new("unused-test-plugin.clap"),
        "test.synth",
        config,
        None,
    )
    .unwrap();
    let (_, first) = pcm_stream(16_384).unwrap();
    let (_, second) = pcm_stream(16_384).unwrap();
    let mut graph = AudioRenderGraph::new(&project, vec![first, second], 8192).unwrap();
    let mut instruments = vec![TrackIsolatedInstrument::new(
        source,
        helper.instance_id(),
        helper.audio_port(),
        config,
    )];
    graph
        .install_isolated_instrument_ports(&project, &mut instruments)
        .unwrap();
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 128];
    graph.render_into(&mut output).unwrap();
    assert!(
        output
            .iter()
            .all(|frame| frame[0] > 0.45 && frame[1] > 0.45)
    );
    drop(graph);
    helper.shutdown().unwrap();
}

#[test]
fn initially_excluded_instrument_recovers_live_without_rebuilding_graph() {
    for initially_muted in [true, false] {
        let mut project = Project::new();
        let source = add_track_with_note(&mut project, "Synth");
        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "Other".into(),
            })
            .unwrap();
        let other = project.tracks()[1].id();
        if initially_muted {
            project
                .apply(DawAction::SetTrackMute {
                    track_id: source,
                    muted: true,
                })
                .unwrap();
        } else {
            project
                .apply(DawAction::SetTrackSolo {
                    track_id: other,
                    solo: true,
                })
                .unwrap();
        }
        let config = ClapIpcConfig::new(
            project.settings().sample_rate(),
            CLAP_IPC_MAX_BLOCK_FRAMES,
            CLAP_IPC_MAX_EVENTS,
        )
        .unwrap();
        let mut helper = ClapInstrumentHelperProcess::spawn(
            helper_path(),
            Path::new("unused-test-plugin.clap"),
            "test.synth",
            config,
            None,
        )
        .unwrap();
        let (_, first) = pcm_stream(16_384).unwrap();
        let (_, second) = pcm_stream(16_384).unwrap();
        let mut graph = AudioRenderGraph::new(&project, vec![first, second], 8192).unwrap();
        let mut instruments = vec![TrackIsolatedInstrument::new(
            source,
            helper.instance_id(),
            helper.audio_port(),
            config,
        )];
        graph
            .install_isolated_instrument_ports(&project, &mut instruments)
            .unwrap();
        let mix = graph.track_mix_controller();
        graph.transport_mut().start();
        let mut output = [[0.0; 2]; 128];
        graph.render_into(&mut output).unwrap();
        assert!(output.iter().all(|frame| *frame == [0.0, 0.0]));
        assert!(mix.set_track_mute_solo(source, false, false));
        assert!(mix.set_track_mute_solo(other, false, false));
        graph.render_into(&mut output).unwrap();
        assert!(
            output
                .iter()
                .all(|frame| frame[0] > 0.45 && frame[1] > 0.45),
            "live recovery failed: muted={initially_muted}"
        );
        assert!(mix.set_track_mute_solo(source, true, false));
        graph.render_into(&mut output).unwrap();
        assert!(output.iter().all(|frame| *frame == [0.0, 0.0]));
        assert!(mix.set_track_mute_solo(source, false, false));
        graph.render_into(&mut output).unwrap();
        assert!(
            output
                .iter()
                .all(|frame| frame[0] > 0.45 && frame[1] > 0.45)
        );
        graph.transport_mut().seek_sample(48_000);
        for _ in 0..8 {
            graph.render_into(&mut output).unwrap();
        }
        assert!(output.iter().all(|frame| *frame == [0.0, 0.0]));
        drop(graph);
        helper.shutdown().unwrap();
    }
}
