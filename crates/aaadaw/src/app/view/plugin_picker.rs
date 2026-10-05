use super::super::{App, Message};
use aaadaw_app::ClapPluginDescriptor;
use iced::widget::{button, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let picking_instrument = app.plugin_picker_instrument_track_id.is_some();
    let target_track_id = app
        .plugin_picker_instrument_track_id
        .or(app.plugin_picker_track_id);
    let track_name = app
        .plugin_picker_track_id
        .or(app.plugin_picker_instrument_track_id)
        .and_then(|track_id| {
            app.project
                .tracks()
                .iter()
                .find(|track| track.id() == track_id)
                .map(|track| track.name())
        })
        .unwrap_or("Track no longer exists");
    let query = app.plugin_picker_search.trim().to_lowercase();
    let plugins = app
        .clap_plugin_scan
        .plugins
        .iter()
        .filter(|plugin| {
            if picking_instrument {
                plugin.is_instrument()
            } else {
                plugin.is_audio_effect()
            }
        })
        .filter(|plugin| matches_query(plugin, &query))
        .collect::<Vec<_>>();

    let current_instrument = target_track_id
        .and_then(|track_id| {
            app.project
                .tracks()
                .iter()
                .find(|track| track.id() == track_id)
        })
        .and_then(|track| track.instrument())
        .map(|instrument| {
            app.clap_plugin_scan
                .plugins
                .iter()
                .find(|plugin| plugin.plugin_id == instrument.plugin_id())
                .map_or_else(
                    || instrument.plugin_id().to_owned(),
                    |plugin| plugin.name.clone(),
                )
        })
        .unwrap_or_else(|| "None".to_owned());

    let mut entries = column![].spacing(2);
    for plugin in plugins.iter().copied() {
        let plugin_id = plugin.plugin_id.clone();
        let vendor = plugin.vendor.as_deref().unwrap_or("Unknown vendor");
        let action = if picking_instrument {
            Message::SelectScannedInstrument(plugin_id)
        } else {
            Message::AddScannedPlugin(plugin_id)
        };
        let button_label = if picking_instrument { "Assign" } else { "Add" };
        entries = entries
            .push(
                row![
                    column![
                        row![
                            text(plugin.name.clone()).size(13),
                            iced::widget::Space::new().width(Length::Fill),
                            text(plugin_kind(plugin)).size(10),
                        ]
                        .align_y(Alignment::Center),
                        text(format!("{vendor} · {}", plugin.plugin_id)).size(10),
                        text(plugin.entry_path.display().to_string()).size(10),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    button(button_label).style(button::primary).on_press(action),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .padding([5, 3]),
            )
            .push(rule::horizontal(1));
    }
    if plugins.is_empty() {
        let empty_text = if app.clap_plugin_scan_busy {
            "Scanning configured CLAP paths…"
        } else if app.clap_plugin_scan.plugins.is_empty() {
            if picking_instrument {
                "No scanned instruments are available. Scan a CLAP path in Settings."
            } else {
                "No scanned plugins are available. Scan a CLAP path in Settings."
            }
        } else if !app.clap_plugin_scan.plugins.iter().any(|plugin| {
            if picking_instrument {
                plugin.is_instrument()
            } else {
                plugin.is_audio_effect()
            }
        }) {
            if picking_instrument {
                "No scanned CLAP instruments are available."
            } else {
                "No scanned CLAP effects are available."
            }
        } else if picking_instrument {
            "No scanned instruments match this search."
        } else {
            "No scanned plugins match this search."
        };
        entries = entries.push(container(text(empty_text).size(12)).padding([12, 4]));
    }

    let scan_state = if app.clap_plugin_scan_busy && app.clap_plugin_scan_is_cached {
        "Refreshing paths; cached results are shown"
    } else if app.clap_plugin_scan_is_cached {
        "Using cached results; choose an entry to add it"
    } else if app.clap_plugin_scan_busy {
        "Scanning; the latest completed results are shown"
    } else {
        if picking_instrument {
            "Choose an instrument to render this track's MIDI items"
        } else {
            "Choose an entry to add it to the track FX chain"
        }
    };
    let contents = column![
        column![
            text(if picking_instrument {
                "Assign instrument"
            } else {
                "Add plugin"
            })
            .size(16),
            text(format!("To: {track_name}")).size(11),
            if picking_instrument {
                text(format!("Current: {current_instrument}")).size(10)
            } else {
                text("FX chain").size(10)
            }
        ]
        .spacing(2),
        container(text(super::CLAP_PLUGIN_RISK).size(11)).padding([4, 6]),
        text_input(
            "Filter name, vendor, or plugin ID",
            &app.plugin_picker_search
        )
        .on_input(Message::PluginPickerSearchChanged)
        .padding([6, 8]),
        text(format!("{} · {} shown", scan_state, plugins.len())).size(10),
        rule::horizontal(1),
        scrollable(container(entries).padding(iced::Padding::default().right(8.0)))
            .height(Length::Fill),
        row![
            text(format!("{} discovered", app.clap_plugin_scan.plugins.len())),
            iced::widget::Space::new().width(Length::Fill),
            button("Cancel")
                .style(button::secondary)
                .on_press(Message::ClosePluginPicker),
        ]
        .align_y(Alignment::Center),
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

fn matches_query(plugin: &ClapPluginDescriptor, query: &str) -> bool {
    query.is_empty()
        || plugin.name.to_lowercase().contains(query)
        || plugin
            .vendor
            .as_ref()
            .is_some_and(|vendor| vendor.to_lowercase().contains(query))
        || plugin.plugin_id.to_lowercase().contains(query)
        || plugin
            .entry_path
            .to_string_lossy()
            .to_lowercase()
            .contains(query)
}

fn plugin_kind(plugin: &ClapPluginDescriptor) -> &'static str {
    match (plugin.is_instrument(), plugin.is_audio_effect()) {
        (true, true) => "Instrument · Effect",
        (true, false) => "Instrument",
        (false, true) => "Effect",
        (false, false) => "Other",
    }
}
