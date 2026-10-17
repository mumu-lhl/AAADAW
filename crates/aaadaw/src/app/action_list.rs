use super::{App, Message, commands, keyboard_config, shortcut};
use iced::Task;
use iced::keyboard::{Key, Modifiers, key::Named};

pub(super) const RESULTS_SCROLL_ID: &str = "aaadaw-action-list-results";
const RESULTS_FIRST_ROW_OFFSET: f32 = 22.0;
pub(super) const RESULTS_ROW_HEIGHT: f32 = 20.0;
const RESULTS_ROW_STEP: f32 = 21.0;
// The filter, shortcut/footer controls, column spacing, and 12px container padding.
pub(super) const RESULTS_VIEWPORT_CHROME_HEIGHT: f32 = 182.0;

#[derive(Debug, Clone, Copy)]
pub(super) enum CaptureMode {
    Add,
    Find,
}

#[derive(Debug, Default)]
pub(super) struct ActionListState {
    pub(super) query: String,
    pub(super) selected: Option<String>,
    pub(super) selected_binding: Option<usize>,
    pub(super) capture: Option<CaptureMode>,
    pub(super) special_key_capture_pending: bool,
    pub(super) input_draft: Option<shortcut::Shortcut>,
    pub(super) input_action: Option<String>,
    pub(super) feedback: String,
    pub(super) found_ids: Option<Vec<String>>,
    pub(super) results_scroll_offset: f32,
    pub(super) results_viewport_height: f32,
}

impl ActionListState {
    pub(super) fn set_query(&mut self, query: String) {
        self.query = query;
        self.found_ids = None;
    }
    pub(super) fn select(&mut self, id: String) {
        self.selected = Some(id);
        self.selected_binding = None;
        self.capture = None;
        self.feedback.clear();
    }
}

impl App {
    pub(super) fn reset_action_list_results_scroll(&mut self) -> Task<Message> {
        self.action_list.results_scroll_offset = 0.0;
        use iced::advanced::widget::operation::scrollable::{self, RelativeOffset};
        iced::advanced::widget::operate(scrollable::snap_to(
            iced::widget::Id::new(RESULTS_SCROLL_ID),
            RelativeOffset {
                x: None,
                y: Some(0.0),
            },
        ))
    }

