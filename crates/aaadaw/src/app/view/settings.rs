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
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    let cpal_output = cpal_output_settings(app);
    #[cfg(not(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    )))]
    let cpal_output: Element<'_, Message> = text("").into();
    #[cfg(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ))]
    let cpal_input = cpal_input_settings(app);
    #[cfg(not(all(
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    )))]
    let cpal_input: Element<'_, Message> = text("").into();
    #[cfg(all(target_os = "linux", feature = "audio-device"))]
    let linux_diagnostics = linux_audio_diagnostics(app);
    #[cfg(not(all(target_os = "linux", feature = "audio-device")))]
    let linux_diagnostics: Element<'_, Message> = text("").into();
    column![
        text("Audio output and recording").size(17),
        linux_diagnostics,
        cpal_output,
        cpal_input,
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

#[cfg(all(target_os = "linux", feature = "audio-device"))]
fn linux_audio_diagnostics(app: &App) -> Element<'_, Message> {
    let backend = app.selected_playback_backend();
    let backend_available = backend.is_available();
    let output_open = app.playback.is_some();
    let last_error = app
        .last_audio_backend_failure
        .as_ref()
        .filter(|(failed_backend, _)| *failed_backend == backend)
        .map(|(_, message)| message.as_str());
    let (state, guidance) = linux_audio_connection_state(
        backend.name(),
        backend_available,
        app.playback_busy,
        output_open,
        last_error,
    );
    let sample_rate = app
        .playback
        .as_ref()
        .and_then(aaadaw_app::RunningAudioPlayback::device_sample_rate)
        .map(|rate| format!("{rate} Hz"))
        .unwrap_or_else(|| "Unavailable until output opens".to_owned());

    let routes = if !output_open {
        text("Output routes are unavailable while the backend is closed. Open playback to inspect its current connections.").size(11)
    } else if app.linux_audio_routes_busy {
        text("Inspecting the backend graph…").size(11)
    } else if let Some((reported_backend, result)) = &app.linux_audio_route_report {
        if *reported_backend != backend {
            text("Backend changed; refresh the route summary.").size(11)
        } else {
            match result {
                Ok(snapshot) if snapshot.output_routes.is_empty() => {
                    text("AAADAW has no connected output route. Check the backend patchbay or default sink.").size(11)
                }
                Ok(snapshot) => text(format!(
                    "Connected output routes: {}",
                    snapshot.output_routes.join(" · ")
                ))
                .size(11),
                Err(error) => text(format!("Route inspection failed: {error}")).size(11),
            }
        }
    } else {
        text("Refresh routes to inspect AAADAW's current output connection.").size(11)
    };
    let input_routes = if !output_open {
        text("Capture input routes are unavailable while the backend is closed.").size(11)
    } else if app.linux_audio_routes_busy {
        text("").size(11)
    } else if let Some((reported_backend, result)) = &app.linux_audio_route_report {
        if *reported_backend != backend {
            text("Backend changed; refresh the route summary.").size(11)
        } else {
            match result {
                Ok(snapshot) if snapshot.input_routes.is_empty() => {
                    text("Capture input routes are shown while recording.").size(11)
                }
                Ok(snapshot) => text(format!(
                    "Connected capture inputs: {}",
                    snapshot.input_routes.join(" · ")
                ))
                .size(11),
                Err(_) => text("").size(11),
            }
        }
    } else {
        text("Capture input routes are shown while recording.").size(11)
    };

    let refresh_enabled = output_open && !app.linux_audio_routes_busy;
    let reconnect_enabled = backend_available
        && !app.playback_playing
        && !app.playback_paused
        && !app.playback_busy
        && !app.io_busy
        && app.recording.is_none()
        && !app.recording_starting
        && !app.recording_stopping;

    column![
        rule::horizontal(1),
        text("Linux audio backend").size(15),
        row![
            text(format!("Selected: {}", backend.name())).size(12),
            text(format!("Output: {state}")).size(12),
            text(format!("Rate: {sample_rate}")).size(12),
        ]
        .spacing(tokens::SPACING_MD)
        .align_y(Alignment::Center),
        text(guidance).size(11),
        last_error
            .map(|error| text(format!("Last backend failure: {error}")).size(11))
            .unwrap_or_else(|| text("").size(11)),
        routes,
        input_routes,
        row![
            button("Refresh routes")
                .on_press_maybe(refresh_enabled.then_some(Message::RefreshLinuxAudioRoutes)),
            button("Reconnect backend")
                .style(button::secondary)
                .on_press_maybe(reconnect_enabled.then_some(Message::ReconnectLinuxAudioBackend)),
        ]
        .spacing(tokens::SPACING_SM),
        text("An open stream or listed route does not guarantee audible output.").size(10),
    ]
    .spacing(tokens::SPACING_SM)
    .width(Length::Fill)
    .into()
}

