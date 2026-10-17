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

/// Non-modifier named keys exposed by iced 0.14; every entry round-trips in shortcut config.
const NAMED_KEYS: &[Named] = &[
    Named::CapsLock,
    Named::NumLock,
    Named::ScrollLock,
    Named::Enter,
    Named::Tab,
    Named::Space,
    Named::ArrowDown,
    Named::ArrowLeft,
    Named::ArrowRight,
    Named::ArrowUp,
    Named::End,
    Named::Home,
    Named::PageDown,
    Named::PageUp,
    Named::Backspace,
    Named::Clear,
    Named::Copy,
    Named::CrSel,
    Named::Cut,
    Named::Delete,
    Named::EraseEof,
    Named::ExSel,
    Named::Insert,
    Named::Paste,
    Named::Redo,
    Named::Undo,
    Named::Accept,
    Named::Again,
    Named::Attn,
    Named::Cancel,
    Named::ContextMenu,
    Named::Escape,
    Named::Execute,
    Named::Find,
    Named::Help,
    Named::Pause,
    Named::Play,
    Named::Props,
    Named::Select,
    Named::ZoomIn,
    Named::ZoomOut,
    Named::BrightnessDown,
    Named::BrightnessUp,
    Named::Eject,
    Named::LogOff,
    Named::Power,
    Named::PowerOff,
    Named::PrintScreen,
    Named::Hibernate,
    Named::Standby,
    Named::WakeUp,
    Named::AllCandidates,
    Named::Alphanumeric,
    Named::CodeInput,
    Named::Compose,
    Named::Convert,
    Named::FinalMode,
    Named::GroupFirst,
    Named::GroupLast,
    Named::GroupNext,
    Named::GroupPrevious,
    Named::ModeChange,
    Named::NextCandidate,
    Named::NonConvert,
    Named::PreviousCandidate,
    Named::Process,
    Named::SingleCandidate,
    Named::HangulMode,
    Named::HanjaMode,
    Named::JunjaMode,
    Named::Eisu,
    Named::Hankaku,
    Named::Hiragana,
    Named::HiraganaKatakana,
    Named::KanaMode,
    Named::KanjiMode,
    Named::Katakana,
    Named::Romaji,
    Named::Zenkaku,
    Named::ZenkakuHankaku,
    Named::Soft1,
    Named::Soft2,
    Named::Soft3,
    Named::Soft4,
    Named::ChannelDown,
    Named::ChannelUp,
    Named::Close,
    Named::MailForward,
    Named::MailReply,
    Named::MailSend,
    Named::MediaClose,
    Named::MediaFastForward,
    Named::MediaPause,
    Named::MediaPlay,
    Named::MediaPlayPause,
    Named::MediaRecord,
    Named::MediaRewind,
    Named::MediaStop,
    Named::MediaTrackNext,
    Named::MediaTrackPrevious,
    Named::New,
    Named::Open,
    Named::Print,
    Named::Save,
    Named::SpellCheck,
    Named::Key11,
    Named::Key12,
    Named::AudioBalanceLeft,
    Named::AudioBalanceRight,
    Named::AudioBassBoostDown,
    Named::AudioBassBoostToggle,
    Named::AudioBassBoostUp,
    Named::AudioFaderFront,
    Named::AudioFaderRear,
    Named::AudioSurroundModeNext,
    Named::AudioTrebleDown,
    Named::AudioTrebleUp,
    Named::AudioVolumeDown,
    Named::AudioVolumeUp,
    Named::AudioVolumeMute,
    Named::MicrophoneToggle,
    Named::MicrophoneVolumeDown,
    Named::MicrophoneVolumeUp,
    Named::MicrophoneVolumeMute,
    Named::SpeechCorrectionList,
    Named::SpeechInputToggle,
    Named::LaunchApplication1,
    Named::LaunchApplication2,
    Named::LaunchCalendar,
    Named::LaunchContacts,
    Named::LaunchMail,
    Named::LaunchMediaPlayer,
    Named::LaunchMusicPlayer,
    Named::LaunchPhone,
    Named::LaunchScreenSaver,
    Named::LaunchSpreadsheet,
    Named::LaunchWebBrowser,
    Named::LaunchWebCam,
    Named::LaunchWordProcessor,
    Named::BrowserBack,
    Named::BrowserFavorites,
    Named::BrowserForward,
    Named::BrowserHome,
    Named::BrowserRefresh,
    Named::BrowserSearch,
    Named::BrowserStop,
    Named::AppSwitch,
    Named::Call,
    Named::Camera,
    Named::CameraFocus,
    Named::EndCall,
    Named::GoBack,
    Named::GoHome,
    Named::HeadsetHook,
    Named::LastNumberRedial,
    Named::Notification,
    Named::MannerMode,
    Named::VoiceDial,
    Named::TV,
    Named::TV3DMode,
    Named::TVAntennaCable,
    Named::TVAudioDescription,
    Named::TVAudioDescriptionMixDown,
    Named::TVAudioDescriptionMixUp,
    Named::TVContentsMenu,
    Named::TVDataService,
    Named::TVInput,
    Named::TVInputComponent1,
    Named::TVInputComponent2,
    Named::TVInputComposite1,
    Named::TVInputComposite2,
    Named::TVInputHDMI1,
    Named::TVInputHDMI2,
    Named::TVInputHDMI3,
    Named::TVInputHDMI4,
    Named::TVInputVGA1,
    Named::TVMediaContext,
    Named::TVNetwork,
    Named::TVNumberEntry,
    Named::TVPower,
    Named::TVRadioService,
    Named::TVSatellite,
    Named::TVSatelliteBS,
    Named::TVSatelliteCS,
    Named::TVSatelliteToggle,
    Named::TVTerrestrialAnalog,
    Named::TVTerrestrialDigital,
    Named::TVTimer,
    Named::AVRInput,
    Named::AVRPower,
    Named::ColorF0Red,
    Named::ColorF1Green,
    Named::ColorF2Yellow,
    Named::ColorF3Blue,
    Named::ColorF4Grey,
    Named::ColorF5Brown,
    Named::ClosedCaptionToggle,
    Named::Dimmer,
    Named::DisplaySwap,
    Named::DVR,
    Named::Exit,
    Named::FavoriteClear0,
    Named::FavoriteClear1,
    Named::FavoriteClear2,
    Named::FavoriteClear3,
    Named::FavoriteRecall0,
    Named::FavoriteRecall1,
    Named::FavoriteRecall2,
    Named::FavoriteRecall3,
    Named::FavoriteStore0,
    Named::FavoriteStore1,
    Named::FavoriteStore2,
    Named::FavoriteStore3,
    Named::Guide,
    Named::GuideNextDay,
    Named::GuidePreviousDay,
    Named::Info,
    Named::InstantReplay,
    Named::Link,
    Named::ListProgram,
    Named::LiveContent,
    Named::Lock,
    Named::MediaApps,
    Named::MediaAudioTrack,
    Named::MediaLast,
    Named::MediaSkipBackward,
    Named::MediaSkipForward,
    Named::MediaStepBackward,
    Named::MediaStepForward,
    Named::MediaTopMenu,
    Named::NavigateIn,
    Named::NavigateNext,
    Named::NavigateOut,
    Named::NavigatePrevious,
    Named::NextFavoriteChannel,
    Named::NextUserProfile,
    Named::OnDemand,
    Named::Pairing,
    Named::PinPDown,
    Named::PinPMove,
    Named::PinPToggle,
    Named::PinPUp,
    Named::PlaySpeedDown,
    Named::PlaySpeedReset,
    Named::PlaySpeedUp,
    Named::RandomToggle,
    Named::RcLowBattery,
    Named::RecordSpeedNext,
    Named::RfBypass,
    Named::ScanChannelsToggle,
    Named::ScreenModeNext,
    Named::Settings,
    Named::SplitScreenToggle,
    Named::STBInput,
    Named::STBPower,
    Named::Subtitle,
    Named::Teletext,
    Named::VideoModeNext,
    Named::Wink,
    Named::ZoomToggle,
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
    Named::F25,
    Named::F26,
    Named::F27,
    Named::F28,
    Named::F29,
    Named::F30,
    Named::F31,
    Named::F32,
    Named::F33,
    Named::F34,
    Named::F35,
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

    pub(super) const fn standard_character(key: char, modifiers: Modifiers) -> Self {
        Self {
            key: ShortcutKey::StandardCharacter(key),
            modifiers,
        }
    }

    pub(super) const fn named(key: Named, modifiers: Modifiers) -> Self {
        Self {
            key: ShortcutKey::Named(key),
            modifiers,
        }
    }

    #[cfg(test)]
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

    /// Convert the Main-section keyboard subset used by REAPER's `.ReaperKeyMap` files.
    /// REAPER writes Windows virtual-key codes and modifier bits even on Linux.
    pub(super) fn reaper_key_codes(self) -> Result<(u32, Vec<u32>), String> {
        let mut flags = 1; // REAPER's base KEY flag, present on every exported binding.
        if self.modifiers.shift() {
            flags |= 4;
        }
        if self.modifiers.contains(Modifiers::CTRL)
            || (!cfg!(target_os = "macos") && self.modifiers.contains(Modifiers::COMMAND))
        {
            flags |= 8;
        }
        if self.modifiers.alt() {
            flags |= 16;
        }
        if self.modifiers.contains(Modifiers::LOGO)
            || (cfg!(target_os = "macos") && self.modifiers.contains(Modifiers::COMMAND))
        {
            flags |= 32;
        }

        let codes = match self.key {
            ShortcutKey::Character(key) | ShortcutKey::StandardCharacter(key) => {
                vec![reaper_character_code(key, self.modifiers.shift())?]
            }
            ShortcutKey::Named(key) | ShortcutKey::StandardNamed(key) => {
                vec![reaper_named_key_code(key)?]
            }
            ShortcutKey::NumPad(code) => vec![reaper_numpad_key_code(code)?],
            ShortcutKey::DeleteBackspace => vec![8, 46],
        };
        Ok((flags, codes))
    }

    pub(super) fn from_reaper_key(flags: u32, code: u32) -> Result<Self, String> {
        const BASE_FLAG: u32 = 1;
        const SHIFT_FLAG: u32 = 4;
        const CTRL_FLAG: u32 = 8;
        const ALT_FLAG: u32 = 16;
        const LOGO_FLAG: u32 = 32;
        const SUPPORTED_FLAGS: u32 = BASE_FLAG | SHIFT_FLAG | CTRL_FLAG | ALT_FLAG | LOGO_FLAG;
        if flags & BASE_FLAG == 0 || flags & !SUPPORTED_FLAGS != 0 {
            return Err(format!("unsupported REAPER key flags: {flags}"));
        }

        let mut modifiers = Modifiers::NONE;
        if flags & SHIFT_FLAG != 0 {
            modifiers.insert(Modifiers::SHIFT);
        }
        if flags & CTRL_FLAG != 0 {
            modifiers.insert(Modifiers::CTRL);
        }
        if flags & ALT_FLAG != 0 {
            modifiers.insert(Modifiers::ALT);
        }
        if flags & LOGO_FLAG != 0 {
            modifiers.insert(Modifiers::LOGO);
        }

        let key = reaper_key(code, flags & SHIFT_FLAG != 0)?;
        Ok(Self { key, modifiers })
    }
}

