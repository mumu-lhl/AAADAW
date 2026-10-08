use super::super::{App, Message, PathPickerTarget, SettingsCategory, commands};
use super::tokens;
use aaadaw_engine::MasterOutputCeiling;
use iced::widget::{
    button, column, container, responsive, row, rule, scrollable, text, text_input,
};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    responsive(move |size| {
        let compact = size.width < 720.0;
        let details = settings_details(app, compact);
        if compact {
            let detail_content: Element<'_, Message> =
                if app.settings_category == SettingsCategory::Audio {
                    details
                } else {
                    scrollable(details).height(Length::Fill).into()
                };
            let categories = [
                (SettingsCategory::KeyboardShortcuts, "Keyboard Shortcuts"),
                (SettingsCategory::ActionMacros, "Actions & Macros"),
                (SettingsCategory::ClapPlugins, "CLAP Plugins"),
                (SettingsCategory::Audio, "Audio"),
            ]
            .into_iter()
            .map(|(category, label)| -> Element<'_, Message> {
                button(text(label).size(12))
                    .height(Length::Fixed(48.0))
                    .padding([tokens::SPACING_SM, tokens::SPACING_MD])
                    .style(if app.settings_category == category {
                        button::primary
                    } else {
                        button::secondary
                    })
                    .on_press(Message::SelectSettingsCategory(category))
                    .into()
            });
            let navigation = scrollable(row(categories).spacing(tokens::SPACING_XS))
                .direction(iced::widget::scrollable::Direction::Horizontal(
                    iced::widget::scrollable::Scrollbar::default(),
                ))
                .height(Length::Fixed(52.0));
            column![
                navigation,
                rule::horizontal(1),
                container(detail_content)
                    .width(Length::Fill)
                    .height(Length::Fill),
            ]
            .spacing(tokens::SPACING_SM)
            .padding(tokens::PANEL_PADDING)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            desktop_settings(app, details)
        }
    })
    .into()
}

fn settings_details(app: &App, compact: bool) -> Element<'_, Message> {
    match app.settings_category {
        SettingsCategory::KeyboardShortcuts => keyboard_shortcuts(app, compact),
        SettingsCategory::ActionMacros => action_macros(app, compact),
        SettingsCategory::ClapPlugins => clap_plugins(app, compact),
        SettingsCategory::Audio => {
            let settings = audio_settings(app, compact);
            if compact {
                scrollable(settings).height(Length::Fill).into()
            } else {
                settings
            }
        }
    }
}

fn desktop_settings<'a>(app: &App, details: Element<'a, Message>) -> Element<'a, Message> {
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

