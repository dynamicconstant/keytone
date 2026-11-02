//! macOS backend: listen-only `CGEventTap` on a dedicated CFRunLoop thread.
//!
//! Requires the "Input Monitoring" privacy permission. Unlike some hook
//! libraries we never call the Text Input Source APIs (which must run on the
//! main thread and crash when called from a tap thread on macOS 14+), and we
//! never translate key codes to characters.

use crate::{Dir, HookError, KeyEvent, Permission};
use core_foundation::base::TCFType;
use core_foundation::mach_port::CFMachPortRef;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    EventField,
};
use std::collections::HashSet;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

static TAP_PORT: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

pub fn permission() -> Permission {
    if unsafe { CGPreflightListenEventAccess() } {
        Permission::Granted
    } else {
        Permission::Denied
    }
}

pub fn request_permission() -> Permission {
    if unsafe { CGRequestListenEventAccess() } {
        Permission::Granted
    } else {
        Permission::Denied
    }
}

pub fn start<F>(callback: F) -> Result<(), HookError>
where
    F: Fn(KeyEvent) + Send + Sync + 'static,
{
    if permission() != Permission::Granted {
        return Err(HookError::PermissionDenied);
    }

    crate::spawn_and_wait("keyhook-mac", move |ready| {
        // Modifier keys only produce FlagsChanged events; track which are held
        // so we can tell presses from releases.
        let held_mods: Mutex<HashSet<&'static str>> = Mutex::new(HashSet::new());

        let tap = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![CGEventType::KeyDown, CGEventType::KeyUp, CGEventType::FlagsChanged],
            move |_proxy, etype, event| {
                match etype {
                    CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                        let port = TAP_PORT.load(Ordering::Acquire);
                        if !port.is_null() {
                            unsafe { CGEventTapEnable(port as CFMachPortRef, true) };
                        }
                    }
                    CGEventType::KeyDown | CGEventType::KeyUp => {
                        let kc = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                        if let Some(code) = map_keycode(kc as u16) {
                            let dir = if matches!(etype, CGEventType::KeyDown) {
                                Dir::Down
                            } else {
                                Dir::Up
                            };
                            callback(KeyEvent { code, dir });
                        }
                    }
                    CGEventType::FlagsChanged => {
                        let kc = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                        let flags = event.get_flags().bits();
                        if kc == 0x39 {
                            // Caps Lock reports only the toggle; play a full press.
                            callback(KeyEvent { code: "CapsLock", dir: Dir::Down });
                            callback(KeyEvent { code: "CapsLock", dir: Dir::Up });
                        } else if let Some((code, mask)) = modifier(kc as u16) {
                            let mut held = held_mods.lock().unwrap();
                            let pressed = flags & mask != 0;
                            let was_held = held.contains(code);
                            if pressed && !was_held {
                                held.insert(code);
                                drop(held);
                                callback(KeyEvent { code, dir: Dir::Down });
                            } else if !pressed && was_held {
                                held.remove(code);
                                drop(held);
                                callback(KeyEvent { code, dir: Dir::Up });
                            }
                        }
                    }
                    _ => {}
                }
                None
            },
        );

        let tap = match tap {
            Ok(t) => t,
            Err(()) => {
                let _ = ready.send(Err(HookError::PermissionDenied));
                return;
            }
        };

        TAP_PORT.store(
            tap.mach_port.as_concrete_TypeRef() as *mut std::ffi::c_void,
            Ordering::Release,
        );
        let source = match tap.mach_port.create_runloop_source(0) {
            Ok(s) => s,
            Err(()) => {
                let _ = ready.send(Err(HookError::Other("could not create run loop source".into())));
                return;
            }
        };
        let rl = CFRunLoop::get_current();
        unsafe { rl.add_source(&source, kCFRunLoopCommonModes) };
        tap.enable();
        let _ = ready.send(Ok(()));
        CFRunLoop::run_current();
    })
}

