//! `keyhook` — a tiny global keyboard listener.
//!
//! Privacy contract: the only information that ever leaves the OS hook is
//! *which physical key* moved and *in which direction*. Characters, text,
//! keyboard layouts and timestamps are never read or exposed.
//!
//! Key codes are normalized to W3C `KeyboardEvent.code` names
//! (`"KeyA"`, `"Digit1"`, `"ShiftLeft"`, `"Space"`, `"Enter"`, ...), which is
//! the same vocabulary used by sound packs and by the UI's keyboard view.
//!
//! Backends:
//! * macOS  — listen-only `CGEventTap` (needs Input Monitoring permission)
//! * Windows — `WH_KEYBOARD_LL` low-level hook
//! * Linux  — X11 `XRecord` through `rdev` (Wayland is not supported)

use std::fmt;

mod codes;
pub use codes::{all_codes, group_of, intern, KeyGroup, ALL_GROUPS};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// Direction of a key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dir {
    Down,
    Up,
}

/// A physical key event. `code` is a W3C `KeyboardEvent.code` name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: &'static str,
    pub dir: Dir,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookError {
    /// The OS refused to install the hook because the user has not granted
    /// permission (macOS Input Monitoring / Accessibility).
    PermissionDenied,
    /// Hooks are not available on this platform/session (e.g. Wayland).
    Unsupported(String),
    Other(String),
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookError::PermissionDenied => write!(
                f,
                "Keyboard monitoring permission has not been granted"
            ),
            HookError::Unsupported(s) => write!(f, "Global key hooks are not supported here: {s}"),
            HookError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for HookError {}

/// Permission state for the global hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Granted,
    Denied,
    /// The platform has no permission concept (Windows, Linux/X11).
    NotRequired,
}

/// Check (without prompting) whether we may listen to global key events.
pub fn permission() -> Permission {
    #[cfg(target_os = "macos")]
    {
        macos::permission()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Permission::NotRequired
    }
}

/// Ask the OS to show its permission prompt (macOS only; no-op elsewhere).
/// Returns the state after asking.
pub fn request_permission() -> Permission {
    #[cfg(target_os = "macos")]
    {
        macos::request_permission()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Permission::NotRequired
    }
}

/// Start listening on a dedicated background thread.
///
/// `callback` is invoked from the hook thread for every key-down and key-up
/// of a *recognized* key. It must be fast and must not block — push into a
/// channel and return. OS auto-repeat may deliver repeated `Down` events
/// while a key is held; de-duplicate downstream.
///
/// Returns once the hook is installed (or failed to install).
pub fn start<F>(callback: F) -> Result<(), HookError>
where
    F: Fn(KeyEvent) + Send + Sync + 'static,
{
    #[cfg(target_os = "macos")]
    {
        macos::start(callback)
    }
    #[cfg(target_os = "windows")]
    {
        windows::start(callback)
    }
    #[cfg(target_os = "linux")]
    {
        linux::start(callback)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = callback;
        Err(HookError::Unsupported("unknown OS".into()))
    }
}

/// Helper used by all backends: wait for the hook thread to report whether
/// installation succeeded.
pub(crate) fn spawn_and_wait<B>(name: &str, body: B) -> Result<(), HookError>
where
    B: FnOnce(std::sync::mpsc::Sender<Result<(), HookError>>) + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || body(tx))
        .map_err(|e| HookError::Other(e.to_string()))?;
    match rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(r) => r,
        // The thread is running and never reported failure: treat as success.
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(HookError::Other("hook thread exited unexpectedly".into()))
        }
    }
}
