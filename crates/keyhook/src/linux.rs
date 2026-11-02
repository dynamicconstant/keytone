//! Linux backend: X11 XRecord via `rdev`.
//!
//! Under a pure Wayland session there is no way for an unprivileged app to
//! observe global key events, so we report `Unsupported`. Under XWayland,
//! only X11 applications' key events are visible.

use crate::{HookError, KeyEvent, Dir};
use rdev::{EventType, Key};

pub fn start<F>(callback: F) -> Result<(), HookError>
where
    F: Fn(KeyEvent) + Send + Sync + 'static,
{
    if std::env::var_os("DISPLAY").is_none() {
        return Err(HookError::Unsupported(
            if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                "Wayland sessions block global key listeners; log in with an X11 session".into()
            } else {
                "no X11 display found".into()
            },
        ));
    }

    crate::spawn_and_wait("keyhook-x11", move |ready| {
        // rdev::listen blocks forever on success and returns only on failure.
        // Report success optimistically after a short grace period.
        let ready2 = ready.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let _ = ready2.send(Ok(()));
        });
        let res = rdev::listen(move |ev| {
            let (key, dir) = match ev.event_type {
                EventType::KeyPress(k) => (k, Dir::Down),
                EventType::KeyRelease(k) => (k, Dir::Up),
                _ => return,
            };
            if let Some(code) = map_key(key) {
                callback(KeyEvent { code, dir });
            }
        });
        if let Err(e) = res {
            let _ = ready.send(Err(HookError::Other(format!("X11 listen failed: {e:?}"))));
        }
    })
}

fn map_key(k: Key) -> Option<&'static str> {
    use Key::*;
    Some(match k {
        KeyA => "KeyA", KeyB => "KeyB", KeyC => "KeyC", KeyD => "KeyD", KeyE => "KeyE",
        KeyF => "KeyF", KeyG => "KeyG", KeyH => "KeyH", KeyI => "KeyI", KeyJ => "KeyJ",
        KeyK => "KeyK", KeyL => "KeyL", KeyM => "KeyM", KeyN => "KeyN", KeyO => "KeyO",
        KeyP => "KeyP", KeyQ => "KeyQ", KeyR => "KeyR", KeyS => "KeyS", KeyT => "KeyT",
        KeyU => "KeyU", KeyV => "KeyV", KeyW => "KeyW", KeyX => "KeyX", KeyY => "KeyY",
        KeyZ => "KeyZ",
        Num0 => "Digit0", Num1 => "Digit1", Num2 => "Digit2", Num3 => "Digit3",
        Num4 => "Digit4", Num5 => "Digit5", Num6 => "Digit6", Num7 => "Digit7",
        Num8 => "Digit8", Num9 => "Digit9",
        Minus => "Minus", Equal => "Equal", LeftBracket => "BracketLeft",
        RightBracket => "BracketRight", BackSlash => "Backslash", SemiColon => "Semicolon",
        Quote => "Quote", BackQuote => "Backquote", Comma => "Comma", Dot => "Period",
        Slash => "Slash", IntlBackslash => "IntlBackslash",
        ShiftLeft => "ShiftLeft", ShiftRight => "ShiftRight", ControlLeft => "ControlLeft",
        ControlRight => "ControlRight", Alt => "AltLeft", AltGr => "AltRight",
        MetaLeft => "MetaLeft", MetaRight => "MetaRight", CapsLock => "CapsLock",
        Function => "Fn",
        Space => "Space", Return => "Enter", Backspace => "Backspace", Delete => "Delete",
        Tab => "Tab", Escape => "Escape",
        UpArrow => "ArrowUp", DownArrow => "ArrowDown", LeftArrow => "ArrowLeft",
        RightArrow => "ArrowRight", Home => "Home", End => "End", PageUp => "PageUp",
        PageDown => "PageDown", Insert => "Insert",
        F1 => "F1", F2 => "F2", F3 => "F3", F4 => "F4", F5 => "F5", F6 => "F6", F7 => "F7",
        F8 => "F8", F9 => "F9", F10 => "F10", F11 => "F11", F12 => "F12",
        Kp0 => "Numpad0", Kp1 => "Numpad1", Kp2 => "Numpad2", Kp3 => "Numpad3",
        Kp4 => "Numpad4", Kp5 => "Numpad5", Kp6 => "Numpad6", Kp7 => "Numpad7",
        Kp8 => "Numpad8", Kp9 => "Numpad9", KpPlus => "NumpadAdd", KpMinus => "NumpadSubtract",
        KpMultiply => "NumpadMultiply", KpDivide => "NumpadDivide", KpDelete => "NumpadDecimal",
        KpReturn => "NumpadEnter", NumLock => "NumLock",
        PrintScreen => "PrintScreen", ScrollLock => "ScrollLock", Pause => "Pause",
        // X11 keycodes rdev doesn't name.
        Unknown(135) => "ContextMenu",
        Unknown(123) => "AudioVolumeUp",
        Unknown(122) => "AudioVolumeDown",
        Unknown(121) => "AudioVolumeMute",
        Unknown(_) => return None,
    })
}
