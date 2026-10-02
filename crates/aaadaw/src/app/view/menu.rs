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
        actions_menu(app, commands::for_actions_menu(app))
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
        contents = contents.push(command(entry));
    }
    if menu == MainMenu::Track && app.selected_track_id().is_none() {
        contents = contents.push(text("Right-click a track to select it").size(11));
    }
    contents.into()
}

fn actions_menu(app: &App, entries: Vec<CommandEntry>) -> Element<'_, Message> {
    let query = app.action_query.trim().to_ascii_lowercase();
    let entries = entries
        .into_iter()
        .filter(|entry| query.is_empty() || entry.matches_query(&query))
        .collect::<Vec<_>>();
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
        results = results.push(command(entry));
    }
    let results: Element<'_, Message> = if category.is_some() {
        scrollable(results).height(Length::Fixed(284.0)).into()
    } else {
        container(text("No matching commands").size(12))
            .height(Length::Fixed(44.0))
            .center_y(Length::Fixed(44.0))
            .into()
    };
    column![
        text_input("Search actions…", &app.action_query)
            .on_input(Message::ActionQueryChanged)
            .on_submit(Message::RunActionQuery)
            .width(Length::Fill),
        rule::horizontal(1),
        results,
    ]
    .spacing(tokens::SECTION_GAP)
    .into()
}

fn command<'a>(entry: CommandEntry) -> iced::widget::Button<'a, Message> {
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
    .style(move |_, status| menu_item_style(status, destructive))
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

fn menu_item_style(status: button::Status, destructive: bool) -> button::Style {
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