fn reaper_character_code(key: char, shifted: bool) -> Result<u32, String> {
    if key.is_ascii_alphabetic() {
        return Ok(key.to_ascii_uppercase() as u32);
    }
    if key.is_ascii_digit() {
        return Ok(key as u32);
    }
    let (code, requires_shift) = match key {
        ' ' => return Ok(32),
        '-' => (189, false),
        '_' => (189, true),
        '=' => (187, false),
        '+' => (187, true),
        '[' => (219, false),
        '{' => (219, true),
        ']' => (221, false),
        '}' => (221, true),
        ';' => (186, false),
        ':' => (186, true),
        '\'' => (222, false),
        '"' => (222, true),
        ',' => (188, false),
        '<' => (188, true),
        '.' => (190, false),
        '>' => (190, true),
        '/' => (191, false),
        '?' => (191, true),
        '\\' => (220, false),
        '|' => (220, true),
        '`' => (192, false),
        '~' => (192, true),
        '!' => (49, true),
        '@' => (50, true),
        '#' => (51, true),
        '$' => (52, true),
        '%' => (53, true),
        '^' => (54, true),
        '&' => (55, true),
        '*' => (56, true),
        '(' => (57, true),
        ')' => (48, true),
        _ => return Err(format!("unsupported REAPER virtual key for {key:?}")),
    };
    if shifted != requires_shift {
        return Err(format!(
            "REAPER key map cannot preserve {key:?} with this Shift state"
        ));
    }
    Ok(code)
}