fn action_macros(app: &App, compact: bool) -> Element<'_, Message> {
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
                    .height(if compact {
                        Length::Fixed(48.0)
                    } else {
                        Length::Shrink
                    })
                    .on_press_maybe((index > 0).then_some(Message::MoveActionMacroStep(index, -1))),
                button("↓")
                    .height(if compact {
                        Length::Fixed(48.0)
                    } else {
                        Length::Shrink
                    })
                    .style(button::text)
                    .on_press_maybe(
                        (index + 1 < app.action_macro_steps.len())
                            .then_some(Message::MoveActionMacroStep(index, 1),)
                    ),
                button("Remove")
                    .style(button::text)
                    .height(if compact {
                        Length::Fixed(48.0)
                    } else {
                        Length::Shrink
                    })
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
                    .height(if compact {
                        Length::Fixed(48.0)
                    } else {
                        Length::Shrink
                    })
                    .style(button::secondary)
                    .on_press(Message::EditActionMacro(id)),
                button("Delete")
                    .height(if compact {
                        Length::Fixed(48.0)
                    } else {
                        Length::Shrink
                    })
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
    let step_picker = iced::widget::pick_list(choices, selected, |choice| {
        Message::ActionMacroStepSelected(choice.id)
    })
    .placeholder("Choose action")
    .width(if compact {
        Length::Fill
    } else {
        Length::Fixed(220.0)
    });
    let add_step = button("Add step")
        .height(if compact {
            Length::Fixed(48.0)
        } else {
            Length::Shrink
        })
        .on_press_maybe(
            app.action_macro_step
                .is_some()
                .then_some(Message::AddActionMacroStep),
        );
    let step_form: Element<'_, Message> = if compact {
        column![
            text_input("Macro name", &app.action_macro_name)
                .on_input(Message::ActionMacroNameChanged)
                .padding([tokens::SPACING_LG, tokens::SPACING_SM])
                .width(Length::Fill),
            step_picker,
            add_step,
        ]
        .spacing(tokens::SPACING_SM)
        .into()
    } else {
        row![
            text_input("Macro name", &app.action_macro_name)
                .on_input(Message::ActionMacroNameChanged)
                .width(Length::Fill),
            step_picker,
            add_step,
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    };
    column![
        text("Actions & Macros").size(17),
        text("Build an ordered macro from supported commands. Macros run the same actions shown in the Actions menu and can be assigned shortcuts below.").size(11),
        rule::horizontal(1),
        step_form,
        scrollable(step_rows).height(Length::Fixed(if compact { 88.0 } else { 112.0 })),
        row![
            button(if is_editing { "Save changes" } else { "Create macro" })
                .height(if compact { Length::Fixed(48.0) } else { Length::Shrink })
                .on_press_maybe((app.action_macro_config_error.is_none() && !app.action_macro_name.trim().is_empty() && !app.action_macro_steps.is_empty()).then_some(Message::SaveActionMacro)),
            button("New")
                .height(if compact { Length::Fixed(48.0) } else { Length::Shrink })
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

fn audio_settings(app: &App, compact: bool) -> Element<'_, Message> {
    let recording_offset = app.audio_recording_offset_query.clone().unwrap_or_else(|| {
        super::super::audio_config::format_recording_offset_ms(
            app.audio_settings.recording_offset_us,
        )
    });
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    let cpal_output = cpal_output_settings(app, compact);
    #[cfg(not(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    )))]
    let cpal_output: Element<'_, Message> = text("").into();
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ))]
    let cpal_input = cpal_input_settings(app, compact);
    #[cfg(not(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    )))]
    let cpal_input: Element<'_, Message> = text("").into();
    #[cfg(all(feature = "audio-device", target_os = "android"))]
    let android_midi: Element<'_, Message> = {
        let description = column![
            text("External MIDI").size(13),
            text(format!(
                "Inputs: {} · Outputs: {}",
                app.android_midi_input_ports, app.android_midi_output_ports
            ))
            .size(11),
            text("USB and paired Bluetooth MIDI 1.0 devices. Input is monitored through the selected instrument track while playback is open; MIDI recording is not available yet.")
                .size(10),
        ]
        .width(Length::Fill)
        .spacing(tokens::SPACING_XS);
        let refresh = button("Refresh MIDI devices")
            .height(Length::Fixed(48.0))
            .on_press(Message::RefreshAndroidMidiDevices);
        let layout: Element<'_, Message> = if compact {
            column![description, refresh.width(Length::Fill)]
                .spacing(tokens::SPACING_SM)
                .into()
        } else {
            row![description, refresh]
                .spacing(tokens::SPACING_SM)
                .align_y(Alignment::Center)
                .into()
        };
        column![layout, text(app.android_midi_feedback.clone()).size(10)]
            .spacing(tokens::SPACING_XS)
            .width(Length::Fill)
            .into()
    };
    #[cfg(not(all(feature = "audio-device", target_os = "android")))]
    let android_midi: Element<'_, Message> = text("").into();
    let master_description = column![
        text("Master sample-peak ceiling").size(13),
        text("Always active · default -1 dBFS").size(10),
    ]
    .width(Length::Fill)
    .spacing(tokens::SPACING_XS);
    let ceiling_picker = iced::widget::pick_list(
        master_ceiling_choices(),
        Some(app.audio_settings.master_output_ceiling),
        Message::SetMasterOutputCeilingDbfs,
    )
    .placeholder("Ceiling")
    .width(if compact {
        Length::Fill
    } else {
        Length::Fixed(128.0)
    });
    let ceiling_control: Element<'_, Message> = if compact {
        column![master_description, ceiling_picker]
            .spacing(tokens::SPACING_SM)
            .into()
    } else {
        row![master_description, ceiling_picker]
            .spacing(tokens::SPACING_SM)
            .align_y(Alignment::Center)
            .into()
    };
    let recording_description = column![
        text("Recording placement offset (ms)").size(13),
        text("Positive moves the take later; negative moves it earlier. JACK's precise reported capture latency is applied automatically when available; this value calibrates the remaining offset.").size(10),
    ]
    .width(Length::Fill)
    .spacing(tokens::SPACING_XS);
    let offset_input = text_input("0.000", &recording_offset)
        .on_input(Message::RecordingOffsetTextChanged)
        .padding(if compact {
            [tokens::SPACING_LG, tokens::SPACING_SM]
        } else {
            [6.0, 8.0]
        })
        .width(if compact {
            Length::Fill
        } else {
            Length::Fixed(108.0)
        });
    let apply_offset = button("Apply")
        .height(if compact {
            Length::Fixed(48.0)
        } else {
            Length::Shrink
        })
        .on_press(Message::ApplyRecordingOffset);
    let recording_control: Element<'_, Message> = if compact {
        column![recording_description, offset_input, apply_offset]
            .spacing(tokens::SPACING_SM)
            .into()
    } else {
        row![recording_description, offset_input, apply_offset]
            .spacing(tokens::SPACING_SM)
            .align_y(Alignment::Center)
            .into()
    };
    column![
        text("Audio output and recording").size(17),
        cpal_output,
        cpal_input,
        android_midi,
        text("Set the final digital sample-peak ceiling. Changes apply during playback and are saved to this user account.").size(11),
        ceiling_control,
        recording_control,
        text("This bounds sample values at the Master output and silences non-finite samples. It is not a true-peak or loudness limiter and does not guarantee safe speaker level or hearing exposure.")
            .size(11),
        text(app.audio_settings_feedback.clone()).size(11),
    ]
    .spacing(tokens::SPACING_SM)
    .width(Length::Fill)
    .into()
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos", target_os = "android")
))]
fn cpal_output_settings(app: &App, compact: bool) -> Element<'_, Message> {
    let saved_id = app.audio_settings.cpal_output_device_id.as_deref();
    let enumeration_feedback = if app.cpal_output_devices_loading {
        "Listing audio output devices…".to_owned()
    } else if let Some(error) = &app.cpal_output_devices_error {
        format!("Output device list unavailable: {error}")
    } else if app.cpal_output_devices.is_empty() {
        "No audio output devices were found. Connect a device and refresh the list.".to_owned()
    } else if saved_id
        .is_some_and(|id| !app.cpal_output_devices.iter().any(|device| device.id == id))
    {
        "The saved endpoint is unavailable. Playback will not switch outputs automatically; choose System default or another endpoint.".to_owned()
    } else {
        "Close and reopen playback to apply an endpoint change.".to_owned()
    };
    let choices = cpal_device_choices(
        app.cpal_output_devices
            .iter()
            .map(|device| (device.id.as_str(), device.name.as_str())),
        saved_id,
        "output",
    );
    let selected = choices
        .iter()
        .find(|choice| choice.id.as_deref() == saved_id)
        .cloned();
    cpal_device_settings(
        "System audio output device",
        "Select an audio output device. System default is an explicit choice.",
        "Output device",
        choices,
        selected,
        app.cpal_output_devices_loading,
        enumeration_feedback,
        Message::RefreshCpalOutputDevices,
        Message::SelectCpalOutputDevice,
        compact,
    )
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos", target_os = "android")
))]
fn cpal_input_settings(app: &App, compact: bool) -> Element<'_, Message> {
    let saved_id = app.audio_settings.cpal_input_device_id.as_deref();
    let enumeration_feedback = if app.cpal_input_devices_loading {
        "Listing audio input devices…".to_owned()
    } else if let Some(error) = &app.cpal_input_devices_error {
        format!("Input device list unavailable: {error}")
    } else if app.cpal_input_devices.is_empty() {
        "No audio input devices were found. Connect a device and refresh the list.".to_owned()
    } else if saved_id
        .is_some_and(|id| !app.cpal_input_devices.iter().any(|device| device.id == id))
    {
        "The saved endpoint is unavailable. Recording will not switch inputs automatically; choose System default or another endpoint.".to_owned()
    } else {
        "The selection applies to the next input connection; active recording keeps its current input.".to_owned()
    };
    let choices = cpal_device_choices(
        app.cpal_input_devices
            .iter()
            .map(|device| (device.id.as_str(), device.name.as_str())),
        saved_id,
        "input",
    );
    let selected = choices
        .iter()
        .find(|choice| choice.id.as_deref() == saved_id)
        .cloned();
    cpal_device_settings(
        "System audio input device",
        "Select an audio input device. System default is an explicit choice.",
        "Input device",
        choices,
        selected,
        app.cpal_input_devices_loading,
        enumeration_feedback,
        Message::RefreshCpalInputDevices,
        Message::SelectCpalInputDevice,
        compact,
    )
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos", target_os = "android")
))]
fn cpal_device_choices<'a>(
    devices: impl Iterator<Item = (&'a str, &'a str)>,
    saved_id: Option<&str>,
    kind: &str,
) -> Vec<super::super::CpalDeviceChoice> {
    use super::super::CpalDeviceChoice;

    let devices = devices.collect::<Vec<_>>();
    let mut choices = vec![CpalDeviceChoice {
        id: None,
        label: "System default".to_owned(),
    }];
    for (id, name) in &devices {
        let base_label = if name.trim().is_empty() {
            format!("Unnamed {kind}")
        } else {
            (*name).to_owned()
        };
        let duplicate_name = devices.iter().filter(|(_, other)| other == name).count() > 1;
        let label = if duplicate_name {
            let suffix = id.chars().rev().take(8).collect::<String>();
            format!(
                "{base_label} [{}]",
                suffix.chars().rev().collect::<String>()
            )
        } else {
            base_label
        };
        choices.push(CpalDeviceChoice {
            id: Some((*id).to_owned()),
            label,
        });
    }
    if let Some(saved_id) = saved_id
        && !devices.iter().any(|(id, _)| *id == saved_id)
    {
        let suffix = saved_id.chars().rev().take(12).collect::<String>();
        choices.push(CpalDeviceChoice {
            id: Some(saved_id.to_owned()),
            label: format!(
                "Unavailable saved {kind} [{}]",
                suffix.chars().rev().collect::<String>()
            ),
        });
    }
    choices
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos", target_os = "android")
))]
#[allow(clippy::too_many_arguments)]
fn cpal_device_settings(
    title: &'static str,
    description: &'static str,
    placeholder: &'static str,
    choices: Vec<super::super::CpalDeviceChoice>,
    selected: Option<super::super::CpalDeviceChoice>,
    loading: bool,
    enumeration_feedback: String,
    refresh_message: Message,
    select_message: fn(Option<String>) -> Message,
    compact: bool,
) -> Element<'static, Message> {
    use super::super::CpalDeviceChoice;

    let description = column![text(title).size(13), text(description).size(10),]
        .width(Length::Fill)
        .spacing(tokens::SPACING_XS);
    let picker = iced::widget::pick_list(choices, selected, move |choice: CpalDeviceChoice| {
        select_message(choice.id)
    })
    .placeholder(placeholder)
    .width(if compact {
        Length::Fill
    } else {
        Length::Fixed(260.0)
    });
    let refresh = button(if loading { "Loading…" } else { "Refresh" })
        .height(if compact {
            Length::Fixed(48.0)
        } else {
            Length::Shrink
        })
        .on_press_maybe((!loading).then_some(refresh_message));
    let controls: Element<'static, Message> = if compact {
        column![description, picker, refresh]
            .spacing(tokens::SPACING_SM)
            .into()
    } else {
        row![description, picker, refresh]
            .spacing(tokens::SPACING_SM)
            .align_y(Alignment::Center)
            .into()
    };
    column![controls, text(enumeration_feedback).size(10),]
        .spacing(tokens::SPACING_XS)
        .width(Length::Fill)
        .into()
}

