use super::super::{App, Message, commands};
use iced::widget::{button, checkbox, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Length};

const TRACK_LABEL_WIDTH: f32 = 190.0;
const ROUTE_CELL_WIDTH: f32 = 126.0;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let tracks = app.project.tracks();
    let graph_busy = commands::project_edit_busy(app);
    let mut matrix_rows: Vec<Element<'_, Message>> = Vec::new();

    let mut header_cells: Vec<Element<'_, Message>> = vec![
        text("Source track")
            .size(13)
            .width(Length::Fixed(TRACK_LABEL_WIDTH))
            .into(),
        container(text("Master / Parent").size(12))
            .width(Length::Fixed(ROUTE_CELL_WIDTH))
            .align_x(Alignment::Center)
            .into(),
    ];
    for (index, destination) in tracks.iter().enumerate() {
        header_cells.push(
            container(
                column![
                    text(format!("{}", index + 1)).size(10),
                    text(destination.name()).size(12),
                ]
                .spacing(2),
            )
            .width(Length::Fixed(ROUTE_CELL_WIDTH))
            .align_x(Alignment::Center)
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

    for (source_index, source) in tracks.iter().enumerate() {
        let source_id = source.id();
        let mut cells: Vec<Element<'_, Message>> = vec![
            text(format!("{}  {}", source_index + 1, source.name()))
                .size(12)
                .width(Length::Fixed(TRACK_LABEL_WIDTH))
                .into(),
            container(checkbox(source.main_send_enabled()).on_toggle_maybe(
                (!graph_busy).then_some(move |enabled| {
                    Message::SetRoutingMatrixMainSend(source_id, enabled)
                }),
            ))
            .width(Length::Fixed(ROUTE_CELL_WIDTH))
            .align_x(Alignment::Center)
            .into(),
        ];

        for destination in tracks {
            let destination_id = destination.id();
            let route_count = source
                .sends()
                .iter()
                .filter(|send| send.destination() == destination_id)
                .count();
            let can_connect = source_id != destination_id
                && (route_count > 0 || app.project.can_route_to(source_id, destination_id));
            let cell: Element<'_, Message> = if route_count > 0 {
                let label = if route_count > 1 {
                    format!("✓ ×{route_count}")
                } else {
                    "✓".to_owned()
                };
                button(text(label).size(11))
                    .on_press(Message::OpenTrackRouting(source_id))
                    .into()
            } else {
                checkbox(false)
                    .on_toggle_maybe((!graph_busy && can_connect).then_some(move |_| {
                        Message::SetRoutingMatrixSend(source_id, destination_id)
                    }))
                    .into()
            };
            cells.push(
                container(cell)
                    .width(Length::Fixed(ROUTE_CELL_WIDTH))
                    .align_x(Alignment::Center)
                    .into(),
            );
        }
        matrix_rows.push(row(cells).spacing(4).align_y(Alignment::Center).into());
    }

    let matrix = scrollable(column(matrix_rows).spacing(4))
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::default(),
            horizontal: scrollable::Scrollbar::default(),
        })
        .width(Length::Fill)
        .height(Length::Fill);
    let content = column![
        row![
            column![
                text("Routing Matrix").size(20),
                text(format!(
                    "{} tracks · Connect tracks; select an active send to edit it.",
                    tracks.len()
                ))
                .size(11),
            ]
            .spacing(2),
            iced::widget::Space::new().width(Length::Fill),
            button("Close").on_press(Message::CloseRoutingMatrix),
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
