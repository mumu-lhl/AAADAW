use aaadaw_core::{DawAction, MidiNoteData, Project};
use aaadaw_engine::{AudioItemStream, AudioRenderGraph, ScheduledMidiEvent, pcm_stream};
use std::hint::black_box;
use std::process::Command;
use std::time::{Duration, Instant};

const SAMPLE_RATE: u32 = 48_000;
const WARMUP_BLOCKS: usize = 16;
const MEASURED_BLOCKS: usize = 256;
const REPEATS: usize = 7;

fn main() {
    println!("AAADAW render callback benchmark");
    println!(
        "sample_rate={SAMPLE_RATE} Hz, warmup_blocks={WARMUP_BLOCKS}, measured_blocks={MEASURED_BLOCKS}, repeats={REPEATS}"
    );
    let rustc = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unavailable".to_owned());
    let available_threads = std::thread::available_parallelism()
        .map(|count| count.get().to_string())
        .unwrap_or_else(|_| "unknown".to_owned());
    println!(
        "host={}-{}, rustc={rustc}, available_threads={available_threads}, profile=bench",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for (track_count, block_frames, with_midi) in
        [(8, 128, false), (64, 128, true), (64, 512, true)]
    {
        benchmark(track_count, block_frames, with_midi);
    }
}

fn benchmark(track_count: usize, block_frames: usize, with_midi: bool) {
    let mut durations = Vec::with_capacity(REPEATS);
    let queued_blocks = WARMUP_BLOCKS + MEASURED_BLOCKS;
    let item_length_samples = (queued_blocks * block_frames) as u64;
    let midi_length_ticks = item_length_samples / 25 + 32;

    for _ in 0..REPEATS {
        let mut project = Project::new();
        let mut producers = Vec::with_capacity(track_count);
        let mut item_streams = Vec::with_capacity(track_count);
        for track_index in 0..track_count {
            project
                .apply(DawAction::CreateTrack {
                    index: track_index,
                    name: format!("Track {}", track_index + 1),
                })
                .expect("benchmark track should be valid");
            let track_id = project.tracks()[track_index].id();
            let (mut producer, consumer) = pcm_stream(queued_blocks * block_frames)
                .expect("benchmark stream capacity should be positive");
            let samples = vec![0.1; queued_blocks * block_frames];
            assert_eq!(producer.push_samples(&samples), samples.len());
            producers.push(producer);
            project
                .apply(DawAction::InsertAudioItem {
                    track_id,
                    media_ref: format!("benchmark://track-{track_index}"),
                    start_sample: 0,
                    source_offset_samples: 0,
                    length_samples: item_length_samples,
                })
                .expect("benchmark audio item should be valid");
            let item_id = project
                .audio_items()
                .last()
                .expect("item was inserted")
                .id();
            item_streams.push(AudioItemStream::new(item_id, consumer));

            if with_midi {
                project
                    .apply(DawAction::InsertMidiItem {
                        track_id,
                        start_tick: 0,
                        length_ticks: midi_length_ticks,
                    })
                    .expect("benchmark MIDI item should be valid");
                let midi_item_id = project.midi_items().last().expect("item was inserted").id();
                let notes = (0..midi_length_ticks.saturating_sub(16))
                    .step_by(8)
                    .map(|tick| MidiNoteData {
                        pitch: 48 + (tick % 36) as u8,
                        tick,
                        duration: 16,
                        velocity: 96,
                    })
                    .collect();
                project
                    .apply(DawAction::AddMidiNotes {
                        item_id: midi_item_id,
                        notes,
                    })
                    .expect("benchmark MIDI notes should be valid");
            }
        }
        black_box(&producers);
        let mut graph = AudioRenderGraph::new_for_audio_items(&project, item_streams, block_frames)
            .expect("benchmark streams should match audio items");
        graph.transport_mut().start();
        let mut output = vec![[0.0; 2]; block_frames];
        let mut midi_output = vec![None; track_count * 16];

        for _ in 0..WARMUP_BLOCKS {
            render(&mut graph, &mut output, &mut midi_output, with_midi);
        }

        let start = Instant::now();
        for _ in 0..MEASURED_BLOCKS {
            render(&mut graph, &mut output, &mut midi_output, with_midi);
        }
        durations.push(start.elapsed());
    }

    durations.sort_unstable();
    let median = durations[durations.len() / 2];
    report(track_count, block_frames, with_midi, median);
}

fn render(
    graph: &mut AudioRenderGraph,
    output: &mut [[f32; 2]],
    midi_output: &mut [Option<ScheduledMidiEvent>],
    with_midi: bool,
) {
    let stats = if with_midi {
        graph
            .render_with_midi(black_box(midi_output), black_box(output))
            .expect("MIDI callback should render")
    } else {
        graph
            .render_into(black_box(output))
            .expect("audio callback should render")
    };
    black_box(stats);
}

fn report(track_count: usize, block_frames: usize, with_midi: bool, elapsed: Duration) {
    let measured_frames = (block_frames * MEASURED_BLOCKS) as f64;
    let callback_ns = elapsed.as_nanos() as f64 / MEASURED_BLOCKS as f64;
    let audio_seconds = measured_frames / SAMPLE_RATE as f64;
    let realtime_multiple = audio_seconds / elapsed.as_secs_f64();
    println!(
        "tracks={track_count:>2}, audio_items={track_count:>2}, midi_items={:>2}, block_frames={block_frames:>3}: median callback={callback_ns:>9.0} ns, realtime={realtime_multiple:>7.1}x",
        if with_midi { track_count } else { 0 }
    );
}
