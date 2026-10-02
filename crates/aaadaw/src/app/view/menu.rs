use super::super::{App, MainMenu, Message, PathPickerTarget, WorkspacePage};
use iced::widget::{button, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length};

const MENU_LEFT: f32 = 14.0;
const MENU_BAR_BOTTOM: f32 = 42.0;
const MENU_WIDTHS: [f32; 7] = [48.0, 48.0, 50.0, 56.0, 48.0, 52.0, 68.0];
const MENU_LABELS: [MainMenu; 7] = [
    MainMenu::File,
    MainMenu::Edit,
    MainMenu::View,
    MainMenu::Insert,
    MainMenu::Item,
    MainMenu::Track,
    MainMenu::Actions,
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

    const fn width(self) -> f32 {
        MENU_WIDTHS[match self {
            Self::File => 0,
            Self::Edit => 1,
            Self::View => 2,
            Self::Insert => 3,
            Self::Item => 4,
            Self::Track => 5,
            Self::Actions => 6,
        }]
    }
}

pub(super) fn bar(app: &App) -> Element<'_, Message> {
    let menus: Vec<Element<'_, Message>> = MENU_LABELS
        .into_iter()
        .map(|menu| {
            button(text(menu.label()).size(13))
                .width(Length::Fixed(menu.width()))
                .padding([5, 7])
                .style(move |_, status| menu_bar_style(app.active_menu == Some(menu), status))
                .on_press(Message::ToggleMainMenu(menu))
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
            .center_y(Length::Fixed(30.0)),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

pub(super) fn anchor_x(menu: MainMenu) -> f32 {
    let index = MENU_LABELS
        .iter()
        .position(|candidate| *candidate == menu)
        .unwrap_or(0);
    MENU_LEFT + MENU_WIDTHS[..index].iter().sum::<f32>()
}

pub(super) const fn bar_bottom() -> f32 {
    MENU_BAR_BOTTOM
}

pub(super) fn dropdown(app: &App, menu: MainMenu) -> Element<'_, Message> {
    let can_edit = !project_edit_busy(app);
    let contents: Element<'_, Message> = match menu {
        MainMenu::File => column![
            command(
                "Open project…",
                Some("Ctrl/Cmd+O"),
                can_edit && !app.is_dirty(),
                false,
                Message::OpenProject
            ),
            command(
                "Save project",
                Some("Ctrl/Cmd+S"),
                !project_file_busy(app),
                false,
                Message::SaveProject
            ),
            command(
                "Save project as…",
                None,
                can_edit,
                false,
                Message::PickPath(PathPickerTarget::SaveProject)
            ),
        ]
        .spacing(1)
        .into(),
        MainMenu::Edit => column![
            command("Undo", Some("Ctrl/Cmd+Z"), can_edit, false, Message::Undo),
            command(
                "Redo",
                Some("Ctrl/Cmd+Shift+Z"),
                can_edit,
                false,
                Message::Redo
            ),
        ]
        .spacing(1)
        .into(),
        MainMenu::View => column![
            command(
                "Arrangement",
                None,
                true,
                false,
                Message::SelectWorkspace(WorkspacePage::Arrangement)
            ),
            command(
                "Media",
                None,
                true,
                false,
                Message::SelectWorkspace(WorkspacePage::Media)
            ),
            command(
                "Project",
                None,
                true,
                false,
                Message::SelectWorkspace(WorkspacePage::Project)
            ),
        ]
        .spacing(1)
        .into(),
        MainMenu::Insert => column![
            command(
                "MIDI item",
                None,
                can_edit && !app.project.tracks().is_empty(),
                false,
                Message::AddMidiItem
            ),
            command(
                "Import audio…",
                None,
                can_edit && !app.project.tracks().is_empty() && app.project_path.is_some(),
                false,
                Message::PickPath(PathPickerTarget::ImportAudioToProject)
            ),
        ]
        .spacing(1)
        .into(),
        MainMenu::Item => item_menu(app, can_edit),
        MainMenu::Track => track_menu(app, can_edit),
        MainMenu::Actions => actions_menu(app),
    };
    let width = match menu {
        MainMenu::Actions => 360.0,
        MainMenu::Item | MainMenu::Track => 284.0,
        _ => 244.0,
    };
    container(contents)
        .width(Length::Fixed(width))
        .padding(4)
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

fn item_menu(app: &App, can_edit: bool) -> Element<'_, Message> {
    let selected_count = app.timeline.selected_items.len();
    let selected_audio = (selected_count == 1)
        .then_some(app.timeline.selected_item)
        .flatten()
        .filter(|item_id| {
            app.project
                .audio_items()
                .iter()
                .any(|item| item.id() == *item_id)
        });
    column![
        command(
            "Duplicate selected audio item",
            None,
            can_edit && selected_audio.is_some(),
            false,
            selected_audio
                .map(Message::DuplicateAudioItem)
                .unwrap_or(Message::DismissMainMenu),
        ),
        command(
            if selected_count == 1 {
                "Delete selected item"
            } else {
                "Delete selected items"
            },
            None,
            can_edit && selected_count > 0,
            true,
            Message::DeleteSelectedItems,
        ),
    ]
    .spacing(1)
    .into()
}

