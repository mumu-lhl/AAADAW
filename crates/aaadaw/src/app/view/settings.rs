use super::super::{App, Message, PathPickerTarget, SettingsCategory, commands};
use super::tokens;
use aaadaw_engine::MasterOutputCeiling;
use iced::widget::{button, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let keyboard_selected = app.settings_category == SettingsCategory::KeyboardShortcuts;
    let macros_selected = app.settings_category == SettingsCategory::ActionMacros;
    let plugins_selected = app.settings_category == SettingsCategory::ClapPlugins;
    let audio_selected = app.settings_category == SettingsCategory::Audio;
    let navigation = column![
        text("Settings").size(13),
        button("Keyboard Shortcuts")
            .width(Length::Fill)
            .style(if keyboard_selected {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::SelectSettingsCategory(
                SettingsCategory::KeyboardShortcuts
            )),
        button("Actions & Macros")
            .width(Length::Fill)
            .style(if macros_selected {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::SelectSettingsCategory(
                SettingsCategory::ActionMacros
            )),
        button("CLAP Plugins")
            .width(Length::Fill)
            .style(if plugins_selected {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::SelectSettingsCategory(
                SettingsCategory::ClapPlugins
            )),
        button("Audio")
            .width(Length::Fill)
            .style(if audio_selected {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::SelectSettingsCategory(SettingsCategory::Audio)),
    ]
    .spacing(4);

    let details = match app.settings_category {
        SettingsCategory::KeyboardShortcuts => keyboard_shortcuts(app),
        SettingsCategory::ActionMacros => action_macros(app),
        SettingsCategory::ClapPlugins => clap_plugins(app),
        SettingsCategory::Audio => audio_settings(app),
    };

    let content = row![
        container(navigation)
            .width(Length::Fixed(178.0))
            .height(Length::Fill)
            .padding(iced::Padding::default().right(10.0)),
        rule::vertical(1),
        details,
    ]
    .spacing(12)
    .height(Length::Fill);

    container(container(content).padding([12, 14]))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn action_macros(app: &App) -> Element<'_, Message> {
    let choices = commands::macro_step_choices();
    let selected = app
        .action_macro_step
        .as_ref()
        .and_then(|id| choices.iter().find(|choice| &choice.id == id).cloned());
    let mut step_rows = column![].spacing(4);
    for (index, id) in app.action_macro_steps.iter().enumerate() {
        step_rows = step_rows.push(
            row![
                text(format!("{}. {}", index + 1, commands::macro_step_label(id)))
                    .width(Length::Fill),
                button("↑")
                    .style(button::text)
                    .on_press_maybe((index > 0).then_some(Message::MoveActionMacroStep(index, -1))),
                button("↓").style(button::text).on_press_maybe(
                    (index + 1 < app.action_macro_steps.len())
                        .then_some(Message::MoveActionMacroStep(index, 1),)
                ),
                button("Remove")
                    .style(button::text)
                    .on_press(Message::RemoveActionMacroStep(index)),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }
    if app.action_macro_steps.is_empty() {
        step_rows = step_rows.push(text("Add at least one supported action.").size(11));
    }

    let mut saved_macros = column![].spacing(4);
    for action_macro in &app.action_macros {
        let id = action_macro.id;
        saved_macros = saved_macros.push(
            row![
                column![
                    text(action_macro.name.clone()).size(13),
                    text(format!("{} actions", action_macro.steps.len())).size(10),
                ]
                .width(Length::Fill)
                .spacing(2),
                button("Edit")
                    .style(button::secondary)
                    .on_press(Message::EditActionMacro(id)),
                button("Delete")
                    .style(button::text)
                    .on_press(Message::DeleteActionMacro(id)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    if app.action_macros.is_empty() {
        saved_macros = saved_macros.push(text("No macros saved yet.").size(11));
    }

    let is_editing = app.action_macro_editing_id.is_some();
    column![
        text("Actions & Macros").size(17),
        text("Build an ordered macro from supported commands. Macros run the same actions shown in the Actions menu and can be assigned shortcuts below.").size(11),
        rule::horizontal(1),
        row![
            text_input("Macro name", &app.action_macro_name)
                .on_input(Message::ActionMacroNameChanged)
                .width(Length::Fill),
            iced::widget::pick_list(choices, selected, |choice| {
                Message::ActionMacroStepSelected(choice.id)
            })
            .placeholder("Choose action")
            .width(Length::Fixed(220.0)),
            button("Add step")
                .on_press_maybe(app.action_macro_step.is_some().then_some(Message::AddActionMacroStep)),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
        scrollable(step_rows).height(Length::Fixed(112.0)),
        row![
            button(if is_editing { "Save changes" } else { "Create macro" })
                .on_press_maybe((app.action_macro_config_error.is_none() && !app.action_macro_name.trim().is_empty() && !app.action_macro_steps.is_empty()).then_some(Message::SaveActionMacro)),
            button("New")
                .style(button::secondary)
                .on_press(Message::NewActionMacro),
        ]
        .spacing(6),
        text(app.action_macro_feedback.clone()).size(11),
        rule::horizontal(1),
        text("Saved macros").size(13),
        scrollable(saved_macros).height(Length::Fill),
    ]
    .spacing(tokens::SECTION_GAP)
    .width(Length::Fill)
    .into()
}

fn audio_settings(app: &App) -> Element<'_, Message> {
    let recording_offset = app.audio_recording_offset_query.clone().unwrap_or_else(|| {
        super::super::audio_config::format_recording_offset_ms(
            app.audio_settings.recording_offset_us,
        )
    });
    column![
        text("Audio output and recording").size(17),
        text("Set the final digital sample-peak ceiling. Changes apply during playback and are saved to this user account.").size(11),
        row![
            column![
            text("Master sample-peak ceiling").size(13),
            text("Always active · default -1 dBFS").size(10),
            ]
            .width(Length::Fill)
            .spacing(tokens::SPACING_XS),
            iced::widget::pick_list(
                master_ceiling_choices(),
                Some(app.audio_settings.master_output_ceiling),
                Message::SetMasterOutputCeilingDbfs,
            )
            .placeholder("Ceiling")
            .width(Length::Fixed(128.0)),
        ]
    .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        row![
            column![
                text("Recording placement offset (ms)").size(13),
                text("Positive moves the take later; negative moves it earlier. JACK's precise reported capture latency is applied automatically when available; this value calibrates the remaining offset.").size(10),
            ]
            .width(Length::Fill)
            .spacing(tokens::SPACING_XS),
            text_input("0.000", &recording_offset)
                .on_input(Message::RecordingOffsetTextChanged)
                .width(Length::Fixed(108.0)),
            button("Apply")
                .on_press(Message::ApplyRecordingOffset),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        text("This bounds sample values at the Master output and silences non-finite samples. It is not a true-peak or loudness limiter and does not guarantee safe speaker level or hearing exposure.")
            .size(11),
        text(app.audio_settings_feedback.clone()).size(11),
    ]
    .spacing(tokens::SPACING_SM)
    .width(Length::Fill)
    .into()
}

fn master_ceiling_choices() -> [MasterOutputCeiling; 13] {
    std::array::from_fn(|index| {
        MasterOutputCeiling::new(index as i8 - 12)
            .expect("Master output ceiling menu contains only supported values")
    })
}

fn keyboard_shortcuts(app: &App) -> Element<'_, Message> {
    let entries = commands::shortcut_entries(app);

    let mut bindings = column![].spacing(5);
    let mut action_category = None;
    for entry in entries {
        if action_category.as_deref() != Some(entry.category.as_str()) {
            action_category = Some(entry.category.clone());
            bindings = bindings
                .push(text(entry.category.clone()).size(12))
                .push(rule::horizontal(1));
        }
        let action_id = entry.id.to_owned();
        let recording = app.shortcut_capture_id.as_deref() == Some(entry.id.as_str());
        let shown_binding = if recording {
            "Press a key…".to_owned()
        } else if entry.binding.is_empty() {
            "Unassigned".to_owned()
        } else {
            entry.binding
        };
        let default_binding = if entry.default_binding.is_empty() {
            "Default: Unassigned".to_owned()
        } else {
            format!("Default: {}", entry.default_binding)
        };
        bindings = bindings.push(
            row![
                column![text(entry.label).size(13), text(default_binding).size(10)]
                    .width(Length::Fill)
                    .spacing(2),
                button(text(shown_binding))
                    .style(if recording {
                        button::warning
                    } else {
                        button::secondary
                    })
                    .width(Length::Fixed(150.0))
                    .on_press(Message::StartShortcutCapture(action_id.clone())),
                button("Clear")
                    .style(button::text)
                    .width(Length::Fixed(48.0))
                    .on_press(Message::ClearShortcutBinding(action_id.clone())),
                button("Default")
                    .style(button::text)
                    .width(Length::Fixed(68.0))
                    .on_press(Message::RestoreShortcutDefault(action_id)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }

    column![
        text("Keyboard shortcuts").size(17),
        text("Select a binding, then press Ctrl/Cmd with a letter, optionally Shift, or Space.")
            .size(12),
        rule::horizontal(1),
        scrollable(container(bindings).padding(iced::Padding::default().right(12.0)))
            .height(Length::Fill),
        text(app.shortcut_editor_feedback.clone()).size(12),
        row![
            button("Restore all defaults")
                .style(button::secondary)
                .on_press(Message::ResetShortcutBindings),
            iced::widget::Space::new().width(Length::Fill),
            button("Save changes").on_press(Message::SaveShortcutBindings),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
    ]
    .spacing(tokens::SPACING_SM)
    .width(Length::Fill)
    .into()
}

fn clap_plugins(app: &App) -> Element<'_, Message> {
    let mut paths = column![].spacing(3);
    for path in &app.clap_plugin_paths {
        let path_message = path.clone();
        let is_default = app.clap_plugin_default_paths.contains(path);
        let availability = if path.is_dir() { "" } else { " (missing)" };
        paths = paths.push(
            row![
                text(format!("{}{availability}", path.display()))
                    .size(11)
                    .width(Length::Fill),
                button(if is_default { "Default" } else { "Remove" })
                    .style(button::text)
                    .on_press_maybe(
                        (!app.clap_plugin_scan_busy && !is_default)
                            .then_some(Message::RemoveClapPluginPath(path_message)),
                    ),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    if app.clap_plugin_paths.is_empty() {
        paths = paths.push(text("No search paths configured").size(11));
    }

    let mut catalog = column![].spacing(5);
    if app.clap_plugin_scan.entries_checked == 0 && app.clap_plugin_scan.plugins.is_empty() {
        catalog = catalog.push(text("No scan results yet").size(11));
    }
    for plugin in &app.clap_plugin_scan.plugins {
        let kind = match (plugin.is_instrument(), plugin.is_audio_effect()) {
            (true, true) => "Instrument · Effect",
            (true, false) => "Instrument",
            (false, true) => "Effect",
            (false, false) => "Other",
        };
        let vendor = plugin.vendor.as_deref().unwrap_or("Unknown vendor");
        catalog = catalog.push(
            column![
                row![
                    text(plugin.name.clone()).size(13),
                    iced::widget::Space::new().width(Length::Fill),
                    text(kind).size(10),
                ]
                .align_y(Alignment::Center),
                text(format!("{vendor} · {}", plugin.plugin_id)).size(10),
                text(plugin.entry_path.display().to_string()).size(10),
                rule::horizontal(1),
            ]
            .spacing(2),
        );
    }
    for error in &app.clap_plugin_scan.errors {
        catalog = catalog.push(
            column![
                text(format!("{}", error.path.display())).size(10),
                text(error.message.clone()).size(11),
                rule::horizontal(1),
            ]
            .spacing(2),
        );
    }

    let catalog_status = if app.clap_plugin_scan_busy && app.clap_plugin_scan_is_cached {
        "Cached results are shown while configured paths are refreshed"
    } else if app.clap_plugin_scan_is_cached {
        "Showing the last cached scan results"
    } else if app.clap_plugin_scan_busy {
        "A background scan is running; previous results remain available"
    } else if app.clap_plugin_scan.entries_checked == 0
        && app.clap_plugin_scan.plugins.is_empty()
        && app.clap_plugin_scan.errors.is_empty()
    {
        "No scan results yet"
    } else {
        "Showing the latest scan results"
    };

    column![
        text("CLAP plug-ins").size(17),
        text("Search configured folders recursively.").size(11),
        text(super::CLAP_IN_PROCESS_RISK).size(11),
        row![
            button("Add search path…").on_press_maybe(
                (!app.clap_plugin_scan_busy)
                    .then_some(Message::PickPath(PathPickerTarget::AddClapPluginPath,))
            ),
            button(if app.clap_plugin_scan_busy {
                "Scanning…"
            } else {
                "Rescan"
            })
            .style(button::secondary)
            .on_press_maybe((!app.clap_plugin_scan_busy).then_some(Message::RescanClapPlugins)),
        ]
        .spacing(6),
        text("Search paths").size(12),
        scrollable(container(paths).padding(iced::Padding::default().right(10.0)))
            .height(Length::Fixed(128.0)),
        rule::horizontal(1),
        text(format!(
            "Discovered plug-ins ({})",
            app.clap_plugin_scan.plugins.len()
        ))
        .size(12),
        text(catalog_status).size(10),
        scrollable(container(catalog).padding(iced::Padding::default().right(10.0)))
            .height(Length::Fill),
        text(app.clap_plugin_settings_feedback.clone()).size(11),
    ]
    .spacing(7)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
