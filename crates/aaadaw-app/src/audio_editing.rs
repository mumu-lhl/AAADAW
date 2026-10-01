//! Audio timeline editing intents that translate into validated project actions.

use aaadaw_core::{DawAction, ItemId, Project};
use std::fmt;

/// A failed audio editing intent that could not be turned into a project action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioEditError {
    ItemNotFound,
    PositionOutOfRange,
}

impl fmt::Display for AudioEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ItemNotFound => "audio item no longer exists",
            Self::PositionOutOfRange => "duplicate would exceed the sample timeline",
        })
    }
}

impl std::error::Error for AudioEditError {}

/// Duplicates an item immediately after itself, preserving its source range and media reference.
pub fn duplicate_audio_item(
    project: &Project,
    item_id: ItemId,
) -> Result<DawAction, AudioEditError> {
    let item = project
        .audio_items()
        .iter()
        .find(|item| item.id() == item_id)
        .ok_or(AudioEditError::ItemNotFound)?;
    let start_sample = item
        .start_sample()
        .checked_add(item.length_samples())
        .ok_or(AudioEditError::PositionOutOfRange)?;
    let _duplicate_end = start_sample
        .checked_add(item.length_samples())
        .ok_or(AudioEditError::PositionOutOfRange)?;

    Ok(DawAction::InsertAudioItem {
        track_id: item.track_id(),
        media_ref: item.media_ref().to_owned(),
        start_sample,
        source_offset_samples: item.source_offset_samples(),
        length_samples: item.length_samples(),
    })
}
