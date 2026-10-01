use crate::Message;
use aaadaw_core::{Project, TrackId};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length};

const MAX_VISIBLE_ITEMS: usize = 200;

pub(super) fn view(project: &Project) -> Element<'static, Message> {
    let sample_rate = f64::from(project.settings().sample_rate());
    let mut audio_items = column![text("Audio items")].spacing(6);
    for item in project.audio_items().iter().take(MAX_VISIBLE_ITEMS) {
        let track_name = track_name(project, item.track_id());
        let start_seconds = item.start_sample() as f64 / sample_rate;
        let duration_seconds = item.length_samples() as f64 / sample_rate;
        let item_row = row![
            text(format!(
                "{} · {:.2}s · {:.2}s · {}",
                track_name,
                start_seconds,
                duration_seconds,
                item.media_ref()
            ))
            .width(Length::Fill),
            button("−1s").on_press(Message::NudgeAudioItem(item.id(), -1)),
            button("+1s").on_press(Message::NudgeAudioItem(item.id(), 1)),
        ]
        .spacing(8);
        #[cfg(feature = "jack-backend")]
        let item_row =
            item_row.push(button("Seek").on_press(Message::SeekToItem(item.start_sample())));
        audio_items = audio_items.push(item_row);
    }
    if project.audio_items().len() > MAX_VISIBLE_ITEMS {
        audio_items = audio_items.push(text(format!(
            "Showing {} of {} audio items",
            MAX_VISIBLE_ITEMS,
            project.audio_items().len()
        )));
    }

    let mut midi_items = column![text("MIDI items")].spacing(6);
    for item in project.midi_items().iter().take(MAX_VISIBLE_ITEMS) {
        midi_items = midi_items.push(text(format!(
            "{} · tick {} · length {} · {} notes",
            track_name(project, item.track_id()),
            item.start_tick(),
            item.length_ticks(),
            item.notes().len()
        )));
    }
    if project.midi_items().len() > MAX_VISIBLE_ITEMS {
        midi_items = midi_items.push(text(format!(
            "Showing {} of {} MIDI items",
            MAX_VISIBLE_ITEMS,
            project.midi_items().len()
        )));
    }

    let content = if project.audio_items().is_empty() && project.midi_items().is_empty() {
        column![
            text("Timeline").size(20),
            text("No items yet. Import audio to place it on the first track."),
            text(format!(
                "Project rate: {} Hz",
                project.settings().sample_rate()
            )),
        ]
    } else {
        column![
            text("Timeline").size(20),
            text(format!(
                "Project rate: {} Hz",
                project.settings().sample_rate()
            )),
            audio_items,
            midi_items,
        ]
        .spacing(12)
    };

    container(scrollable(content))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(18)
        .into()
}

fn track_name(project: &Project, track_id: TrackId) -> String {
    project
        .tracks()
        .iter()
        .find(|track| track.id() == track_id)
        .map(|track| track.name().to_owned())
        .unwrap_or_else(|| format!("Track {}", track_id.value()))
}
