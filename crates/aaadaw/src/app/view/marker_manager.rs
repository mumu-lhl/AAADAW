use super::super::{App, Message, commands};
use aaadaw_core::{Project, ProjectMarker};
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let Some(manager) = app.marker_manager.as_ref() else {
        return text("No marker manager open").into();
    };
    let header = row![
        text("#").width(Length::Fixed(50.0)),
        text("Name").width(Length::Fill),
        text("Start").width(Length::Fixed(116.0)),
        text("Current").width(Length::Fixed(64.0)),
        text("").width(Length::Fixed(104.0)),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let mut entries = column![].spacing(1);
    let mut visible_count = 0;
    if manager.show_markers {
        for marker in app
            .project
            .markers()
            .iter()
            .filter(|marker| matches_filter(&app.project, marker, &manager.filter))
        {
            visible_count += 1;
            let marker_id = marker.id();
            let position = format_position(&app.project, marker.position_ticks());
            let current = if marker.position_ticks() == app.timeline.edit_cursor_tick {
                "•"
            } else {
                ""
            };
            let is_selected = manager.selected_marker == Some(marker_id);
            let row_button = button(
                row![
                    text(marker_manager_id_label(marker)).width(Length::Fixed(96.0)),
                    text(marker.name()).width(Length::Fill),
                    text(position).width(Length::Fixed(116.0)),
                    text(current).width(Length::Fixed(64.0)),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([3, 5])
            .style(move |theme, status| {
                if is_selected {
                    button::primary(theme, status)
                } else {
                    button::secondary(theme, status)
                }
            })
            .on_press(Message::MarkerManagerSelectMarker(marker_id));
            let row_content = row![
                row_button,
                button("Edit").on_press(Message::MarkerManagerEditMarker(marker_id)),
                button("Delete").on_press_maybe(
                    (!commands::project_edit_busy(app))
                        .then_some(Message::MarkerManagerDeleteMarker(marker_id)),
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center);
            entries = entries.push(container(row_content).width(Length::Fill).style(
                move |_theme| {
                    if is_selected {
                        iced::widget::container::Style {
                            background: Some(iced::Color::from_rgb8(53, 74, 81).into()),
                            ..iced::widget::container::Style::default()
                        }
                    } else {
                        iced::widget::container::Style::default()
                    }
                },
            ));
        }
    }
    if visible_count == 0 {
        let empty_message = if manager.show_markers && !manager.filter.trim().is_empty() {
            "No markers match the filter"
        } else if manager.show_regions && !manager.show_markers {
            "Region support is still in progress"
        } else if manager.show_markers {
            "No project markers"
        } else {
            "Enable Markers to list project markers"
        };
        entries = entries.push(container(text(empty_message).size(12)).padding(8));
    }

    let toolbar = row![
        text_input("Filter", &manager.filter)
            .on_input(Message::MarkerManagerFilterChanged)
            .width(Length::Fill),
        button("Clear").on_press(Message::MarkerManagerClearFilter),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let filters = row![
        checkbox(manager.show_regions)
            .label("Regions")
            .on_toggle(Message::MarkerManagerShowRegionsChanged),
        checkbox(manager.show_markers)
            .label("Markers")
            .on_toggle(Message::MarkerManagerShowMarkersChanged),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let content = column![
        toolbar,
        header,
        scrollable(entries).height(Length::Fill),
        filters,
        row![
            text("Region/Marker Manager").size(10),
            iced::widget::Space::new().width(Length::Fill),
            button("Close").on_press(Message::CloseMarkerManager),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(6)
    .padding(8)
    .width(Length::Fill)
    .height(Length::Fill);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn matches_filter(project: &Project, marker: &ProjectMarker, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || marker.name().to_lowercase().contains(&query)
        || marker_manager_id_label(marker)
            .to_lowercase()
            .contains(&query)
        || format_position(project, marker.position_ticks())
            .to_lowercase()
            .contains(&query)
}

fn marker_manager_id_label(marker: &ProjectMarker) -> String {
    format!("M{}", marker.number())
}

fn format_position(project: &Project, tick: u64) -> String {
    let Ok(position) = project.musical_position_at_tick(tick) else {
        return format!("{tick} ticks");
    };
    let meter = project.time_signature_at_tick(tick);
    let ticks_per_beat =
        (u64::from(project.settings().ppq()) * 4 / u64::from(meter.denominator())).max(1);
    let fraction = (position.tick_in_beat() * 100 + ticks_per_beat / 2) / ticks_per_beat;
    if fraction >= 100 {
        let next_beat_tick =
            tick.saturating_add(ticks_per_beat.saturating_sub(position.tick_in_beat()));
        if let Ok(next) = project.musical_position_at_tick(next_beat_tick) {
            return format!("{}.{}.00", next.measure(), next.beat());
        }
    }
    format!("{}.{}.{fraction:02}", position.measure(), position.beat())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_core::DawAction;

    #[test]
    fn marker_manager_searches_name_id_and_reaper_bar_beat_position() {
        let mut project = Project::new();
        project
            .apply(DawAction::InsertNumberedProjectMarker {
                number: 5,
                position_ticks: 1920,
                name: "Bridge".to_owned(),
            })
            .unwrap();
        let marker = &project.markers()[0];

        assert_eq!(format_position(&project, 1920), "1.3.00");
        assert_eq!(marker_manager_id_label(marker), "M5");
        assert!(matches_filter(&project, marker, "bridge"));
        assert!(matches_filter(&project, marker, "5"));
        assert!(matches_filter(&project, marker, "m5"));
        assert!(matches_filter(&project, marker, "1.3.00"));
        assert!(!matches_filter(&project, marker, "verse"));
    }
}
