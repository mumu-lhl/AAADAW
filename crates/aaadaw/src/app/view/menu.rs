use super::super::commands::CommandEntry;
use super::super::{App, MainMenu, Message, commands};
use super::tokens;
use iced::widget::{button, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length};

const MENU_LEFT: f32 = tokens::SPACING_LG;
const MENU_BAR_HEIGHT: f32 = 30.0;

#[derive(Clone, Copy)]
struct MenuLayout {
    menu: MainMenu,
    button_width: f32,
    popup_width: f32,
}

const MENU_LAYOUT: [MenuLayout; 7] = [
    MenuLayout {
        menu: MainMenu::File,
        button_width: 48.0,
        popup_width: 244.0,
    },
    MenuLayout {
        menu: MainMenu::Edit,
        button_width: 48.0,
        popup_width: 284.0,
    },
    MenuLayout {
        menu: MainMenu::View,
        button_width: 50.0,
        popup_width: 244.0,
    },
    MenuLayout {
        menu: MainMenu::Insert,
        button_width: 56.0,
        popup_width: 244.0,
    },
    MenuLayout {
        menu: MainMenu::Item,
        button_width: 48.0,
        popup_width: 284.0,
    },
    MenuLayout {
        menu: MainMenu::Track,
        button_width: 52.0,
        popup_width: 284.0,
    },
    MenuLayout {
        menu: MainMenu::Actions,
        button_width: 68.0,
        popup_width: 400.0,
    },
];

impl MainMenu {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Insert => "Insert",
            Self::Item => "Item",
            Self::Track => "Track",
            Self::Actions => "Actions",
        }
    }
}

pub(super) fn bar(app: &App) -> Element<'_, Message> {
    let menus: Vec<Element<'_, Message>> = MENU_LAYOUT
        .into_iter()
        .map(|layout| {
            button(text(layout.menu.label()).size(13))
                .width(Length::Fixed(layout.button_width))
                .padding([tokens::SPACING_XS, tokens::SPACING_SM])
                .style(move |_, status| {
                    menu_bar_style(app.active_menu == Some(layout.menu), status)
                })
                .on_press(Message::ToggleMainMenu(layout.menu))
                .into()
        })
        .collect();
    let project_name = app
        .project_path
        .as_ref()
        .and_then(|path| path.file_name())
        .map_or_else(
            || "New project".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
    let title = format!("{project_name}{}", if app.is_dirty() { "  *" } else { "" });

    row![
        row(menus).spacing(0).align_y(Alignment::Center),
        container(text(title).size(13))
            .width(Length::Fill)
            .center_y(Length::Fixed(MENU_BAR_HEIGHT)),
    ]
    .spacing(tokens::SPACING_SM)
    .align_y(Alignment::Center)
    .into()
}

pub(super) fn anchor_x(menu: MainMenu) -> f32 {
    let mut x = MENU_LEFT;
    for layout in MENU_LAYOUT {
        if layout.menu == menu {
            return x;
        }
        x += layout.button_width;
    }
    MENU_LEFT
}

pub(super) const fn bar_bottom() -> f32 {
    tokens::SPACING_LG + MENU_BAR_HEIGHT
}

pub(super) fn dropdown(app: &App, menu: MainMenu) -> Element<'_, Message> {
    let contents = if menu == MainMenu::Actions {
        actions_menu(app)
    } else {
        menu_commands(app, menu, commands::for_menu(app, menu))
    };
    let popup_width = MENU_LAYOUT
        .iter()
        .find(|layout| layout.menu == menu)
        .map_or(244.0, |layout| layout.popup_width);
    container(contents)
        .width(Length::Fixed(popup_width))
        .padding(tokens::PANEL_PADDING)
        .style(|_| container::Style {
            background: Some(Color::from_rgb8(37, 41, 44).into()),
            border: Border::default()
                .color(Color::from_rgb8(83, 91, 96))
                .width(1.0)
                .rounded(2.0),
            ..container::Style::default()
        })
        .into()
}

fn menu_commands(app: &App, menu: MainMenu, entries: Vec<CommandEntry>) -> Element<'_, Message> {
    let mut contents = column![].spacing(tokens::ROW_GAP);
    for entry in entries {
        if entry.separator_before {
            contents = contents.push(rule::horizontal(1));
        }
        let selected = app.menu_selected_command == Some(entry.id);
        contents = contents.push(command(entry, selected));
    }
    if menu == MainMenu::Track && app.selected_track_id().is_none() {
        contents = contents.push(text("Right-click a track to select it").size(11));
    }
    contents =
        contents.push(text("↑/↓ select · ←/→ switch sections · Enter run · Esc close").size(9));
    contents.into()
}

