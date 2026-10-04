use aaadaw_core::{DawAction, Project};
use aaadaw_engine::{AudioRenderGraph, pcm_stream};
use std::hint::black_box;
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
    for (track_count, block_frames) in [(8, 128), (64, 128), (64, 512)] {
        benchmark(track_count, block_frames);
    }
}

fn benchmark(track_count: usize, block_frames: usize) {
    let mut durations = Vec::with_capacity(REPEATS);
    let queued_blocks = WARMUP_BLOCKS + MEASURED_BLOCKS;

    for _ in 0..REPEATS {
        let mut project = Project::new();
        let mut streams = Vec::with_capacity(track_count);
        let mut producers = Vec::with_capacity(track_count);
        for track_index in 0..track_count {
            project
                .apply(DawAction::CreateTrack {
                    index: track_index,
                    name: format!("Track {}", track_index + 1),
                })
                .expect("benchmark track should be valid");
            let (mut producer, consumer) = pcm_stream(queued_blocks * block_frames)
                .expect("benchmark stream capacity should be positive");
            let samples = vec![0.1; queued_blocks * block_frames];
            assert_eq!(producer.push_samples(&samples), samples.len());
            producers.push(producer);
            streams.push(consumer);
        }
        black_box(&producers);
        let mut graph = AudioRenderGraph::new(&project, streams, block_frames)
            .expect("benchmark streams should match tracks");
        graph.transport_mut().start();
        let mut output = vec![[0.0; 2]; block_frames];

        for _ in 0..WARMUP_BLOCKS {
            black_box(
                graph
                    .render_into(black_box(&mut output))
                    .expect("warmup callback should render"),
            );
        }

        let start = Instant::now();
        for _ in 0..MEASURED_BLOCKS {
            black_box(
                graph
                    .render_into(black_box(&mut output))
                    .expect("measured callback should render"),
            );
        }
        durations.push(start.elapsed());
    }

    durations.sort_unstable();
    let median = durations[durations.len() / 2];
    report(track_count, block_frames, median);
}

fn report(track_count: usize, block_frames: usize, elapsed: Duration) {
    let measured_frames = (block_frames * MEASURED_BLOCKS) as f64;
    let callback_ns = elapsed.as_nanos() as f64 / MEASURED_BLOCKS as f64;
    let audio_seconds = measured_frames / SAMPLE_RATE as f64;
    let realtime_multiple = audio_seconds / elapsed.as_secs_f64();
    println!(
        "tracks={track_count:>2}, block_frames={block_frames:>3}: median callback={callback_ns:>9.0} ns, realtime={realtime_multiple:>7.1}x"
    );
}
