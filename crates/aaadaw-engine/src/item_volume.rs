use aaadaw_core::{ItemId, MAX_AUDIO_ITEM_VOLUME};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

type VolumeEntries = Vec<(ItemId, Arc<AtomicU32>)>;

/// Publishes bounded live audio-item volume edits to a compiled graph.
#[derive(Clone)]
pub struct ItemVolumeController(Arc<VolumeEntries>);

impl ItemVolumeController {
    /// Publishes an item's linear gain without replacing the graph.
    pub fn set_volume(&self, item_id: ItemId, volume: f32) -> bool {
        if !valid_volume(volume) {
            return false;
        }
        let Some((_, value)) = self.0.iter().find(|(id, _)| *id == item_id) else {
            return false;
        };
        value.store(volume.to_bits(), Ordering::Release);
        true
    }
}

pub(super) struct ItemVolumeReader {
    shared: Option<Arc<AtomicU32>>,
    cached: f32,
}

impl ItemVolumeReader {
    /// Reads one atomic scalar without locking, allocating, or waiting.
    pub(super) fn read(&mut self) -> f32 {
        if let Some(shared) = &self.shared {
            let value = f32::from_bits(shared.load(Ordering::Acquire));
            if valid_volume(value) {
                self.cached = value;
            }
        }
        self.cached
    }
}

pub(super) fn compile(
    ids: &[Option<ItemId>],
    volumes: Vec<f32>,
) -> (ItemVolumeController, Vec<ItemVolumeReader>) {
    let mut entries = Vec::new();
    let readers = ids
        .iter()
        .copied()
        .zip(volumes)
        .map(|(id, cached)| {
            debug_assert!(valid_volume(cached));
            let shared = id.map(|id| {
                let value = Arc::new(AtomicU32::new(cached.to_bits()));
                entries.push((id, value.clone()));
                value
            });
            ItemVolumeReader { shared, cached }
        })
        .collect();
    (ItemVolumeController(Arc::new(entries)), readers)
}

fn valid_volume(volume: f32) -> bool {
    volume.is_finite() && (0.0..=MAX_AUDIO_ITEM_VOLUME).contains(&volume)
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
        for index in 0..2 {
            project
                .apply(DawAction::InsertAudioItem {
                    track_id,
                    media_ref: "asset://constant".into(),
                    start_sample: index * 48_000,
                    source_offset_samples: 0,
                    length_samples: 48_000,
                })
                .unwrap();
        }
        [project.audio_items()[0].id(), project.audio_items()[1].id()]
    }

    #[test]
    fn item_volume_updates_are_published_without_rebuilding_the_controller() {
        let [item_id, _] = item_ids();
        let (controller, mut readers) = compile(&[Some(item_id)], vec![1.0]);
        assert_eq!(readers[0].read(), 1.0);
        assert!(controller.set_volume(item_id, 0.5));
        assert_eq!(readers[0].read(), 0.5);
        assert!(controller.set_volume(item_id, 0.0));
        assert_eq!(readers[0].read(), 0.0);
    }

    #[test]
    fn item_volume_controller_rejects_unknown_items_and_out_of_range_values() {
        let [item_id, other_id] = item_ids();
        let (controller, _) = compile(&[Some(item_id)], vec![1.0]);
        assert!(!controller.set_volume(other_id, 0.5));
        assert!(!controller.set_volume(item_id, f32::NAN));
        assert!(!controller.set_volume(item_id, -0.1));
        assert!(!controller.set_volume(item_id, MAX_AUDIO_ITEM_VOLUME + 1.0));
    }
}
