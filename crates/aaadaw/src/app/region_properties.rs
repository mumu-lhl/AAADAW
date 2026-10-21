use super::{App, Message, commands};
use aaadaw_core::{DawAction, RegionId};
use iced::Task;

pub(super) const NAME_INPUT_ID: &str = "project-region-name";
pub(super) const NUMBER_INPUT_ID: &str = "project-region-number";
pub(super) const POSITION_INPUT_ID: &str = "project-region-position";
pub(super) const LENGTH_INPUT_ID: &str = "project-region-length";

#[derive(Debug, Clone)]
pub(super) struct RegionProperties {
    pub(super) region_id: RegionId,
    generation: u64,
    pub(super) name: String,
    pub(super) number: String,
    pub(super) position: String,
    pub(super) length: String,
    pub(super) error: Option<String>,
}

impl App {
    pub(super) fn open_region_properties(&mut self, region_id: RegionId) -> Task<Message> {
        if let Some(window_id) = self.region_properties_window_id {
            return iced::window::gain_focus(window_id)
                .chain(iced::widget::operation::focus(NAME_INPUT_ID));
        }
        let Some(region) = self
            .project
            .regions()
            .iter()
            .find(|region| region.id() == region_id)
        else {
            self.status = "Project region no longer exists".to_owned();
            return Task::none();
        };
        self.region_properties = Some(RegionProperties {
            region_id,
            generation: self.project_generation,
            name: region.name().to_owned(),
            number: region.number().to_string(),
            position: super::item_properties::format_marker_position(
                &self.project,
                region.start_ticks(),
            )
            .unwrap_or_else(|_| format!("{} ticks", region.start_ticks())),
            length: super::item_properties::format_marker_length(
                &self.project,
                region.start_ticks(),
                region.end_ticks(),
            )
            .unwrap_or_else(|_| format!("{} ticks", region.end_ticks() - region.start_ticks())),
            error: None,
        });
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(500.0, 190.0),
            min_size: Some(iced::Size::new(500.0, 190.0)),
            ..iced::window::Settings::default()
        });
        self.region_properties_window_id = Some(window_id);
        task.then(|_| iced::widget::operation::focus(NAME_INPUT_ID))
    }

    pub(super) fn close_region_properties(&mut self) -> Task<Message> {
        self.region_properties = None;
        self.region_properties_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn apply_region_properties(&mut self) -> Task<Message> {
        let Some(draft) = self.region_properties.as_ref() else {
            return Task::none();
        };
        if draft.generation != self.project_generation {
            self.status = "Project changed while editing this region".to_owned();
            return self.close_region_properties();
        }
        if commands::project_edit_busy(self) {
            return self.region_error(
                "Stop playback or finish the current project operation before editing a region",
            );
        }
        let region_id = draft.region_id;
        let name = draft.name.clone();
        let number_text = draft.number.clone();
        let position_text = draft.position.clone();
        let length_text = draft.length.clone();
        let number = match number_text.parse::<u32>() {
            Ok(number) if (1..=aaadaw_core::MAX_PROJECT_REGION_NUMBER).contains(&number) => number,
            _ => {
                return self.region_error(format!(
                    "Region ID must be between 1 and {}",
                    aaadaw_core::MAX_PROJECT_REGION_NUMBER
                ));
            }
        };
        let start_ticks =
            match super::item_properties::parse_marker_position(&self.project, &position_text) {
                Ok(tick) => tick,
                Err(error) => {
                    return self.region_error(format!("Invalid region position: {error}"));
                }
            };
        let end_ticks = match super::item_properties::parse_marker_length_end_tick(
            &self.project,
            start_ticks,
            &length_text,
        ) {
            Ok(tick) => tick,
            Err(error) => return self.region_error(format!("Invalid region length: {error}")),
        };
        if start_ticks >= end_ticks {
            return self.region_error("Region end must be after its start");
        }
        let Some(region) = self
            .project
            .regions()
            .iter()
            .find(|region| region.id() == region_id)
        else {
            return self.region_error("Project region no longer exists");
        };
        if region.name() == name
            && region.number() == number
            && region.start_ticks() == start_ticks
            && region.end_ticks() == end_ticks
        {
            return self.close_region_properties();
        }

        self.apply_action(
            DawAction::UpdateProjectRegion {
                region_id,
                number,
                start_ticks,
                end_ticks,
                name: name.clone(),
            },
            "Project region updated",
        );
        if self.project.regions().iter().any(|region| {
            region.id() == region_id
                && region.name() == name
                && region.number() == number
                && region.start_ticks() == start_ticks
                && region.end_ticks() == end_ticks
        }) {
            self.close_region_properties()
        } else {
            let error = self.status.clone();
            self.region_error(error)
        }
    }

    fn region_error(&mut self, error: impl Into<String>) -> Task<Message> {
        let error = error.into();
        self.status = error.clone();
        if let Some(draft) = &mut self.region_properties {
            draft.error = Some(error);
        }
        Task::none()
    }
}
