//! Windows backend: `WH_KEYBOARD_LL` low-level keyboard hook.
//!
//! We deliberately never call `ToUnicode`/`ToUnicodeEx` (unlike many hook
//! libraries): translating keys to characters from a hook corrupts dead-key
//! state, which breaks typing accents (é, ñ, ü) in other apps.

use crate::{Dir, HookError, KeyEvent};
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

type Callback = Box<dyn Fn(KeyEvent) + Send + Sync + 'static>;
static CALLBACK: OnceLock<Callback> = OnceLock::new();

pub fn start<F>(callback: F) -> Result<(), HookError>
where
    F: Fn(KeyEvent) + Send + Sync + 'static,
{
    if CALLBACK.set(Box::new(callback)).is_err() {
        return Err(HookError::Other("key hook already started".into()));
    }
    crate::spawn_and_wait("keyhook-win", |ready| unsafe {
        let hmod = GetModuleHandleW(std::ptr::null());
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hmod, 0);
        if hook.is_null() {
            let _ = ready.send(Err(HookError::Other(format!(
                "SetWindowsHookExW failed (error {})",
                windows_sys::Win32::Foundation::GetLastError()
            ))));
            return;
        }
        let _ = ready.send(Ok(()));
        // A low-level hook is serviced by this thread's message loop.
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    })
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = &*(lparam as *const KBDLLHOOKSTRUCT);
        let dir = match wparam as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(Dir::Down),
            WM_KEYUP | WM_SYSKEYUP => Some(Dir::Up),
            _ => None,
        };
        if let Some(dir) = dir {
            let extended = info.flags & LLKHF_EXTENDED != 0;
            if let Some(code) = map_vk(info.vkCode, extended) {
                if let Some(cb) = CALLBACK.get() {
                    cb(KeyEvent { code, dir });
                }
            }
        }
    }
    // Never swallow the event: we only observe.
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

fn map_vk(vk: u32, extended: bool) -> Option<&'static str> {
    Some(match vk {
        0x41 => "KeyA", 0x42 => "KeyB", 0x43 => "KeyC", 0x44 => "KeyD", 0x45 => "KeyE",
        0x46 => "KeyF", 0x47 => "KeyG", 0x48 => "KeyH", 0x49 => "KeyI", 0x4A => "KeyJ",
        0x4B => "KeyK", 0x4C => "KeyL", 0x4D => "KeyM", 0x4E => "KeyN", 0x4F => "KeyO",
        0x50 => "KeyP", 0x51 => "KeyQ", 0x52 => "KeyR", 0x53 => "KeyS", 0x54 => "KeyT",
        0x55 => "KeyU", 0x56 => "KeyV", 0x57 => "KeyW", 0x58 => "KeyX", 0x59 => "KeyY",
        0x5A => "KeyZ",
        0x30 => "Digit0", 0x31 => "Digit1", 0x32 => "Digit2", 0x33 => "Digit3",
        0x34 => "Digit4", 0x35 => "Digit5", 0x36 => "Digit6", 0x37 => "Digit7",
        0x38 => "Digit8", 0x39 => "Digit9",
        0xBD => "Minus", 0xBB => "Equal", 0xDB => "BracketLeft", 0xDD => "BracketRight",
        0xDC => "Backslash", 0xBA => "Semicolon", 0xDE => "Quote", 0xC0 => "Backquote",
        0xBC => "Comma", 0xBE => "Period", 0xBF => "Slash", 0xE2 => "IntlBackslash",
        0xA0 | 0x10 => "ShiftLeft", 0xA1 => "ShiftRight",
        0xA2 | 0x11 => "ControlLeft", 0xA3 => "ControlRight",
        0xA4 | 0x12 => "AltLeft", 0xA5 => "AltRight",
        0x5B => "MetaLeft", 0x5C => "MetaRight", 0x14 => "CapsLock",
        0x20 => "Space",
        0x0D => if extended { "NumpadEnter" } else { "Enter" },
        0x08 => "Backspace", 0x2E => "Delete", 0x09 => "Tab", 0x1B => "Escape",
        0x26 => "ArrowUp", 0x28 => "ArrowDown", 0x25 => "ArrowLeft", 0x27 => "ArrowRight",
        0x24 => "Home", 0x23 => "End", 0x21 => "PageUp", 0x22 => "PageDown", 0x2D => "Insert",
        0x70 => "F1", 0x71 => "F2", 0x72 => "F3", 0x73 => "F4", 0x74 => "F5", 0x75 => "F6",
        0x76 => "F7", 0x77 => "F8", 0x78 => "F9", 0x79 => "F10", 0x7A => "F11", 0x7B => "F12",
        0x7C => "F13", 0x7D => "F14", 0x7E => "F15", 0x7F => "F16", 0x80 => "F17",
        0x81 => "F18", 0x82 => "F19", 0x83 => "F20",
        0x60 => "Numpad0", 0x61 => "Numpad1", 0x62 => "Numpad2", 0x63 => "Numpad3",
        0x64 => "Numpad4", 0x65 => "Numpad5", 0x66 => "Numpad6", 0x67 => "Numpad7",
        0x68 => "Numpad8", 0x69 => "Numpad9", 0x6B => "NumpadAdd", 0x6D => "NumpadSubtract",
        0x6A => "NumpadMultiply", 0x6F => "NumpadDivide", 0x6E => "NumpadDecimal",
        0x90 => "NumLock",
        0x2C => "PrintScreen", 0x91 => "ScrollLock", 0x13 => "Pause", 0x5D => "ContextMenu",
        0x2F => "Help", 0xAF => "AudioVolumeUp", 0xAE => "AudioVolumeDown",
        0xAD => "AudioVolumeMute",
        _ => return None,
    })
}
