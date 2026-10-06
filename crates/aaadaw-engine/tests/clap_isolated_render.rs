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
    let mut project = Project::new();
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

    drop(graph);
    failed.shutdown().unwrap();
    working.shutdown().unwrap();
}
