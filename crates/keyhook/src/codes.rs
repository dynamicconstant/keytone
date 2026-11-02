//! The canonical key vocabulary (W3C `KeyboardEvent.code`) and key groups.

/// Groups let a sound pack say "all modifiers sound like X" without listing
/// every key. Resolution is: specific key -> group -> pack default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyGroup {
    Letters,
    Digits,
    Punctuation,
    Modifiers,
    Space,
    Enter,
    Backspace,
    Tab,
    Escape,
    Arrows,
    Navigation,
    Function,
    Numpad,
    Other,
}

pub const ALL_GROUPS: &[KeyGroup] = &[
    KeyGroup::Letters,
    KeyGroup::Digits,
    KeyGroup::Punctuation,
    KeyGroup::Modifiers,
    KeyGroup::Space,
    KeyGroup::Enter,
    KeyGroup::Backspace,
    KeyGroup::Tab,
    KeyGroup::Escape,
    KeyGroup::Arrows,
    KeyGroup::Navigation,
    KeyGroup::Function,
    KeyGroup::Numpad,
    KeyGroup::Other,
];

impl KeyGroup {
    pub fn name(self) -> &'static str {
        match self {
            KeyGroup::Letters => "letters",
            KeyGroup::Digits => "digits",
            KeyGroup::Punctuation => "punctuation",
            KeyGroup::Modifiers => "modifiers",
            KeyGroup::Space => "space",
            KeyGroup::Enter => "enter",
            KeyGroup::Backspace => "backspace",
            KeyGroup::Tab => "tab",
            KeyGroup::Escape => "escape",
            KeyGroup::Arrows => "arrows",
            KeyGroup::Navigation => "navigation",
            KeyGroup::Function => "function",
            KeyGroup::Numpad => "numpad",
            KeyGroup::Other => "other",
        }
    }

    pub fn from_name(s: &str) -> Option<KeyGroup> {
        ALL_GROUPS.iter().copied().find(|g| g.name() == s)
    }
}

/// Every key code this library can emit.
const CODES: &[&str] = &[
    // letters
    "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK",
    "KeyL", "KeyM", "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV",
    "KeyW", "KeyX", "KeyY", "KeyZ",
    // digits
    "Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8",
    "Digit9",
    // punctuation
    "Minus", "Equal", "BracketLeft", "BracketRight", "Backslash", "Semicolon", "Quote",
    "Backquote", "Comma", "Period", "Slash", "IntlBackslash",
    // modifiers
    "ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight", "AltLeft", "AltRight", "MetaLeft",
    "MetaRight", "CapsLock", "Fn",
    // singles
    "Space", "Enter", "Backspace", "Delete", "Tab", "Escape",
    // arrows & navigation
    "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown",
    "Insert",
    // function
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14",
    "F15", "F16", "F17", "F18", "F19", "F20",
    // numpad
    "Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6", "Numpad7",
    "Numpad8", "Numpad9", "NumpadAdd", "NumpadSubtract", "NumpadMultiply", "NumpadDivide",
    "NumpadDecimal", "NumpadEnter", "NumpadEqual", "NumLock",
    // other
    "PrintScreen", "ScrollLock", "Pause", "ContextMenu", "Help", "AudioVolumeUp",
    "AudioVolumeDown", "AudioVolumeMute",
];

pub fn all_codes() -> &'static [&'static str] {
    CODES
}

/// Intern an arbitrary string as one of our static codes.
pub fn intern(code: &str) -> Option<&'static str> {
    CODES.iter().copied().find(|c| *c == code)
}

pub fn group_of(code: &str) -> KeyGroup {
    match code {
        c if c.starts_with("Key") => KeyGroup::Letters,
        c if c.starts_with("Digit") => KeyGroup::Digits,
        c if c.starts_with("Numpad") || c == "NumLock" => {
            if c == "NumpadEnter" {
                KeyGroup::Enter
            } else {
                KeyGroup::Numpad
            }
        }
        "Minus" | "Equal" | "BracketLeft" | "BracketRight" | "Backslash" | "Semicolon"
        | "Quote" | "Backquote" | "Comma" | "Period" | "Slash" | "IntlBackslash" => {
            KeyGroup::Punctuation
        }
        "ShiftLeft" | "ShiftRight" | "ControlLeft" | "ControlRight" | "AltLeft" | "AltRight"
        | "MetaLeft" | "MetaRight" | "CapsLock" | "Fn" => KeyGroup::Modifiers,
        "Space" => KeyGroup::Space,
        "Enter" => KeyGroup::Enter,
        "Backspace" | "Delete" => KeyGroup::Backspace,
        "Tab" => KeyGroup::Tab,
        "Escape" => KeyGroup::Escape,
        "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" => KeyGroup::Arrows,
        "Home" | "End" | "PageUp" | "PageDown" | "Insert" => KeyGroup::Navigation,
        c if c.len() <= 3 && c.starts_with('F') && c[1..].chars().all(|d| d.is_ascii_digit()) => {
            KeyGroup::Function
        }
        _ => KeyGroup::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups() {
        assert_eq!(group_of("KeyQ"), KeyGroup::Letters);
        assert_eq!(group_of("Digit7"), KeyGroup::Digits);
        assert_eq!(group_of("NumpadEnter"), KeyGroup::Enter);
        assert_eq!(group_of("Numpad4"), KeyGroup::Numpad);
        assert_eq!(group_of("F11"), KeyGroup::Function);
        assert_eq!(group_of("Fn"), KeyGroup::Modifiers);
        assert_eq!(group_of("Delete"), KeyGroup::Backspace);
        assert_eq!(group_of("PrintScreen"), KeyGroup::Other);
    }

    #[test]
    fn group_names_roundtrip() {
        for g in ALL_GROUPS {
            assert_eq!(KeyGroup::from_name(g.name()), Some(*g));
        }
    }

    #[test]
    fn codes_unique() {
        let mut v = CODES.to_vec();
        v.sort();
        v.dedup();
        assert_eq!(v.len(), CODES.len());
    }
}
