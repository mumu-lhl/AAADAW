use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShortcutKey {
    Character(char),
    Named(Named),
    // Existing configurations and the default delete action bind both keys.
    DeleteBackspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Shortcut {
    key: ShortcutKey,
    modifiers: Modifiers,
}

const NAMED_KEYS: &[Named] = &[
    Named::Space,
    Named::Enter,
    Named::Tab,
    Named::Escape,
    Named::Backspace,
    Named::Delete,
    Named::Insert,
    Named::Home,
    Named::End,
    Named::PageUp,
    Named::PageDown,
    Named::ArrowLeft,
    Named::ArrowRight,
    Named::ArrowUp,
    Named::ArrowDown,
    Named::PrintScreen,
    Named::Pause,
    Named::F1,
    Named::F2,
    Named::F3,
    Named::F4,
    Named::F5,
    Named::F6,
    Named::F7,
    Named::F8,
    Named::F9,
    Named::F10,
    Named::F11,
    Named::F12,
    Named::F13,
    Named::F14,
    Named::F15,
    Named::F16,
    Named::F17,
    Named::F18,
    Named::F19,
    Named::F20,
    Named::F21,
    Named::F22,
    Named::F23,
    Named::F24,
];

impl Shortcut {
    pub(super) const fn character(key: char, modifiers: Modifiers) -> Self {
        Self {
            key: ShortcutKey::Character(key),
            modifiers,
        }
    }

    #[cfg(feature = "audio-device")]
    pub(super) const fn named(key: Named, modifiers: Modifiers) -> Self {
        Self {
            key: ShortcutKey::Named(key),
            modifiers,
        }
    }

    pub(super) const fn delete_backspace() -> Self {
        Self {
            key: ShortcutKey::DeleteBackspace,
            modifiers: Modifiers::NONE,
        }
    }

    pub(super) fn parse(value: &str) -> Result<Option<Self>, String> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }
        let mut parts = value.split('+').map(str::trim).collect::<Vec<_>>();
        let key = parse_key(parts.pop().unwrap_or_default())?;
        let mut modifiers = Modifiers::NONE;
        for part in parts {
            let modifier = match part.to_ascii_lowercase().as_str() {
                "mod" => Modifiers::COMMAND,
                "ctrl" | "control" => Modifiers::CTRL,
                "cmd" | "super" | "meta" | "win" | "logo" => Modifiers::LOGO,
                "alt" | "option" => Modifiers::ALT,
                "shift" => Modifiers::SHIFT,
                _ => return Err(format!("Unknown shortcut modifier: {part}")),
            };
            if modifiers.contains(modifier) {
                return Err(format!("Repeated shortcut modifier: {part}"));
            }
            modifiers.insert(modifier);
        }
        Ok(Some(Self { key, modifiers }))
    }

    pub(super) fn capture(key: &str, modifiers: Modifiers) -> Result<Self, String> {
        Ok(Self {
            key: parse_key(key)?,
            modifiers,
        })
    }

    pub(super) fn config_label(self) -> String {
        let mut parts = modifier_labels(self.modifiers);
        parts.push(match self.key {
            ShortcutKey::Character('+') => "Plus".to_owned(),
            ShortcutKey::Character(';') => "Semicolon".to_owned(),
            ShortcutKey::Character(key) => key.to_ascii_uppercase().to_string(),
            ShortcutKey::Named(key) => format!("{key:?}"),
            ShortcutKey::DeleteBackspace => "Delete/Backspace".to_owned(),
        });
        parts.join("+")
    }

    pub(super) fn matches(self, key: &Key<&str>, modifiers: Modifiers) -> bool {
        self.modifiers == modifiers
            && match self.key {
                ShortcutKey::Character(expected) => {
                    matches!(key, Key::Character(value) if value.eq_ignore_ascii_case(&expected.to_string()))
                }
                ShortcutKey::Named(expected) => *key == Key::Named(expected),
                ShortcutKey::DeleteBackspace => {
                    matches!(key, Key::Named(Named::Delete | Named::Backspace))
                }
            }
    }

    pub(super) fn conflicts(self, other: Self) -> bool {
        if self.modifiers != other.modifiers {
            return false;
        }
        self.key == other.key
            || matches!(
                (self.key, other.key),
                (
                    ShortcutKey::DeleteBackspace,
                    ShortcutKey::Named(Named::Delete | Named::Backspace)
                ) | (
                    ShortcutKey::Named(Named::Delete | Named::Backspace),
                    ShortcutKey::DeleteBackspace
                )
            )
    }

    pub(super) fn is_reserved(self) -> bool {
        self.key == ShortcutKey::Named(Named::Escape) && self.modifiers == Modifiers::NONE
    }
}

