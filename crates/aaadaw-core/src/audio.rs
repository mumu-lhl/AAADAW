use crate::{ItemId, TrackId};

/// A timeline placement of an audio source on a track.
///
/// `start_sample` and `length_samples` use the project's sample clock;
/// `source_offset_samples` is relative to the item's source view and may be
/// negative when a non-looping item's contents begin before that view. Bounded
/// sections retain their absolute media start separately from the offset within
/// the section. The media reference is opaque to the core and resolved by the
/// media layer.
pub const MAX_AUDIO_ITEM_VOLUME: f32 = 16.0;
/// Maximum linear volume for an audio item's active take.
pub const MAX_AUDIO_ITEM_TAKE_VOLUME: f32 = 16.0;

/// An inactive audio take stored alongside the active take's playback fields.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioItemTake {
    pub(crate) index: u32,
    pub(crate) media_ref: String,
    pub(crate) name: String,
    pub(crate) source_offset_samples: i64,
    pub(crate) phase_inverted: bool,
    pub(crate) loop_source: bool,
    pub(crate) source_section_start_samples: Option<u64>,
    pub(crate) source_section_length_samples: Option<u64>,
    pub(crate) volume: f32,
    pub(crate) pan: f32,
    pub(crate) playback_rate: f64,
}

impl AudioItemTake {
    /// Take's zero-based position in the item's ordered take list.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Opaque reference to the take's media source.
    pub fn media_ref(&self) -> &str {
        &self.media_ref
    }

    /// User-assigned take name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioItem {
    pub(crate) id: ItemId,
    pub(crate) track_id: TrackId,
    pub(crate) media_ref: String,
    pub(crate) name: String,
    pub(crate) start_sample: u64,
    pub(crate) source_offset_samples: i64,
    pub(crate) length_samples: u64,
    pub(crate) muted: bool,
    pub(crate) phase_inverted: bool,
    pub(crate) loop_source: bool,
    pub(crate) source_section_start_samples: Option<u64>,
    pub(crate) source_section_length_samples: Option<u64>,
    pub(crate) fades: crate::AudioItemFades,
    pub(crate) volume: f32,
    pub(crate) take_volume: f32,
    pub(crate) take_pan: f32,
    pub(crate) take_playback_rate: f64,
    pub(crate) other_takes: Vec<AudioItemTake>,
    pub(crate) active_take_index: u32,
}

impl AudioItem {
    /// Returns this item's identifier.
    pub fn id(&self) -> ItemId {
        self.id
    }

    /// Returns the identifier of the track containing this item.
    pub fn track_id(&self) -> TrackId {
        self.track_id
    }

    /// Returns the opaque media reference resolved outside the core.
    pub fn media_ref(&self) -> &str {
        &self.media_ref
    }

    /// Returns the active take name, or an empty string when the source label is shown.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the absolute start position in project samples.
    pub fn start_sample(&self) -> u64 {
        self.start_sample
    }

    /// Returns the signed source offset in media sample frames.
    pub fn source_offset_samples(&self) -> i64 {
        self.source_offset_samples
    }

    /// Returns the item's duration in project samples.
    pub fn length_samples(&self) -> u64 {
        self.length_samples
    }

    /// Returns whether this audio item is muted.
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// Returns whether this item's active take has inverted playback polarity.
    pub fn is_phase_inverted(&self) -> bool {
        self.phase_inverted
    }

    /// Returns whether the item's source repeats when playback reaches its end.
    pub fn is_loop_source(&self) -> bool {
        self.loop_source
    }

    /// Returns the first source frame in a bounded loop section.
    pub fn source_section_start_samples(&self) -> Option<u64> {
        self.source_section_start_samples
    }

    /// Returns the source-section length in source frames when this item loops a copied section.
    /// `None` means the complete media source is the loop section.
    pub fn source_section_length_samples(&self) -> Option<u64> {
        self.source_section_length_samples
    }

    /// Returns the manual fade parameters.
    pub fn fades(&self) -> crate::AudioItemFades {
        self.fades
    }

    /// Returns this item's linear playback volume, where 1.0 is unity and 0.0 is silent.
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Returns the active take's linear playback volume.
    pub fn take_volume(&self) -> f32 {
        self.take_volume
    }

    /// Returns the active take's stereo balance in the inclusive range -1..=1.
    pub fn take_pan(&self) -> f32 {
        self.take_pan
    }

    /// Returns the active Take's source playback rate, where `1.0` is normal speed.
    pub fn take_playback_rate(&self) -> f64 {
        self.take_playback_rate
    }

    /// Number of takes stored on this item.
    pub fn take_count(&self) -> usize {
        self.other_takes.len() + 1
    }

    /// Zero-based index of the take currently used for playback.
    pub fn active_take_index(&self) -> u32 {
        self.active_take_index
    }

