mod frame_rate_menu;
use super::{App, Message};
use aaadaw_core::{BpmBasis, DawAction, FrameRate, ProjectNotes, TimeSignature};
use iced::widget::{button, checkbox, column, container, row, text, text_editor, text_input};
use iced::{Element, Length, Task};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Tab {
    General,
    Video,
    Notes,
}

pub(super) struct Draft {
    pub tab: Tab,
    pub notes: ProjectNotes,
    pub notes_editor: text_editor::Content,

    pub rate: FrameRate,
    pub tempo_bpm: String,
    pub meter_numerator: String,
    pub meter_denominator: String,
    pub start_measure: String,
    pub bpm_basis: BpmBasis,
    generation: u64,
    pub menu_open: bool,
    pub menu_epoch: u64,
}

impl Draft {
    fn from_app(app: &App) -> Self {
        Self {
            tab: Tab::General,
            notes: app.project.project_notes().clone(),
            notes_editor: text_editor::Content::with_text(&app.project.project_notes().text),
            rate: app.project.settings().frame_rate(),
            tempo_bpm: app.project.tempo_at_tick(0).to_string(),
            meter_numerator: app
                .project
                .time_signature_at_tick(0)
                .numerator()
                .to_string(),
            meter_denominator: app
                .project
                .time_signature_at_tick(0)
                .denominator()
                .to_string(),
            start_measure: app.project.settings().start_measure().to_string(),
            bpm_basis: app.project.settings().bpm_basis(),
            generation: app.project_generation,
            menu_open: false,
            menu_epoch: 0,
        }
    }
}

