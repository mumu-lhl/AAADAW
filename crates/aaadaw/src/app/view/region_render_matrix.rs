use super::super::{App, Message, commands};
use iced::widget::{button, checkbox, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let mut matrix_rows: Vec<Element<'_, Message>> = Vec::new();
    let mut header_cells: Vec<Element<'_, Message>> = vec![
        text("Render target")
            .size(13)
            .width(Length::Fixed(190.0))
            .into(),
    ];
    let format_region_time = |ticks| {
        app.project
            .sample_at_tick(ticks)
            .ok()
            .and_then(|sample| app.project.project_time_at_sample(sample).ok())
            .map(crate::project_time::format_project_time)
            .unwrap_or_else(|| "—".to_owned())
    };
    for region in app.project.regions() {
        header_cells.push(
            container(
                column![
                    text(format!("#{} {}", region.number(), region.name())).size(12),
                    text(format!(
                        "{}–{}",
                        format_region_time(region.start_ticks()),
                        format_region_time(region.end_ticks())
                    ))
                    .size(10),
                ]
                .spacing(2),
            )
            .width(Length::Fixed(150.0))
            .padding([2, 6])
            .into(),
        );
    }
    matrix_rows.push(
        row(header_cells)
            .spacing(4)
            .align_y(Alignment::Center)
            .into(),
    );
    matrix_rows.push(rule::horizontal(1).into());

    let master_cells = app.project.regions().iter().map(|region| {
        let region_id = region.id();
        checkbox(region.renders_master_mix())
            .on_toggle_maybe(
                (!commands::project_edit_busy(app)).then_some(move |enabled| {
                    Message::SetRegionRenderMatrixMaster(region_id, enabled)
                }),
            )
            .width(Length::Fixed(150.0))
            .into()
    });
    let mut master_row: Vec<Element<'_, Message>> = vec![
        text("Master mix")
            .size(12)
            .width(Length::Fixed(190.0))
            .into(),
    ];
    master_row.extend(master_cells);
    matrix_rows.push(row(master_row).spacing(4).align_y(Alignment::Center).into());

    for track in app.project.tracks() {
        let track_id = track.id();
        let cells = app.project.regions().iter().map(|region| {
            let region_id = region.id();
            checkbox(region.render_track_ids().contains(&track_id))
                .on_toggle_maybe(
                    (!commands::project_edit_busy(app)).then_some(move |enabled| {
                        Message::SetRegionRenderMatrixTrack(region_id, track_id, enabled)
                    }),
                )
                .width(Length::Fixed(150.0))
                .into()
        });
        let mut track_row: Vec<Element<'_, Message>> = vec![
            text(track.name())
                .size(12)
                .width(Length::Fixed(190.0))
                .into(),
        ];
        track_row.extend(cells);
        matrix_rows.push(row(track_row).spacing(4).align_y(Alignment::Center).into());
    }

    let matrix = scrollable(column(matrix_rows).spacing(4))
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::default(),
            horizontal: scrollable::Scrollbar::default(),
        })
        .height(Length::Fill);
    let content = column![
        row![
            column![
                text("Region Render Matrix").size(20),
                text("Choose whether each region renders the Master mix and which tracks render as stems.").size(11),
            ]
            .spacing(2),
            iced::widget::Space::new().width(Length::Fill),
            button("Close").on_press(Message::CloseRegionRenderMatrix),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        rule::horizontal(1),
        matrix,
    ]
    .spacing(8)
    .padding(12)
    .width(Length::Fill)
    .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
