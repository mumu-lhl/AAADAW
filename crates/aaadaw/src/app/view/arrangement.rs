use super::super::{App, Message};
use aaadaw_core::Track;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let mut track_list = column![
        row![
            text("Tracks").size(18).width(Length::Fill),
            button("Add track").on_press(Message::AddTrack),
        ]
        .spacing(8),
    ]
    .spacing(10);
    for track in app.project.tracks() {
        let track_id = track.id();
        let edited_name = app
            .track_name_edits
            .get(&track_id)
            .map_or(track.name(), String::as_str);
        track_list = track_list.push(track_row(
            track,
            edited_name,
            app.track_name_edits.contains_key(&track_id),
        ));
    }
    if app.project.tracks().is_empty() {
        track_list = track_list.push(text("No tracks yet. Add one to start."));
    }
    let tracks = container(scrollable(track_list))
        .width(300)
        .height(Length::Fill)
        .padding(14);
    let editor = crate::timeline::view(
        &app.project,
        &app.audio_asset_source_statuses,
        &app.audio_item_start_edits,
    );
    row![tracks, editor].spacing(12).height(Length::Fill).into()
}

fn track_row<'a>(track: &'a Track, edited_name: &'a str, has_edit: bool) -> Element<'a, Message> {
    let track_id = track.id();
    let heading = row![
        text_input("Track name", edited_name)
            .on_input(move |name| Message::TrackNameChanged(track_id, name))
            .on_submit(Message::CommitTrackName(track_id))
            .width(Length::Fill),
        button(if has_edit { "Save" } else { "Rename" })
            .on_press(Message::CommitTrackName(track_id)),
        button("↑").on_press(Message::MoveTrack(track_id, -1)),
        button("↓").on_press(Message::MoveTrack(track_id, 1)),
        button("Delete").on_press(Message::DeleteTrack(track_id)),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let controls = row![
        button(if track.is_muted() { "Unmute" } else { "Mute" })
            .on_press(Message::ToggleMute(track_id)),
        button(if track.is_solo() { "Unsolo" } else { "Solo" })
            .on_press(Message::ToggleSolo(track_id)),
        button("−dB").on_press(Message::AdjustVolume(track_id, -1.0)),
        button("+dB").on_press(Message::AdjustVolume(track_id, 1.0)),
        button("◀").on_press(Message::AdjustPan(track_id, -0.1)),
        button("▶").on_press(Message::AdjustPan(track_id, 0.1)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    column![heading, controls].spacing(6).into()
}
