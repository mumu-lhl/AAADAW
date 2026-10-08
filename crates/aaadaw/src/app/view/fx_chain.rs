use super::super::{App, Message};
use super::tokens;
use aaadaw_core::TrackFxPlugin;
use iced::widget::{
    button, column, container, mouse_area, row, rule, scrollable, slider, text, text_input, tooltip,
};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let Some(track_id) = app.fx_chain_track_id else {
        return container(text("No track is selected for this FX chain"))
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    };
    let Some(track) = app
        .project
        .tracks()
        .iter()
        .find(|track| track.id() == track_id)
    else {
        return container(
            column![
                text("Track no longer exists").size(14),
                button("Close").on_press(Message::CloseTrackFxChain),
            ]
            .spacing(10),
        )
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    };

    let mut plugin_rows = column![].spacing(2);
    for (index, plugin) in track.fx_chain().iter().enumerate() {
        let selected = app.fx_chain_selected_index == Some(index);
        let name = plugin_name(app, plugin);
        let enabled = plugin.is_enabled();
        plugin_rows = plugin_rows.push(
            row![
                mouse_area(text("⋮⋮").size(13))
                    .on_press(Message::BeginFxChainPluginDrag(index))
                    .on_enter(Message::HoverFxChainPluginDragTarget(index))
                    .on_exit(Message::LeaveFxChainPluginDragTarget(index)),
                button(if enabled { "On" } else { "Byp" })
                    .style(if enabled {
                        button::success
                    } else {
                        button::warning
                    })
                    .on_press(Message::ToggleFxChainPlugin(index))
                    .padding([3, 5]),
                button(text(name).size(12))
                    .width(Length::Fill)
                    .style(if selected {
                        button::primary
                    } else {
                        button::secondary
                    })
                    .on_press(Message::SelectFxChainPlugin(index))
                    .padding([4, 6]),
                button("Up")
                    .on_press_maybe((index > 0).then_some(Message::ReorderFxChainPlugin {
                        from: index,
                        to: index.saturating_sub(1),
                    }),)
                    .padding([3, 5]),
                button("Down")
                    .on_press_maybe((index + 1 < track.fx_chain().len()).then_some(
                        Message::ReorderFxChainPlugin {
                            from: index,
                            to: index + 1,
                        },
                    ),)
                    .padding([3, 5]),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }
    if track.fx_chain().is_empty() {
        plugin_rows = plugin_rows.push(
            container(text("No effects on this track").size(11))
                .padding([8, 4])
                .width(Length::Fill),
        );
    }

    let selected_plugin = app
        .fx_chain_selected_index
        .and_then(|index| track.fx_chain().get(index));
    let editor_feedback: Element<'_, Message> = if app.fx_chain_editor_status.is_empty() {
        iced::widget::Space::new().height(0).into()
    } else {
        container(text(app.fx_chain_editor_status.clone()).size(10))
            .padding([4, 0])
            .width(Length::Fill)
            .into()
    };
    let editor = match selected_plugin {
        Some(plugin) => selected_plugin_details(app, plugin, false),
        None => column![
            text("Track FX").size(16),
            rule::horizontal(1),
            text("Select a plugin or use Add to insert one from the scanned catalog.").size(12),
        ]
        .spacing(8),
    };

    let list_panel = column![
        row![
            text("FX chain").size(14),
            iced::widget::Space::new().width(Length::Fill),
            text(format!("{}", track.fx_chain().len())).size(11),
        ]
        .align_y(Alignment::Center),
        container(text(super::CLAP_PLUGIN_RISK).size(11)).padding([4, 6]),
        rule::horizontal(1),
        scrollable(plugin_rows).height(Length::Fill),
        editor_feedback,
        rule::horizontal(1),
        row![
            button("Add…").on_press(Message::OpenPluginPicker),
            button("Remove").style(button::danger).on_press_maybe(
                app.fx_chain_selected_index
                    .map(|_| Message::RemoveSelectedFxPlugin),
            ),
        ]
        .spacing(6),
    ]
    .spacing(8)
    .padding(10)
    .width(Length::Fixed(290.0))
    .height(Length::Fill);

    let editor_panel = container(editor.width(Length::Fill))
        .padding(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(container::rounded_box);
    let contents = column![
        row![
            column![text(track.name()).size(15), text("Track effects").size(10),].spacing(2),
            iced::widget::Space::new().width(Length::Fill),
            button("Close")
                .style(button::secondary)
                .on_press(Message::CloseTrackFxChain),
        ]
        .align_y(Alignment::Center),
        rule::horizontal(1),
        row![list_panel, editor_panel]
            .spacing(10)
            .height(Length::Fill),
    ]
    .spacing(8)
    .padding(12)
    .width(Length::Fill)
    .height(Length::Fill);

    container(contents)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub(super) fn mobile_view(app: &App) -> Element<'_, Message> {
    let Some(track_id) = app.fx_chain_track_id else {
        return container(text("No track is selected for this FX chain"))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    };
    let Some(track) = app
        .project
        .tracks()
        .iter()
        .find(|track| track.id() == track_id)
    else {
        return container(text("Track no longer exists"))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    };

    let mut plugin_rows = column![].spacing(tokens::ROW_GAP);
    for (index, plugin) in track.fx_chain().iter().enumerate() {
        let selected = app.fx_chain_selected_index == Some(index);
        let name = plugin_name(app, plugin);
        let mobile_name = tooltip::Tooltip::new(
            text(super::arrangement::truncate_track_name(&name, 24))
                .size(12)
                .width(Length::Fill),
            text(name.clone()).size(12),
            tooltip::Position::Bottom,
        );
        let enabled = plugin.is_enabled();
        plugin_rows = plugin_rows.push(
            container(
                column![
                    row![
                        mobile_name,
                        touch_button(
                            button(if enabled { "On" } else { "Byp" })
                                .style(if enabled {
                                    button::success
                                } else {
                                    button::warning
                                })
                                .on_press(Message::ToggleFxChainPlugin(index)),
                            true,
                        ),
                    ]
                    .spacing(tokens::SPACING_XS)
                    .align_y(Alignment::Center),
                    row![
                        touch_button(
                            button("Select")
                                .style(if selected {
                                    button::primary
                                } else {
                                    button::secondary
                                })
                                .on_press(Message::SelectFxChainPlugin(index)),
                            true,
                        ),
                        touch_button(
                            button("Up").on_press_maybe((index > 0).then_some(
                                Message::ReorderFxChainPlugin {
                                    from: index,
                                    to: index.saturating_sub(1),
                                }
                            ),),
                            true,
                        ),
                        touch_button(
                            button("Down").on_press_maybe(
                                (index + 1 < track.fx_chain().len()).then_some(
                                    Message::ReorderFxChainPlugin {
                                        from: index,
                                        to: index + 1,
                                    },
                                ),
                            ),
                            true,
                        ),
                    ]
                    .spacing(tokens::SPACING_XS),
                ]
                .spacing(tokens::SPACING_XS),
            )
            .width(Length::Fill)
            .padding(tokens::PANEL_PADDING)
            .style(container::rounded_box),
        );
    }
    if track.fx_chain().is_empty() {
        plugin_rows = plugin_rows.push(text("No effects on this track").size(11));
    }
    let editor: Element<'_, Message> = app
        .fx_chain_selected_index
        .and_then(|index| track.fx_chain().get(index))
        .map(|plugin| selected_plugin_details(app, plugin, true).into())
        .unwrap_or_else(|| text("Select a plugin to inspect its host parameters.").into());
    let controls = row![
        touch_button(button("Add…").on_press(Message::OpenPluginPicker), true),
        touch_button(
            button("Remove").style(button::danger).on_press_maybe(
                app.fx_chain_selected_index
                    .map(|_| Message::RemoveSelectedFxPlugin),
            ),
            true,
        ),
    ]
    .spacing(tokens::SPACING_XS);
    let track_title = tooltip::Tooltip::new(
        text(format!(
            "{} · {} effect(s)",
            super::arrangement::truncate_track_name(track.name(), 16),
            track.fx_chain().len()
        ))
        .size(13)
        .width(Length::Fill),
        text(track.name()).size(12),
        tooltip::Position::Bottom,
    );
    let content = column![
        track_title,
        text("Only install trusted CLAP plugins.").size(10),
        plugin_rows,
        controls,
        rule::horizontal(1),
        editor,
    ]
    .spacing(tokens::SPACING_SM)
    .padding(tokens::PANEL_PADDING)
    .width(Length::Fill)
    .height(Length::Shrink);
    // Keep the chain and parameter editor in one vertical scroll region so all
    // controls remain reachable on short phone windows with the transport fixed.
    container(scrollable(content).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn selected_plugin_details<'a>(
    app: &'a App,
    plugin: &'a TrackFxPlugin,
    touch_targets: bool,
) -> iced::widget::Column<'a, Message> {
    let descriptor = app
        .clap_plugin_scan
        .plugins
        .iter()
        .find(|descriptor| descriptor.plugin_id == plugin.plugin_id());
    let name = descriptor
        .map(|descriptor| descriptor.name.as_str())
        .unwrap_or(plugin.plugin_id());
    let vendor = descriptor
        .and_then(|descriptor| descriptor.vendor.as_deref())
        .unwrap_or("Unknown vendor");
    let kind = descriptor.map_or("Not in the current scan results", |descriptor| {
        match (descriptor.is_instrument(), descriptor.is_audio_effect()) {
            (true, true) => "Instrument and effect",
            (true, false) => "Instrument",
            (false, true) => "Audio effect",
            (false, false) => "Other CLAP plugin",
        }
    });
    let editor_state = if !app.fx_chain_editor_status.is_empty() {
        app.fx_chain_editor_status.as_str()
    } else if app.fx_chain_plugin_gui_identity.is_some() {
        "Native CLAP editor is attached in this pane."
    } else {
        "Opening the plugin editor…"
    };
    let mut parameter_controls = column![].spacing(3);
    for parameter in &app.fx_chain_parameters {
        if parameter.read_only
            || !parameter.min_value.is_finite()
            || !parameter.max_value.is_finite()
            || parameter.max_value < parameter.min_value
        {
            continue;
        }
        let id = parameter.id;
        let (minimum, maximum) = if parameter.stepped {
            (parameter.min_value.ceil(), parameter.max_value.floor())
        } else {
            (parameter.min_value, parameter.max_value)
        };
        if maximum <= minimum {
            continue;
        }
        let minimum = minimum as f32;
        let maximum = maximum as f32;
        if !minimum.is_finite() || !maximum.is_finite() || maximum <= minimum {
            continue;
        }
        let value = if parameter.stepped {
            (parameter.value as f32).round().clamp(minimum, maximum)
        } else {
            (parameter.value as f32).clamp(minimum, maximum)
        };
        let step = if parameter.stepped {
            1.0
        } else {
            ((maximum - minimum) / 100.0).max(f32::EPSILON)
        };
        let displayed_value = if parameter.stepped {
            parameter.value.to_string()
        } else {
            parameter.display_value.as_str().to_owned()
        };
        #[cfg(feature = "audio-device")]
        let write_armed = app
            .fx_chain_track_id
            .zip(app.fx_chain_selected_index)
            .is_some_and(|(track_id, chain_index)| {
                app.fx_automation_write_target == Some((track_id, chain_index, id))
            });
        #[cfg(not(feature = "audio-device"))]
        let write_armed = false;
        let lane_visible = app
            .fx_chain_track_id
            .zip(app.fx_chain_selected_index)
            .is_some_and(|(track_id, chain_index)| {
                app.timeline
                    .fx_automation_lanes
                    .contains(&(track_id, chain_index, id))
            });
        let parameter_label = if parameter.stepped {
            column![
                text(parameter.name.as_str()).size(11),
                text(parameter.display_value.as_str()).size(10),
            ]
        } else {
            column![text(parameter.name.as_str()).size(11)]
        };
        let slider_control = super::arrangement::slider_interaction(
            slider(minimum..=maximum, value, move |value| {
                Message::FxParameterChanged(id, f64::from(value))
            })
            .step(step)
            .on_release(Message::FxParameterEnded(id))
            .height(if touch_targets {
                tokens::TOUCH_TARGET_MIN
            } else {
                16.0
            })
            .width(Length::Fill)
            .into(),
            None,
            Message::CancelFxParameterGesture(id),
        );
        let value_input = text_input(
            "value",
            app.fx_parameter_value_edits
                .get(&id)
                .map(String::as_str)
                .unwrap_or(displayed_value.as_str()),
        )
        .on_input(move |value| Message::FxParameterValueTextChanged(id, value))
        .on_submit(Message::CommitFxParameterValue(id))
        .padding(if touch_targets {
            [tokens::SPACING_LG, tokens::SPACING_SM]
        } else {
            [6, 8]
        })
        .width(if touch_targets {
            Length::Fill
        } else {
            Length::Fixed(78.0)
        });
        let reset_button = touch_button(
            button("Reset").on_press(Message::ResetFxParameterValue(id)),
            touch_targets,
        );
        let write_button = touch_button(
            button(if write_armed { "Write*" } else { "Write" })
                .on_press(Message::FxAutomationWriteToggled(id)),
            touch_targets,
        );
        let lane_button = touch_button(
            button(if lane_visible { "Hide" } else { "Show" }).on_press(
                Message::FxAutomationLaneToggled {
                    parameter_id: id,
                    name: parameter.name.clone(),
                    min_value: parameter.min_value,
                    max_value: parameter.max_value,
                    stepped: parameter.stepped,
                },
            ),
            touch_targets,
        );
        let parameter_row: Element<'_, Message> = if touch_targets {
            column![
                row![parameter_label.width(Length::Fill), value_input]
                    .spacing(tokens::SPACING_XS)
                    .align_y(Alignment::Center),
                slider_control,
                row![reset_button, write_button, lane_button].spacing(tokens::SPACING_XS),
            ]
            .spacing(tokens::SPACING_XS)
            .into()
        } else {
            row![
                parameter_label.width(Length::Fixed(120.0)),
                slider_control,
                value_input,
                reset_button.padding([3, 5]),
                write_button.padding([3, 5]),
                lane_button.padding([3, 5]),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
        };
        parameter_controls = parameter_controls.push(parameter_row);
    }
    if app.fx_chain_parameters.iter().all(|parameter| {
        parameter.read_only
            || parameter.max_value <= parameter.min_value
            || (parameter.stepped && parameter.max_value.floor() <= parameter.min_value.ceil())
    }) {
        parameter_controls = parameter_controls
            .push(text("No editable host parameters are exposed by this plugin.").size(11));
    }
    let parameter_view: Element<'a, Message> = if touch_targets {
        parameter_controls.into()
    } else {
        scrollable(parameter_controls).height(Length::Fill).into()
    };
    column![
        text(name).size(17).width(Length::Fill),
        text(format!("{vendor} · {kind}")).size(11),
        text(plugin.plugin_id()).size(10).width(Length::Fill),
        text(plugin.bundle_path()).size(10).width(Length::Fill),
        rule::horizontal(1),
        text(if descriptor.is_none() {
            "This reference remains editable, but its plugin is not present in the current scan results."
        } else {
            editor_state
        })
        .size(12),
        rule::horizontal(1),
        text(
            "Parameters · Enter a value and press Return for exact entry; stepped parameters use whole numbers; Reset uses the plugin default.",
        )
        .size(10),
        parameter_view,
    ]
    .spacing(8)
}

fn touch_button<'a>(
    button: iced::widget::Button<'a, Message>,
    touch_targets: bool,
) -> iced::widget::Button<'a, Message> {
    if touch_targets {
        button
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .padding([tokens::SPACING_XS, tokens::SPACING_SM])
    } else {
        button
    }
}

fn plugin_name(app: &App, plugin: &TrackFxPlugin) -> String {
    app.clap_plugin_scan
        .plugins
        .iter()
        .find(|descriptor| descriptor.plugin_id == plugin.plugin_id())
        .map(|descriptor| descriptor.name.clone())
        .unwrap_or_else(|| format!("Missing: {}", plugin.plugin_id()))
}
