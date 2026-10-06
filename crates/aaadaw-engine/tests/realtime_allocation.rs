use aaadaw_core::{DawAction, MidiNoteData, Project};
use aaadaw_engine::{
    AudioItemStream, AudioRenderGraph, AudioStreamPosition, ClapIpcConfig, ClapIpcMapping,
    MasterOutputCeiling, audio_monitor_stream, stereo_pcm_stream,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct TrackingAllocator;

thread_local! {
    static ALLOCATION_STATE: Cell<(bool, usize)> = const { Cell::new((false, 0)) };
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

// This integration-test executable contains one test. Tracking is enabled only
// around render calls and the callback-side IPC methods on the current test
// thread, excluding graph and harness setup.
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: the original layout is forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: the original layout is forwarded unchanged to the system allocator.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: pointer was allocated by this system allocator and layout is unchanged.
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: pointer was allocated by this system allocator and layout is unchanged.
        unsafe { System.dealloc(pointer, layout) }
    }
}

fn count_allocation() {
    let _ = ALLOCATION_STATE.try_with(|state| {
        let (tracking, allocations) = state.get();
        if tracking {
            state.set((tracking, allocations.saturating_add(1)));
        }
    });
}

struct AllocationTracking;

impl AllocationTracking {
    fn start() -> Self {
        ALLOCATION_STATE.with(|state| state.set((true, 0)));
        Self
    }

    fn finish(self) -> usize {
        drop(self);
        ALLOCATION_STATE.with(|state| {
            let (_, allocations) = state.get();
            allocations
        })
    }
}

impl Drop for AllocationTracking {
    fn drop(&mut self) {
        let _ = ALLOCATION_STATE.try_with(|state| {
            let (_, allocations) = state.get();
            state.set((false, allocations));
        });
    }
}

#[test]
fn render_callback_does_not_allocate_on_the_rendering_thread() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Realtime allocation check".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::SetTrackRecordArm {
            track_id,
            armed: true,
        })
        .expect("track should arm for live monitoring");
    project
        .apply(DawAction::SetTrackVolumeAutomation {
            track_id,
            points: vec![
                aaadaw_core::VolumeAutomationPoint::new(0, -3.0).unwrap(),
                aaadaw_core::VolumeAutomationPoint::new(128 * 64, -12.0).unwrap(),
                aaadaw_core::VolumeAutomationPoint::new(128 * 127, 0.0).unwrap(),
            ],
        })
        .expect("automation should be available to the render callback");
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://realtime-allocation-check".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 128 * 128,
        })
        .expect("audio item creation should succeed");
    let audio_item_id = project.audio_items()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .expect("MIDI item creation should succeed");
    let midi_item_id = project.midi_items()[0].id();
    let notes = (0..512)
        .step_by(16)
        .map(|tick| MidiNoteData {
            pitch: 48 + (tick % 36) as u8,
            tick,
            duration: 8,
            velocity: 100,
        })
        .collect();
    project
        .apply(DawAction::AddMidiNotes {
            item_id: midi_item_id,
            notes,
        })
        .expect("MIDI note creation should succeed");
    let (mut producer, consumer) =
        stereo_pcm_stream(128 * 128).expect("queue capacity should be valid");
    producer.set_stereo_content(true);
    let source = vec![[0.25, -0.125]; 128 * 128];
    assert_eq!(producer.push_frames_at(0, &source), source.len());
    let position = AudioStreamPosition::new(0);
    let mut graph = AudioRenderGraph::new_for_audio_items(
        &project,
        vec![AudioItemStream::new_stereo_with_position(
            audio_item_id,
            consumer,
            position,
        )],
        128,
    )
    .expect("stream should match the audio item");
    let (mut monitor_producer, monitor_consumer, monitor_gate) = audio_monitor_stream(256);
    graph.install_input_monitor(monitor_consumer, monitor_gate);
    assert!(
        graph
            .input_monitor_controller()
            .expect("monitor graph should expose its control")
            .set_track_enabled(track_id, true)
    );
    let mix = graph.track_mix_controller();
    assert!(mix.set_track_mix(track_id, 6.0, 0.0));
    let master = graph.master_output_safety_controller();
    master.set_ceiling(MasterOutputCeiling::new(-12).expect("ceiling is supported"));
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 128];
    let mut midi_output = [None; 128];

    let ipc_config = ClapIpcConfig::new(48_000, 16, 8).expect("IPC config should be valid");
    let ipc_mapping = ClapIpcMapping::create(ipc_config).expect("IPC mapping should be created");
    ipc_mapping.region().accept_handshake(ipc_config);
    let ipc_port = ipc_mapping.audio_port();
    let mut ipc_reader = ipc_port
        .reader(1, 0, 0)
        .expect("reader should match the IPC block size");
    let mut ipc_sequence = 0;
    let mut ipc_start_sample = 0;
    let mut ipc_output = [[0.0; 2]; 64];

    let tracking = AllocationTracking::start();
    let mut last_stats = None;
    let mut midi_events_seen = 0;
    for _ in 0..128 {
        for _ in 0..128 {
            assert!(monitor_producer.push_frame([0.01, 0.01]));
        }
        let stats = graph
            .render_with_midi(&mut midi_output, &mut output)
            .expect("preallocated MIDI and audio callback block should render");
        midi_events_seen += stats.midi_event_count;
        last_stats = Some(stats);
    }
    // Exercise the same bounded port methods used by an audio callback. The test
    // helper completes requests inline; process and mapping setup stay outside the
    // callback contract, while submit and read stay inside it.
    for callback_frames in [7, 23, 16, 64].into_iter().cycle().take(256) {
        let required_end = ipc_reader
            .next_sample()
            .saturating_add(callback_frames as u64);
        while ipc_start_sample < required_end {
            ipc_port
                .try_submit(1, ipc_sequence, ipc_start_sample, &[], 16)
                .expect("bounded callback request should fit");
            let request = ipc_mapping
                .region()
                .try_claim_request()
                .expect("test helper should claim the request");
            assert!(request.process(|request| {
                request.audio.fill([0.125, -0.125]);
                true
            }));
            ipc_sequence += 1;
            ipc_start_sample += 16;
        }
        assert!(ipc_reader.read_into(&mut ipc_output[..callback_frames]));
        assert!(
            ipc_output[..callback_frames]
                .iter()
                .all(|frame| *frame == [0.125, -0.125])
        );
    }
    graph.transport_mut().stop();
    for _ in 0..128 {
        for _ in 0..128 {
            assert!(monitor_producer.push_frame([0.01, -0.01]));
        }
        let stats = graph
            .render_into(&mut output)
            .expect("stopped standby-monitor block should render");
        assert!(!stats.block.is_playing);
    }
    let allocations = tracking.finish();

    assert_eq!(allocations, 0, "render callback allocated on its thread");
    let stats = last_stats.expect("render loop contains at least one block");
    assert!(stats.master_guarded_samples > 0);
    assert!(midi_events_seen > 0);
}
