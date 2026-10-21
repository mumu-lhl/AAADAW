use super::{App, Message, commands};
use aaadaw_core::{DawAction, MarkerId};
use iced::Task;

pub(super) const NAME_INPUT_ID: &str = "project-marker-name";
pub(super) const NUMBER_INPUT_ID: &str = "project-marker-number";
pub(super) const POSITION_INPUT_ID: &str = "project-marker-position";

#[derive(Debug, Clone)]
pub(super) struct MarkerProperties {
    pub(super) marker_id: MarkerId,
    generation: u64,
    pub(super) name: String,
    pub(super) number: String,
    pub(super) position: String,
    pub(super) error: Option<String>,
}

impl App {
    pub(super) fn open_marker_properties(&mut self, marker_id: MarkerId) -> Task<Message> {
        if let Some(window_id) = self.marker_properties_window_id {
            return iced::window::gain_focus(window_id)
                .chain(iced::widget::operation::focus(NAME_INPUT_ID));
        }
        let Some(marker) = self
            .project
            .markers()
            .iter()
            .find(|marker| marker.id() == marker_id)
        else {
            self.status = "Project marker no longer exists".to_owned();
            return Task::none();
        };
        self.marker_properties = Some(MarkerProperties {
            marker_id,
            generation: self.project_generation,
            name: marker.name().to_owned(),
            number: marker.number().to_string(),
            position: super::item_properties::format_marker_position(
                &self.project,
                marker.position_ticks(),
            )
            .unwrap_or_else(|_| format!("{} ticks", marker.position_ticks())),
            error: None,
        });
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(480.0, 184.0),
            min_size: Some(iced::Size::new(480.0, 184.0)),
            ..iced::window::Settings::default()
        });
        self.marker_properties_window_id = Some(window_id);
        task.then(|_| iced::widget::operation::focus(NAME_INPUT_ID))
    }

    pub(super) fn close_marker_properties(&mut self) -> Task<Message> {
        self.marker_properties = None;
        self.marker_properties_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn apply_marker_properties(&mut self) -> Task<Message> {
        let Some(draft) = self.marker_properties.as_ref() else {
            return Task::none();
        };
        if draft.generation != self.project_generation {
            self.status = "Project changed while editing this marker".to_owned();
            return self.close_marker_properties();
        }
        if commands::project_edit_busy(self) {
            let error =
                "Stop playback or finish the current project operation before editing a marker";
            self.status = error.to_owned();
            if let Some(draft) = &mut self.marker_properties {
                draft.error = Some(error.to_owned());
            }
            return Task::none();
        }
        let marker_id = draft.marker_id;
        let name = draft.name.clone();
        let number = draft.number.clone();
        let position = draft.position.clone();
        let number = match number.parse::<u32>() {
            Ok(number) if (1..=aaadaw_core::MAX_PROJECT_MARKER_NUMBER).contains(&number) => number,
            _ => {
                let error = format!(
                    "Marker ID must be between 1 and {}",
                    aaadaw_core::MAX_PROJECT_MARKER_NUMBER
                );
                self.status = error.clone();
                if let Some(draft) = &mut self.marker_properties {
                    draft.error = Some(error);
                }
                return Task::none();
            }
        };
        let position_ticks =
            match super::item_properties::parse_marker_position(&self.project, &position) {
                Ok(position_ticks) => position_ticks,
                Err(error) => {
                    self.status = error.clone();
                    if let Some(draft) = &mut self.marker_properties {
                        draft.error = Some(error);
                    }
                    return Task::none();
                }
            };
        let Some(marker) = self
            .project
            .markers()
            .iter()
            .find(|marker| marker.id() == marker_id)
        else {
            let error = "Project marker no longer exists".to_owned();
            self.status = error.clone();
            if let Some(draft) = &mut self.marker_properties {
                draft.error = Some(error);
            }
            return Task::none();
        };
        if marker.name() == name
            && marker.number() == number
            && marker.position_ticks() == position_ticks
        {
            return self.close_marker_properties();
        }

        self.apply_action(
            DawAction::UpdateProjectMarkerProperties {
                marker_id,
                number,
                position_ticks,
                name: name.clone(),
            },
            "Project marker updated",
        );
        if self.project.markers().iter().any(|marker| {
            marker.id() == marker_id
                && marker.name() == name
                && marker.number() == number
                && marker.position_ticks() == position_ticks
        }) {
            self.close_marker_properties()
        } else {
            let error = self.status.clone();
            if let Some(draft) = &mut self.marker_properties {
                draft.error = Some(error);
            }
            Task::none()
        }
    }
}
