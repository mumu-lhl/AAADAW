use crate::{ItemId, TrackId};

/// A timeline placement of an audio source on a track.
///
/// `start_sample` and `length_samples` use the project's sample clock;
/// `source_offset_samples` is relative to the referenced media source. The
/// media reference is opaque to the core and resolved by the media layer.
pub const MAX_AUDIO_ITEM_VOLUME: f32 = 16.0;
/// Maximum linear volume for an audio item's active take.
pub const MAX_AUDIO_ITEM_TAKE_VOLUME: f32 = 16.0;

#[derive(Clone, Debug, PartialEq)]
pub struct AudioItem {
    pub(crate) id: ItemId,
    pub(crate) track_id: TrackId,
    pub(crate) media_ref: String,
    pub(crate) start_sample: u64,
    pub(crate) source_offset_samples: u64,
    pub(crate) length_samples: u64,
    pub(crate) muted: bool,
    pub(crate) loop_source: bool,
    pub(crate) source_section_start_samples: Option<u64>,
    pub(crate) source_section_length_samples: Option<u64>,
    pub(crate) fades: crate::AudioItemFades,
    pub(crate) volume: f32,
    pub(crate) take_volume: f32,
    pub(crate) take_pan: f32,
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

    /// Returns the absolute start position in project samples.
    pub fn start_sample(&self) -> u64 {
        self.start_sample
    }

    /// Returns the source offset in media sample frames.
    pub fn source_offset_samples(&self) -> u64 {
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

    /// Returns the exclusive end position in project samples.
    pub fn end_sample(&self) -> u64 {
        self.start_sample + self.length_samples
    }
}
