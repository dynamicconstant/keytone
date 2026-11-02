//! Commands callable from the UI via `invoke()`.

use crate::settings::SettingsPatch;
use crate::state::{audio_files, err, AppState, HookStatus, PackDetail};
use keytone_core::engine::{list_output_devices, EngineInfo};
use keytone_core::keyhook::{self, Dir};
use keytone_core::library::PackSummary;
use keytone_core::pack::{RandomCfg, Target};
use keytone_core::recorder::Recording;
use keytone_core::sound::decode_file;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

type R<T> = Result<T, String>;

#[derive(Serialize)]
pub struct Snapshot {
    settings: crate::settings::Settings,
    packs: Vec<PackSummary>,
    active: Option<PackDetail>,
    active_error: Option<String>,
    engine: EngineInfo,
    hook: HookStatus,
    platform: &'static str,
    version: &'static str,
}

fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

fn detail(st: &AppState) -> Option<PackDetail> {
    let pack = st.player.pack()?;
    let id = { st.settings.lock().active_pack.clone() }?;
    Some(PackDetail {
        id,
        files: audio_files(&pack.dir),
        warnings: pack.warnings.clone(),
        meta: pack.meta.clone(),
    })
}

fn active_id(st: &AppState) -> R<String> {
    st.settings.lock().active_pack.clone().ok_or_else(|| "No pack is active".to_string())
}

fn parse_dir(d: &str) -> R<Dir> {
    match d {
        "down" => Ok(Dir::Down),
        "up" => Ok(Dir::Up),
        _ => Err(format!("bad direction {d:?}")),
    }
}

#[tauri::command]
pub fn get_state(st: State<'_, AppState>) -> Snapshot {
    // Never hold the settings lock across calls that lock it again (parking_lot
    // mutexes are not re-entrant): take a copy first.
    let has_active = st.settings.lock().active_pack.is_some();
    let active_error = if st.player.pack().is_none() && has_active {
        st.load_active().err()
    } else {
        None
    };
    let settings = st.settings.lock().clone();
    let active = detail(&st);
    Snapshot {
        settings,
        packs: st.library.list(),
        active,
        active_error,
        engine: st.player.engine().info(),
        hook: st.hook.lock().clone(),
        platform: platform(),
        version: env!("CARGO_PKG_VERSION"),
    }
}

// ---------------------------------------------------------------- playback

pub fn apply_enabled(app: &AppHandle, on: bool) {
    let st = app.state::<AppState>();
    st.player.set_enabled(on);
    st.settings.lock().enabled = on;
    st.save_settings();
    if let Some(item) = st.tray_enabled_item.lock().as_ref() {
        let _ = item.set_checked(on);
    }
    let _ = app.emit("enabled-changed", on);
}

#[tauri::command]
pub fn set_enabled(app: AppHandle, on: bool) {
    apply_enabled(&app, on);
}

#[tauri::command]
pub fn set_volume(st: State<'_, AppState>, volume: f32) {
    st.player.engine().set_volume(volume);
    st.settings.lock().volume = volume;
    st.save_settings();
}

/// The UI turns live key events on while it is visible and off when hidden,
/// so key activity never crosses into the webview needlessly.
#[tauri::command]
pub fn set_key_feedback(st: State<'_, AppState>, on: bool) {
    st.key_feedback.store(on, Ordering::Relaxed);
}

#[tauri::command]
pub fn preview_key(st: State<'_, AppState>, code: String) -> R<()> {
    let pack = st.player.pack().ok_or("No pack is active")?;
    let c = pack.resolve(&code, Dir::Down);
    if c.is_empty() {
        return Ok(());
    }
    let s = c[fastrand::usize(..c.len())].clone();
    st.player.engine().play(keytone_core::mapper::PlayCmd { sound: s, gain: pack.meta.volume, pitch: 1.0 });
    Ok(())
}

/// Preview a sound reference ("file.wav" or "#segment") of the active pack.
#[tauri::command]
pub async fn preview_ref(st: State<'_, AppState>, reference: String) -> R<()> {
    let pack = st.player.pack().ok_or("No pack is active")?;
    let sound = match pack.clip(&reference) {
        Some(s) => s,
        None => {
            let name = keytone_core::pack::safe_file_name(&reference).map_err(err)?;
            let mut s = decode_file(&pack.dir.join(name), keytone_core::pack::MAX_CLIP_SECS).map_err(|e| format!("{e:#}"))?;
            s.trim_leading_silence(0.003);
            Arc::new(s)
        }
    };
    st.player.engine().play(keytone_core::mapper::PlayCmd { sound, gain: pack.meta.volume, pitch: 1.0 });
    Ok(())
}