fn track_menu(app: &App, can_edit: bool) -> Element<'_, Message> {
    let selected = app.timeline.selected_track.and_then(|track_id| {
        app.project
            .tracks()
            .iter()
            .enumerate()
            .find(|(_, track)| track.id() == track_id)
            .map(|(index, track)| (track_id, index, track))
    });
    let track_id = selected.map(|(track_id, _, _)| track_id);
    let track_index = selected.map(|(_, index, _)| index);
    let track = selected.map(|(_, _, track)| track);
    let mut entries = column![
        command("Add track", None, can_edit, false, Message::AddTrack),
        rule::horizontal(1),
        command(
            "Rename selected track…",
            None,
            can_edit && track_id.is_some(),
            false,
            track_id
                .map(Message::BeginTrackNameEdit)
                .unwrap_or(Message::DismissMainMenu),
        ),
    ]
    .spacing(1);
    if let (Some(track_id), Some(track)) = (track_id, track) {
        entries = entries
            .push(command(
                if track.is_muted() {
                    "Unmute selected track"
                } else {
                    "Mute selected track"
                },
                None,
                can_edit,
                false,
                Message::ToggleMute(track_id),
            ))
            .push(command(
                if track.is_solo() {
                    "Unsolo selected track"
                } else {
                    "Solo selected track"
                },
                None,
                can_edit,
                false,
                Message::ToggleSolo(track_id),
            ))
            .push(command(
                "Move selected track up",
                None,
                can_edit && track_index.is_some_and(|index| index > 0),
                false,
                Message::MoveTrack(track_id, -1),
            ))
            .push(command(
                "Move selected track down",
                None,
                can_edit && track_index.is_some_and(|index| index + 1 < app.project.tracks().len()),
                false,
                Message::MoveTrack(track_id, 1),
            ))
            .push(command(
                "Delete selected track",
                None,
                can_edit,
                true,
                Message::DeleteTrack(track_id),
            ));
    } else {
        entries = entries.push(text("Right-click a track to select it").size(11));
    }
    entries.into()
}

fn actions_menu(app: &App) -> Element<'_, Message> {
    let query = app.action_query.trim().to_ascii_lowercase();
    let entries = action_entries(app)
        .into_iter()
        .filter(|entry| {
            query.is_empty()
                || format!("{} {}", entry.category, entry.label)
                    .to_ascii_lowercase()
                    .contains(&query)
        })
        .collect::<Vec<_>>();
    let mut results = column![].spacing(1);
    let mut category = None;
    for entry in entries {
        if category != Some(entry.category) {
            category = Some(entry.category);
            results = results.push(text(entry.category).size(10));
        }
        results = results.push(command(
            entry.label,
            entry.shortcut,
            entry.enabled,
            entry.destructive,
            entry.message,
        ));
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
            .width(Length::Fill),
        rule::horizontal(1),
        results,
    ]
    .spacing(5)
    .padding(4)
    .into()
}

struct ActionEntry {
    category: &'static str,
    label: &'static str,
    shortcut: Option<&'static str>,
    enabled: bool,
    destructive: bool,
    message: Message,
}

