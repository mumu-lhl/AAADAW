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

/// Sets an item's exact sample-clock start position while preserving its source range.
pub fn set_audio_item_start_sample(
    project: &Project,
    item_id: ItemId,
    start_sample: u64,
) -> Result<DawAction, AudioEditError> {
    let item = project
        .audio_items()
        .iter()
        .find(|item| item.id() == item_id)
        .ok_or(AudioEditError::ItemNotFound)?;
    let _end_sample = start_sample
        .checked_add(item.length_samples())
        .ok_or(AudioEditError::PositionOutOfRange)?;

    Ok(DawAction::EditAudioItem {
        item_id,
        media_ref: item.media_ref().to_owned(),
        start_sample,
        source_offset_samples: item.source_offset_samples(),
        length_samples: item.length_samples(),
    })
}

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

    Ok(DawAction::DuplicateAudioItemAt {
        item_id,
        track_id: item.track_id(),
        start_sample,
    })
}