// ---------------------------------------------------------------- packs

#[tauri::command]
pub async fn set_active_pack(st: State<'_, AppState>, id: String) -> R<Option<PackDetail>> {
    st.library.dir_of(&id).map_err(err)?;
    st.settings.lock().active_pack = Some(id);
    st.save_settings();
    st.load_active()?;
    Ok(detail(&st))
}

#[tauri::command]
pub async fn set_mapping(st: State<'_, AppState>, targets: Vec<Target>, dir: String, refs: Option<Vec<String>>) -> R<Option<PackDetail>> {
    let id = active_id(&st)?;
    let dir = parse_dir(&dir)?;
    let mut pf = st.library.read(&id).map_err(err)?;
    for t in &targets {
        *pf.slot_mut(t).get_mut(dir) = refs.clone();
    }
    st.library.write(&id, &pf).map_err(err)?;
    st.load_active()?;
    Ok(detail(&st))
}

#[derive(Deserialize)]
pub struct MetaPatch {
    name: Option<String>,
    author: Option<String>,
    description: Option<String>,
    volume: Option<f32>,
    trim_silence: Option<bool>,
    random: Option<RandomCfg>,
}

#[tauri::command]
pub async fn update_pack_meta(st: State<'_, AppState>, patch: MetaPatch) -> R<Option<PackDetail>> {
    let id = active_id(&st)?;
    let mut pf = st.library.read(&id).map_err(err)?;
    if let Some(v) = patch.name {
        if !v.trim().is_empty() {
            pf.name = v.trim().to_string();
        }
    }
    if let Some(v) = patch.author {
        pf.author = v;
    }
    if let Some(v) = patch.description {
        pf.description = v;
    }
    if let Some(v) = patch.volume {
        pf.volume = v.clamp(0.0, 2.0);
    }
    if let Some(v) = patch.trim_silence {
        pf.trim_silence = v;
    }
    if let Some(v) = patch.random {
        pf.random = RandomCfg { pitch_cents: v.pitch_cents.clamp(0.0, 300.0), gain_db: v.gain_db.clamp(0.0, 12.0) };
    }
    st.library.write(&id, &pf).map_err(err)?;
    st.load_active()?;
    Ok(detail(&st))
}