    pub(super) fn action_list_entries(&self) -> Vec<commands::CommandEntry> {
        let words = self.action_list.query.to_lowercase();
        let words = words.split_whitespace().collect::<Vec<_>>();
        commands::for_actions_menu(self)
            .into_iter()
            .filter(|entry| {
                let id = commands::stable_id(entry.id).unwrap_or_default();
                self.action_list
                    .found_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&id))
                    && words
                        .iter()
                        .all(|word| entry.matches_query(word) || id.contains(word))
            })
            .collect()
    }

    pub(super) fn action_list_selected_entry(&self) -> Option<commands::CommandEntry> {
        self.action_list_entries().into_iter().find(|entry| {
            commands::stable_id(entry.id).as_ref() == self.action_list.selected.as_ref()
        })
    }

    pub(super) fn action_list_bindings(&self, id: &str) -> Vec<shortcut::Shortcut> {
        let bindings = self
            .shortcut_bindings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let value = commands::binding_for_id(id, &bindings);
        shortcut::parse_bindings(&value).unwrap_or_default()
    }

    fn visible_action_list_entry_ids(&self) -> Vec<String> {
        self.action_list_entries()
            .into_iter()
            .filter_map(|entry| commands::stable_id(entry.id))
            .collect()
    }

    fn select_action_list_boundary(&mut self, select_last: bool) -> Task<Message> {
        let ids = self.visible_action_list_entry_ids();
        let selected = if select_last { ids.last() } else { ids.first() };
        if let Some(id) = selected {
            self.action_list.select(id.clone());
            use iced::advanced::widget::operation::scrollable::{self, RelativeOffset};
            self.action_list.results_scroll_offset = if select_last {
                (RESULTS_FIRST_ROW_OFFSET + ids.len() as f32 * RESULTS_ROW_STEP
                    - self.action_list.results_viewport_height)
                    .max(0.0)
            } else {
                0.0
            };

            return iced::advanced::widget::operate(scrollable::snap_to(
                iced::widget::Id::new(RESULTS_SCROLL_ID),
                RelativeOffset {
                    x: None,
                    y: Some(if select_last { 1.0 } else { 0.0 }),
                },
            ));
        }
        Task::none()
    }

    fn scroll_action_list_entry_into_view(
        &mut self,
        index: usize,
        entry_count: usize,
    ) -> Task<Message> {
        if self.action_list.results_viewport_height <= 0.0 {
            return Task::none();
        }
        let row_top = RESULTS_FIRST_ROW_OFFSET + index as f32 * RESULTS_ROW_STEP;
        let row_bottom = row_top + RESULTS_ROW_HEIGHT;
        let current_top = self.action_list.results_scroll_offset;
        let viewport_bottom = current_top + self.action_list.results_viewport_height;
        let desired_top = if row_top < current_top {
            Some(row_top)
        } else if row_bottom > viewport_bottom {
            Some(row_bottom - self.action_list.results_viewport_height)
        } else {
            None
        };
        let Some(desired_top) = desired_top else {
            return Task::none();
        };
        let maximum = (RESULTS_FIRST_ROW_OFFSET + entry_count as f32 * RESULTS_ROW_STEP
            - self.action_list.results_viewport_height)
            .max(0.0);
        let offset = desired_top.clamp(0.0, maximum);
        self.action_list.results_scroll_offset = offset;
        use iced::advanced::widget::operation::scrollable::{self, AbsoluteOffset};
        iced::advanced::widget::operate(scrollable::scroll_to(
            iced::widget::Id::new(RESULTS_SCROLL_ID),
            AbsoluteOffset {
                x: None,
                y: Some(offset),
            },
        ))
    }

    fn select_adjacent_action_list_entry(&mut self, move_down: bool) -> Task<Message> {
        let ids = self.visible_action_list_entry_ids();
        if ids.is_empty() {
            return Task::none();
        }
        let current_index = self
            .action_list
            .selected
            .as_ref()
            .and_then(|selected| ids.iter().position(|id| id == selected));
        let selected_index = match (current_index, move_down) {
            (Some(index), true) => (index + 1).min(ids.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
            (None, true) => 0,
            (None, false) => ids.len() - 1,
        };
        self.action_list.select(ids[selected_index].clone());
        self.scroll_action_list_entry_into_view(selected_index, ids.len())
    }

    pub(super) fn open_action_list(&mut self) -> Task<Message> {
        if cfg!(target_os = "android") {
            return self.open_settings();
        }
        if let Some(id) = self.action_list_window_id {
            return iced::window::gain_focus(id);
        }
        self.active_menu = None;
        self.action_list.capture = None;
        self.action_list.results_viewport_height = 475.0 - RESULTS_VIEWPORT_CHROME_HEIGHT;
        let (id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(701.0, 475.0),
            min_size: Some(iced::Size::new(560.0, 360.0)),
            ..Default::default()
        });
        self.action_list_window_id = Some(id);
        task.discard()
    }

    pub(super) fn open_action_input(&mut self) -> Task<Message> {
        let Some(entry) = self.action_list_selected_entry() else {
            return Task::none();
        };
        let Some(action) = commands::stable_id(entry.id) else {
            return Task::none();
        };
        if let Some(window) = self.action_input_window_id {
            return iced::window::gain_focus(window);
        }
        self.action_list.input_action = Some(action);
        self.action_list.special_key_capture_pending = false;
        self.action_list.input_draft = None;
        self.action_list.feedback.clear();
        self.action_list.capture = Some(CaptureMode::Add);
        let (window, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(387.0, 229.0),
            min_size: Some(iced::Size::new(387.0, 229.0)),
            ..Default::default()
        });
        self.action_input_window_id = Some(window);
        task.discard()
    }

    pub(super) fn close_action_input(&mut self) -> Task<Message> {
        self.action_list.capture = None;
        self.action_list.special_key_capture_pending = false;
        self.action_list.input_draft = None;
        self.action_list.input_action = None;
        self.action_input_window_id
            .take()
            .map_or_else(Task::none, |window| {
                let close = iced::window::close(window);
                match self.action_list_window_id {
                    Some(parent) => Task::batch([close, iced::window::gain_focus(parent)]),
                    None => close,
                }
            })
    }

    pub(super) fn confirm_action_input(&mut self) -> Task<Message> {
        self.confirm_action_input_with(keyboard_config::save)
    }

    fn confirm_action_input_with(
        &mut self,
        persist: impl FnOnce(&commands::ShortcutBindings) -> Result<(), String>,
    ) -> Task<Message> {
        let (Some(id), Some(chord)) = (
            self.action_list.input_action.clone(),
            self.action_list.input_draft,
        ) else {
            return Task::none();
        };
        let mut values = self.action_list_bindings(&id);
        values.push(chord);
        if self.commit_action_list_bindings(&id, &values, persist) {
            self.close_action_input()
        } else {
            Task::none()
        }
    }

    pub(super) fn close_action_list(&mut self) -> Task<Message> {
        let parent_window = self.action_list_window_id.take();
        let child = self.close_action_input();
        self.action_list.capture = None;
        let parent = parent_window.map_or_else(Task::none, |id| {
            let close = iced::window::close(id);
            match self.main_window_id {
                Some(main) => Task::batch([close, iced::window::gain_focus(main)]),
                None => close,
            }
        });
        Task::batch([child, parent])
    }

    pub(super) fn run_action_list(&mut self, close: bool) -> Task<Message> {
        let Some(entry) = self.action_list_selected_entry() else {
            return Task::none();
        };
        let command = entry.id;
        let close = if close {
            self.close_action_list()
        } else {
            Task::none()
        };
        let run = if entry.enabled {
            self.update(Message::ExecuteCommand(command))
        } else {
            Task::none()
        };
        Task::batch([close, run])
    }

    fn persist_action_list_bindings(&mut self, id: &str, values: &[shortcut::Shortcut]) -> bool {
        self.commit_action_list_bindings(id, values, keyboard_config::save)
    }

    fn commit_action_list_bindings(
        &mut self,
        id: &str,
        values: &[shortcut::Shortcut],
        persist: impl FnOnce(&commands::ShortcutBindings) -> Result<(), String>,
    ) -> bool {
        let mut candidate = self
            .shortcut_bindings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        candidate.insert(id.to_owned(), shortcut::serialize_bindings(values));
        let result = commands::validate_bindings_with_macros(&candidate, &self.action_macros)
            .and_then(|bindings| persist(&bindings).map(|()| bindings));
        match result {
            Ok(bindings) => {
                // Update only this action's settings draft; unrelated drafts stay private.
                if let Some(value) = bindings.get(id) {
                    self.shortcut_binding_edits
                        .insert(id.to_owned(), value.clone());
                }
                self.shortcut_defaults_restored.remove(id);
                *self
                    .shortcut_bindings
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = bindings;
                self.action_list.feedback = "Shortcut saved".to_owned();
                true
            }
            Err(error) => {
                self.action_list.feedback =
                    commands::friendly_shortcut_error(&error, &self.action_macros);
                false
            }
        }
    }

    pub(super) fn delete_action_list_binding(&mut self) {
        let Some(entry) = self.action_list_selected_entry() else {
            return;
        };
        let Some(id) = commands::stable_id(entry.id) else {
            return;
        };
        let mut bindings = self.action_list_bindings(&id);
        let Some(index) = self
            .action_list
            .selected_binding
            .filter(|index| *index < bindings.len())
        else {
            return;
        };
        bindings.remove(index);
        if self.persist_action_list_bindings(&id, &bindings) {
            self.action_list.selected_binding = None;
        }
    }

    pub(super) fn action_list_keyboard_event(
        &mut self,
        event: iced::Event,
        status: iced::event::Status,
    ) -> Task<Message> {
        let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key,
            physical_key,
            location,
            modifiers,
            repeat: false,
            ..
        }) = event
        else {
            return Task::none();
        };
        if self.action_input_window_id.is_some() {
            if key == Key::Named(Named::Escape) && modifiers == Modifiers::NONE {
                return self.close_action_input();
            }
            if key == Key::Named(Named::Enter)
                && modifiers == Modifiers::NONE
                && !self.action_list.special_key_capture_pending
                && self.action_list.input_draft.is_some()
            {
                return self.confirm_action_input();
            }
        }
        if key == Key::Named(Named::Escape) && modifiers == Modifiers::NONE {
            if self.action_list.capture.take().is_some() {
                return Task::none();
            }
            return self.close_action_list();
        }
        if let Some(mode) = self.action_list.capture {
            let input = shortcut::ShortcutInput {
                logical_key: key,
                physical_key,
                location,
                modifiers,
            };
            let chord = match shortcut::Shortcut::capture_input(&input) {
                Ok(chord) => chord,
                Err(error) => {
                    self.action_list.feedback = error;
                    return Task::none();
                }
            };
            match mode {
                CaptureMode::Add => {
                    self.action_list.input_draft = Some(chord);
                    self.action_list.special_key_capture_pending = false;
                    self.action_list.feedback.clear();
                }
                CaptureMode::Find => {
                    let ids = commands::for_actions_menu(self)
                        .into_iter()
                        .filter_map(|entry| {
                            let id = commands::stable_id(entry.id)?;
                            self.action_list_bindings(&id)
                                .iter()
                                .any(|binding| binding.matches_input(&input))
                                .then_some(id)
                        })
                        .collect::<Vec<_>>();
                    self.action_list.query.clear();
                    self.action_list.selected = ids.first().cloned();
                    self.action_list.selected_binding = None;
                    self.action_list.feedback = format!(
                        "{}: {} action(s)",
                        commands::format_shortcut_label(&chord.config_label()),
                        ids.len()
                    );
                    self.action_list.found_ids = Some(ids);
                    self.action_list.capture = None;
                }
            }
        } else if status == iced::event::Status::Ignored
            && modifiers == Modifiers::NONE
            && matches!(
                key,
                Key::Named(Named::ArrowUp | Named::ArrowDown | Named::Home | Named::End)
            )
        {
            match key {
                Key::Named(Named::ArrowUp) => return self.select_adjacent_action_list_entry(false),
                Key::Named(Named::ArrowDown) => {
                    return self.select_adjacent_action_list_entry(true);
                }
                Key::Named(Named::Home) => return self.select_action_list_boundary(false),
                Key::Named(Named::End) => return self.select_action_list_boundary(true),
                _ => unreachable!("only action-list result navigation keys are handled"),
            }
        } else if status == iced::event::Status::Ignored
            && key == Key::Named(Named::Enter)
            && modifiers == Modifiers::NONE
        {
            return self.run_action_list(false);
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_list_binding_commit_accepts_duplicate_and_save_failure_is_atomic() {
        let mut app = App::default();
        app.shortcut_binding_edits
            .insert("file.open-project".into(), "Alt+O".into());
        // REAPER keeps both an explicitly assigned action and the factory
        // action that already uses the same chord in the Action List.
        let duplicate = shortcut::parse_bindings("Mod+S").unwrap();
        assert!(
            app.commit_action_list_bindings("edit.undo", &duplicate, |bindings| {
                assert_eq!(bindings["edit.undo"], "Mod+S");
                Ok(())
            })
        );
        let active = app.shortcut_bindings.read().unwrap().clone();
        assert_eq!(active["edit.undo"], "Mod+S");
        assert!(commands::binding_for_id("file.save-project", &active).contains("Mod+S"));
        assert_eq!(
            commands::from_shortcut(&Key::Character("s"), Modifiers::COMMAND, &active, &[]),
            Some(commands::CommandId::Undo),
            "explicit action binding takes precedence over a colliding factory default"
        );
        let values = shortcut::parse_bindings("Mod+Z; Alt+F12").unwrap();
        assert!(
            !app.commit_action_list_bindings("edit.undo", &values, |_| Err("disk full".into()))
        );
        assert_eq!(app.shortcut_bindings.read().unwrap()["edit.undo"], "Mod+S");
        assert!(app.action_list.feedback.contains("disk full"));
        assert!(
            app.commit_action_list_bindings("edit.undo", &values, |bindings| {
                assert_eq!(bindings["edit.undo"], "Mod+Z; Alt+F12");
                Ok(())
            })
        );
        assert_eq!(
            app.shortcut_bindings.read().unwrap()["edit.undo"],
            "Mod+Z; Alt+F12"
        );
        assert_eq!(app.shortcut_binding_edits["file.open-project"], "Alt+O");
    }

    #[test]
    fn action_list_search_disabled_actions_and_running_share_the_registry() {
        let mut app = App::default();
        app.action_list.set_query("edit undo".into());
        let entries = app.action_list_entries();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].enabled);
        app.action_list.select("edit.undo".into());
        let _ = app.run_action_list(false);
        assert!(app.project.tracks().is_empty());
        app.action_list.set_query("track add".into());
        app.action_list.select("track.add".into());
        let _ = app.run_action_list(false);
        assert_eq!(app.project.tracks().len(), 1);
        app.action_list.set_query("edit undo".into());
        app.action_list.select("edit.undo".into());
        assert!(app.action_list_selected_entry().unwrap().enabled);
        let _ = app.run_action_list(false);
        assert!(app.project.tracks().is_empty());
        app.action_list.set_query("no matching action".into());
        assert!(app.action_list_selected_entry().is_none());
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;

    fn key_event(
        key: Key,
        physical_key: iced::keyboard::key::Physical,
        modifiers: Modifiers,
    ) -> iced::Event {
        let modified_key = key.clone();
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key,
            modified_key,
            physical_key,
            location: iced::keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    }

    #[test]
    fn special_key_capture_records_enter_and_tab_without_confirming_the_binding() {
        let mut app = App::default();
        app.action_list.select("edit.undo".into());
        let _ = app.open_action_input();
        app.action_list.input_draft = Some(
            shortcut::Shortcut::parse("Alt+F12")
                .expect("test shortcut should parse")
                .expect("test shortcut should contain one binding"),
        );

        let _ = app.update(Message::ActionInputSpecialKey);
        assert!(app.action_list.special_key_capture_pending);
        assert!(app.action_list.input_draft.is_none());
        let _ = app.action_list_keyboard_event(
            key_event(
                Key::Named(Named::Enter),
                iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
                Modifiers::NONE,
            ),
            iced::event::Status::Ignored,
        );
        assert_eq!(
            app.action_list
                .input_draft
                .map(|binding| binding.config_label()),
            Some("StandardEnter".to_owned())
        );
        assert!(!app.action_list.special_key_capture_pending);
        assert!(app.action_input_window_id.is_some());

        let _ = app.update(Message::ActionInputSpecialKey);
        let _ = app.action_list_keyboard_event(
            key_event(
                Key::Named(Named::Tab),
                iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Tab),
                Modifiers::NONE,
            ),
            iced::event::Status::Captured,
        );
        assert_eq!(
            app.action_list
                .input_draft
                .map(|binding| binding.config_label()),
            Some("Tab".to_owned())
        );
        assert!(app.action_input_window_id.is_some());
        let _ = app.confirm_action_input_with(|bindings| {
            assert_eq!(bindings["edit.undo"], "Mod+Z; Tab");
            Ok(())
        });
        assert!(app.action_input_window_id.is_none());
        assert_eq!(
            app.shortcut_bindings.read().unwrap()["edit.undo"],
            "Mod+Z; Tab"
        );
    }

    #[test]
    fn input_draft_waits_for_confirmation_and_survives_save_failure() {
        let mut app = App::default();
        app.action_list.select("edit.undo".into());
        let _ = app.open_action_input();
        let window = app.action_input_window_id;
        let _ = app.update(Message::ActionListSelect("track.add".into()));
        assert_eq!(app.action_list.selected.as_deref(), Some("edit.undo"));
        let _ = app.update(Message::ActionListFindShortcut);
        assert!(matches!(app.action_list.capture, Some(CaptureMode::Add)));
        let event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(Named::F12),
            modified_key: Key::Named(Named::F12),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::F12),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::ALT,
            text: None,
            repeat: false,
        });
        let _ = app.action_list_keyboard_event(event, iced::event::Status::Ignored);
        assert!(app.shortcut_bindings.read().unwrap().is_empty());
        assert!(app.action_list.input_draft.is_some());
        let _ = app.confirm_action_input_with(|_| Err("disk full".into()));
        assert!(app.shortcut_bindings.read().unwrap().is_empty());
        assert_eq!(app.action_input_window_id, window);
        assert!(app.action_list.input_draft.is_some());
        let _ = app.confirm_action_input_with(|bindings| {
            assert!(bindings["edit.undo"].contains("Alt+F12"));
            Ok(())
        });
        assert!(app.action_input_window_id.is_none());
        assert!(app.action_list.input_draft.is_none());
        assert!(app.shortcut_bindings.read().unwrap()["edit.undo"].contains("Alt+F12"));
        let before = app.shortcut_bindings.read().unwrap().clone();
        let _ = app.open_action_input();
        app.action_list.input_draft = shortcut::Shortcut::parse("Alt+F11").unwrap();
        let _ = app.close_action_input();
        assert_eq!(*app.shortcut_bindings.read().unwrap(), before);
    }
}
