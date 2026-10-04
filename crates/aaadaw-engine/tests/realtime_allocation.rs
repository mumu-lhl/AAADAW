use aaadaw_core::{DawAction, Project};
use aaadaw_engine::{AudioRenderGraph, pcm_stream};
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
    let (mut producer, consumer) = pcm_stream(128 * 128).expect("queue capacity should be valid");
    let source = vec![0.25; 128 * 128];
    assert_eq!(producer.push_samples(&source), source.len());
    let mut graph = AudioRenderGraph::new(&project, vec![consumer], 128)
        .expect("one stream should match the track count");
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 128];

    let tracking = AllocationTracking::start();
    for _ in 0..128 {
        graph
            .render_into(&mut output)
            .expect("preallocated callback block should render");
    }
    let allocations = tracking.finish();

    assert_eq!(allocations, 0, "render callback allocated on its thread");
}
