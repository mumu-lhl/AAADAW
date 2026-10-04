use aaadaw_core::{DawAction, MidiNoteData, Project};
use aaadaw_engine::{
    AudioItemStream, AudioRenderGraph, MasterOutputCeiling, audio_monitor_stream, pcm_stream,
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
// around render calls on the current test thread, excluding graph and harness setup.
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
    let (mut producer, consumer) = pcm_stream(128 * 128).expect("queue capacity should be valid");
    let source = vec![0.25; 128 * 128];
    assert_eq!(producer.push_samples(&source), source.len());
    let mut graph = AudioRenderGraph::new_for_audio_items(
        &project,
        vec![AudioItemStream::new(audio_item_id, consumer)],
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