pub(super) fn offline_jobs_panel(app: &App) -> Element<'_, Message> {
    let visible_rows = 2
        + usize::from(app.active_offline_job.is_some())
        + app
            .offline_job_queue
            .len()
            .max(usize::from(app.offline_job_queue.is_empty()))
        + app.offline_job_history.len();
    let panel_height = (46.0 + visible_rows as f32 * 18.0).min(280.0);
    let mut jobs =
        column![rule::horizontal(1), text("Offline jobs").size(11)].spacing(tokens::ROW_GAP);
    if let Some(active) = &app.active_offline_job {
        jobs = jobs.push(
            row![
                text(format!("Active: {}", active.job.label())).size(10),
                button("Cancel")
                    .padding([1, 6])
                    .on_press(Message::CancelOfflineRender),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    } else {
        jobs = jobs.push(text("No active job").size(10));
    }
    jobs = jobs.push(
        text(format!(
            "Waiting: {} / {} ({} slots free)",
            app.offline_job_queue.len(),
            super::super::offline_job_queue::MAX_PENDING_OFFLINE_JOBS,
            app.offline_job_queue.remaining_capacity(),
        ))
        .size(10),
    );
    if app.offline_job_queue.is_empty() {
        jobs = jobs.push(text("Queue is empty").size(10));
    } else {
        for (id, job) in app.offline_job_queue.iter() {
            jobs = jobs.push(
                row![
                    text(format!("Queued: {}", job.label())).size(10),
                    button("Remove")
                        .padding([1, 6])
                        .on_press(Message::RemoveQueuedOfflineJob(id.value())),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
    }
    for result in &app.offline_job_history {
        jobs = jobs.push(text(result).size(9));
    }
    container(scrollable(jobs).height(Length::Fixed(panel_height)))
        .width(Length::Fixed(420.0))
        .padding(tokens::PANEL_PADDING)
        .style(|_| container::Style {
            background: Some(Color::from_rgb8(37, 41, 44).into()),
            border: Border::default()
                .color(Color::from_rgb8(83, 91, 96))
                .width(1.0)
                .rounded(2.0),
            ..container::Style::default()
        })
        .into()
}

fn actions_menu(app: &App) -> Element<'_, Message> {
    let entries = commands::matching_actions_menu(app, &app.action_query);
    let mut results = column![].spacing(tokens::ROW_GAP);
    let mut category = None;
    for entry in entries {
        if category != Some(entry.category) {
            category = Some(entry.category);
            results = results.push(text(entry.category).size(10));
        }
        if entry.separator_before {
            results = results.push(rule::horizontal(1));
        }
        let selected = app.menu_selected_command == Some(entry.id);
        results = results.push(command(entry, selected));
    }
    let results: Element<'_, Message> = if category.is_some() {
        scrollable(results)
            .id(iced::widget::Id::new("aaadaw-actions-menu-results"))
            .height(Length::Fixed(284.0))
            .on_scroll(|viewport| Message::ActionMenuScrolled(viewport.absolute_offset().y))
            .into()
    } else {
        container(text("No matching commands").size(12))
            .height(Length::Fixed(44.0))
            .center_y(Length::Fixed(44.0))
            .into()
    };
    column![
        text_input("Search actions…", &app.action_query)
            .id(super::super::messages::action_search_input_id())
            .on_input(Message::ActionQueryChanged)
            .width(Length::Fill),
        text("↑/↓ results · Enter run · Alt+←/→ switch menus · Esc close").size(9),
        rule::horizontal(1),
        results,
    ]
    .spacing(tokens::SECTION_GAP)
    .into()
}

fn command<'a>(entry: CommandEntry, selected: bool) -> iced::widget::Button<'a, Message> {
    let message = Message::ExecuteCommand(entry.id);
    let destructive = entry.destructive;
    button(
        row![
            text(entry.label.clone()).width(Length::Fill),
            text(entry.shortcut.unwrap_or_default()).size(11),
        ]
        .spacing(tokens::SPACING_MD)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([tokens::SPACING_XS, tokens::SPACING_LG])
    .style(move |_, status| menu_item_style(status, destructive, selected))
    .on_press_maybe(entry.enabled.then_some(message))
}

fn menu_bar_style(active: bool, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Hovered => (
            Color::from_rgb8(58, 64, 68),
            Color::from_rgb8(242, 244, 245),
        ),
        button::Status::Pressed => (
            Color::from_rgb8(76, 91, 98),
            Color::from_rgb8(250, 251, 251),
        ),
        _ if active => (
            Color::from_rgb8(68, 78, 83),
            Color::from_rgb8(245, 247, 248),
        ),
        _ => (Color::TRANSPARENT, Color::from_rgb8(207, 212, 215)),
    };
    button::Style {
        background: (background.a > 0.0).then_some(Background::Color(background)),
        text_color,
        border: Border::default().rounded(0.0),
        ..button::Style::default()
    }
}

fn menu_item_style(status: button::Status, destructive: bool, selected: bool) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Disabled => (Color::TRANSPARENT, Color::from_rgb8(116, 122, 126)),
        button::Status::Hovered | button::Status::Pressed => (
            if destructive {
                Color::from_rgb8(119, 58, 53)
            } else {
                Color::from_rgb8(67, 91, 103)
            },
            Color::from_rgb8(248, 249, 250),
        ),
        _ if selected => (
            Color::from_rgb8(67, 91, 103),
            Color::from_rgb8(248, 249, 250),
        ),
        _ => (
            Color::TRANSPARENT,
            if destructive {
                Color::from_rgb8(225, 171, 164)
            } else {
                Color::from_rgb8(218, 222, 224)
            },
        ),
    };
    button::Style {
        background: (background.a > 0.0).then_some(Background::Color(background)),
        text_color,
        border: Border::default().rounded(0.0),
        ..button::Style::default()
    }
}