fn reaper_named_key_code(key: Named) -> Result<u32, String> {
    let code = match key {
        Named::Backspace => 8,
        Named::Tab => 9,
        Named::Enter => 13,
        Named::Pause => 19,
        Named::CapsLock => 20,
        Named::Escape => 27,
        Named::Space => 32,
        Named::PageUp => 33,
        Named::PageDown => 34,
        Named::End => 35,
        Named::Home => 36,
        Named::ArrowLeft => 37,
        Named::ArrowUp => 38,
        Named::ArrowRight => 39,
        Named::ArrowDown => 40,
        Named::PrintScreen => 44,
        Named::Insert => 45,
        Named::Delete => 46,
        Named::NumLock => 144,
        Named::ScrollLock => 145,
        Named::ContextMenu => 93,
        Named::F1 => 112,
        Named::F2 => 113,
        Named::F3 => 114,
        Named::F4 => 115,
        Named::F5 => 116,
        Named::F6 => 117,
        Named::F7 => 118,
        Named::F8 => 119,
        Named::F9 => 120,
        Named::F10 => 121,
        Named::F11 => 122,
        Named::F12 => 123,
        Named::F13 => 124,
        Named::F14 => 125,
        Named::F15 => 126,
        Named::F16 => 127,
        Named::F17 => 128,
        Named::F18 => 129,
        Named::F19 => 130,
        Named::F20 => 131,
        Named::F21 => 132,
        Named::F22 => 133,
        Named::F23 => 134,
        Named::F24 => 135,
        Named::Clear => 12,
        _ => return Err(format!("unsupported REAPER named key: {key:?}")),
    };
    Ok(code)
}