impl App {
    pub(super) fn open_project_settings(&mut self) -> Task<Message> {
        if let Some(window) = self.project_settings_window_id {
            if self
                .project_settings
                .as_ref()
                .is_none_or(|draft| draft.generation != self.project_generation)
            {
                self.project_settings = Some(Draft::from_app(self));
            }
            return iced::window::gain_focus(window);
        }
        self.project_settings = Some(Draft::from_app(self));
        let (window, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(606.0, 535.0),
            min_size: Some(iced::Size::new(400.0, 535.0)),
            ..iced::window::Settings::default()
        });
        self.project_settings_window_id = Some(window);
        task.discard()
    }

    pub(super) fn show_project_notes_on_load(&mut self) -> Task<Message> {
        if !self.project.project_notes().show_on_load {
            return Task::none();
        }
        let task = self.open_project_settings();
        if let Some(draft) = &mut self.project_settings {
            draft.tab = Tab::Notes;
        }
        task
    }

    pub(super) fn close_project_settings(&mut self) -> Task<Message> {
        self.project_settings = None;
        self.project_settings_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn apply_project_settings(&mut self) -> Task<Message> {
        let Some(draft) = self.project_settings.as_ref() else {
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
        let tempo_bpm = match draft.tempo_bpm.parse::<f64>() {
            Ok(value) if value.is_finite() && value > 0.0 => value,
            _ => {
                self.status = "Enter a finite positive project tempo".into();
                return Task::none();
            }
        };
        let numerator = match draft.meter_numerator.parse::<u32>() {
            Ok(value) => value,
            _ => {
                self.status = "Enter a valid time-signature numerator".into();
                return Task::none();
            }
        };
        let denominator = match draft.meter_denominator.parse::<u32>() {
            Ok(value) => value,
            _ => {
                self.status = "Enter a valid time-signature denominator".into();
                return Task::none();
            }
        };
        let signature = match TimeSignature::new(numerator, denominator) {
            Ok(signature) => signature,
            Err(_) => {
                self.status = "Enter a valid time signature".into();
                return Task::none();
            }
        };
        let start_measure = match draft.start_measure.parse::<i32>() {
            Ok(value) => value,
            _ => {
                self.status = "Enter a valid project start measure".into();
                return Task::none();
            }
        };
        let bpm_basis = draft.bpm_basis;
        let rate = draft.rate;
        let notes = draft.notes.clone();
        let mut actions = Vec::new();
        if self.project.settings().bpm_basis() != bpm_basis {
            actions.push(DawAction::SetProjectBpmBasis { basis: bpm_basis });
        }
        if self.project.tempo_at_tick(0) != tempo_bpm {
            actions.push(DawAction::SetTempo {
                start_tick: 0,
                bpm: tempo_bpm,
            });
        }
        if self.project.time_signature_at_tick(0) != signature {
            actions.push(DawAction::SetTimeSignature {
                start_tick: 0,
                signature,
            });
        }
        if self.project.settings().start_measure() != start_measure {
            actions.push(DawAction::SetProjectStartMeasure {
                measure: start_measure,
            });
        }
        if self.project.settings().frame_rate() != rate {
            actions.push(DawAction::SetFrameRate { rate });
        }
        if self.project.project_notes() != &notes {
            actions.push(DawAction::SetProjectNotes {
                notes: notes.clone(),
            });
        }
        if !actions.is_empty() {
            let action = if actions.len() == 1 {
                actions.pop().unwrap()
            } else {
                DawAction::BatchTransaction {
                    tx_id: self.revision,
                    actions,
                }
            };
            self.apply_action(action, "Project settings changed");
            if self.project.settings().frame_rate() != rate
                || self.project.tempo_at_tick(0) != tempo_bpm
                || self.project.time_signature_at_tick(0) != signature
                || self.project.settings().start_measure() != start_measure
                || self.project.settings().bpm_basis() != bpm_basis
                || self.project.project_notes() != &notes
            {
                return Task::none();
            }
        }
        self.close_project_settings()
    }
}

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let Some(draft) = app.project_settings.as_ref() else {
        return text("Project Settings").into();
    };
    let content: Element<'_, Message> = match draft.tab {
        Tab::General => column![
            row![
                text("BPM:").size(13).width(Length::Fixed(32.0)),
                text_input("120", &draft.tempo_bpm)
                    .on_input(Message::ProjectSettingsTempoChanged)
                    .size(13)
                    .width(Length::Fixed(68.0)),
                text("Time signature:").size(13).width(Length::Fixed(112.0)),
                text_input("4", &draft.meter_numerator)
                    .on_input(Message::ProjectSettingsMeterNumeratorChanged)
                    .size(13)
                    .width(Length::Fixed(48.0)),
                text("/").size(13),
                text_input("4", &draft.meter_denominator)
                    .on_input(Message::ProjectSettingsMeterDenominatorChanged)
                    .size(13)
                    .width(Length::Fixed(48.0)),
                text("BPM basis:").size(13).width(Length::Fixed(66.0)),
                iced::widget::pick_list(
                    BpmBasis::ALL,
                    Some(draft.bpm_basis),
                    Message::ProjectSettingsBpmBasisChanged,
                )
                .text_size(13)
                .width(Length::Fixed(110.0))
            ]
            .spacing(6),
            row![
                text("Start measure:").size(13).width(Length::Fixed(112.0)),
                text_input("1", &draft.start_measure)
                    .on_input(Message::ProjectSettingsStartMeasureChanged)
                    .size(13)
                    .width(Length::Fixed(96.0))
            ]
            .spacing(6),
            iced::widget::space().height(Length::Fill)
        ]
        .spacing(12)
        .into(),
        Tab::Video => column![
            row![
                text("Frame rate:").size(13).width(82),
                iced::widget::keyed_column![(draft.menu_epoch, frame_rate_menu::view(draft.rate))]
            ]
            .spacing(6),
            iced::widget::space().height(Length::Fill)
        ]
        .into(),
        Tab::Notes => column![
            row![
                text("Title:").size(13).width(56),
                text_input("", &draft.notes.title)
                    .on_input(Message::ProjectNotesTitleChanged)
                    .size(13)
            ]
            .spacing(6),
            row![
                text("Author:").size(13).width(56),
                text_input("", &draft.notes.author)
                    .on_input(Message::ProjectNotesAuthorChanged)
                    .size(13)
            ]
            .spacing(6),
            text_editor(&draft.notes_editor)
                .on_action(Message::ProjectNotesEdited)
                .size(13)
                .height(Length::Fill)
                .wrapping(if draft.notes.word_wrap {
                    iced::widget::text::Wrapping::Word
                } else {
                    iced::widget::text::Wrapping::None
                }),
            row![
                checkbox(draft.notes.show_on_load)
                    .label("Show notes on project load")
                    .on_toggle(Message::ProjectNotesShowOnLoadChanged)
                    .text_size(13),
                iced::widget::space().width(Length::Fill),
                checkbox(draft.notes.word_wrap)
                    .label("Word wrap")
                    .on_toggle(Message::ProjectNotesWrapChanged)
                    .text_size(13)
            ]
        ]
        .spacing(8)
        .into(),
    };
    container(
        column![
            row![
                button("General").on_press(Message::ProjectSettingsTabChanged(Tab::General)),
                button("Video").on_press(Message::ProjectSettingsTabChanged(Tab::Video)),
                button("Notes").on_press(Message::ProjectSettingsTabChanged(Tab::Notes))
            ]
            .spacing(4),
            content,
            row![
                iced::widget::space().width(Length::Fill),
                button("OK").on_press(Message::ApplyProjectSettings),
                button("Cancel").on_press(Message::CloseProjectSettings)
            ]
            .spacing(8)
        ]
        .spacing(12),
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
    fn general_tempo_and_meter_apply_atomically_and_undo_without_losing_notes() {
        let mut app = App::default();
        let notes = ProjectNotes {
            title: "Notes".into(),
            ..ProjectNotes::default()
        };
        app.project
            .apply(DawAction::SetProjectNotes {
                notes: notes.clone(),
            })
            .unwrap();
        let _ = app.open_project_settings();
        let before = app.project.snapshot();
        let _ = app.update(Message::ProjectSettingsTempoChanged("96".into()));
        let _ = app.update(Message::ProjectSettingsMeterNumeratorChanged("3".into()));
        let _ = app.update(Message::ProjectSettingsMeterDenominatorChanged("8".into()));
        let _ = app.update(Message::ProjectSettingsStartMeasureChanged("3".into()));
        let _ = app.update(Message::ProjectSettingsBpmBasisChanged(BpmBasis::Eighth));
        assert_eq!(app.project.snapshot(), before);
        let _ = app.apply_project_settings();
        assert_eq!(app.project.tempo_at_tick(0), 96.0);
        assert_eq!(
            app.project.time_signature_at_tick(0),
            TimeSignature::new(3, 8).unwrap()
        );
        assert_eq!(app.project.settings().start_measure(), 3);
        assert_eq!(app.project.settings().bpm_basis(), BpmBasis::Eighth);
        assert_eq!(app.project.sample_at_tick(960).unwrap(), 60_000);
        assert!(app.project.undo().unwrap());
        assert_eq!(app.project.tempo_at_tick(0), 120.0);
        assert_eq!(
            app.project.time_signature_at_tick(0),
            TimeSignature::new(4, 4).unwrap()
        );
        assert_eq!(app.project.settings().start_measure(), 1);
        assert_eq!(app.project.settings().bpm_basis(), BpmBasis::Quarter);
        assert_eq!(app.project.project_notes(), &notes);
    }

    #[test]
    fn invalid_general_settings_leave_every_draft_unapplied() {
        let mut app = App::default();
        let before = app.project.snapshot();
        let _ = app.open_project_settings();
        let _ = app.update(Message::ProjectSettingsTempoChanged("NaN".into()));
        let _ = app.update(Message::ProjectSettingsMeterDenominatorChanged("3".into()));
        let _ = app.update(Message::ProjectSettingsStartMeasureChanged(
            "2147483648".into(),
        ));
        let _ = app.update(Message::ProjectFrameRateChanged(FrameRate::Fps25));
        let _ = app.apply_project_settings();
        assert_eq!(app.project.snapshot(), before);
        assert!(app.project_settings.is_some());
        assert!(!app.status.is_empty());
    }

    #[test]
    fn notes_drafts_cancel_then_commit_without_rewinding_media_history() {
        let mut app = App::default();
        let original = app.project.snapshot();
        let _ = app.open_project_settings();
        let _ = app.update(Message::ProjectNotesTitleChanged("标题".into()));
        let _ = app.update(Message::ProjectNotesEdited(text_editor::Action::Edit(
            text_editor::Edit::Paste(std::sync::Arc::new("第一行\nsecond line".into())),
        )));
        assert_eq!(app.project.snapshot(), original);
        let _ = app.close_project_settings();
        assert_eq!(app.project.snapshot(), original);
        let _ = app.open_project_settings();
        let _ = app.update(Message::ProjectNotesAuthorChanged("作者".into()));
        let _ = app.update(Message::ProjectNotesShowOnLoadChanged(true));
        let _ = app.update(Message::ProjectNotesWrapChanged(false));
        let _ = app.apply_project_settings();
        assert_eq!(app.project.project_notes().author, "作者");
        assert!(app.project.project_notes().show_on_load);
        assert!(!app.project.project_notes().word_wrap);
        assert!(!app.project.can_undo());
        let _ = app.show_project_notes_on_load();
        assert_eq!(app.project_settings.as_ref().unwrap().tab, Tab::Notes);
        assert_eq!(
            app.project_settings.as_ref().unwrap().notes,
            *app.project.project_notes()
        );
    }

    #[test]
    fn stale_notes_and_untouched_multiline_text_are_preserved() {
        let mut app = App::default();
        let notes = ProjectNotes {
            text: "多行\r\n\r\nlast\r\n".into(),
            ..ProjectNotes::default()
        };
        app.project
            .apply(DawAction::SetProjectNotes {
                notes: notes.clone(),
            })
            .unwrap();
        let _ = app.open_project_settings();
        let _ = app.update(Message::ProjectNotesTitleChanged("Only title".into()));
        let _ = app.apply_project_settings();
        assert_eq!(app.project.project_notes().text, notes.text);
        let _ = app.open_project_settings();
        let _ = app.update(Message::ProjectNotesTitleChanged("stale title".into()));
        app.project_generation += 1;
        let _ = app.apply_project_settings();
        assert_eq!(app.project.project_notes().title, "Only title");
    }

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
        assert_eq!(
            app.project_settings.as_ref().unwrap().rate,
            FrameRate::Fps30
        );
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
        assert_eq!(
            app.project_settings.as_ref().unwrap().rate,
            FrameRate::Fps30
        );
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
        assert!(!app.project_settings.as_ref().unwrap().menu_open);
        assert_eq!(app.project_settings.as_ref().unwrap().menu_epoch, 1);
        let _ = app.update(Message::RuntimeKeyboardEvent(
            event,
            iced::event::Status::Ignored,
            window,
        ));
        assert!(app.project_settings_window_id.is_none());
        assert_eq!(app.project.settings().frame_rate(), FrameRate::Fps30);
    }

    #[test]
    fn changing_project_settings_tabs_closes_the_video_menu() {
        use iced::keyboard::{Key, Modifiers, key::Named};
        let mut app = App::default();
        let _ = app.open_project_settings();
        let window = app.project_settings_window_id.unwrap();
        let _ = app.update(Message::ProjectSettingsTabChanged(Tab::Video));
        let _ = app.update(Message::ProjectFrameRateMenuChanged(true));
        let original_rate = app.project_settings.as_ref().unwrap().rate;
        let _ = app.update(Message::ProjectSettingsTabChanged(Tab::General));
        assert!(!app.project_settings.as_ref().unwrap().menu_open);
        let event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(Named::ArrowDown),
            modified_key: Key::Named(Named::ArrowDown),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::ArrowDown),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
        });
        let _ = app.update(Message::RuntimeKeyboardEvent(
            event,
            iced::event::Status::Ignored,
            window,
        ));
        assert_eq!(app.project_settings.as_ref().unwrap().rate, original_rate);
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
        assert!(!app.project_settings.as_ref().unwrap().menu_open);
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
