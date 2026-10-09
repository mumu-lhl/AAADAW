use super::{App, Message, commands, keyboard_config, shortcut};
use iced::Task;
use iced::keyboard::{Key, Modifiers, key::Named};

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
    pub(super) feedback: String,
    pub(super) found_ids: Option<Vec<String>>,
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

    pub(super) fn open_action_list(&mut self) -> Task<Message> {
        if cfg!(target_os = "android") {
            return self.open_settings();
        }
        if let Some(id) = self.action_list_window_id {
            return iced::window::gain_focus(id);
        }
        self.active_menu = None;
        self.action_list.capture = None;
        let (id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(701.0, 475.0),
            min_size: Some(iced::Size::new(560.0, 360.0)),
            ..Default::default()
        });
        self.action_list_window_id = Some(id);
        task.discard()
    }

    pub(super) fn close_action_list(&mut self) -> Task<Message> {
        self.action_list.capture = None;
        self.action_list_window_id
            .take()
            .map_or_else(Task::none, |id| {
                let close = iced::window::close(id);
                match self.main_window_id {
                    Some(main) => Task::batch([close, iced::window::gain_focus(main)]),
                    None => close,
                }
            })
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
                    let Some(entry) = self.action_list_selected_entry() else {
                        return Task::none();
                    };
                    let Some(id) = commands::stable_id(entry.id) else {
                        return Task::none();
                    };
                    let mut values = self.action_list_bindings(&id);
                    values.push(chord);
                    if self.persist_action_list_bindings(&id, &values) {
                        self.action_list.capture = None;
                    }
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
    fn action_list_binding_commit_is_atomic_on_conflict_and_save_failure() {
        let mut app = App::default();
        app.shortcut_binding_edits
            .insert("file.open-project".into(), "Alt+O".into());
        let conflict = shortcut::parse_bindings("Mod+S").unwrap();
        assert!(
            !app.commit_action_list_bindings("edit.undo", &conflict, |_| panic!(
                "conflict must not write"
            ))
        );
        assert!(app.shortcut_bindings.read().unwrap().is_empty());
        let values = shortcut::parse_bindings("Mod+Z; Alt+F12").unwrap();
        assert!(
            !app.commit_action_list_bindings("edit.undo", &values, |_| Err("disk full".into()))
        );
        assert!(app.shortcut_bindings.read().unwrap().is_empty());
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
