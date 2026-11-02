use crate::settings::Settings;
use keytone_core::keyhook;
use keytone_core::library::Library;
use keytone_core::pack::{LoadedPack, PackFile};
use keytone_core::recorder::Recording;
use keytone_core::sound::Sound;
use keytone_core::Player;
use parking_lot::Mutex;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Default)]
pub struct HookStatus {
    pub running: bool,
    /// "granted" | "denied" | "not_required"
    pub permission: String,
    pub error: Option<String>,
    /// "permission" | "unsupported" | "other"
    pub error_kind: Option<String>,
}

pub struct AppState {
    pub player: Player,
    pub library: Library,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub hook: Mutex<HookStatus>,
    pub recording: Mutex<Option<Recording>>,
    pub take: Mutex<Option<Arc<Sound>>>,
    /// The UI asked for live key events (only while its window is visible).
    pub key_feedback: Arc<AtomicBool>,
    pub tray_enabled_item: Mutex<Option<tauri::menu::CheckMenuItem<tauri::Wry>>>,
}

impl AppState {
    pub fn save_settings(&self) {
        let s = self.settings.lock().clone();
        if let Err(e) = s.save(&self.settings_path) {
            log::warn!("saving settings: {e:#}");
        }
    }

    /// Load (or reload) the active pack into the player.
    pub fn load_active(&self) -> Result<Option<Arc<LoadedPack>>, String> {
        let id = self.settings.lock().active_pack.clone();
        let Some(id) = id else {
            self.player.set_pack(None);
            return Ok(None);
        };
        let dir = self.library.dir_of(&id).map_err(err)?;
        match LoadedPack::load(&dir) {
            Ok(p) => {
                for w in &p.warnings {
                    log::warn!("pack {id}: {w}");
                }
                let p = Arc::new(p);
                self.player.set_pack(Some(p.clone()));
                Ok(Some(p))
            }
            Err(e) => {
                self.player.set_pack(None);
                Err(format!("Could not load pack “{id}”: {e:#}"))
            }
        }
    }

    pub fn start_hook(&self) {
        let mut st = self.hook.lock();
        st.permission = match keyhook::permission() {
            keyhook::Permission::Granted => "granted",
            keyhook::Permission::Denied => "denied",
            keyhook::Permission::NotRequired => "not_required",
        }
        .into();
        if st.running {
            return;
        }
        match self.player.start_hook() {
            Ok(()) => {
                st.running = true;
                st.error = None;
                st.error_kind = None;
            }
            Err(e) => {
                log::warn!("key hook: {e}");
                st.error_kind = Some(
                    match e {
                        keyhook::HookError::PermissionDenied => "permission",
                        keyhook::HookError::Unsupported(_) => "unsupported",
                        keyhook::HookError::Other(_) => "other",
                    }
                    .into(),
                );
                st.error = Some(e.to_string());
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PackDetail {
    pub id: String,
    pub meta: PackFile,
    /// Audio files present in the pack folder.
    pub files: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn audio_files(dir: &std::path::Path) -> Vec<String> {
    const EXT: &[&str] = &["wav", "mp3", "ogg", "flac", "m4a", "aac", "oga"];
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let ext = p.extension()?.to_str()?.to_lowercase();
            if p.is_file() && EXT.contains(&ext.as_str()) {
                Some(e.file_name().to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect();
    v.sort();
    v
}
