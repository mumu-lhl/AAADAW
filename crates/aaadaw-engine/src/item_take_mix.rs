use aaadaw_core::{ItemId, MAX_AUDIO_ITEM_TAKE_VOLUME};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

type MixEntries = Vec<(ItemId, Arc<AtomicU64>)>;

/// Publishes active take volume and stereo balance as one bounded atomic update.
#[derive(Clone)]
pub struct ItemTakeMixController(Arc<MixEntries>);

impl ItemTakeMixController {
    /// Publishes the active take's linear gain and stereo balance without graph replacement.
    pub fn set_mix(&self, item_id: ItemId, volume: f32, pan: f32) -> bool {
        if !valid_mix(volume, pan) {
            return false;
        }
        let Some((_, value)) = self.0.iter().find(|(id, _)| *id == item_id) else {
            return false;
        };
        value.store(pack(volume, pan), Ordering::Release);
        true
    }
}

pub(super) struct ItemTakeMixReader {
    shared: Option<Arc<AtomicU64>>,
    cached: (f32, f32),
}

impl ItemTakeMixReader {
    /// Reads a coherent pair without locking, allocating, or waiting.
    pub(super) fn read(&mut self) -> (f32, f32) {
        if let Some(shared) = &self.shared {
            let (volume, pan) = unpack(shared.load(Ordering::Acquire));
            if valid_mix(volume, pan) {
                self.cached = (volume, pan);
            }
        }
        self.cached
    }
}

pub(super) fn compile(
    ids: &[Option<ItemId>],
    mixes: Vec<(f32, f32)>,
) -> (ItemTakeMixController, Vec<ItemTakeMixReader>) {
    let mut entries = Vec::new();
    let readers = ids
        .iter()
        .copied()
        .zip(mixes)
        .map(|(id, cached)| {
            debug_assert!(valid_mix(cached.0, cached.1));
            let shared = id.map(|id| {
                let value = Arc::new(AtomicU64::new(pack(cached.0, cached.1)));
                entries.push((id, value.clone()));
                value
            });
            ItemTakeMixReader { shared, cached }
        })
        .collect();
    (ItemTakeMixController(Arc::new(entries)), readers)
}

fn valid_mix(volume: f32, pan: f32) -> bool {
    volume.is_finite()
        && (0.0..=MAX_AUDIO_ITEM_TAKE_VOLUME).contains(&volume)
        && pan.is_finite()
        && (-1.0..=1.0).contains(&pan)
}

fn pack(volume: f32, pan: f32) -> u64 {
    (u64::from(volume.to_bits()) << 32) | u64::from(pan.to_bits())
}

fn unpack(value: u64) -> (f32, f32) {
    (
        f32::from_bits((value >> 32) as u32),
        f32::from_bits(value as u32),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_core::{DawAction, Project};

    fn item_ids() -> [ItemId; 2] {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Audio".into(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        for start_sample in [0, 48_000] {
            project
                .apply(DawAction::InsertAudioItem {
                    track_id,
                    media_ref: "asset://constant".into(),
                    start_sample,
                    source_offset_samples: 0,
                    length_samples: 48_000,
                })
                .unwrap();
        }
        [project.audio_items()[0].id(), project.audio_items()[1].id()]
    }

    #[test]
    fn take_mix_updates_are_coherent_and_live() {
        let [item_id, _] = item_ids();
        let (controller, mut readers) = compile(&[Some(item_id)], vec![(1.0, 0.0)]);
        assert_eq!(readers[0].read(), (1.0, 0.0));
        assert!(controller.set_mix(item_id, 0.5, -0.75));
        assert_eq!(readers[0].read(), (0.5, -0.75));
    }

    #[test]
    fn take_mix_controller_rejects_unknown_items_and_invalid_values() {
        let [item_id, other_id] = item_ids();
        let (controller, _) = compile(&[Some(item_id)], vec![(1.0, 0.0)]);
        assert!(!controller.set_mix(other_id, 0.5, 0.0));
        assert!(!controller.set_mix(item_id, f32::NAN, 0.0));
        assert!(!controller.set_mix(item_id, 1.0, f32::INFINITY));
        assert!(!controller.set_mix(item_id, MAX_AUDIO_ITEM_TAKE_VOLUME + 1.0, 0.0));
        assert!(!controller.set_mix(item_id, 1.0, -1.01));
    }
}
