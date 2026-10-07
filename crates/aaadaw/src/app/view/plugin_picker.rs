use super::super::{App, Message};
use aaadaw_app::ClapPluginDescriptor;
use aaadaw_core::TrackId;
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
    let searched_plugins = app
        .clap_plugin_scan
        .plugins
        .iter()
        .filter(|plugin| matches_query(plugin, &query))
        .collect::<Vec<_>>();
    let plugins = searched_plugins
        .iter()
        .copied()
        .filter(|plugin| {
            if picking_instrument {
                plugin.is_instrument()
            } else {
                plugin.is_audio_effect()
            }
        })
        .collect::<Vec<_>>();
    let counts = plugin_counts(&app.clap_plugin_scan.plugins);
    let matching_counts = plugin_counts(searched_plugins.iter().copied());

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
        if app.clap_plugin_scan_busy {
            entries = entries
                .push(container(text("Scanning configured CLAP paths…").size(12)).padding([12, 4]));
        } else if !app.clap_plugin_scan.plugins.is_empty() && !query.is_empty() {
            let mut no_match = column![].spacing(8);
            if !picking_instrument && (matching_counts.instruments > 0 || matching_counts.other > 0)
            {
                no_match = no_match.push(
                    text(format!(
                        "No audio effects match ‘{}’. The search matches {} instrument(s) and {} other plug-in(s); only plug-ins advertising the audio-effect feature can be added here.",
                        query, matching_counts.instruments, matching_counts.other
                    ))
                    .size(12),
                );
                if matching_counts.instruments > 0
                    && let Some(track_id) = app.plugin_picker_track_id
                {
                    no_match = no_match.push(instrument_picker_button(track_id));
                }
            } else {
                no_match = no_match.push(
                    text(format!(
                        "No {} match ‘{}’.",
                        picker_type(picking_instrument),
                        query
                    ))
                    .size(12),
                );
            }
            entries = entries.push(
                no_match
                    .push(
                        button("Clear search")
                            .style(button::secondary)
                            .on_press(Message::PluginPickerSearchChanged(String::new())),
                    )
                    .padding([12, 4]),
            );
        } else if app.clap_plugin_scan.plugins.is_empty() {
            entries = entries.push(
                column![
                    text(if picking_instrument {
                        "No CLAP instruments have been discovered. Add a search path or rescan in Settings."
                    } else {
                        "No CLAP audio effects have been discovered. Add a search path or rescan in Settings."
                    })
                    .size(12),
                    button("Open CLAP plugin settings")
                        .style(button::secondary)
                        .on_press(Message::OpenClapPluginSettings),
                ]
                .spacing(8)
                .padding([12, 4]),
            );
        } else if picking_instrument {
            entries = entries.push(
                container(text("No scanned CLAP instruments are available.").size(12))
                    .padding([12, 4]),
            );
        } else {
            let explanation = format!(
                "No audio effects are available. {} instrument(s) and {} other plug-in(s) were discovered; only CLAP plug-ins advertising the audio-effect feature can be inserted into an FX chain.",
                counts.instruments, counts.other
            );
            let mut guidance = column![text(explanation).size(12)].spacing(8);
            if counts.instruments > 0
                && let Some(track_id) = app.plugin_picker_track_id
            {
                guidance = guidance.push(instrument_picker_button(track_id));
            }
            guidance = guidance.push(
                button("Open CLAP plugin settings")
                    .style(button::secondary)
                    .on_press(Message::OpenClapPluginSettings),
            );
            entries = entries.push(guidance.padding([12, 4]));
        }
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
        text(format!("{scan_state} · {} shown", plugins.len())).size(10),
        rule::horizontal(1),
        scrollable(container(entries).padding(iced::Padding::default().right(8.0)))
            .height(Length::Fill),
        row![
            text(format!(
                "{} discovered · {} instruments · {} effects · {} other",
                app.clap_plugin_scan.plugins.len(),
                counts.instruments,
                counts.effects,
                counts.other
            ))
            .size(10),
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

fn picker_type(picking_instrument: bool) -> &'static str {
    if picking_instrument {
        "instruments"
    } else {
        "audio effects"
    }
}

fn instrument_picker_button(track_id: TrackId) -> Element<'static, Message> {
    button("Choose an instrument for this track")
        .style(button::primary)
        .on_press(Message::OpenTrackInstrumentPicker(track_id))
        .into()
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PluginCounts {
    instruments: usize,
    effects: usize,
    other: usize,
}

fn plugin_counts<'a>(plugins: impl IntoIterator<Item = &'a ClapPluginDescriptor>) -> PluginCounts {
    let mut counts = PluginCounts::default();
    for plugin in plugins {
        let (instrument, effect) = (plugin.is_instrument(), plugin.is_audio_effect());
        counts.instruments += usize::from(instrument);
        counts.effects += usize::from(effect);
        counts.other += usize::from(!instrument && !effect);
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn plugin(name: &str, features: &[&str]) -> ClapPluginDescriptor {
        ClapPluginDescriptor {
            entry_path: PathBuf::from(format!("/plugins/{name}.clap")),
            plugin_id: format!("org.example.{name}"),
            name: name.to_owned(),
            vendor: None,
            features: features
                .iter()
                .map(|feature| (*feature).to_owned())
                .collect(),
        }
    }

    #[test]
    fn counts_instruments_effects_and_other_without_treating_instruments_as_effects() {
        let plugins = vec![
            plugin("synth", &["instrument"]),
            plugin("effect", &["audio-effect"]),
            plugin("hybrid", &["instrument", "audio-effect"]),
            plugin("other", &["utility"]),
        ];

        assert_eq!(
            plugin_counts(&plugins),
            PluginCounts {
                instruments: 2,
                effects: 2,
                other: 1,
            }
        );
    }
}
