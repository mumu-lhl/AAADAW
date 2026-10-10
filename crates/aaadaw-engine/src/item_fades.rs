use aaadaw_core::{AudioFade, AudioItemFades, FadeShape, ItemId};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

struct SharedFade {
    writer: Mutex<()>,
    sequence: AtomicU64,
    fade_in: AtomicU64,
    fade_out: AtomicU64,
    shapes: AtomicU8,
}

impl SharedFade {
    fn new(fades: AudioItemFades) -> Self {
        Self {
            writer: Mutex::new(()),
            sequence: AtomicU64::new(0),
            fade_in: AtomicU64::new(fades.fade_in.length_samples().to_bits()),
            fade_out: AtomicU64::new(fades.fade_out.length_samples().to_bits()),
            shapes: AtomicU8::new(
                fades.fade_in.shape().code() | (fades.fade_out.shape().code() << 4),
            ),
        }
    }

    fn publish(&self, fades: AudioItemFades) {
        // Only control-side writers lock. The callback never takes this lock.
        let _writer = self
            .writer
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.sequence.fetch_add(1, Ordering::SeqCst);
        self.fade_in
            .store(fades.fade_in.length_samples().to_bits(), Ordering::SeqCst);
        self.fade_out
            .store(fades.fade_out.length_samples().to_bits(), Ordering::SeqCst);
        self.shapes.store(
            fades.fade_in.shape().code() | (fades.fade_out.shape().code() << 4),
            Ordering::SeqCst,
        );
        self.sequence.fetch_add(1, Ordering::SeqCst);
    }
}

type FadeEntries = Vec<(ItemId, Arc<SharedFade>)>;

/// Control-thread publication of coherent manual fade pairs to a fixed graph.
#[derive(Clone)]
pub struct ItemFadeController(Arc<FadeEntries>);

impl ItemFadeController {
    pub fn set_fades(&self, item_id: ItemId, fades: AudioItemFades) -> bool {
        let Some((_, shared)) = self.0.iter().find(|(id, _)| *id == item_id) else {
            return false;
        };
        shared.publish(fades);
        true
    }
}

pub(super) struct ItemFadeReader {
    shared: Option<Arc<SharedFade>>,
    cached: AudioItemFades,
}

impl ItemFadeReader {
    /// One bounded attempt per source and callback; retain the last coherent
    /// value if a writer is preempted. Never spin, allocate, or lock here.
    pub(super) fn read(&mut self) -> AudioItemFades {
        if let Some(shared) = &self.shared {
            let before = shared.sequence.load(Ordering::SeqCst);
            if before & 1 == 0 {
                let fade_in = shared.fade_in.load(Ordering::SeqCst);
                let fade_out = shared.fade_out.load(Ordering::SeqCst);
                let shapes = shared.shapes.load(Ordering::SeqCst);
                if shared.sequence.load(Ordering::SeqCst) == before {
                    // Publication only accepts validated domain values.
                    self.cached = AudioItemFades {
                        fade_in: AudioFade::new(
                            f64::from_bits(fade_in),
                            FadeShape::from_code(shapes & 15).expect("validated fade shape"),
                        )
                        .expect("validated duration"),
                        fade_out: AudioFade::new(
                            f64::from_bits(fade_out),
                            FadeShape::from_code(shapes >> 4).expect("validated fade shape"),
                        )
                        .expect("validated duration"),
                    };
                }
            }
        }
        self.cached
    }
}

pub(super) fn compile(
    ids: &[Option<ItemId>],
    fades: Vec<AudioItemFades>,
) -> (ItemFadeController, Vec<ItemFadeReader>) {
    let mut entries = Vec::new();
    let readers = ids
        .iter()
        .copied()
        .zip(fades)
        .map(|(id, cached)| {
            let shared = id.map(|id| {
                let shared = Arc::new(SharedFade::new(cached));
                entries.push((id, shared.clone()));
                shared
            });
            ItemFadeReader { shared, cached }
        })
        .collect();
    (ItemFadeController(Arc::new(entries)), readers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(value: f64, shape: FadeShape) -> AudioItemFades {
        AudioItemFades {
            fade_in: AudioFade::new(value, shape).unwrap(),
            fade_out: AudioFade::new(value + 0.5, shape).unwrap(),
        }
    }

    #[test]
    fn preempted_writer_keeps_previous_pair_without_waiting() {
        let shared = Arc::new(SharedFade::new(pair(1.0, FadeShape::Smooth)));
        let mut reader = ItemFadeReader {
            shared: Some(shared.clone()),
            cached: pair(1.0, FadeShape::Smooth),
        };
        let _lock = shared.writer.lock().unwrap();
        shared.sequence.store(1, Ordering::SeqCst);
        shared.fade_in.store(9.0f64.to_bits(), Ordering::SeqCst);
        assert_eq!(reader.read(), pair(1.0, FadeShape::Smooth));
        shared.fade_out.store(9.5f64.to_bits(), Ordering::SeqCst);
        shared.shapes.store(0, Ordering::SeqCst);
        shared.sequence.store(2, Ordering::SeqCst);
        assert_eq!(reader.read(), pair(9.0, FadeShape::Linear));
    }

    #[test]
    fn concurrent_publish_never_reads_a_torn_duration_or_shape_pair() {
        let first = pair(1.0, FadeShape::Smooth);
        let second = pair(9.0, FadeShape::Linear);
        let shared = Arc::new(SharedFade::new(first));
        let mut reader = ItemFadeReader {
            shared: Some(shared.clone()),
            cached: first,
        };
        std::thread::scope(|scope| {
            let writer = scope.spawn(|| {
                for _ in 0..10_000 {
                    shared.publish(second);
                    shared.publish(first);
                }
            });
            for _ in 0..20_000 {
                let value = reader.read();
                assert!(value == first || value == second);
            }
            writer.join().unwrap();
        });
    }
}
