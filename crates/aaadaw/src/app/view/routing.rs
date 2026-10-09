use super::super::{App, Message};
use super::tokens;
use aaadaw_core::{AudioSendParameters, DawAction, TrackId};
use iced::widget::{
    button, checkbox, column, container, pick_list, row, rule, scrollable, text, text_input,
};
use iced::{Element, Length};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Destination {
    id: TrackId,
    name: String,
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
        text("Sends").size(16),
        text("Add new send").size(12),
        pick_list(targets.clone(), None::<Destination>, move |target| {
            Message::RoutingChange(DawAction::CreateAudioSend {
                track_id: source,
                destination: target.id,
                parameters: AudioSendParameters::default(),
            })
        })
        .width(Length::Fill),
    ]
    .spacing(tokens::ROW_GAP);
    for send in track.sends() {
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
                text("Audio · Post-fader").size(11).width(Length::Fill),
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
            let parameters = send.parameters();
            receives = receives.push(
                container(
                    column![
                        button(sender.name()).on_press(Message::OpenTrackRouting(sender_id)),
                        text(format!(
                            "Post-fader · {} dB · Pan {}",
                            parameters.volume_db, parameters.pan
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
        text(format!(
            "Main output: {}",
            track
                .output_track()
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
        scrollable(connections).height(Length::Fill),
        text(&app.status).size(11),
        button("Close").on_press(Message::CloseTrackRouting),
    ]
    .spacing(tokens::SECTION_GAP)
    .padding(tokens::PANEL_PADDING)
    .into()
}