/// Modifier key -> (code, device-dependent flag mask).
fn modifier(kc: u16) -> Option<(&'static str, u64)> {
    Some(match kc {
        0x38 => ("ShiftLeft", 0x0000_0002),
        0x3C => ("ShiftRight", 0x0000_0004),
        0x3B => ("ControlLeft", 0x0000_0001),
        0x3E => ("ControlRight", 0x0000_2000),
        0x3A => ("AltLeft", 0x0000_0020),
        0x3D => ("AltRight", 0x0000_0040),
        0x37 => ("MetaLeft", 0x0000_0008),
        0x36 => ("MetaRight", 0x0000_0010),
        0x3F => ("Fn", 0x0080_0000),
        _ => return None,
    })
}

/// macOS virtual key codes (kVK_*) -> W3C codes. Positional (ANSI), so they
/// stay correct for any keyboard layout.
fn map_keycode(kc: u16) -> Option<&'static str> {
    Some(match kc {
        0x00 => "KeyA", 0x0B => "KeyB", 0x08 => "KeyC", 0x02 => "KeyD", 0x0E => "KeyE",
        0x03 => "KeyF", 0x05 => "KeyG", 0x04 => "KeyH", 0x22 => "KeyI", 0x26 => "KeyJ",
        0x28 => "KeyK", 0x25 => "KeyL", 0x2E => "KeyM", 0x2D => "KeyN", 0x1F => "KeyO",
        0x23 => "KeyP", 0x0C => "KeyQ", 0x0F => "KeyR", 0x01 => "KeyS", 0x11 => "KeyT",
        0x20 => "KeyU", 0x09 => "KeyV", 0x0D => "KeyW", 0x07 => "KeyX", 0x10 => "KeyY",
        0x06 => "KeyZ",
        0x1D => "Digit0", 0x12 => "Digit1", 0x13 => "Digit2", 0x14 => "Digit3",
        0x15 => "Digit4", 0x17 => "Digit5", 0x16 => "Digit6", 0x1A => "Digit7",
        0x1C => "Digit8", 0x19 => "Digit9",
        0x1B => "Minus", 0x18 => "Equal", 0x21 => "BracketLeft", 0x1E => "BracketRight",
        0x2A => "Backslash", 0x29 => "Semicolon", 0x27 => "Quote", 0x32 => "Backquote",
        0x2B => "Comma", 0x2F => "Period", 0x2C => "Slash", 0x0A => "IntlBackslash",
        0x31 => "Space", 0x24 => "Enter", 0x33 => "Backspace", 0x75 => "Delete",
        0x30 => "Tab", 0x35 => "Escape",
        0x7E => "ArrowUp", 0x7D => "ArrowDown", 0x7B => "ArrowLeft", 0x7C => "ArrowRight",
        0x73 => "Home", 0x77 => "End", 0x74 => "PageUp", 0x79 => "PageDown", 0x72 => "Help",
        0x7A => "F1", 0x78 => "F2", 0x63 => "F3", 0x76 => "F4", 0x60 => "F5", 0x61 => "F6",
        0x62 => "F7", 0x64 => "F8", 0x65 => "F9", 0x6D => "F10", 0x67 => "F11", 0x6F => "F12",
        0x69 => "F13", 0x6B => "F14", 0x71 => "F15", 0x6A => "F16", 0x40 => "F17",
        0x4F => "F18", 0x50 => "F19", 0x5A => "F20",
        0x52 => "Numpad0", 0x53 => "Numpad1", 0x54 => "Numpad2", 0x55 => "Numpad3",
        0x56 => "Numpad4", 0x57 => "Numpad5", 0x58 => "Numpad6", 0x59 => "Numpad7",
        0x5B => "Numpad8", 0x5C => "Numpad9", 0x45 => "NumpadAdd", 0x4E => "NumpadSubtract",
        0x43 => "NumpadMultiply", 0x4B => "NumpadDivide", 0x41 => "NumpadDecimal",
        0x4C => "NumpadEnter", 0x51 => "NumpadEqual", 0x47 => "NumLock",
        0x48 => "AudioVolumeUp", 0x49 => "AudioVolumeDown", 0x4A => "AudioVolumeMute",
        _ => return None,
    })
}
