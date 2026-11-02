use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub active_pack: Option<String>,
    pub volume: f32,
    pub enabled: bool,
    /// Global shortcut that toggles sounds on/off, e.g. "CmdOrCtrl+Shift+K".
    pub toggle_hotkey: Option<String>,
    /// `None` = follow the system default output.
    pub output_device: Option<String>,
    /// `None` = driver default.
    pub buffer_frames: Option<u32>,
    pub launch_at_login: bool,
    /// Bundled packs that have been installed at least once (so deleting
    /// one doesn't make it reappear on next launch).
    pub installed_bundled: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            active_pack: Some("soft-thock".into()),
            volume: 0.7,
            enabled: true,
            toggle_hotkey: Some("CmdOrCtrl+Shift+K".into()),
            output_device: None,
            // Linux desktops route ALSA through PulseAudio/PipeWire, whose
            // bridges handle small fixed buffers poorly; use their default.
            buffer_frames: if cfg!(target_os = "linux") { None } else { Some(256) },
            launch_at_login: false,
            installed_bundled: Vec::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }
}

/// Partial update sent from the UI. Double `Option` distinguishes
/// "not sent" from "set to null".
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SettingsPatch {
    pub volume: Option<f32>,
    pub enabled: Option<bool>,
    #[serde(deserialize_with = "double_opt")]
    pub toggle_hotkey: Option<Option<String>>,
    #[serde(deserialize_with = "double_opt")]
    pub output_device: Option<Option<String>>,
    #[serde(deserialize_with = "double_opt")]
    pub buffer_frames: Option<Option<u32>>,
    pub launch_at_login: Option<bool>,
}

fn double_opt<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_distinguishes_missing_from_null() {
        let p: SettingsPatch = serde_json::from_str(r#"{"output_device": null, "volume": 0.5}"#).unwrap();
        assert_eq!(p.output_device, Some(None));
        assert_eq!(p.toggle_hotkey, None);
        assert_eq!(p.volume, Some(0.5));
    }

    #[test]
    fn tolerant_load() {
        let s: Settings = serde_json::from_str(r#"{"volume": 0.3}"#).unwrap();
        assert_eq!(s.volume, 0.3);
        assert!(s.enabled);
    }
}
