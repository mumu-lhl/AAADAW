use super::super::{App, Message};
use super::tokens;
use aaadaw_core::{AudioSendParameters, AudioSendTap, DawAction, TrackId};
use iced::widget::{
    button, checkbox, column, container, pick_list, row, rule, scrollable, text, text_input,
};
use iced::{Element, Length};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Tap(AudioSendTap);
impl fmt::Display for Tap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0 {
            AudioSendTap::PostFader => "Post-fader (Post-pan)",
            AudioSendTap::PreFx => "Pre-FX",
            AudioSendTap::PreFader => "Pre-fader (Post-FX)",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MidiSourceChannel(Option<u8>);
impl fmt::Display for MidiSourceChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(channel) => write!(f, "Channel {}", channel + 1),
            None => f.write_str("All"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MidiDestinationChannel(Option<u8>);
impl fmt::Display for MidiDestinationChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(channel) => write!(f, "Channel {}", channel + 1),
            None => f.write_str("Original"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MidiSourceBus(Option<u8>);
impl fmt::Display for MidiSourceBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(bus) => write!(f, "Bus {}", bus + 1),
            None => f.write_str("All"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MidiDestinationBus(Option<u8>);
impl fmt::Display for MidiDestinationBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(bus) => write!(f, "Bus {}", bus + 1),
            None => f.write_str("All"),
        }
    }
}

fn midi_source_channel_choices() -> Vec<MidiSourceChannel> {
    std::iter::once(MidiSourceChannel(None))
        .chain((0..16).map(|channel| MidiSourceChannel(Some(channel))))
        .collect()
}

fn midi_destination_channel_choices() -> Vec<MidiDestinationChannel> {
    std::iter::once(MidiDestinationChannel(None))
        .chain((0..16).map(|channel| MidiDestinationChannel(Some(channel))))
        .collect()
}

fn midi_source_bus_choices() -> Vec<MidiSourceBus> {
    std::iter::once(MidiSourceBus(None))
        .chain((0..16).map(|bus| MidiSourceBus(Some(bus))))
        .collect()
}

fn midi_destination_bus_choices() -> Vec<MidiDestinationBus> {
    std::iter::once(MidiDestinationBus(None))
        .chain((0..16).map(|bus| MidiDestinationBus(Some(bus))))
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Destination {
    id: TrackId,
    name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Parent {
    id: Option<TrackId>,
    name: String,
}
impl fmt::Display for Parent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}
impl fmt::Display for Destination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let Some(track) = app
        .routing_track_id
        .and_then(|id| app.project.tracks().iter().find(|track| track.id() == id))
    else {
        return column![
            text("Track unavailable"),
            button("Close").on_press(Message::CloseTrackRouting)
        ]
        .padding(tokens::PANEL_PADDING)
        .into();
    };
    let source = track.id();
    let routing_destination_filter = app.routing_destination_filter.filter(|destination| {
        track
            .sends()
            .iter()
            .any(|send| send.destination() == *destination)
    });
    let focused_destination_name = routing_destination_filter.and_then(|destination| {
        app.project
            .tracks()
            .iter()
            .find(|candidate| candidate.id() == destination)
            .map(|destination| destination.name())
    });
    let parents: Vec<_> = std::iter::once(Parent {
        id: None,
        name: "No folder (root)".into(),
    })
    .chain(
        app.project
            .tracks()
            .iter()
            .filter(|candidate| app.project.can_parent_to(source, candidate.id()))
            .map(|candidate| Parent {
                id: Some(candidate.id()),
                name: candidate.name().into(),
            }),
    )
    .collect();
    let selected_parent = parents
        .iter()
        .find(|parent| parent.id == track.parent_track())
        .cloned();
    let targets: Vec<_> = app
        .project
        .tracks()
        .iter()
        .filter(|candidate| app.project.can_route_to(source, candidate.id()))
        .map(|candidate| Destination {
            id: candidate.id(),
            name: candidate.name().to_owned(),
        })
        .collect();
    let mut sends = column![
        text(
            focused_destination_name
                .map_or_else(|| "Sends".to_owned(), |name| format!("Sends to {name}"))
        )
        .size(16),
    ]
    .spacing(tokens::ROW_GAP);
    if routing_destination_filter.is_none() {
        sends = sends.push(text("Add new send").size(12));
        sends = sends.push(
            pick_list(targets.clone(), None::<Destination>, move |target| {
                Message::RoutingChange(DawAction::CreateAudioSend {
                    track_id: source,
                    destination: target.id,
                    parameters: AudioSendParameters::default(),
                })
            })
            .width(Length::Fill),
        );
    }
    for send in track.sends().iter().filter(|send| {
        routing_destination_filter.is_none_or(|destination| send.destination() == destination)
    }) {
        let id = send.id();
        let destination = send.destination();
        let parameters = send.parameters();
        let selected = targets
            .iter()
            .find(|target| target.id == destination)
            .cloned();
        let volume = app.routing_send_drafts.get(&id).map_or_else(
            || parameters.volume_db.to_string(),
            |draft| draft.volume_db.clone(),
        );
        let pan = app
            .routing_send_drafts
            .get(&id)
            .map_or_else(|| parameters.pan.to_string(), |draft| draft.pan.clone());
        let controls = column![
            row![
                text("Audio").size(11).width(Length::Fill),
                button("Remove").on_press(Message::RoutingChange(DawAction::DeleteAudioSend {
                    track_id: source,
                    send_id: id
                }))
            ],
            pick_list(targets.clone(), selected, move |target| {
                Message::RoutingChange(DawAction::UpdateAudioSend {
                    track_id: source,
                    send_id: id,
                    destination: target.id,
                    parameters,
                })
            })
            .width(Length::Fill),
            pick_list(
                [
                    Tap(AudioSendTap::PostFader),
                    Tap(AudioSendTap::PreFx),
                    Tap(AudioSendTap::PreFader)
                ],
                Some(Tap(parameters.tap)),
                move |tap| Message::RoutingChange(DawAction::UpdateAudioSend {
                    track_id: source,
                    send_id: id,
                    destination,
                    parameters: AudioSendParameters {
                        tap: tap.0,
                        ..parameters
                    },
                }),
            )
            .width(Length::Fill),
            row![
                column![
                    text("Volume · dB").size(11),
                    text_input("0", &volume)
                        .on_input(move |value| Message::RoutingSendDraft(id, false, value))
                        .on_submit(Message::CommitRoutingSend(id))
                ],
                column![
                    text("Pan · -1 to 1").size(11),
                    text_input("0", &pan)
                        .on_input(move |value| Message::RoutingSendDraft(id, true, value))
                        .on_submit(Message::CommitRoutingSend(id))
                ],
                button("Apply").on_press(Message::CommitRoutingSend(id)),
            ]
            .spacing(tokens::ROW_GAP),
            row![
                checkbox(parameters.midi_enabled)
                    .label(format!(
                        "Send MIDI · Ch {} → {} · Bus {} → {}",
                        MidiSourceChannel(parameters.midi_source_channel),
                        MidiDestinationChannel(parameters.midi_destination_channel),
                        MidiSourceBus(parameters.midi_source_bus),
                        MidiDestinationBus(parameters.midi_destination_bus)
                    ))
                    .on_toggle(move |midi_enabled| Message::RoutingChange(
                        DawAction::UpdateAudioSend {
                            track_id: source,
                            send_id: id,
                            destination,
                            parameters: AudioSendParameters {
                                midi_enabled,
                                ..parameters
                            },
                        }
                    )),
            ]
            .spacing(tokens::SECTION_GAP),
            row![
                column![
                    text("MIDI source").size(11),
                    pick_list(
                        midi_source_channel_choices(),
                        Some(MidiSourceChannel(parameters.midi_source_channel)),
                        move |channel| Message::RoutingChange(DawAction::UpdateAudioSend {
                            track_id: source,
                            send_id: id,
                            destination,
                            parameters: AudioSendParameters {
                                midi_source_channel: channel.0,
                                ..parameters
                            },
                        })
                    )
                ]
                .width(Length::Fill),
                text("→"),
                column![
                    text("MIDI destination").size(11),
                    pick_list(
                        midi_destination_channel_choices(),
                        Some(MidiDestinationChannel(parameters.midi_destination_channel)),
                        move |channel| Message::RoutingChange(DawAction::UpdateAudioSend {
                            track_id: source,
                            send_id: id,
                            destination,
                            parameters: AudioSendParameters {
                                midi_destination_channel: channel.0,
                                ..parameters
                            },
                        })
                    )
                ]
                .width(Length::Fill),
            ]
            .spacing(tokens::ROW_GAP),
            row![
                column![
                    text("MIDI source bus").size(11),
                    pick_list(
                        midi_source_bus_choices(),
                        Some(MidiSourceBus(parameters.midi_source_bus)),
                        move |bus| Message::RoutingChange(DawAction::UpdateAudioSend {
                            track_id: source,
                            send_id: id,
                            destination,
                            parameters: AudioSendParameters {
                                midi_source_bus: bus.0,
                                ..parameters
                            },
                        })
                    )
                ]
                .width(Length::Fill),
                text("→"),
                column![
                    text("MIDI destination bus").size(11),
                    pick_list(
                        midi_destination_bus_choices(),
                        Some(MidiDestinationBus(parameters.midi_destination_bus)),
                        move |bus| Message::RoutingChange(DawAction::UpdateAudioSend {
                            track_id: source,
                            send_id: id,
                            destination,
                            parameters: AudioSendParameters {
                                midi_destination_bus: bus.0,
                                ..parameters
                            },
                        })
                    )
                ]
                .width(Length::Fill),
            ]
            .spacing(tokens::ROW_GAP),
            row![
                checkbox(parameters.muted)
                    .label("Mute")
                    .on_toggle(
                        move |muted| Message::RoutingChange(DawAction::UpdateAudioSend {
                            track_id: source,
                            send_id: id,
                            destination,
                            parameters: AudioSendParameters {
                                muted,
                                ..parameters
                            },
                        })
                    ),
                checkbox(parameters.phase_inverted)
                    .label("Invert phase")
                    .on_toggle(move |phase_inverted| Message::RoutingChange(
                        DawAction::UpdateAudioSend {
                            track_id: source,
                            send_id: id,
                            destination,
                            parameters: AudioSendParameters {
                                phase_inverted,
                                ..parameters
                            },
                        }
                    )),
            ]
            .spacing(tokens::SECTION_GAP),
        ]
        .spacing(tokens::ROW_GAP);
        sends = sends.push(
            container(controls)
                .padding(tokens::PANEL_PADDING)
                .style(container::bordered_box),
        );
    }
    if let Some(destination_name) = focused_destination_name {
        return column![
            text(format!(
                "Send settings · {} → {destination_name}",
                track.name()
            ))
            .size(18),
            scrollable(sends).width(Length::Fill).height(Length::Fill),
            text(&app.status).size(11),
            button("Close").on_press(Message::CloseTrackRouting),
        ]
        .spacing(tokens::SECTION_GAP)
        .padding(tokens::PANEL_PADDING)
        .into();
    }
    let mut receives = column![text("Receives").size(16)].spacing(tokens::ROW_GAP);
    let mut receive_count = 0;
    for sender in app.project.tracks() {
        for send in sender
            .sends()
            .iter()
            .filter(|send| send.destination() == source)
        {
            receive_count += 1;
            let sender_id = sender.id();
            let send_id = send.id();
            let parameters = send.parameters();
            receives = receives.push(
                container(
                    column![
                        row![
                            button(sender.name())
                                .on_press(Message::OpenTrackRouting(sender_id))
                                .width(Length::Fill),
                            button("Remove").on_press(Message::RoutingChange(
                                DawAction::DeleteAudioSend {
                                    track_id: sender_id,
                                    send_id,
                                },
                            )),
                        ]
                        .spacing(tokens::ROW_GAP),
                        text(format!(
                            "{} · {} dB · Pan {} · MIDI {}",
                            Tap(parameters.tap),
                            parameters.volume_db,
                            parameters.pan,
                            if parameters.midi_enabled {
                                format!(
                                    "{} → {}",
                                    MidiSourceChannel(parameters.midi_source_channel),
                                    MidiDestinationChannel(parameters.midi_destination_channel)
                                ) + &format!(
                                    " · {} → {}",
                                    MidiSourceBus(parameters.midi_source_bus),
                                    MidiDestinationBus(parameters.midi_destination_bus)
                                )
                            } else {
                                "Off".to_owned()
                            }
                        ))
                        .size(11),
                        text(format!(
                            "{}{}",
                            if parameters.muted { "Muted " } else { "" },
                            if parameters.phase_inverted {
                                "Phase inverted"
                            } else {
                                ""
                            }
                        ))
                        .size(11),
                    ]
                    .spacing(tokens::ROW_GAP),
                )
                .padding(tokens::PANEL_PADDING)
                .style(container::bordered_box),
            );
        }
    }
    if receive_count == 0 {
        receives = receives.push(text("No receives").size(12));
    }
    let connections: Element<'_, Message> = if app.is_mobile_main_window() {
        column![sends, rule::horizontal(1), receives]
            .spacing(tokens::SECTION_GAP)
            .into()
    } else {
        row![
            sends.width(Length::FillPortion(1)),
            receives.width(Length::FillPortion(1))
        ]
        .spacing(tokens::SECTION_GAP)
        .into()
    };
    column![
        text(format!("Routing for {}", track.name())).size(18),
        row![
            checkbox(track.is_folder())
                .label("Folder track")
                .on_toggle(
                    move |enabled| Message::RoutingChange(DawAction::SetTrackFolder {
                        track_id: source,
                        enabled
                    })
                ),
            text("Parent folder").size(12),
            pick_list(parents, selected_parent, move |parent| {
                Message::RoutingChange(DawAction::SetTrackParent {
                    track_id: source,
                    parent: parent.id,
                })
            })
            .width(Length::Fill),
        ]
        .spacing(tokens::SECTION_GAP),
        text(format!(
            "Main output: {}",
            track
                .effective_output_track()
                .and_then(|id| app
                    .project
                    .tracks()
                    .iter()
                    .find(|candidate| candidate.id() == id))
                .map_or("Master", |target| target.name())
        ))
        .size(12),
        checkbox(track.main_send_enabled())
            .label("Master/parent send")
            .on_toggle(
                move |enabled| Message::RoutingChange(DawAction::SetTrackMainSend {
                    track_id: source,
                    enabled
                })
            ),
        checkbox(track.is_phase_inverted())
            .label("Invert track polarity")
            .on_toggle(move |_| Message::TogglePhase(source)),
        scrollable(connections).height(Length::Fill),
        text(&app.status).size(11),
        button("Close").on_press(Message::CloseTrackRouting),
    ]
    .spacing(tokens::SECTION_GAP)
    .padding(tokens::PANEL_PADDING)
    .into()
}
