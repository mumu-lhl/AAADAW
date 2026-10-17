use super::{App, Message};
use iced::Task;

impl App {
    pub(super) fn open_snap_grid_settings(&mut self) -> Task<Message> {
        if let Some(window_id) = self.snap_grid_settings_window_id {
            return iced::window::gain_focus(window_id);
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(460.0, 190.0),
            min_size: Some(iced::Size::new(400.0, 170.0)),
            ..iced::window::Settings::default()
        });
        self.snap_grid_settings_window_id = Some(window_id);
        task.discard()
    }

    pub(super) fn close_snap_grid_settings(&mut self) -> Task<Message> {
        self.snap_grid_settings_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }
}
