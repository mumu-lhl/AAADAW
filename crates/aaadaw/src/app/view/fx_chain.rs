use super::super::{App, Message};
use aaadaw_core::TrackFxPlugin;
use iced::widget::{button, column, container, row, rule, scrollable, text};
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
    let editor = match selected_plugin {
        Some(plugin) => selected_plugin_details(app, plugin),
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
        rule::horizontal(1),
        scrollable(plugin_rows).height(Length::Fill),
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
    .width(Length::Fixed(250.0))
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

fn selected_plugin_details<'a>(
    app: &'a App,
    plugin: &'a TrackFxPlugin,
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
    column![
        text(name).size(17),
        text(format!("{vendor} · {kind}")).size(11),
        text(plugin.plugin_id()).size(10),
        text(plugin.bundle_path()).size(10),
        rule::horizontal(1),
        text(if descriptor.is_some() {
            "The plugin is selected in this chain. Its native editor is not attached yet."
        } else {
            "This reference remains editable, but its plugin is not present in the current scan results."
        })
        .size(12),
    ]
    .spacing(8)
}

fn plugin_name(app: &App, plugin: &TrackFxPlugin) -> String {
    app.clap_plugin_scan
        .plugins
        .iter()
        .find(|descriptor| descriptor.plugin_id == plugin.plugin_id())
        .map(|descriptor| descriptor.name.clone())
        .unwrap_or_else(|| format!("Missing: {}", plugin.plugin_id()))
}
