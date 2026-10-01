use crate::{ItemId, TrackId};

/// A timeline placement of an audio source on a track.
///
/// `start_sample` and `length_samples` use the project's sample clock;
/// `source_offset_samples` is relative to the referenced media source. The
/// media reference is opaque to the core and resolved by the media layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioItem {
    pub(crate) id: ItemId,
    pub(crate) track_id: TrackId,
    pub(crate) media_ref: String,
    pub(crate) start_sample: u64,
    pub(crate) source_offset_samples: u64,
    pub(crate) length_samples: u64,
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

    /// Returns the exclusive end position in project samples.
    pub fn end_sample(&self) -> u64 {
        self.start_sample + self.length_samples
    }
}