/// Copy an audio file into the active pack. Returns its file name.
#[tauri::command]
pub async fn import_sound(st: State<'_, AppState>, path: String) -> R<String> {
    let id = active_id(&st)?;
    st.library.add_sound_file(&id, &PathBuf::from(path)).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn delete_sound(st: State<'_, AppState>, file: String) -> R<Option<PackDetail>> {
    let id = active_id(&st)?;
    let name = keytone_core::pack::safe_file_name(&file).map_err(err)?.to_string();
    let mut pf = st.library.read(&id).map_err(err)?;
    let strip = |v: &mut Option<Vec<String>>| {
        if let Some(list) = v {
            list.retain(|r| r != &name);
        }
    };
    for slot in std::iter::once(&mut pf.defaults).chain(pf.groups.values_mut()).chain(pf.keys.values_mut()) {
        strip(&mut slot.down);
        strip(&mut slot.up);
    }
    st.library.write(&id, &pf).map_err(err)?;
    let _ = std::fs::remove_file(st.library.dir_of(&id).map_err(err)?.join(&name));
    st.load_active()?;
    Ok(detail(&st))
}

#[tauri::command]
pub async fn create_pack(st: State<'_, AppState>, name: String) -> R<String> {
    st.library.create_empty(name.trim()).map_err(err)
}

#[tauri::command]
pub async fn duplicate_pack(st: State<'_, AppState>, id: String, name: String) -> R<String> {
    st.library.duplicate(&id, name.trim()).map_err(err)
}

#[tauri::command]
pub async fn delete_pack(st: State<'_, AppState>, id: String) -> R<()> {
    st.library.delete(&id).map_err(err)?;
    let was_active = st.settings.lock().active_pack.as_deref() == Some(id.as_str());
    if was_active {
        let next = st.library.list().into_iter().next().map(|p| p.id);
        st.settings.lock().active_pack = next;
        st.save_settings();
        let _ = st.load_active();
    }
    Ok(())
}

#[tauri::command]
pub async fn import_pack(st: State<'_, AppState>, path: String) -> R<String> {
    st.library.import(&PathBuf::from(path)).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn export_pack(st: State<'_, AppState>, id: String, dest: String) -> R<()> {
    st.library.export(&id, &PathBuf::from(dest)).map_err(|e| format!("{e:#}"))
}

// ---------------------------------------------------------------- recording

#[derive(Serialize)]
pub struct RecStatus {
    active: bool,
    level: f32,
    elapsed: f32,
    max_secs: u32,
}

#[tauri::command]
pub async fn start_recording(st: State<'_, AppState>) -> R<()> {
    let rec = Recording::start().map_err(|e| format!("{e:#}"))?;
    *st.recording.lock() = Some(rec);
    Ok(())
}

#[tauri::command]
pub fn recording_status(st: State<'_, AppState>) -> RecStatus {
    let g = st.recording.lock();
    RecStatus {
        active: g.is_some(),
        level: g.as_ref().map(|r| r.level()).unwrap_or(0.0),
        elapsed: g.as_ref().map(|r| r.elapsed_secs()).unwrap_or(0.0),
        max_secs: keytone_core::recorder::MAX_RECORD_SECS,
    }
}

/// Stop recording; keeps the trimmed take for preview/saving. Returns its length in seconds.
#[tauri::command]
pub async fn stop_recording(st: State<'_, AppState>) -> R<f32> {
    let rec = st.recording.lock().take().ok_or("Not recording")?;
    let take = rec.finish().map_err(|e| format!("{e:#}"))?;
    let secs = take.duration_secs();
    let take = Arc::new(take);
    *st.take.lock() = Some(take.clone());
    st.player.engine().play_sound(take);
    Ok(secs)
}

#[tauri::command]
pub fn preview_take(st: State<'_, AppState>) -> R<()> {
    let t = st.take.lock().clone().ok_or("Nothing recorded yet")?;
    st.player.engine().play_sound(t);
    Ok(())
}

/// Save the last take into the active pack as WAV. Returns the file name.
#[tauri::command]
pub async fn save_take(st: State<'_, AppState>, name: String) -> R<String> {
    let id = active_id(&st)?;
    let take = st.take.lock().clone().ok_or("Nothing recorded yet")?;
    let (file, path) = st.library.new_file_name(&id, if name.trim().is_empty() { "recording" } else { name.trim() }, "wav").map_err(err)?;
    take.write_wav(&path).map_err(|e| format!("{e:#}"))?;
    *st.take.lock() = None;
    Ok(file)
}

#[tauri::command]
pub fn discard_take(st: State<'_, AppState>) {
    *st.recording.lock() = None;
    *st.take.lock() = None;
}

// ---------------------------------------------------------------- settings & system

#[tauri::command]
pub fn list_devices() -> Vec<String> {
    list_output_devices()
}

#[tauri::command]
pub async fn update_settings(app: AppHandle, patch: SettingsPatch) -> R<Snapshot> {
    let st = app.state::<AppState>();
    if let Some(v) = patch.volume {
        st.player.engine().set_volume(v);
        st.settings.lock().volume = v;
    }
    if let Some(on) = patch.enabled {
        apply_enabled(&app, on);
    }
    let mut reconfigure = false;
    if let Some(d) = patch.output_device {
        st.settings.lock().output_device = d;
        reconfigure = true;
    }
    if let Some(b) = patch.buffer_frames {
        st.settings.lock().buffer_frames = b;
        reconfigure = true;
    }
    if reconfigure {
        let (d, b) = {
            let s = st.settings.lock();
            (s.output_device.clone(), s.buffer_frames)
        };
        st.player.engine().configure(d, b);
    }
    if let Some(hk) = patch.toggle_hotkey {
        crate::register_hotkey(&app, hk.as_deref())?;
        st.settings.lock().toggle_hotkey = hk;
    }
    if let Some(on) = patch.launch_at_login {
        use tauri_plugin_autostart::ManagerExt;
        let al = app.autolaunch();
        let r = if on { al.enable() } else { al.disable() };
        r.map_err(|e| format!("Could not change launch-at-login: {e}"))?;
        st.settings.lock().launch_at_login = on;
    }
    st.save_settings();
    Ok(get_state(st))
}

#[tauri::command]
pub fn request_permission(st: State<'_, AppState>) -> HookStatus {
    let _ = keyhook::request_permission();
    st.start_hook();
    st.hook.lock().clone()
}

#[tauri::command]
pub fn retry_hook(st: State<'_, AppState>) -> HookStatus {
    st.start_hook();
    st.hook.lock().clone()
}

#[tauri::command]
pub fn open_permission_settings() -> R<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
            .spawn()
            .map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}
