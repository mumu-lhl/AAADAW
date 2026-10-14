use super::super::{App, Message};
use aaadaw_core::{MidiFileType, MidiMarkerType};
use iced::widget::{button, checkbox, column, container, radio, row, rule, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let Some(draft) = app.midi_export_dialog.as_ref() else {
        return container(text("MIDI export settings are unavailable"))
            .center(Length::Fill)
            .into();
    };
    let has_markers = !app.project.markers().is_empty();
    let marker_options_enabled = draft.export_markers && has_markers;
    let ppq_valid = draft
        .ppq
        .trim()
        .parse::<u16>()
        .is_ok_and(|ppq| (1..=0x7fff).contains(&ppq));
    let destination_valid = !draft.destination.trim().is_empty();

    let content = column![
        text("Export to MIDI file:").size(14),
        row![
            text_input("midi_export.mid", &draft.destination)
                .on_input(Message::MidiExportDestinationChanged)
                .size(13)
                .padding([5, 8])
                .width(Length::Fill),
            button("Browse…").on_press(Message::BrowseMidiExportDestination),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        radio(
            "Merge to single MIDI track (type 0 MIDI file)",
            MidiFileType::Type0,
            Some(draft.file_type),
            Message::MidiExportFileTypeChanged,
        ),
        radio(
            "Multitrack MIDI file (type 1 MIDI file)",
            MidiFileType::Type1,
            Some(draft.file_type),
            Message::MidiExportFileTypeChanged,
        ),
        row![
            text("PPQN:").size(13),
            text_input("960", &draft.ppq)
                .on_input(Message::MidiExportPpqChanged)
                .size(13)
                .padding([4, 8])
                .width(Length::Fixed(92.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        checkbox(draft.embed_tempo_and_time_signature)
            .label("Embed project tempo/time signature changes")
            .on_toggle(Message::MidiExportEmbedTempoChanged),
        checkbox(draft.embed_smpte_offset)
            .label("Embed SMPTE offset")
            .on_toggle(Message::MidiExportEmbedSmpteChanged),
        row![
            checkbox(draft.export_markers)
                .label("Export project markers as MIDI:")
                .on_toggle_maybe(has_markers.then_some(Message::MidiExportMarkersChanged),),
            radio(
                "markers",
                MidiMarkerType::Markers,
                Some(draft.marker_type),
                Message::MidiExportMarkerTypeChanged,
            )
            .style(move |theme, status| {
                marker_radio_style(theme, status, marker_options_enabled)
            }),
            radio(
                "cues",
                MidiMarkerType::Cues,
                Some(draft.marker_type),
                Message::MidiExportMarkerTypeChanged,
            )
            .style(move |theme, status| {
                marker_radio_style(theme, status, marker_options_enabled)
            }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        checkbox(draft.only_markers_starting_with_hash)
            .label("Only export markers that begin with '#'")
            .on_toggle_maybe(marker_options_enabled.then_some(Message::MidiExportHashOnlyChanged),),
        checkbox(draft.export_hidden_markers)
            .label("Export hidden markers")
            .on_toggle_maybe(
                marker_options_enabled.then_some(Message::MidiExportHiddenMarkersChanged),
            ),
        rule::horizontal(1),
        row![
            iced::widget::Space::new().width(Length::Fill),
            button("Cancel").on_press(Message::CloseMidiExportDialog),
            button("OK").style(button::primary).on_press_maybe(
                (ppq_valid && destination_valid).then_some(Message::ConfirmMidiExport)
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(2)
    .padding(10);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn marker_radio_style(
    theme: &iced::Theme,
    status: iced::widget::radio::Status,
    enabled: bool,
) -> iced::widget::radio::Style {
    if enabled {
        return iced::widget::radio::default(theme, status);
    }
    let disabled = iced::Color::from_rgb8(112, 116, 120);
    iced::widget::radio::Style {
        background: iced::Color::TRANSPARENT.into(),
        dot_color: disabled,
        border_width: 1.0,
        border_color: disabled,
        text_color: Some(disabled),
    }
}