fn reaper_numpad_key_code(code: Code) -> Result<u32, String> {
    let code = match code {
        Code::Numpad0 => 96,
        Code::Numpad1 => 97,
        Code::Numpad2 => 98,
        Code::Numpad3 => 99,
        Code::Numpad4 => 100,
        Code::Numpad5 => 101,
        Code::Numpad6 => 102,
        Code::Numpad7 => 103,
        Code::Numpad8 => 104,
        Code::Numpad9 => 105,
        Code::NumpadAdd => 107,
        Code::NumpadSubtract => 109,
        Code::NumpadMultiply => 106,
        Code::NumpadDivide => 111,
        Code::NumpadDecimal => 110,
        _ => return Err(format!("unsupported REAPER numeric keypad key: {code:?}")),
    };
    Ok(code)
}

fn reaper_key(code: u32, shifted: bool) -> Result<ShortcutKey, String> {
    if (65..=90).contains(&code) {
        return Ok(ShortcutKey::Character(
            (code as u8 as char).to_ascii_lowercase(),
        ));
    }
    if (48..=57).contains(&code) {
        let base = code as u8 as char;
        let key = if shifted {
            match base {
                '1' => '!',
                '2' => '@',
                '3' => '#',
                '4' => '$',
                '5' => '%',
                '6' => '^',
                '7' => '&',
                '8' => '*',
                '9' => '(',
                '0' => ')',
                _ => unreachable!(),
            }
        } else {
            base
        };
        return Ok(ShortcutKey::Character(key));
    }
    if (96..=105).contains(&code) {
        return Ok(ShortcutKey::NumPad(match code {
            96 => Code::Numpad0,
            97 => Code::Numpad1,
            98 => Code::Numpad2,
            99 => Code::Numpad3,
            100 => Code::Numpad4,
            101 => Code::Numpad5,
            102 => Code::Numpad6,
            103 => Code::Numpad7,
            104 => Code::Numpad8,
            105 => Code::Numpad9,
            _ => unreachable!(),
        }));
    }
    if matches!(code, 106 | 107 | 109 | 110 | 111) {
        return Ok(ShortcutKey::NumPad(match code {
            106 => Code::NumpadMultiply,
            107 => Code::NumpadAdd,
            109 => Code::NumpadSubtract,
            110 => Code::NumpadDecimal,
            111 => Code::NumpadDivide,
            _ => unreachable!(),
        }));
    }
    if (112..=135).contains(&code) {
        return Ok(ShortcutKey::Named(match code {
            112 => Named::F1,
            113 => Named::F2,
            114 => Named::F3,
            115 => Named::F4,
            116 => Named::F5,
            117 => Named::F6,
            118 => Named::F7,
            119 => Named::F8,
            120 => Named::F9,
            121 => Named::F10,
            122 => Named::F11,
            123 => Named::F12,
            124 => Named::F13,
            125 => Named::F14,
            126 => Named::F15,
            127 => Named::F16,
            128 => Named::F17,
            129 => Named::F18,
            130 => Named::F19,
            131 => Named::F20,
            132 => Named::F21,
            133 => Named::F22,
            134 => Named::F23,
            135 => Named::F24,
            _ => unreachable!(),
        }));
    }
    if matches!(
        code,
        186 | 187 | 188 | 189 | 190 | 191 | 192 | 219 | 220 | 221 | 222
    ) {
        return Ok(ShortcutKey::Character(match code {
            186 => {
                if shifted {
                    ':'
                } else {
                    ';'
                }
            }
            187 => {
                if shifted {
                    '+'
                } else {
                    '='
                }
            }
            188 => {
                if shifted {
                    '<'
                } else {
                    ','
                }
            }
            189 => {
                if shifted {
                    '_'
                } else {
                    '-'
                }
            }
            190 => {
                if shifted {
                    '>'
                } else {
                    '.'
                }
            }
            191 => {
                if shifted {
                    '?'
                } else {
                    '/'
                }
            }
            192 => {
                if shifted {
                    '~'
                } else {
                    '`'
                }
            }
            219 => {
                if shifted {
                    '{'
                } else {
                    '['
                }
            }
            220 => {
                if shifted {
                    '|'
                } else {
                    '\\'
                }
            }
            221 => {
                if shifted {
                    '}'
                } else {
                    ']'
                }
            }
            222 => {
                if shifted {
                    '"'
                } else {
                    '\''
                }
            }
            _ => unreachable!(),
        }));
    }

    let named = match code {
        8 => Named::Backspace,
        9 => Named::Tab,
        12 => Named::Clear,
        13 => Named::Enter,
        19 => Named::Pause,
        20 => Named::CapsLock,
        27 => Named::Escape,
        32 => Named::Space,
        33 => Named::PageUp,
        34 => Named::PageDown,
        35 => Named::End,
        36 => Named::Home,
        37 => Named::ArrowLeft,
        38 => Named::ArrowUp,
        39 => Named::ArrowRight,
        40 => Named::ArrowDown,
        44 => Named::PrintScreen,
        45 => Named::Insert,
        46 => Named::Delete,
        93 => Named::ContextMenu,
        144 => Named::NumLock,
        145 => Named::ScrollLock,
        _ => return Err(format!("unsupported REAPER virtual-key code: {code}")),
    };
    Ok(ShortcutKey::Named(named))
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
    fn every_non_modifier_named_key_captures_matches_and_round_trips() {
        let modifiers = Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT | Modifiers::LOGO;
        for &key in NAMED_KEYS {
            let input = ShortcutInput {
                logical_key: Key::Named(key),
                physical_key: Physical::Unidentified(iced::keyboard::key::NativeCode::Unidentified),
                location: Location::Standard,
                modifiers,
            };
            let shortcut = Shortcut::capture_input(&input)
                .unwrap_or_else(|error| panic!("{key:?} should be capturable: {error}"));
            assert!(shortcut.matches_input(&input), "{key:?} should match");
            let label = shortcut.config_label();
            assert_eq!(
                Shortcut::parse(&label).unwrap().unwrap(),
                shortcut,
                "{key:?} should round-trip as {label}"
            );
        }

        for key in [
            Named::Alt,
            Named::AltGraph,
            Named::Control,
            Named::Fn,
            Named::FnLock,
            Named::Shift,
            Named::Symbol,
            Named::SymbolLock,
            Named::Meta,
            Named::Hyper,
            Named::Super,
        ] {
            let input = ShortcutInput {
                logical_key: Key::Named(key),
                physical_key: Physical::Unidentified(iced::keyboard::key::NativeCode::Unidentified),
                location: Location::Standard,
                modifiers: Modifiers::NONE,
            };
            assert!(
                Shortcut::capture_input(&input).is_err(),
                "modifier key {key:?} must not be treated as a binding key"
            );
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
        for invalid in ["Ctrl+Ctrl+S", "Ctrl+", "F36", "Hyper+S", "F1;", "F1;;F2"] {
            assert!(parse_bindings(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn reaper_virtual_keys_round_trip_main_keyboard_modifiers_and_numpad() {
        for value in [
            "F11",
            "Shift+F11",
            "Ctrl+F11",
            "Alt+F11",
            "Super+F11",
            "Ctrl+Alt+Shift+F12",
            "Shift+Plus",
            "Mod+Z",
            "NumPad7",
            "NumPadAdd",
        ] {
            let shortcut = Shortcut::parse(value).unwrap().unwrap();
            let (flags, codes) = shortcut.reaper_key_codes().unwrap();
            assert_eq!(codes.len(), 1, "{value}");
            assert_eq!(
                Shortcut::from_reaper_key(flags, codes[0]).unwrap(),
                shortcut,
                "{value}"
            );
        }
        let (flags, codes) = Shortcut::delete_backspace().reaper_key_codes().unwrap();
        assert_eq!(flags, 1);
        assert_eq!(codes, [8, 46]);
    }

    #[test]
    fn reaper_virtual_key_conversion_rejects_unsupported_scope_bits_and_layout_keys() {
        for (flags, code) in [(0, 90), (3, 90), (64 | 1, 90), (1, 255)] {
            assert!(Shortcut::from_reaper_key(flags, code).is_err());
        }
        assert!(
            Shortcut::parse("F35")
                .unwrap()
                .unwrap()
                .reaper_key_codes()
                .is_err()
        );
    }
}
