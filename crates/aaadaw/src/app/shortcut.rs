use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{Key, Location, Modifiers};

/// Preserve the full event identity until capture and dispatch have resolved it.
#[derive(Debug, Clone)]
pub(crate) struct ShortcutInput {
    pub(super) logical_key: Key,
    pub(super) physical_key: Physical,
    pub(super) location: Location,
    pub(super) modifiers: Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShortcutKey {
    Character(char),
    Named(Named),
    StandardCharacter(char),
    StandardNamed(Named),
    NumPad(Code),
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

const NUMPAD_KEYS: &[Code] = &[
    Code::Numpad0,
    Code::Numpad1,
    Code::Numpad2,
    Code::Numpad3,
    Code::Numpad4,
    Code::Numpad5,
    Code::Numpad6,
    Code::Numpad7,
    Code::Numpad8,
    Code::Numpad9,
    Code::NumpadDecimal,
    Code::NumpadAdd,
    Code::NumpadSubtract,
    Code::NumpadMultiply,
    Code::NumpadDivide,
    Code::NumpadEnter,
    Code::NumpadEqual,
    Code::NumpadComma,
];

fn numpad_code(input: &ShortcutInput) -> Option<Code> {
    match input.physical_key {
        Physical::Code(code) if NUMPAD_KEYS.contains(&code) => Some(code),
        Physical::Unidentified(_) if input.location == Location::Numpad => {
            let mut candidates = NUMPAD_KEYS
                .iter()
                .copied()
                .filter(|code| numpad_logical_matches(*code, &input.logical_key.as_ref()));
            let candidate = candidates.next()?;
            // Decimal and comma can produce the same text; do not invent identity.
            candidates.next().is_none().then_some(candidate)
        }
        _ => None,
    }
}

/// Logical aliases also cover NumLock-off navigation and decimal locales.
fn numpad_logical_matches(code: Code, key: &Key<&str>) -> bool {
    match code {
        Code::Numpad0 => matches!(key, Key::Character("0") | Key::Named(Named::Insert)),
        Code::Numpad1 => matches!(key, Key::Character("1") | Key::Named(Named::End)),
        Code::Numpad2 => matches!(key, Key::Character("2") | Key::Named(Named::ArrowDown)),
        Code::Numpad3 => matches!(key, Key::Character("3") | Key::Named(Named::PageDown)),
        Code::Numpad4 => matches!(key, Key::Character("4") | Key::Named(Named::ArrowLeft)),
        Code::Numpad5 => matches!(key, Key::Character("5") | Key::Named(Named::Clear)),
        Code::Numpad6 => matches!(key, Key::Character("6") | Key::Named(Named::ArrowRight)),
        Code::Numpad7 => matches!(key, Key::Character("7") | Key::Named(Named::Home)),
        Code::Numpad8 => matches!(key, Key::Character("8") | Key::Named(Named::ArrowUp)),
        Code::Numpad9 => matches!(key, Key::Character("9") | Key::Named(Named::PageUp)),
        Code::NumpadDecimal => matches!(key, Key::Character("." | ",") | Key::Named(Named::Delete)),
        Code::NumpadAdd => matches!(key, Key::Character("+")),
        Code::NumpadSubtract => matches!(key, Key::Character("-")),
        Code::NumpadMultiply => matches!(key, Key::Character("*")),
        Code::NumpadDivide => matches!(key, Key::Character("/")),
        Code::NumpadEnter => matches!(key, Key::Named(Named::Enter)),
        Code::NumpadEqual => matches!(key, Key::Character("=")),
        Code::NumpadComma => matches!(key, Key::Character(",")),
        _ => false,
    }
}

fn logical_key(key: ShortcutKey) -> Option<Key> {
    match key {
        ShortcutKey::Character(c) | ShortcutKey::StandardCharacter(c) => {
            Some(Key::Character(c.to_string().into()))
        }
        ShortcutKey::Named(n) | ShortcutKey::StandardNamed(n) => Some(Key::Named(n)),
        _ => None,
    }
}

fn is_standard(key: ShortcutKey) -> bool {
    matches!(
        key,
        ShortcutKey::StandardCharacter(_) | ShortcutKey::StandardNamed(_)
    )
}

fn character_label(key: char) -> String {
    match key {
        '+' => "Plus".to_owned(),
        ';' => "Semicolon".to_owned(),
        _ => key.to_ascii_uppercase().to_string(),
    }
}

impl Shortcut {
    pub(super) const fn character(key: char, modifiers: Modifiers) -> Self {
        Self {
            key: ShortcutKey::Character(key),
            modifiers,
        }
    }

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

    pub(super) fn capture_input(input: &ShortcutInput) -> Result<Self, String> {
        if let Some(code) = numpad_code(input) {
            return Ok(Self {
                key: ShortcutKey::NumPad(code),
                modifiers: input.modifiers,
            });
        }
        if input.location == Location::Numpad {
            return Err("Unsupported numeric keypad key".to_owned());
        }
        let text = match input.logical_key.as_ref() {
            Key::Character(value) => value.to_owned(),
            Key::Named(value) => format!("{value:?}"),
            Key::Unidentified => return Err("Unsupported unidentified shortcut key".to_owned()),
        };
        let mut shortcut = Self::capture(&text, input.modifiers)?;
        // Only ambiguous logical keys need a location-qualified new binding.
        if NUMPAD_KEYS
            .iter()
            .any(|code| numpad_logical_matches(*code, &input.logical_key.as_ref()))
        {
            shortcut.key = match shortcut.key {
                ShortcutKey::Character(c) => ShortcutKey::StandardCharacter(c),
                ShortcutKey::Named(n) => ShortcutKey::StandardNamed(n),
                other => other,
            };
        }
        Ok(shortcut)
    }

    pub(super) fn matches_input(self, input: &ShortcutInput) -> bool {
        if self.modifiers != input.modifiers {
            return false;
        }
        match self.key {
            ShortcutKey::NumPad(expected) => numpad_code(input) == Some(expected),
            ShortcutKey::StandardCharacter(_) | ShortcutKey::StandardNamed(_) => {
                input.location != Location::Numpad
                    && numpad_code(input).is_none()
                    && self.matches(&input.logical_key.as_ref(), input.modifiers)
            }
            _ => self.matches(&input.logical_key.as_ref(), input.modifiers),
        }
    }

    pub(super) fn config_label(self) -> String {
        let mut parts = modifier_labels(self.modifiers);
        parts.push(match self.key {
            ShortcutKey::Character(key) => character_label(key),
            ShortcutKey::StandardCharacter(key) => format!("Standard{}", character_label(key)),
            ShortcutKey::StandardNamed(key) => format!("Standard{key:?}"),
            ShortcutKey::NumPad(code) => {
                format!("NumPad{}", format!("{code:?}").trim_start_matches("Numpad"))
            }
            ShortcutKey::Named(key) => format!("{key:?}"),
            ShortcutKey::DeleteBackspace => "Delete/Backspace".to_owned(),
        });
        parts.join("+")
    }

    pub(super) fn matches(self, key: &Key<&str>, modifiers: Modifiers) -> bool {
        self.modifiers == modifiers
            && match self.key {
                ShortcutKey::Character(expected) | ShortcutKey::StandardCharacter(expected) => {
                    matches!(key, Key::Character(value) if value.eq_ignore_ascii_case(&expected.to_string()))
                }
                ShortcutKey::Named(expected) | ShortcutKey::StandardNamed(expected) => {
                    *key == Key::Named(expected)
                }
                ShortcutKey::NumPad(_) => false,
                ShortcutKey::DeleteBackspace => {
                    matches!(key, Key::Named(Named::Delete | Named::Backspace))
                }
            }
    }

    pub(super) fn conflicts(self, other: Self) -> bool {
        if self.modifiers != other.modifiers {
            return false;
        }
        if self.key == other.key {
            return true;
        }
        if let (Some(left), Some(right)) = (logical_key(self.key), logical_key(other.key)) {
            return left == right;
        }
        for (left, right) in [(self.key, other.key), (other.key, self.key)] {
            if let ShortcutKey::NumPad(code) = left
                && !is_standard(right)
            {
                if let Some(key) = logical_key(right)
                    && numpad_logical_matches(code, &key.as_ref())
                {
                    return true;
                }
                if right == ShortcutKey::DeleteBackspace && code == Code::NumpadDecimal {
                    return true;
                }
            }
            if left == ShortcutKey::DeleteBackspace
                && matches!(
                    right,
                    ShortcutKey::Named(Named::Delete | Named::Backspace)
                        | ShortcutKey::StandardNamed(Named::Delete | Named::Backspace)
                )
            {
                return true;
            }
        }
        false
    }

    pub(super) fn is_reserved(self) -> bool {
        matches!(
            self.key,
            ShortcutKey::Named(Named::Escape) | ShortcutKey::StandardNamed(Named::Escape)
        ) && self.modifiers == Modifiers::NONE
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
    if value
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("standard"))
    {
        return match parse_key(&value[8..])? {
            ShortcutKey::Character(c) => Ok(ShortcutKey::StandardCharacter(c)),
            ShortcutKey::Named(n) => Ok(ShortcutKey::StandardNamed(n)),
            _ => Err(format!("Unsupported standard key: {value}")),
        };
    }
    if let Some(code) = NUMPAD_KEYS
        .iter()
        .find(|code| format!("{code:?}").eq_ignore_ascii_case(value))
    {
        return Ok(ShortcutKey::NumPad(*code));
    }
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

    fn input(key: Key, code: Code, location: Location) -> ShortcutInput {
        ShortcutInput {
            logical_key: key,
            physical_key: Physical::Code(code),
            location,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn numeric_keypad_and_standard_bindings_preserve_distinct_identity() {
        let standard = input(Key::Character("7".into()), Code::Digit7, Location::Standard);
        let numeric = input(Key::Character("7".into()), Code::Numpad7, Location::Numpad);
        let numlock_off = input(Key::Named(Named::Home), Code::Numpad7, Location::Numpad);
        let standard_binding = Shortcut::capture_input(&standard).unwrap();
        let numeric_binding = Shortcut::capture_input(&numeric).unwrap();
        assert_eq!(standard_binding.config_label(), "Standard7");
        assert_eq!(numeric_binding.config_label(), "NumPad7");
        assert_eq!(
            Shortcut::parse(&standard_binding.config_label())
                .unwrap()
                .unwrap(),
            standard_binding
        );
        let alternatives = vec![standard_binding, numeric_binding];
        assert_eq!(
            parse_bindings(&serialize_bindings(&alternatives)).unwrap(),
            alternatives
        );
        assert_eq!(
            Shortcut::capture_input(&numlock_off).unwrap(),
            numeric_binding
        );
        assert!(standard_binding.matches_input(&standard));
        assert!(!standard_binding.matches_input(&numeric));
        assert!(!standard_binding.matches_input(&numlock_off));
        assert!(numeric_binding.matches_input(&numeric));
        assert!(numeric_binding.matches_input(&numlock_off));
        assert!(!numeric_binding.matches_input(&standard));
        assert!(!standard_binding.conflicts(numeric_binding));
        let mut no_scancode = numeric.clone();
        no_scancode.physical_key =
            Physical::Unidentified(iced::keyboard::key::NativeCode::Unidentified);
        assert_eq!(
            Shortcut::capture_input(&no_scancode).unwrap(),
            numeric_binding
        );
        assert!(numeric_binding.matches_input(&no_scancode));
        let old = Shortcut::parse("7").unwrap().unwrap();
        assert!(old.matches_input(&standard));
        assert!(old.matches_input(&numeric));
        assert!(!old.matches_input(&numlock_off));
        assert!(old.conflicts(standard_binding));
        assert!(old.conflicts(numeric_binding));
        assert!(
            Shortcut::parse("Home")
                .unwrap()
                .unwrap()
                .conflicts(numeric_binding)
        );
        assert!(
            !Shortcut::parse("StandardHome")
                .unwrap()
                .unwrap()
                .conflicts(numeric_binding)
        );
    }

    #[test]
    fn numeric_keypad_enter_is_distinct_and_legacy_enter_keeps_old_behavior() {
        let standard = input(Key::Named(Named::Enter), Code::Enter, Location::Standard);
        let numeric = input(
            Key::Named(Named::Enter),
            Code::NumpadEnter,
            Location::Numpad,
        );
        let standard_binding = Shortcut::capture_input(&standard).unwrap();
        let numeric_binding = Shortcut::capture_input(&numeric).unwrap();
        assert_eq!(standard_binding.config_label(), "StandardEnter");
        assert_eq!(numeric_binding.config_label(), "NumPadEnter");
        assert!(!standard_binding.matches_input(&numeric));
        assert!(!numeric_binding.matches_input(&standard));
        assert!(!standard_binding.conflicts(numeric_binding));
        let legacy = Shortcut::parse("Enter").unwrap().unwrap();
        assert!(legacy.matches_input(&standard));
        assert!(legacy.matches_input(&numeric));
        assert!(legacy.conflicts(standard_binding));
        assert!(legacy.conflicts(numeric_binding));
    }

    #[test]
    fn all_supported_numeric_keypad_keys_capture_match_and_round_trip() {
        for (code, key) in [
            (Code::Numpad0, Key::Character("0")),
            (Code::Numpad1, Key::Character("1")),
            (Code::Numpad2, Key::Character("2")),
            (Code::Numpad3, Key::Character("3")),
            (Code::Numpad4, Key::Character("4")),
            (Code::Numpad5, Key::Character("5")),
            (Code::Numpad6, Key::Character("6")),
            (Code::Numpad7, Key::Character("7")),
            (Code::Numpad8, Key::Character("8")),
            (Code::Numpad9, Key::Character("9")),
            (Code::NumpadDecimal, Key::Character(".")),
            (Code::NumpadAdd, Key::Character("+")),
            (Code::NumpadSubtract, Key::Character("-")),
            (Code::NumpadMultiply, Key::Character("*")),
            (Code::NumpadDivide, Key::Character("/")),
            (Code::NumpadEqual, Key::Character("=")),
            (Code::NumpadComma, Key::Character(",")),
            (Code::NumpadEnter, Key::Named(Named::Enter)),
        ] {
            let owned = match key {
                Key::Character(c) => Key::Character(c.into()),
                Key::Named(n) => Key::Named(n),
                Key::Unidentified => unreachable!(),
            };
            let mut event = input(owned, code, Location::Numpad);
            event.modifiers = Modifiers::CTRL | Modifiers::ALT;
            let binding = Shortcut::capture_input(&event).unwrap();
            assert!(binding.matches_input(&event));
            assert_eq!(
                Shortcut::parse(&binding.config_label()).unwrap().unwrap(),
                binding
            );
            event.modifiers = Modifiers::NONE;
            assert!(!binding.matches_input(&event));
        }
        let deletion = input(
            Key::Named(Named::Delete),
            Code::NumpadDecimal,
            Location::Numpad,
        );
        let binding = Shortcut::capture_input(&deletion).unwrap();
        assert_eq!(binding.config_label(), "NumPadDecimal");
        assert!(binding.conflicts(Shortcut::delete_backspace()));
        assert!(!binding.conflicts(Shortcut::parse("StandardDelete").unwrap().unwrap()));
    }

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