fn master_ceiling_choices() -> [MasterOutputCeiling; 13] {
    std::array::from_fn(|index| {
        MasterOutputCeiling::new(index as i8 - 12)
            .expect("Master output ceiling menu contains only supported values")
    })
}

fn keyboard_shortcuts(app: &App, compact: bool) -> Element<'_, Message> {
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
        let label = column![text(entry.label).size(13), text(default_binding).size(10)]
            .width(Length::Fill)
            .spacing(2);
        let binding = button(text(shown_binding))
            .height(if compact {
                Length::Fixed(48.0)
            } else {
                Length::Shrink
            })
            .style(if recording {
                button::warning
            } else {
                button::secondary
            })
            .width(if compact {
                Length::Fill
            } else {
                Length::Fixed(150.0)
            })
            .on_press(Message::StartShortcutCapture(action_id.clone()));
        let clear = button("Clear")
            .height(if compact {
                Length::Fixed(48.0)
            } else {
                Length::Shrink
            })
            .style(button::text)
            .width(Length::Fixed(if compact { 64.0 } else { 48.0 }))
            .on_press(Message::ClearShortcutBinding(action_id.clone()));
        let restore = button("Default")
            .height(if compact {
                Length::Fixed(48.0)
            } else {
                Length::Shrink
            })
            .style(button::text)
            .width(Length::Fixed(if compact { 72.0 } else { 68.0 }))
            .on_press(Message::RestoreShortcutDefault(action_id));
        let row: Element<'_, Message> = if compact {
            column![label, row![binding, clear, restore].spacing(tokens::ROW_GAP)]
                .spacing(tokens::ROW_GAP)
                .into()
        } else {
            row![label, binding, clear, restore]
                .spacing(6)
                .align_y(Alignment::Center)
                .into()
        };
        bindings = bindings.push(row);
    }

    let save_controls: Element<'_, Message> = if compact {
        column![
            button("Restore all defaults")
                .height(Length::Fixed(48.0))
                .style(button::secondary)
                .on_press(Message::ResetShortcutBindings),
            button("Save changes")
                .height(Length::Fixed(48.0))
                .on_press(Message::SaveShortcutBindings),
        ]
        .spacing(tokens::SPACING_XS)
        .into()
    } else {
        row![
            button("Restore all defaults")
                .style(button::secondary)
                .on_press(Message::ResetShortcutBindings),
            iced::widget::Space::new().width(Length::Fill),
            button("Save changes").on_press(Message::SaveShortcutBindings),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center)
        .into()
    };
    column![
        text("Keyboard shortcuts").size(17),
        text(commands::shortcut_capture_help()).size(12),
        rule::horizontal(1),
        scrollable(container(bindings).padding(iced::Padding::default().right(12.0)))
            .height(Length::Fill),
        text(app.shortcut_editor_feedback.clone()).size(12),
        save_controls,
    ]
    .spacing(tokens::SPACING_SM)
    .width(Length::Fill)
    .into()
}

fn clap_plugins(app: &App, compact: bool) -> Element<'_, Message> {
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
                    .height(if compact {
                        Length::Fixed(48.0)
                    } else {
                        Length::Shrink
                    })
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
        text(if cfg!(target_os = "android") {
            "Import Android ARM64 .clap libraries into app storage. Only install trusted plugins."
        } else {
            "Search configured folders recursively."
        })
        .size(11),
        text(super::CLAP_PLUGIN_RISK).size(11),
        row![
            button(if cfg!(target_os = "android") {
                "Import plugin…"
            } else {
                "Add search path…"
            })
            .height(if compact {
                Length::Fixed(48.0)
            } else {
                Length::Shrink
            })
            .on_press_maybe(
                (!app.clap_plugin_scan_busy)
                    .then_some(Message::PickPath(PathPickerTarget::AddClapPluginPath,))
            ),
            button(if app.clap_plugin_scan_busy {
                "Scanning…"
            } else {
                "Rescan"
            })
            .height(if compact {
                Length::Fixed(48.0)
            } else {
                Length::Shrink
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