fn action_entries(app: &App) -> Vec<ActionEntry> {
    let can_edit = !project_edit_busy(app);
    let can_save = !project_file_busy(app);
    let selected_count = app.timeline.selected_items.len();
    let selected_audio = (selected_count == 1)
        .then_some(app.timeline.selected_item)
        .flatten()
        .filter(|item_id| {
            app.project
                .audio_items()
                .iter()
                .any(|item| item.id() == *item_id)
        });
    let selected_track = app.timeline.selected_track.and_then(|track_id| {
        app.project
            .tracks()
            .iter()
            .enumerate()
            .find(|(_, track)| track.id() == track_id)
            .map(|(index, track)| (track_id, index, track))
    });
    let track_id = selected_track.map(|(track_id, _, _)| track_id);
    let track_index = selected_track.map(|(_, index, _)| index);
    let track = selected_track.map(|(_, _, track)| track);
    let entries = vec![
        ActionEntry {
            category: "File",
            label: "Open project…",
            shortcut: Some("Ctrl/Cmd+O"),
            enabled: can_edit && !app.is_dirty(),
            destructive: false,
            message: Message::OpenProject,
        },
        ActionEntry {
            category: "File",
            label: "Save project",
            shortcut: Some("Ctrl/Cmd+S"),
            enabled: can_save,
            destructive: false,
            message: Message::SaveProject,
        },
        ActionEntry {
            category: "File",
            label: "Save project as…",
            shortcut: None,
            enabled: can_edit,
            destructive: false,
            message: Message::PickPath(PathPickerTarget::SaveProject),
        },
        ActionEntry {
            category: "Edit",
            label: "Undo",
            shortcut: Some("Ctrl/Cmd+Z"),
            enabled: can_edit,
            destructive: false,
            message: Message::Undo,
        },
        ActionEntry {
            category: "Edit",
            label: "Redo",
            shortcut: Some("Ctrl/Cmd+Shift+Z"),
            enabled: can_edit,
            destructive: false,
            message: Message::Redo,
        },
        ActionEntry {
            category: "View",
            label: "Arrangement",
            shortcut: None,
            enabled: true,
            destructive: false,
            message: Message::SelectWorkspace(WorkspacePage::Arrangement),
        },
        ActionEntry {
            category: "View",
            label: "Media",
            shortcut: None,
            enabled: true,
            destructive: false,
            message: Message::SelectWorkspace(WorkspacePage::Media),
        },
        ActionEntry {
            category: "View",
            label: "Project",
            shortcut: None,
            enabled: true,
            destructive: false,
            message: Message::SelectWorkspace(WorkspacePage::Project),
        },
        ActionEntry {
            category: "Insert",
            label: "MIDI item",
            shortcut: None,
            enabled: can_edit && !app.project.tracks().is_empty(),
            destructive: false,
            message: Message::AddMidiItem,
        },
        ActionEntry {
            category: "Insert",
            label: "Import audio…",
            shortcut: None,
            enabled: can_edit && !app.project.tracks().is_empty() && app.project_path.is_some(),
            destructive: false,
            message: Message::PickPath(PathPickerTarget::ImportAudioToProject),
        },
        ActionEntry {
            category: "Item",
            label: "Duplicate selected audio item",
            shortcut: None,
            enabled: can_edit && selected_audio.is_some(),
            destructive: false,
            message: selected_audio
                .map(Message::DuplicateAudioItem)
                .unwrap_or(Message::DismissMainMenu),
        },
        ActionEntry {
            category: "Item",
            label: "Delete selected items",
            shortcut: None,
            enabled: can_edit && selected_count > 0,
            destructive: true,
            message: Message::DeleteSelectedItems,
        },
        ActionEntry {
            category: "Track",
            label: "Add track",
            shortcut: None,
            enabled: can_edit,
            destructive: false,
            message: Message::AddTrack,
        },
        ActionEntry {
            category: "Track",
            label: "Rename selected track…",
            shortcut: None,
            enabled: can_edit && track_id.is_some(),
            destructive: false,
            message: track_id
                .map(Message::BeginTrackNameEdit)
                .unwrap_or(Message::DismissMainMenu),
        },
        ActionEntry {
            category: "Track",
            label: if track.is_some_and(|track| track.is_muted()) {
                "Unmute selected track"
            } else {
                "Mute selected track"
            },
            shortcut: None,
            enabled: can_edit && track_id.is_some(),
            destructive: false,
            message: track_id
                .map(Message::ToggleMute)
                .unwrap_or(Message::DismissMainMenu),
        },
        ActionEntry {
            category: "Track",
            label: if track.is_some_and(|track| track.is_solo()) {
                "Unsolo selected track"
            } else {
                "Solo selected track"
            },
            shortcut: None,
            enabled: can_edit && track_id.is_some(),
            destructive: false,
            message: track_id
                .map(Message::ToggleSolo)
                .unwrap_or(Message::DismissMainMenu),
        },
        ActionEntry {
            category: "Track",
            label: "Move selected track up",
            shortcut: None,
            enabled: can_edit && track_index.is_some_and(|index| index > 0),
            destructive: false,
            message: track_id
                .map(|track_id| Message::MoveTrack(track_id, -1))
                .unwrap_or(Message::DismissMainMenu),
        },
        ActionEntry {
            category: "Track",
            label: "Move selected track down",
            shortcut: None,
            enabled: can_edit
                && track_index.is_some_and(|index| index + 1 < app.project.tracks().len()),
            destructive: false,
            message: track_id
                .map(|track_id| Message::MoveTrack(track_id, 1))
                .unwrap_or(Message::DismissMainMenu),
        },
        ActionEntry {
            category: "Track",
            label: "Delete selected track",
            shortcut: None,
            enabled: can_edit && track_id.is_some(),
            destructive: true,
            message: track_id
                .map(Message::DeleteTrack)
                .unwrap_or(Message::DismissMainMenu),
        },
    ];
    entries
}

fn command(
    label: impl Into<String>,
    shortcut: Option<&str>,
    enabled: bool,
    destructive: bool,
    message: Message,
) -> iced::widget::Button<'static, Message> {
    let label = label.into();
    let shortcut = shortcut.unwrap_or_default().to_owned();
    button(
        row![text(label).width(Length::Fill), text(shortcut).size(11),]
            .spacing(12)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([5, 8])
    .style(move |_, status| menu_item_style(status, destructive))
    .on_press_maybe(enabled.then_some(message))
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

fn project_edit_busy(app: &App) -> bool {
    app.io_busy
        || app.path_picker_busy
        || app.import_busy
        || app.audio_asset_management_busy
        || app.playback_busy()
        || app.playback_active()
}

fn project_file_busy(app: &App) -> bool {
    app.io_busy || app.path_picker_busy || app.import_busy || app.audio_asset_management_busy
}
