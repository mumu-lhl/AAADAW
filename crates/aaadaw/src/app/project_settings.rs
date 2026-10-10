use super::{App, Message};
use aaadaw_core::{DawAction, FrameRate};
use iced::widget::{button, column, container, pick_list, row, text};
use iced::{Element, Length, Task};

#[derive(Clone, Copy)]
pub(super) struct Draft {
    pub rate: FrameRate,
    generation: u64,
    pub menu_open: bool,
    pub menu_epoch: u64,
}

impl App {
    pub(super) fn open_project_settings(&mut self) -> Task<Message> {
        if let Some(window) = self.project_settings_window_id {
            if self
                .project_settings
                .is_none_or(|draft| draft.generation != self.project_generation)
            {
                self.project_settings = Some(Draft {
                    rate: self.project.settings().frame_rate(),
                    generation: self.project_generation,
                    menu_open: false,
                    menu_epoch: 0,
                });
            }
            return iced::window::gain_focus(window);
        }
        self.project_settings = Some(Draft {
            rate: self.project.settings().frame_rate(),
            generation: self.project_generation,
            menu_open: false,
            menu_epoch: 0,
        });
        let (window, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(606.0, 535.0),
            min_size: Some(iced::Size::new(400.0, 535.0)),
            ..iced::window::Settings::default()
        });
        self.project_settings_window_id = Some(window);
        task.discard()
    }

    pub(super) fn close_project_settings(&mut self) -> Task<Message> {
        self.project_settings = None;
        self.project_settings_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn apply_project_settings(&mut self) -> Task<Message> {
        let Some(draft) = self.project_settings else {
            return Task::none();
        };
        if draft.generation != self.project_generation {
            return self.close_project_settings();
        }
        if self.io_busy
            || self.path_picker_busy
            || self.import_busy
            || self.audio_asset_management_busy
            || self.playback_busy()
        {
            self.status = "Wait for the current operation before applying project settings".into();
            return Task::none();
        }
        if self.project.settings().frame_rate() != draft.rate {
            self.apply_action(
                DawAction::SetFrameRate { rate: draft.rate },
                "Project frame rate changed",
            );
            if self.project.settings().frame_rate() != draft.rate {
                return Task::none();
            }
        }
        self.close_project_settings()
    }
}

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let Some(draft) = app.project_settings else {
        return text("Project Settings").into();
    };
    container(
        column![
            text("Video").size(13),
            row![
                text("Frame rate:").size(13).width(82),
                iced::widget::keyed_column![(
                    draft.menu_epoch,
                    pick_list(
                        FrameRate::ALL,
                        Some(draft.rate),
                        Message::ProjectFrameRateChanged
                    )
                    .on_open(Message::ProjectFrameRateMenuChanged(true))
                    .on_close(Message::ProjectFrameRateMenuChanged(false))
                )]
            ]
            .spacing(6),
            iced::widget::space().height(Length::Fill),
            row![
                iced::widget::space().width(Length::Fill),
                button("OK").on_press(Message::ApplyProjectSettings),
                button("Cancel").on_press(Message::CloseProjectSettings)
            ]
            .spacing(8),
        ]
        .spacing(14),
    )
    .padding(12)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_frame_rate_drafts_cancel_and_commit_as_one_undoable_action() {
        let mut app = App::default();
        let original = app.project.snapshot();
        let _ = app.open_project_settings();
        let _ = app.update(Message::ProjectFrameRateChanged(FrameRate::Fps23976));
        assert_eq!(app.project.snapshot(), original);
        let _ = app.close_project_settings();
        assert_eq!(app.project.snapshot(), original);
        let _ = app.open_project_settings();
        assert_eq!(app.project_settings.unwrap().rate, FrameRate::Fps30);
        let _ = app.update(Message::ProjectFrameRateChanged(FrameRate::Fps2997Drop));
        let _ = app.apply_project_settings();
        assert_eq!(app.project.settings().frame_rate(), FrameRate::Fps2997Drop);
        assert!(app.project_settings.is_none());
        assert!(app.project.undo().unwrap());
        assert_eq!(app.project.snapshot(), original);
        assert!(!app.project.can_undo());
        assert!(app.project.redo().unwrap());
        assert_eq!(app.project.settings().frame_rate(), FrameRate::Fps2997Drop);
    }
    #[test]
    fn project_settings_busy_and_stale_drafts_cannot_mutate_the_project() {
        let mut app = App::default();
        let _ = app.open_project_settings();
        let _ = app.update(Message::ProjectFrameRateChanged(FrameRate::Fps24));
        app.io_busy = true;
        let _ = app.apply_project_settings();
        assert_eq!(app.project.settings().frame_rate(), FrameRate::Fps30);
        assert!(app.project_settings.is_some());
        app.io_busy = false;
        app.project_generation += 1;
        let _ = app.apply_project_settings();
        assert_eq!(app.project.settings().frame_rate(), FrameRate::Fps30);
        assert!(app.project_settings.is_none());
        let _ = app.open_project_settings();
        app.project_generation += 1;
        app.project_settings.as_mut().unwrap().rate = FrameRate::Fps25;
        let _ = app.open_project_settings();
        assert_eq!(app.project_settings.unwrap().rate, FrameRate::Fps30);
        let _ = app.apply_project_settings();
        assert!(!app.project.can_undo());
    }
    #[test]
    fn project_settings_escape_respects_an_open_control_and_cancels_the_dialog() {
        use iced::keyboard::{Key, Modifiers, key::Named};
        let mut app = App::default();
        let _ = app.open_project_settings();
        let window = app.project_settings_window_id.unwrap();
        let _ = app.update(Message::ProjectFrameRateChanged(FrameRate::Fps24));
        let event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(Named::Escape),
            modified_key: Key::Named(Named::Escape),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
        });
        let _ = app.update(Message::RuntimeKeyboardEvent(
            event.clone(),
            iced::event::Status::Captured,
            window,
        ));
        assert_eq!(app.project_settings_window_id, Some(window));
        let _ = app.update(Message::ProjectFrameRateMenuChanged(true));
        let _ = app.update(Message::RuntimeKeyboardEvent(
            event.clone(),
            iced::event::Status::Ignored,
            window,
        ));
        assert_eq!(app.project_settings_window_id, Some(window));
        assert!(!app.project_settings.unwrap().menu_open);
        assert_eq!(app.project_settings.unwrap().menu_epoch, 1);
        let _ = app.update(Message::RuntimeKeyboardEvent(
            event,
            iced::event::Status::Ignored,
            window,
        ));
        assert!(app.project_settings_window_id.is_none());
        assert_eq!(app.project.settings().frame_rate(), FrameRate::Fps30);
    }

    #[test]
    fn enter_on_an_open_frame_menu_does_not_commit_project_settings() {
        use iced::keyboard::{Key, Modifiers, key::Named};
        let mut app = App::default();
        let _ = app.open_project_settings();
        let window = app.project_settings_window_id.unwrap();
        let _ = app.update(Message::ProjectFrameRateChanged(FrameRate::Fps25));
        let _ = app.update(Message::ProjectFrameRateMenuChanged(true));
        let event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(Named::Enter),
            modified_key: Key::Named(Named::Enter),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
        });
        let _ = app.update(Message::RuntimeKeyboardEvent(
            event.clone(),
            iced::event::Status::Ignored,
            window,
        ));
        assert_eq!(app.project_settings_window_id, Some(window));
        assert!(!app.project_settings.unwrap().menu_open);
        assert_eq!(app.project.settings().frame_rate(), FrameRate::Fps30);
        assert!(!app.project.can_undo());
        let _ = app.update(Message::RuntimeKeyboardEvent(
            event,
            iced::event::Status::Ignored,
            window,
        ));
        assert!(app.project_settings_window_id.is_none());
        assert_eq!(app.project.settings().frame_rate(), FrameRate::Fps25);
    }

    #[test]
    fn factory_project_settings_shortcut_yields_to_explicit_user_binding() {
        use super::super::commands::{self, CommandId};
        use iced::keyboard::{Key, Modifiers, key::Named};
        let mut bindings = commands::ShortcutBindings::new();
        let key = Key::Named(Named::Enter);
        assert_eq!(
            commands::from_shortcut(&key.as_ref(), Modifiers::ALT, &bindings, &[]),
            Some(CommandId::OpenProjectSettings)
        );
        bindings.insert("actions.show-list".into(), "Alt+Enter".into());
        assert_eq!(
            commands::from_shortcut(&key.as_ref(), Modifiers::ALT, &bindings, &[]),
            Some(CommandId::OpenActionList)
        );
    }
}
