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
            button("−1s").on_press(Message::NudgeAudioItem(item.id(), -1, 1_000)),
            button("−100ms").on_press(Message::NudgeAudioItem(item.id(), -1, 100)),
            button("−10ms").on_press(Message::NudgeAudioItem(item.id(), -1, 10)),
            button("+10ms").on_press(Message::NudgeAudioItem(item.id(), 1, 10)),
            button("+100ms").on_press(Message::NudgeAudioItem(item.id(), 1, 100)),
            button("+1s").on_press(Message::NudgeAudioItem(item.id(), 1, 1_000)),
            button("Delete").on_press(Message::DeleteAudioItem(item.id())),
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

    let midi_heading = row![
        text("MIDI items").width(Length::Fill),
        button("Add 4-beat item").on_press(Message::AddMidiItem),
    ];
    let mut midi_items = column![midi_heading].spacing(6);
    for item in project.midi_items().iter().take(MAX_VISIBLE_ITEMS) {
        let mut item_heading = row![
            text(format!(
                "{} · tick {} · length {} · {} notes",
                track_name(project, item.track_id()),
                item.start_tick(),
                item.length_ticks(),
                item.notes().len()
            ))
            .width(Length::Fill),
            button("−beat").on_press(Message::NudgeMidiItem(item.id(), -1)),
            button("+beat").on_press(Message::NudgeMidiItem(item.id(), 1)),
            button("Add C4").on_press(Message::AddMidiNote(item.id())),
            button("Delete item").on_press(Message::DeleteMidiItem(item.id())),
        ];
        if !item.notes().is_empty() {
            item_heading = item_heading
                .push(button("Quantize 1/16").on_press(Message::QuantizeMidiItem(item.id())));
        }
        let mut item_content = column![item_heading].spacing(4);
        for note in item.notes().iter().take(MAX_VISIBLE_ITEMS) {
            item_content = item_content.push(
                row![
                    text(format!(
                        "{} · tick {} · {} ticks · vel {}",
                        midi_pitch_name(note.pitch()),
                        note.tick(),
                        note.duration(),
                        note.velocity()
                    ))
                    .width(Length::Fill),
                    button("−1/16").on_press(Message::NudgeMidiNote(item.id(), note.id(), -1)),
                    button("+1/16").on_press(Message::NudgeMidiNote(item.id(), note.id(), 1)),
                    button("Pitch−").on_press(Message::AdjustMidiNotePitch(
                        item.id(),
                        note.id(),
                        -1
                    )),
                    button("Pitch+").on_press(Message::AdjustMidiNotePitch(
                        item.id(),
                        note.id(),
                        1
                    )),
                    button("Vel−").on_press(Message::AdjustMidiNoteVelocity(
                        item.id(),
                        note.id(),
                        -1
                    )),
                    button("Vel+").on_press(Message::AdjustMidiNoteVelocity(
                        item.id(),
                        note.id(),
                        1
                    )),
                    button("Delete").on_press(Message::DeleteMidiNote(item.id(), note.id())),
                ]
                .spacing(6),
            );
        }
        if item.notes().len() > MAX_VISIBLE_ITEMS {
            item_content = item_content.push(text(format!(
                "Showing {} of {} notes",
                MAX_VISIBLE_ITEMS,
                item.notes().len()
            )));
        }
        midi_items = midi_items.push(container(item_content).padding(8));
    }
    if project.midi_items().len() > MAX_VISIBLE_ITEMS {
        midi_items = midi_items.push(text(format!(
            "Showing {} of {} MIDI items",
            MAX_VISIBLE_ITEMS,
            project.midi_items().len()
        )));
    }

    let mut content = column![
        text("Timeline").size(20),
        text(format!(
            "Project rate: {} Hz",
            project.settings().sample_rate()
        )),
    ];
    if project.audio_items().is_empty() && project.midi_items().is_empty() {
        content = content.push(text(
            "No items yet. Import audio or add a MIDI item to start editing.",
        ));
    }
    let content = content.push(audio_items).push(midi_items).spacing(12);

    container(scrollable(content))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(18)
        .into()
}

fn midi_pitch_name(pitch: u8) -> String {
    const PITCH_CLASSES: [&str; 12] = [
        "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
    ];
    let pitch_class = PITCH_CLASSES[usize::from(pitch % 12)];
    let octave = i16::from(pitch) / 12 - 1;
    format!("{pitch_class}{octave}")
}

fn track_name(project: &Project, track_id: TrackId) -> String {
    project
        .tracks()
        .iter()
        .find(|track| track.id() == track_id)
        .map(|track| track.name().to_owned())
        .unwrap_or_else(|| format!("Track {}", track_id.value()))
}