fn modifier_labels(modifiers: Modifiers) -> Vec<String> {
    let mut labels = Vec::new();
    if modifiers.contains(Modifiers::COMMAND) {
        labels.push("Mod".to_owned());
    }
    let other = if cfg!(target_os = "macos") {
        (Modifiers::CTRL, "Ctrl")
    } else {
        (Modifiers::LOGO, "Super")
    };
    if modifiers.contains(other.0) {
        labels.push(other.1.to_owned());
    }
    if modifiers.alt() {
        labels.push("Alt".to_owned());
    }
    if modifiers.shift() {
        labels.push("Shift".to_owned());
    }
    labels
}

fn parse_key(value: &str) -> Result<ShortcutKey, String> {
    if value.eq_ignore_ascii_case("delete/backspace") {
        return Ok(ShortcutKey::DeleteBackspace);
    }
    if value.eq_ignore_ascii_case("plus") {
        return Ok(ShortcutKey::Character('+'));
    }
    if value.eq_ignore_ascii_case("semicolon") {
        return Ok(ShortcutKey::Character(';'));
    }
    if let Some(key) = NAMED_KEYS
        .iter()
        .find(|key| format!("{key:?}").eq_ignore_ascii_case(value))
    {
        return Ok(ShortcutKey::Named(*key));
    }
    let mut chars = value.chars();
    if let Some(key) = chars.next()
        && chars.next().is_none()
        && !key.is_control()
        && !key.is_whitespace()
    {
        return Ok(ShortcutKey::Character(key.to_ascii_lowercase()));
    }
    Err(format!("Unsupported shortcut key: {value}"))
}

/// Semicolons separate alternative bindings; the Semicolon key uses its name.
pub(super) fn parse_bindings(value: &str) -> Result<Vec<Shortcut>, String> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut shortcuts = Vec::new();
    for part in value.split(';') {
        let shortcut = Shortcut::parse(part)?
            .ok_or_else(|| "An alternative shortcut cannot be empty".to_owned())?;
        if !shortcuts.contains(&shortcut) {
            shortcuts.push(shortcut);
        }
    }
    Ok(shortcuts)
}

pub(super) fn serialize_bindings(shortcuts: &[Shortcut]) -> String {
    shortcuts
        .iter()
        .map(|shortcut| shortcut.config_label())
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_chords_capture_round_trip_and_match_exact_modifiers() {
        for (key, modifiers) in [
            (Key::Named(Named::F12), Modifiers::CTRL | Modifiers::ALT),
            (Key::Named(Named::Home), Modifiers::SHIFT),
            (Key::Named(Named::Enter), Modifiers::ALT),
            (Key::Character("7"), Modifiers::CTRL | Modifiers::SHIFT),
            (Key::Character("/"), Modifiers::ALT),
            (Key::Character("+"), Modifiers::CTRL),
            (Key::Character(";"), Modifiers::NONE),
            (Key::Character("s"), Modifiers::LOGO | Modifiers::ALT),
        ] {
            let key_text = match key {
                Key::Named(key) => format!("{key:?}"),
                Key::Character(key) => key.to_owned(),
                Key::Unidentified => unreachable!(),
            };
            let captured = Shortcut::capture(&key_text, modifiers).unwrap();
            let parsed = Shortcut::parse(&captured.config_label()).unwrap().unwrap();
            assert_eq!(parsed, captured);
            assert!(parsed.matches(&key, modifiers));
            assert!(!parsed.matches(&key, modifiers ^ Modifiers::SHIFT));
        }
    }

    #[test]
    fn shortcut_chords_keep_control_and_super_distinct() {
        let ctrl = Shortcut::parse("Ctrl+S").unwrap().unwrap();
        let logo = Shortcut::parse("Super+S").unwrap().unwrap();
        assert!(!ctrl.conflicts(logo));
        assert!(ctrl.matches(&Key::Character("s"), Modifiers::CTRL));
        assert!(logo.matches(&Key::Character("s"), Modifiers::LOGO));
    }

    #[test]
    fn shortcut_aliases_and_legacy_delete_are_checked_for_overlap() {
        let combined = Shortcut::parse("Delete/Backspace").unwrap().unwrap();
        for name in ["Delete", "Backspace"] {
            assert!(combined.conflicts(Shortcut::parse(name).unwrap().unwrap()));
        }
        assert!(
            Shortcut::parse("Ctrl+Alt+F1")
                .unwrap()
                .unwrap()
                .conflicts(Shortcut::parse("alt+control+f1").unwrap().unwrap())
        );
    }

    #[test]
    fn shortcut_alternatives_validate_and_preserve_punctuation() {
        let shortcuts = parse_bindings("Ctrl+Alt+F1; Shift+Home; Semicolon; ctrl+alt+f1").unwrap();
        assert_eq!(shortcuts.len(), 3);
        assert_eq!(
            parse_bindings(&serialize_bindings(&shortcuts)).unwrap(),
            shortcuts
        );
        for invalid in ["Ctrl+Ctrl+S", "Ctrl+", "F25", "Hyper+S", "F1;", "F1;;F2"] {
            assert!(parse_bindings(invalid).is_err(), "{invalid}");
        }
    }
}