    /// Returns a serializable copy of the take currently used for playback.
    pub fn active_take_snapshot(&self) -> crate::AudioItemTakeSnapshot {
        crate::AudioItemTakeSnapshot {
            index: self.active_take_index,
            media_ref: self.media_ref.clone(),
            name: self.name.clone(),
            source_offset_samples: self.source_offset_samples,
            phase_inverted: self.phase_inverted,
            loop_source: self.loop_source,
            source_section_start_samples: self.source_section_start_samples,
            source_section_length_samples: self.source_section_length_samples,
            volume: self.take_volume,
            pan: self.take_pan,
            playback_rate: self.take_playback_rate,
        }
    }

    /// Inactive takes in their original list positions.
    pub fn other_takes(&self) -> &[AudioItemTake] {
        &self.other_takes
    }

    /// Returns the exclusive end position in project samples.
    pub fn end_sample(&self) -> u64 {
        self.start_sample + self.length_samples
    }

    pub(crate) fn active_take(&self) -> AudioItemTake {
        AudioItemTake {
            index: self.active_take_index,
            media_ref: self.media_ref.clone(),
            name: self.name.clone(),
            source_offset_samples: self.source_offset_samples,
            phase_inverted: self.phase_inverted,
            loop_source: self.loop_source,
            source_section_start_samples: self.source_section_start_samples,
            source_section_length_samples: self.source_section_length_samples,
            volume: self.take_volume,
            pan: self.take_pan,
            playback_rate: self.take_playback_rate,
        }
    }

    pub(crate) fn set_active_take(&mut self, take: &AudioItemTake) {
        self.media_ref.clone_from(&take.media_ref);
        self.name.clone_from(&take.name);
        self.source_offset_samples = take.source_offset_samples;
        self.phase_inverted = take.phase_inverted;
        self.loop_source = take.loop_source;
        self.source_section_start_samples = take.source_section_start_samples;
        self.source_section_length_samples = take.source_section_length_samples;
        self.take_volume = take.volume;
        self.take_pan = take.pan;
        self.take_playback_rate = take.playback_rate;
        self.active_take_index = take.index;
    }

    pub(crate) fn switch_active_take(&mut self, index: u32) -> bool {
        if index == self.active_take_index {
            return false;
        }
        let Some(position) = self.other_takes.iter().position(|take| take.index == index) else {
            return false;
        };
        let mut previous = self.active_take();
        let next = self.other_takes.remove(position);
        previous.index = self.active_take_index;
        self.set_active_take(&next);
        self.other_takes.push(previous);
        self.other_takes.sort_unstable_by_key(|take| take.index);
        true
    }

    pub(crate) fn duplicate_active_take(&mut self) -> bool {
        let Some(next_active_index) = self.active_take_index.checked_add(1) else {
            return false;
        };
        for take in &mut self.other_takes {
            let Some(index) = take.index.checked_add(1) else {
                return false;
            };
            take.index = index;
        }
        let mut duplicate = self.active_take();
        duplicate.index = 0;
        self.other_takes.push(duplicate);
        self.other_takes.sort_unstable_by_key(|take| take.index);
        self.active_take_index = next_active_index;
        true
    }

    pub(crate) fn append_take_snapshot(&mut self, take: &crate::AudioItemTakeSnapshot) -> bool {
        let Ok(index) = u32::try_from(self.take_count()) else {
            return false;
        };
        self.other_takes.push(AudioItemTake {
            index,
            media_ref: take.media_ref.clone(),
            name: take.name.clone(),
            source_offset_samples: take.source_offset_samples,
            phase_inverted: take.phase_inverted,
            loop_source: take.loop_source,
            source_section_start_samples: take.source_section_start_samples,
            source_section_length_samples: take.source_section_length_samples,
            volume: take.volume,
            pan: take.pan,
            playback_rate: take.playback_rate,
        });
        self.switch_active_take(index)
    }

    pub(crate) fn delete_active_take(&mut self) -> bool {
        if self.take_count() <= 1 {
            return false;
        }
        let removed_index = self.active_take_index;
        let Ok(last_index) = u32::try_from(self.take_count() - 2) else {
            return false;
        };
        let next_active_index = removed_index.min(last_index);
        let next_active_old_index = if next_active_index >= removed_index {
            let Some(index) = next_active_index.checked_add(1) else {
                return false;
            };
            index
        } else {
            next_active_index
        };
        let Some(position) = self
            .other_takes
            .iter()
            .position(|take| take.index == next_active_old_index)
        else {
            return false;
        };
        let mut next_active = self.other_takes.remove(position);
        next_active.index = next_active_index;
        for take in &mut self.other_takes {
            if take.index > removed_index {
                take.index -= 1;
            }
        }
        self.set_active_take(&next_active);
        self.other_takes.sort_unstable_by_key(|take| take.index);
        true
    }

    pub(crate) fn move_active_take_to_top(&mut self) -> bool {
        if self.active_take_index == 0 {
            return false;
        }
        let active_index = self.active_take_index;
        for take in &mut self.other_takes {
            if take.index < active_index {
                take.index += 1;
            }
        }
        self.active_take_index = 0;
        self.other_takes.sort_unstable_by_key(|take| take.index);
        true
    }

    pub(crate) fn crop_to_active_take(&mut self) -> bool {
        if self.other_takes.is_empty() {
            return false;
        }
        self.other_takes.clear();
        self.active_take_index = 0;
        true
    }
}
