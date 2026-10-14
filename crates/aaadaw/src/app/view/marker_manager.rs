use super::super::{App, Message, commands};
use aaadaw_core::{Project, ProjectMarker, ProjectRegion};
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let Some(manager) = app.marker_manager.as_ref() else {
        return text("No marker manager open").into();
    };
    let header = row![
        text("#").width(Length::Fixed(52.0)),
        text("Name").width(Length::Fill),
        text("Start").width(Length::Fixed(86.0)),
        text("End").width(Length::Fixed(86.0)),
        text("Length").width(Length::Fixed(86.0)),
        text("Current").size(12).width(Length::Fixed(50.0)),
        text("Visible").size(12).width(Length::Fixed(50.0)),
        text("").width(Length::Fixed(104.0)),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let mut entries = column![].spacing(1);
    let mut visible_count = 0;
    enum Entry<'a> {
        Region(&'a ProjectRegion),
        Marker(&'a ProjectMarker),
    }
    let mut visible_entries = Vec::new();
    if manager.show_regions {
        visible_entries.extend(
            app.project
                .regions()
                .iter()
                .filter(|region| matches_region_filter(&app.project, region, &manager.filter))
                .map(Entry::Region),
        );
    }
    if manager.show_markers {
        visible_entries.extend(
            app.project
                .markers()
                .iter()
                .filter(|marker| matches_filter(&app.project, marker, &manager.filter))
                .map(Entry::Marker),
        );
    }
    visible_entries.sort_by_key(|entry| match entry {
        Entry::Region(region) => (region.start_ticks(), 0_u8, region.id().value()),
        Entry::Marker(marker) => (marker.position_ticks(), 1_u8, marker.id().value()),
    });
    for entry in visible_entries {
        visible_count += 1;
        let (row_button, visibility_control, edit_button, delete_button, is_selected) = match entry
        {
            Entry::Marker(marker) => {
                let marker_id = marker.id();
                let position = format_position(&app.project, marker.position_ticks());
                let visibility_control: Element<'_, Message> = checkbox(marker.is_visible())
                    .on_toggle_maybe((!commands::project_edit_busy(app)).then_some(
                        move |visible| {
                            Message::MarkerManagerSetMarkerVisibility(marker_id, visible)
                        },
                    ))
                    .width(Length::Fixed(50.0))
                    .into();
                let current = if marker.position_ticks() == app.timeline.edit_cursor_tick {
                    "•"
                } else {
                    ""
                };
                let is_selected = app.selected_render_markers.contains(&marker_id);
                let row_button = manager_row(ManagerRow {
                    number: marker_manager_id_label(marker),
                    name: marker.name(),
                    start: position,
                    end: "-".to_owned(),
                    length: "-".to_owned(),
                    current,
                    selected: is_selected,
                    message: Message::MarkerManagerSelectMarker(
                        marker_id,
                        app.keyboard_modifiers.command(),
                    ),
                });
                (
                    row_button,
                    visibility_control,
                    button("Edit").on_press(Message::MarkerManagerEditMarker(marker_id)),
                    button("Delete").on_press_maybe(
                        (!commands::project_edit_busy(app))
                            .then_some(Message::MarkerManagerDeleteMarker(marker_id)),
                    ),
                    is_selected,
                )
            }
            Entry::Region(region) => {
                let region_id = region.id();
                let visibility_control: Element<'_, Message> = checkbox(region.is_visible())
                    .on_toggle_maybe((!commands::project_edit_busy(app)).then_some(
                        move |visible| {
                            Message::MarkerManagerSetRegionVisibility(region_id, visible)
                        },
                    ))
                    .width(Length::Fixed(50.0))
                    .into();
                let start = format_position(&app.project, region.start_ticks());
                let end = format_position(&app.project, region.end_ticks());
                let length = super::super::item_properties::format_marker_length(
                    &app.project,
                    region.start_ticks(),
                    region.end_ticks(),
                )
                .unwrap_or_else(|_| format!("{} ticks", region.end_ticks() - region.start_ticks()));
                let current = if region.start_ticks() <= app.timeline.edit_cursor_tick
                    && app.timeline.edit_cursor_tick < region.end_ticks()
                {
                    "•"
                } else {
                    ""
                };
                let is_selected = app.selected_render_regions.contains(&region_id);
                let row_button = manager_row(ManagerRow {
                    number: region_manager_id_label(region),
                    name: region.name(),
                    start,
                    end,
                    length,
                    current,
                    selected: is_selected,
                    message: Message::MarkerManagerSelectRegion(
                        region_id,
                        app.keyboard_modifiers.command(),
                    ),
                });
                (
                    row_button,
                    visibility_control,
                    button("Edit").on_press(Message::MarkerManagerEditRegion(region_id)),
                    button("Delete").on_press_maybe(
                        (!commands::project_edit_busy(app))
                            .then_some(Message::MarkerManagerDeleteRegion(region_id)),
                    ),
                    is_selected,
                )
            }
        };
        let row_content = row![row_button, visibility_control, edit_button, delete_button]
            .spacing(4)
            .align_y(Alignment::Center);
        entries = entries.push(
            container(row_content)
                .width(Length::Fill)
                .style(move |_theme| {
                    if is_selected {
                        iced::widget::container::Style {
                            background: Some(iced::Color::from_rgb8(53, 74, 81).into()),
                            ..iced::widget::container::Style::default()
                        }
                    } else {
                        iced::widget::container::Style::default()
                    }
                }),
        );
    }
    if visible_count == 0 {
        let empty_message = if !manager.filter.trim().is_empty() {
            "No regions or markers match the filter"
        } else if manager.show_regions && manager.show_markers {
            "No project regions or markers"
        } else if manager.show_regions {
            "No project regions"
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

fn matches_region_filter(project: &Project, region: &ProjectRegion, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || region.name().to_lowercase().contains(&query)
        || region_manager_id_label(region)
            .to_lowercase()
            .contains(&query)
        || format_position(project, region.start_ticks())
            .to_lowercase()
            .contains(&query)
        || format_position(project, region.end_ticks())
            .to_lowercase()
            .contains(&query)
}

fn marker_manager_id_label(marker: &ProjectMarker) -> String {
    format!("M{}", marker.number())
}

fn region_manager_id_label(region: &ProjectRegion) -> String {
    format!("R{}", region.number())
}

struct ManagerRow<'a> {
    number: String,
    name: &'a str,
    start: String,
    end: String,
    length: String,
    current: &'a str,
    selected: bool,
    message: Message,
}

fn manager_row<'a>(row: ManagerRow<'a>) -> Element<'a, Message> {
    button(
        row![
            text(row.number).width(Length::Fixed(52.0)),
            text(row.name).width(Length::Fill),
            text(row.start).width(Length::Fixed(86.0)),
            text(row.end).width(Length::Fixed(86.0)),
            text(row.length).width(Length::Fixed(86.0)),
            text(row.current).width(Length::Fixed(50.0)),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([3, 5])
    .style(move |theme, status| {
        if row.selected {
            button::primary(theme, status)
        } else {
            button::secondary(theme, status)
        }
    })
    .on_press(row.message)
    .into()
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

    #[test]
    fn region_manager_searches_name_number_and_both_bar_beat_bounds() {
        let mut project = Project::new();
        project
            .apply(DawAction::InsertProjectRegion {
                start_ticks: 960,
                end_ticks: 2_880,
                name: "Verse".to_owned(),
            })
            .unwrap();
        let region = &project.regions()[0];

        assert_eq!(region_manager_id_label(region), "R1");
        assert_eq!(format_position(&project, region.start_ticks()), "1.2.00");
        assert_eq!(format_position(&project, region.end_ticks()), "1.4.00");
        assert!(matches_region_filter(&project, region, "verse"));
        assert!(matches_region_filter(&project, region, "r1"));
        assert!(matches_region_filter(&project, region, "1.2.00"));
        assert!(matches_region_filter(&project, region, "1.4.00"));
        assert!(!matches_region_filter(&project, region, "chorus"));
    }
}