#[cfg(all(target_os = "linux", feature = "audio-device"))]
pub(super) fn linux_audio_connection_state(
    backend_name: &str,
    backend_available: bool,
    preparing: bool,
    output_open: bool,
    last_error: Option<&str>,
) -> (&'static str, String) {
    if !backend_available {
        return (
            "Unavailable",
            "This build has no enabled Linux audio backend. Rebuild with JACK or PipeWire support."
                .to_owned(),
        );
    }
    if preparing {
        return (
            "Preparing",
            "Wait for project and audio output preparation to finish.".to_owned(),
        );
    }
    if output_open {
        return (
            "Open",
            "The stream is open. Check the listed route and backend mixer if you cannot hear audio."
                .to_owned(),
        );
    }
    if let Some(error) = last_error {
        let recovery = match backend_name {
            "JACK" => "Check that the JACK server is running and has an active playback device.",
            "PipeWire" => {
                "Check that PipeWire and its session manager are running and that an output device is available."
            }
            _ => "Check that the selected audio service is running and has an output device.",
        };
        return (
            "Failed",
            format!(
                "{error}. {recovery} Select another available backend if needed, then reconnect."
            ),
        );
    }
    (
        "Closed",
        "Press Play or Reconnect backend to open the selected audio output.".to_owned(),
    )
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos")
))]
fn cpal_output_settings(app: &App) -> Element<'_, Message> {
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
    )
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos")
))]
fn cpal_input_settings(app: &App) -> Element<'_, Message> {
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
    )
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos")
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
    any(target_os = "windows", target_os = "macos")
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
) -> Element<'static, Message> {
    use super::super::CpalDeviceChoice;

    column![
        row![
            column![text(title).size(13), text(description).size(10),]
                .width(Length::Fill)
                .spacing(tokens::SPACING_XS),
            iced::widget::pick_list(choices, selected, move |choice: CpalDeviceChoice| {
                select_message(choice.id)
            },)
            .placeholder(placeholder)
            .width(Length::Fixed(260.0)),
            button(if loading { "Loading…" } else { "Refresh" })
                .on_press_maybe((!loading).then_some(refresh_message)),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        text(enumeration_feedback).size(10),
    ]
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
        text(commands::shortcut_capture_help()).size(12),
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
        text(super::CLAP_PLUGIN_RISK).size(11),
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

#[cfg(all(test, target_os = "linux", feature = "audio-device"))]
mod linux_audio_diagnostics_tests {
    use super::linux_audio_connection_state;

    #[test]
    fn unavailable_backend_has_build_guidance() {
        let (state, guidance) =
            linux_audio_connection_state("Unavailable", false, false, false, None);

        assert_eq!(state, "Unavailable");
        assert!(guidance.contains("JACK or PipeWire"));
    }

    #[test]
    fn connection_progress_is_distinct_from_transport_state() {
        let (state, guidance) = linux_audio_connection_state("JACK", true, true, false, None);

        assert_eq!(state, "Preparing");
        assert!(guidance.contains("preparation"));
    }

    #[test]
    fn open_output_explains_route_does_not_guarantee_audible_audio() {
        let (state, guidance) = linux_audio_connection_state("PipeWire", true, false, true, None);

        assert_eq!(state, "Open");
        assert!(guidance.contains("backend mixer"));
    }

    #[test]
    fn backend_failure_includes_recovery_guidance() {
        let (state, guidance) =
            linux_audio_connection_state("JACK", true, false, false, Some("server unavailable"));

        assert_eq!(state, "Failed");
        assert!(guidance.contains("server unavailable"));
        assert!(guidance.contains("JACK server is running"));
        assert!(guidance.contains("Select another available backend"));
    }

    #[test]
    fn closed_output_has_a_clear_next_step() {
        let (state, guidance) = linux_audio_connection_state("PipeWire", true, false, false, None);

        assert_eq!(state, "Closed");
        assert!(guidance.contains("Press Play or Reconnect backend"));
    }
}
